use std::convert::TryFrom;

use diesel::sqlite::SqliteConnection;
use ruma_events::room::member::MembershipState;
use ruma_identifiers::{RoomAliasId, RoomId, UserId};
use slog::Logger;

use api::{MatrixApi, RocketchatApi};
use db::UserOnRocketchatServer;
use config::Config;
use errors::*;
use super::RocketchatServer;

/// A room that is managed by the application service. This can be either a bridged room or an
/// admin room.
pub struct Room {}

impl Room {
    /// Indicates if the room is bridged for a given user.
    pub fn is_bridged_for_user(
        config: &Config,
        matrix_api: &MatrixApi,
        rocketchat_server_id: &str,
        rocketchat_channel_id: &str,
        matrix_user_id: &UserId,
    ) -> Result<bool> {
        let room_alias_id = Room::build_room_alias_id(config, rocketchat_server_id, rocketchat_channel_id)?;

        match matrix_api.get_room_alias(room_alias_id)? {
            Some(matrix_room_id) => {
                let is_user_in_room = Room::user_ids(matrix_api, matrix_room_id, None)?.iter().any(|id| id == matrix_user_id);
                Ok(is_user_in_room)
            }
            None => Ok(false),
        }
    }

    /// Get the Rocket.Chat server this room is connected to, if any.
    pub fn rocketchat_server(
        connection: &SqliteConnection,
        matrix_api: &MatrixApi,
        matrix_room_id: RoomId,
    ) -> Result<Option<RocketchatServer>> {
        let alias = matrix_api.get_room_canonical_alias(matrix_room_id)?.map(|alias| alias.to_string()).unwrap_or_default();
        let rocketchat_server_id = alias.split('#').nth(2).unwrap_or_default();
        RocketchatServer::find_by_id(connection, rocketchat_server_id)
    }

    /// Get the Rocket.Chat server for an admin room.
    pub fn rocketchat_server_for_admin_room(
        connection: &SqliteConnection,
        matrix_api: &MatrixApi,
        matrix_room_id: RoomId,
    ) -> Result<Option<RocketchatServer>> {
        let rocketchat_server_url = matrix_api.get_room_topic(matrix_room_id)?;
        match rocketchat_server_url {
            Some(rocketchat_server_url) => RocketchatServer::find_by_url(connection, &rocketchat_server_url),
            None => Ok(None),
        }
    }

    /// Users that are currently in the room.
    pub fn user_ids(matrix_api: &MatrixApi, matrix_room_id: RoomId, sender_id: Option<UserId>) -> Result<Vec<UserId>> {
        let member_events = matrix_api.get_room_members(matrix_room_id.clone(), sender_id)?;

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

    /// Check if the room is a direct message room.
    pub fn is_direct_message_room(config: &Config, matrix_api: &MatrixApi, room_id: RoomId) -> Result<bool> {
        let room_creator_id = matrix_api.get_room_creator(room_id)?;
        Ok(config.is_application_service_virtual_user(&room_creator_id))
    }

    /// Checks if a room is an admin room.
    pub fn is_admin_room(config: &Config, matrix_api: &MatrixApi, matrix_room_id: RoomId) -> Result<bool> {
        // it cannot be an admin room if the bot user does not have access to it
        if !Room::is_accessible_by_bot(matrix_api, matrix_room_id.clone())? {
            return Ok(false);
        }

        let matrix_bot_user_id = config.matrix_bot_user_id()?;
        let matrix_user_ids = Room::user_ids(matrix_api, matrix_room_id.clone(), None)?;
        let bot_user_in_room = matrix_user_ids.iter().any(|id| id == &matrix_bot_user_id);
        let room_creator_id = matrix_api.get_room_creator(matrix_room_id)?;

        Ok(!config.is_application_service_user(&room_creator_id) && bot_user_in_room)
    }

    /// Gets the Rocket.Chat channel id for a room that is bridged to Matrix.
    pub fn rocketchat_channel_id(matrix_api: &MatrixApi, matrix_room_id: RoomId) -> Result<Option<String>> {
        let room_canonical_alias = match matrix_api.get_room_canonical_alias(matrix_room_id.clone())? {
            Some(room_canonical_alias) => room_canonical_alias.alias().to_string(),
            None => return Ok(None),
        };

        let rocketchat_channel_id = room_canonical_alias.split('#').nth(2).unwrap_or_default();
        Ok(Some(rocketchat_channel_id.to_string()))
    }

    /// Checks if an admin room is connected to a Rocket.Chat server.
    pub fn is_connected(connection: &SqliteConnection, matrix_api: &MatrixApi, matrix_room_id: RoomId) -> Result<bool> {
        match matrix_api.get_room_topic(matrix_room_id)? {
            Some(rocketchat_server_url) => {
                let server = RocketchatServer::find_by_url(connection, &rocketchat_server_url)?;
                Ok(server.is_some())
            }
            None => Ok(false),
        }
    }

    /// Find the Matrix user in a direct message room
    pub fn direct_message_matrix_user(
        config: &Config,
        matrix_api: &MatrixApi,
        matrix_room_id: RoomId,
    ) -> Result<Option<UserId>> {
        let room_creator_id = matrix_api.get_room_creator(matrix_room_id.clone())?;
        let user_ids = Room::user_ids(matrix_api, matrix_room_id.clone(), Some(room_creator_id))?;
        if user_ids.len() > 2 {
            bail_error!(ErrorKind::GettingMatrixUserForDirectMessageRoomError);
        }

        let matrix_user_id = user_ids.into_iter().find(|id| !config.is_application_service_virtual_user(id));
        Ok(matrix_user_id)
    }

    /// Determine if the bot user has access to a room.
    pub fn is_accessible_by_bot(matrix_api: &MatrixApi, matrix_room_id: RoomId) -> Result<bool> {
        matrix_api.is_room_accessible_by_bot(matrix_room_id)
    }

    /// Build the room alias local part for a room that is bridged to a Rocket.Chat server.
    pub fn build_room_alias_name(config: &Config, rocketchat_server_id: &str, rocketchat_channel_id: &str) -> String {
        format!("{}#{}#{}", config.sender_localpart, rocketchat_server_id, rocketchat_channel_id)
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

    /// Find the Rocket.Chat server and channel for a direct message room on Matrix.
    /// It uses the virtual users ID for determine which server is used and queries the Rocket.Chat
    /// server to find the matching direct message room.
    /// This is done to avoid the usage of aliases in direct message rooms.
    pub fn rocketchat_for_direct_room(
        config: &Config,
        conn: &SqliteConnection,
        logger: &Logger,
        matrix_api: &MatrixApi,
        room_id: RoomId,
    ) -> Result<Option<(RocketchatServer, String)>> {
        if !Room::is_direct_message_room(config, matrix_api, room_id.clone())? {
            debug!(logger, "Room is not a direct message room, will not continue to find a matching DM user.");
            return Ok(None);
        }

        let room_creator = matrix_api.get_room_creator(room_id.clone())?;
        let user_ids = Room::user_ids(matrix_api, room_id, Some(room_creator))?;

        let user_matrix_id = match user_ids.iter().find(|id| !config.is_application_service_virtual_user(id)) {
            Some(user_id) => user_id,
            None => {
                debug!(logger, "No Matrix user found for the receiver of this direct message");
                return Ok(None);
            }
        };

        let virtual_user_matrix_id = match user_ids.iter().find(|id| config.is_application_service_virtual_user(id)) {
            Some(user_id) => user_id,
            None => {
                debug!(logger, "No existing virtual user found for this direct message");
                return Ok(None);
            }
        };

        //TODO: Move this into it's own fuction, to make sure the same logic is used everywhere
        let virtual_user_local_part = virtual_user_matrix_id.localpart().to_owned();
        let id_parts: Vec<&str> = virtual_user_local_part.splitn(2, '_').collect();
        let server_and_user_id: Vec<&str> = id_parts.into_iter().nth(1).unwrap_or_default().splitn(2, '_').collect();
        let server_id = server_and_user_id.clone().into_iter().nth(0).unwrap_or_default().to_string();
        let virtual_user_id = server_and_user_id.clone().into_iter().nth(1).unwrap_or_default();

        let server = match RocketchatServer::find_by_id(conn, &server_id)? {
            Some(server) => server,
            None => {
                debug!(logger, "No connected Rocket.Chat server with ID {} found for this direct message", &server_id);
                return Ok(None);
            }
        };

        let user_on_rocketchat_server = match UserOnRocketchatServer::find_by_matrix_user_id(conn, user_matrix_id, server_id)? {
            Some(user_on_rocketchat_server) => user_on_rocketchat_server,
            None => {
                debug!(logger, "Matrix user {} is not logged into the Rocket.Chat server {}", user_matrix_id, server.id);
                return Ok(None);
            }
        };

        let rocketchat_user_id = user_on_rocketchat_server.rocketchat_user_id.clone().unwrap_or_default();
        let rocketchat_auth_token = user_on_rocketchat_server.rocketchat_auth_token.clone().unwrap_or_default();
        let rocketchat_api = RocketchatApi::new(server.rocketchat_url.clone(), logger.clone())?
            .with_credentials(rocketchat_user_id, rocketchat_auth_token);

        // It is safe to check if a direct channel ID contains the virtual users ID, because a user
        // can only have one direct message room with another user. Which means when the virtual user
        // ID is part of the channel name, the direct message channel with that user is found.
        let direct_message_channel_ids = rocketchat_api.direct_messages_list()?;
        let channel_ids: Vec<String> = direct_message_channel_ids
            .iter()
            .filter_map(|c| if c.id.to_lowercase().contains(virtual_user_id) {
                Some(c.id.clone())
            } else {
                None
            })
            .collect();

        if let Some(channel_id) = channel_ids.into_iter().next() {
            return Ok(Some((server, channel_id)));
        }

        debug!(
            logger,
            "No direct message channel for the user {} found on the Rocket.Chat server {}",
            virtual_user_id,
            server.id
        );
        Ok(None)
    }
}
