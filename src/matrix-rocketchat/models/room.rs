use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use diesel::sqlite::SqliteConnection;
use ruma_events::room::member::MembershipState;
use ruma_identifiers::{RoomAliasId, RoomId, UserId};
use slog::Logger;

use api::{MatrixApi, RocketchatApi};
use config::Config;
use errors::*;
use i18n::*;
use models::{RocketchatServer, UserOnRocketchatServer, VirtualUser};

/// The delay in milliseconds between two API requests (to not DOS the server)
pub const API_QUERY_DELAY: u64 = 500;

lazy_static! {
    /// Direct room cache
    static ref DM_ROOMS: Mutex<HashMap<(String, String), RoomId>> = { Mutex::new(HashMap::new()) };
}

/// A room that is managed by the application service. This can be either a bridged room or an
/// admin room.
pub struct Room<'a> {
    /// The rooms Matrix ID
    pub id: RoomId,
    /// The application service config
    config: &'a Config,
    /// Logger context
    logger: &'a Logger,
    /// API to call the Matrix homeserver
    matrix_api: &'a dyn MatrixApi,
}

impl<'a> Room<'a> {
    /// Create a new room model, to interact with Matrix rooms.
    pub fn new(config: &'a Config, logger: &'a Logger, matrix_api: &'a dyn MatrixApi, id: RoomId) -> Room<'a> {
        Room { config, logger, matrix_api, id }
    }

    /// Create a room on the Matrix homeserver with the power levels for a bridged room.
    pub fn create(
        matrix_api: &dyn MatrixApi,
        alias: Option<String>,
        display_name: &Option<String>,
        creator_id: &UserId,
        invited_user_id: &UserId,
    ) -> Result<RoomId> {
        let room_id = matrix_api.create_room(display_name.clone(), alias, creator_id)?;
        matrix_api.set_default_powerlevels(room_id.clone(), creator_id.clone())?;
        matrix_api.invite(room_id.clone(), invited_user_id.clone(), creator_id.clone())?;

        Ok(room_id)
    }

    /// Get an existing direct message room.
    pub fn get_dm(
        config: &'a Config,
        logger: &'a Logger,
        matrix_api: &'a dyn MatrixApi,
        channel_id: String,
        sender_id: &UserId,
        receiver_id: &UserId,
    ) -> Result<Option<Room<'a>>> {
        // If the user does not exist yet, there is no existing direct message room
        if matrix_api.get_display_name(sender_id.clone())?.is_none() {
            return Ok(None);
        }

        match DM_ROOMS.lock() {
            Ok(dm_rooms) => match dm_rooms.get(&(channel_id.clone(), receiver_id.to_string())) {
                Some(room_id) => {
                    debug!(logger, "Found room {} for receiver {} in cache", channel_id, receiver_id);
                    let room = Room::new(config, logger, matrix_api, room_id.clone());
                    return Ok(Some(room));
                }
                None => {
                    debug!(logger, "Room {} for receiver {} not found in cache", channel_id, receiver_id);
                }
            },
            Err(err) => {
                warn!(logger, "Could lock DM cache to get room {} with receiver {}: {}", channel_id, receiver_id, err);
            }
        }

        for room_id in matrix_api.get_joined_rooms(sender_id.clone())? {
            let room = Room::new(config, logger, matrix_api, room_id);
            let user_ids = room.user_ids(Some(sender_id.clone()))?;
            if user_ids.iter().all(|id| id == sender_id || id == receiver_id) {
                room.add_to_cache(channel_id, &receiver_id);
                return Ok(Some(room));
            }
        }

        Ok(None)
    }

    /// Bridges a room that is already bridged (for other users) for a new user.
    pub fn bridge_for_user(&self, user_id: UserId, rocketchat_channel_name: String) -> Result<()> {
        debug!(self.logger, "Briding existing room, Rocket.Chat channel: {}", rocketchat_channel_name);

        if self.user_ids(None)?.iter().any(|id| id == &user_id) {
            bail_error!(
                ErrorKind::RocketchatChannelAlreadyBridged(rocketchat_channel_name.clone()),
                t!(["errors", "rocketchat_channel_already_bridged"])
                    .with_vars(vec![("rocketchat_room_name", rocketchat_channel_name)])
            );
        }

        let bot_user_id = self.config.matrix_bot_user_id()?;
        self.matrix_api.invite(self.id.clone(), user_id, bot_user_id)
    }

    /// Get the Rocket.Chat server this room is connected to, if any.
    pub fn rocketchat_server(&self, connection: &SqliteConnection) -> Result<Option<RocketchatServer>> {
        let alias = self.matrix_api.get_room_canonical_alias(self.id.clone())?.map(|a| a.to_string()).unwrap_or_default();
        let rocketchat_server_id = alias.split('#').nth(2).unwrap_or_default();
        RocketchatServer::find_by_id(connection, rocketchat_server_id)
    }

    /// Get the Rocket.Chat server for an admin room.
    pub fn rocketchat_server_for_admin_room(&self, connection: &SqliteConnection) -> Result<Option<RocketchatServer>> {
        let rocketchat_server_url = self.matrix_api.get_room_topic(self.id.clone())?;
        match rocketchat_server_url {
            Some(rocketchat_server_url) => RocketchatServer::find_by_url(connection, &rocketchat_server_url),
            None => Ok(None),
        }
    }

    /// Users that are currently in the room.
    pub fn user_ids(&self, sender_id: Option<UserId>) -> Result<Vec<UserId>> {
        let member_events = self.matrix_api.get_room_members(self.id.clone(), sender_id)?;

        let mut user_ids = Vec::new();
        for member_event in member_events {
            match member_event.content.membership {
                MembershipState::Join => {
                    let state_key = member_event.state_key.clone();
                    let user_id = UserId::try_from(state_key.as_ref()).chain_err(|| ErrorKind::InvalidUserId(state_key))?;
                    user_ids.push(user_id)
                }
                _ => continue,
            }
        }

        Ok(user_ids)
    }

    /// Check if the room is a direct message room.
    pub fn is_direct_message_room(&self) -> Result<bool> {
        let room_creator_id = self.matrix_api.get_room_creator(self.id.clone())?;
        Ok(self.config.is_application_service_virtual_user(&room_creator_id))
    }

    /// Checks if a room is an admin room.
    pub fn is_admin_room(&self) -> Result<bool> {
        // it cannot be an admin room if the bot user does not have access to it
        if !self.is_accessible_by_bot()? {
            return Ok(false);
        }

        let matrix_bot_user_id = self.config.matrix_bot_user_id()?;
        let user_ids = self.user_ids(None)?;
        let bot_user_in_room = user_ids.iter().any(|id| id == &matrix_bot_user_id);
        let room_creator_id = self.matrix_api.get_room_creator(self.id.clone())?;

        Ok(!self.config.is_application_service_user(&room_creator_id) && bot_user_in_room)
    }

    /// Gets the Rocket.Chat channel id for a room that is bridged to Matrix.
    pub fn rocketchat_channel_id(&self) -> Result<Option<String>> {
        let room_canonical_alias = match self.matrix_api.get_room_canonical_alias(self.id.clone())? {
            Some(room_canonical_alias) => room_canonical_alias.alias().to_string(),
            None => return Ok(None),
        };

        let rocketchat_channel_id = room_canonical_alias.split('#').nth(2).unwrap_or_default();
        Ok(Some(rocketchat_channel_id.to_string()))
    }

    /// Checks if an admin room is connected to a Rocket.Chat server.
    pub fn is_connected(&self, connection: &SqliteConnection) -> Result<bool> {
        match self.matrix_api.get_room_topic(self.id.clone())? {
            Some(rocketchat_server_url) => {
                let server = RocketchatServer::find_by_url(connection, &rocketchat_server_url)?;
                Ok(server.is_some())
            }
            None => Ok(false),
        }
    }

    /// Find the Matrix user in a direct message room
    pub fn direct_message_matrix_user(&self) -> Result<Option<UserId>> {
        let room_creator_id = self.matrix_api.get_room_creator(self.id.clone())?;
        let user_ids = self.user_ids(Some(room_creator_id))?;
        if user_ids.len() > 2 {
            bail_error!(ErrorKind::GettingMatrixUserForDirectMessageRoomError);
        }

        let user_id = user_ids.into_iter().find(|id| !self.config.is_application_service_virtual_user(id));
        Ok(user_id)
    }

    /// Determine if the bot user has access to a room.
    pub fn is_accessible_by_bot(&self) -> Result<bool> {
        self.matrix_api.is_room_accessible_by_bot(self.id.clone())
    }

    /// Find the Rocket.Chat server and channel for a direct message room on Matrix.
    /// It uses the virtual users ID for determine which server is used and queries the Rocket.Chat
    /// server to find the matching direct message room.
    /// This is done to avoid the usage of aliases in direct message rooms.
    pub fn rocketchat_for_direct_room(&self, conn: &SqliteConnection) -> Result<Option<(RocketchatServer, String)>> {
        if !self.is_direct_message_room()? {
            debug!(self.logger, "Room is not a direct message room, will not continue to find a matching DM user.");
            return Ok(None);
        }

        let room_creator = self.matrix_api.get_room_creator(self.id.clone())?;
        let user_ids = self.user_ids(Some(room_creator))?;

        let user_matrix_id = match user_ids.iter().find(|id| !self.config.is_application_service_virtual_user(id)) {
            Some(user_id) => user_id,
            None => {
                debug!(self.logger, "No Matrix user found for the receiver of this direct message");
                return Ok(None);
            }
        };

        let virtual_user_id = match user_ids.iter().find(|id| self.config.is_application_service_virtual_user(id)) {
            Some(user_id) => user_id,
            None => {
                debug!(self.logger, "No existing virtual user found for this direct message");
                return Ok(None);
            }
        };

        let (server_id, virtual_user_id) = VirtualUser::rocketchat_server_and_user_id_from_matrix_id(virtual_user_id);
        let server = match RocketchatServer::find_by_id(conn, &server_id)? {
            Some(server) => server,
            None => {
                debug!(self.logger, "No connected Rocket.Chat server with ID {} found for this direct message", &server_id);
                return Ok(None);
            }
        };

        let user_on_rocketchat_server = UserOnRocketchatServer::find(conn, user_matrix_id, server_id)?;
        let rocketchat_user_id = user_on_rocketchat_server.rocketchat_user_id.clone().unwrap_or_default();
        let rocketchat_auth_token = user_on_rocketchat_server.rocketchat_auth_token().unwrap_or_default();
        let rocketchat_api = RocketchatApi::new(server.rocketchat_url.clone(), self.logger.clone())?
            .with_credentials(rocketchat_user_id, rocketchat_auth_token);

        // It is safe to check if a direct channel ID contains the virtual users ID, because a user
        // can only have one direct message room with another user. Which means when the virtual user
        // ID is part of the channel name, the direct message channel with that user is found.
        let direct_message_channels = rocketchat_api.dm_list()?;
        for direct_message_channel in direct_message_channels {
            if direct_message_channel.id.to_lowercase().contains(&virtual_user_id) {
                return Ok(Some((server, direct_message_channel.id.clone())));
            }
        }

        debug!(self.logger, "No direct message channel for user {} on Rocket.Chat {} found", virtual_user_id, server.id);
        Ok(None)
    }

    /// Join a user into a room. The join will be skipped if the user is already in the room.
    pub fn join_user(&self, user_id: UserId, inviting_user_id: UserId) -> Result<()> {
        let user_joined_already = self.user_ids(Some(inviting_user_id.clone()))?.iter().any(|id| id == &user_id);
        if !user_joined_already {
            debug!(self.logger, "Adding virtual user {} to room {}", user_id, self.id);
            self.matrix_api.invite(self.id.clone(), user_id.clone(), inviting_user_id)?;

            if user_id.to_string().starts_with(&format!("@{}", self.config.sender_localpart)) {
                self.matrix_api.join(self.id.clone(), user_id)?;
            }
        }

        Ok(())
    }

    /// Join all users that are in a Rocket.Chat room to the Matrix room.
    pub fn join_all_rocketchat_users(
        &self,
        rocketchat_api: &dyn RocketchatApi,
        usernames: &[String],
        rocketchat_server_id: &str,
    ) -> Result<()> {
        debug!(self.logger, "Starting to add virtual users to room {}", self.id);

        let virtual_user = VirtualUser::new(self.config, self.logger, self.matrix_api);

        let bot_user_id = self.config.matrix_bot_user_id()?;
        for username in usernames.iter() {
            let rocketchat_user = rocketchat_api.users_info(username)?;
            let user_id = virtual_user.find_or_register(rocketchat_server_id, &rocketchat_user.id, username)?;
            self.join_user(user_id, bot_user_id.clone())?;
            thread::sleep(Duration::from_millis(API_QUERY_DELAY))
        }

        debug!(self.logger, "Successfully added {} virtual users to room {}", usernames.len(), self.id);

        Ok(())
    }

    /// Get all aliases fro a room.
    pub fn aliases(&self) -> Result<Vec<RoomAliasId>> {
        let bot_user_id = self.config.matrix_bot_user_id()?;
        self.matrix_api.get_room_aliases(self.id.clone(), bot_user_id)
    }

    /// Forget a room (also leaves the room if the user is still in it)
    pub fn forget(&self, user_id: UserId) -> Result<()> {
        self.matrix_api.leave_room(self.id.clone(), user_id.clone())?;
        self.matrix_api.forget_room(self.id.clone(), user_id)
    }

    /// Return a list of users that are logged in on the Rocket.Chat server.
    pub fn logged_in_users(
        &self,
        connection: &SqliteConnection,
        rocketchat_server_id: String,
    ) -> Result<Vec<UserOnRocketchatServer>> {
        let room_creator = self.matrix_api.get_room_creator(self.id.clone())?;
        let user_ids = self
            .user_ids(Some(room_creator))?
            .into_iter()
            .filter(|id| !self.config.is_application_service_virtual_user(id))
            .collect();
        UserOnRocketchatServer::find_by_matrix_user_ids(connection, user_ids, rocketchat_server_id)
    }

    /// Add a room to the cache.
    /// This will speed-up future direct messages because the direct message room lookup is done via
    /// cache instead of going through the users rooms.
    fn add_to_cache(&self, channel_id: String, receiver_id: &UserId) {
        match DM_ROOMS.lock() {
            Ok(mut dm_rooms) => {
                debug!(self.logger, "Adding DM room {} with receiver {} to cache", channel_id, receiver_id);
                dm_rooms.insert((channel_id, receiver_id.to_string()), self.id.clone());
            }
            Err(err) => {
                warn!(self.logger, "Could not add DM room {} with receiver {} to cache: {}", channel_id, receiver_id, err);
            }
        }
    }
}
