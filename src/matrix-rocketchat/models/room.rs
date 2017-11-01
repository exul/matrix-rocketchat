use std::convert::TryFrom;

use diesel::sqlite::SqliteConnection;
use ruma_events::room::member::MembershipState;
use ruma_identifiers::{RoomAliasId, RoomId, UserId};
use slog::Logger;

use api::{MatrixApi, RocketchatApi};
use config::Config;
use errors::*;
use handlers::rocketchat::VirtualUserHandler;
use models::{RocketchatServer, UserOnRocketchatServer};

/// A room that is managed by the application service. This can be either a bridged room or an
/// admin room.
pub struct Room<'a> {
    /// The application service config
    config: &'a Config,
    /// Logger context
    logger: &'a Logger,
    /// API to call the Matrix homeserver
    matrix_api: &'a MatrixApi,
    /// The rooms Matrix ID
    pub id: RoomId,
}

impl<'a> Room<'a> {
    /// Create a new room model, to interact with Matrix rooms.
    pub fn new(config: &'a Config, logger: &'a Logger, matrix_api: &'a MatrixApi, id: RoomId) -> Room<'a> {
        Room {
            config: config,
            logger: logger,
            matrix_api: matrix_api,
            id: id,
        }
    }

    /// Indicates if the room is bridged for a given user.
    pub fn is_bridged_for_user(
        config: &Config,
        logger: &Logger,
        matrix_api: &MatrixApi,
        rocketchat_server_id: &str,
        rocketchat_channel_id: &str,
        user_id: &UserId,
    ) -> Result<bool> {
        let room_alias_id = Room::build_room_alias_id(config, rocketchat_server_id, rocketchat_channel_id)?;

        match matrix_api.get_room_alias(room_alias_id)? {
            Some(room_id) => {
                let room = Room::new(config, logger, matrix_api, room_id);
                let is_user_in_room = room.user_ids(None)?.iter().any(|id| id == user_id);
                Ok(is_user_in_room)
            }
            None => Ok(false),
        }
    }

    /// Build the room alias id for a room that is bridged to a Rocket.Chat server.
    pub fn build_room_alias_id(
        config: &Config,
        rocketchat_server_id: &str,
        rocketchat_channel_id: &str,
    ) -> Result<RoomAliasId> {
        let room_alias_name = Room::build_room_alias_name(config, rocketchat_server_id, rocketchat_channel_id);
        let room_alias_id = format!("#{}:{}", room_alias_name, config.hs_domain);
        let room_alias =
            RoomAliasId::try_from(&room_alias_id).chain_err(|| ErrorKind::InvalidRoomAliasId(room_alias_id.clone()))?;
        Ok(room_alias)
    }

    /// Build the room alias local part for a room that is bridged to a Rocket.Chat server.
    pub fn build_room_alias_name(config: &Config, rocketchat_server_id: &str, rocketchat_channel_id: &str) -> String {
        format!("{}#{}#{}", config.sender_localpart, rocketchat_server_id, rocketchat_channel_id)
    }

    /// Gets the matrix room ID for a Rocket.Chat channel name and a Rocket.Chat server.
    pub fn matrix_id_from_rocketchat_channel_name(
        config: &Config,
        matrix_api: &MatrixApi,
        rocketchat_api: &RocketchatApi,
        rocketchat_server_id: &str,
        rocketchat_channel_name: String,
    ) -> Result<Option<RoomId>> {
        let channel_id = rocketchat_api
            .channels_list()?
            .iter()
            .filter_map(|channel| {
                if channel.name == Some(rocketchat_channel_name.clone()) {
                    return Some(channel.id.clone());
                }

                None
            })
            .next();

        match channel_id {
            Some(channel_id) => {
                Room::matrix_id_from_rocketchat_channel_id(config, matrix_api, rocketchat_server_id, &channel_id)
            }
            None => Ok(None),
        }
    }

    /// Gets the matrix room ID for a Rocket.Chat channel ID and a Rocket.Chat server.
    pub fn matrix_id_from_rocketchat_channel_id(
        config: &Config,
        matrix_api: &MatrixApi,
        rocketchat_server_id: &str,
        rocketchat_channel_id: &str,
    ) -> Result<Option<RoomId>> {
        let room_alias_id = Room::build_room_alias_id(config, rocketchat_server_id, rocketchat_channel_id)?;
        matrix_api.get_room_alias(room_alias_id)
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
                    let user_id = UserId::try_from(&state_key).chain_err(|| ErrorKind::InvalidUserId(state_key))?;
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

        let (server_id, virtual_user_id) = VirtualUserHandler::rocketchat_server_and_user_id_from_matrix_id(virtual_user_id);
        let server = match RocketchatServer::find_by_id(conn, &server_id)? {
            Some(server) => server,
            None => {
                debug!(self.logger, "No connected Rocket.Chat server with ID {} found for this direct message", &server_id);
                return Ok(None);
            }
        };

        let user_on_rocketchat_server = match UserOnRocketchatServer::find_by_matrix_user_id(conn, user_matrix_id, server_id)? {
            Some(user_on_rocketchat_server) => user_on_rocketchat_server,
            None => {
                debug!(self.logger, "Matrix user {} is not logged into the Rocket.Chat server {}", user_matrix_id, server.id);
                return Ok(None);
            }
        };

        let rocketchat_user_id = user_on_rocketchat_server.rocketchat_user_id.clone().unwrap_or_default();
        let rocketchat_auth_token = user_on_rocketchat_server.rocketchat_auth_token.clone().unwrap_or_default();
        let rocketchat_api = RocketchatApi::new(server.rocketchat_url.clone(), self.logger.clone())?
            .with_credentials(rocketchat_user_id, rocketchat_auth_token);

        // It is safe to check if a direct channel ID contains the virtual users ID, because a user
        // can only have one direct message room with another user. Which means when the virtual user
        // ID is part of the channel name, the direct message channel with that user is found.
        let direct_message_channel_ids = rocketchat_api.direct_messages_list()?;
        let channel_ids: Vec<String> = direct_message_channel_ids
            .iter()
            .filter_map(|c| if c.id.to_lowercase().contains(&virtual_user_id) {
                Some(c.id.clone())
            } else {
                None
            })
            .collect();

        if let Some(channel_id) = channel_ids.into_iter().next() {
            return Ok(Some((server, channel_id)));
        }

        debug!(
            self.logger,
            "No direct message channel for the user {} found on the Rocket.Chat server {}",
            virtual_user_id,
            server.id
        );
        Ok(None)
    }
}
