use std::convert::TryFrom;

use ruma_identifiers::{RoomAliasId, RoomId, UserId};
use slog::Logger;

use api::{MatrixApi, RocketchatApi};
use config::Config;
use errors::*;
use models::Room;

/// A channel or group on a Rocket.Chat server.
pub struct RocketchatRoom<'a> {
    /// The channel or group ID
    pub id: String,
    /// The ID of the channels or groups Rocket.Chat server
    pub server_id: &'a str,
    /// The application service config
    config: &'a Config,
    /// Logger context
    logger: &'a Logger,
    /// API to call the Matrix homeserver
    matrix_api: &'a dyn MatrixApi,
}

impl<'a> RocketchatRoom<'a> {
    /// Create a rocketchat room model, to interact with Rocket.chat channels and groups.
    pub fn new(
        config: &'a Config,
        logger: &'a Logger,
        matrix_api: &'a dyn MatrixApi,
        id: String,
        server_id: &'a str,
    ) -> RocketchatRoom<'a> {
        RocketchatRoom { config, logger, matrix_api, id, server_id }
    }

    /// Create a new rocketchat room model based on the Rocket.Chat channel or group name and server.
    pub fn from_name(
        config: &'a Config,
        logger: &'a Logger,
        matrix_api: &'a dyn MatrixApi,
        name: &'a str,
        server_id: &'a str,
        rocketchat_api: &'a dyn RocketchatApi,
    ) -> Result<RocketchatRoom<'a>> {
        let mut rocketchat_rooms = rocketchat_api.channels_list()?;
        let groups = rocketchat_api.groups_list()?;
        rocketchat_rooms.extend(groups.iter().cloned());
        let id =
            rocketchat_rooms
                .iter()
                .filter_map(|rocketchat_room| {
                    if rocketchat_room.name == Some(name.to_string()) {
                        Some(rocketchat_room.id.clone())
                    } else {
                        None
                    }
                })
                .next()
                .unwrap_or_default();

        let rocketchat_room = RocketchatRoom::new(config, logger, matrix_api, id, server_id);
        Ok(rocketchat_room)
    }

    /// Bridges a new room between Rocket.Chat and Matrix. It creates the room on the Matrix
    /// homeserver and manages the rooms virtual users.
    pub fn bridge(
        &self,
        rocketchat_api: &dyn RocketchatApi,
        name: &Option<String>,
        userlist: &[String],
        creator_id: &UserId,
        invited_user_id: &UserId,
    ) -> Result<RoomId> {
        debug!(self.logger, "Briding new room, Rocket.Chat channel/group: {}", name.clone().unwrap_or_default());

        let matrix_room_alias = self.build_room_alias_name();
        let alias = Some(matrix_room_alias);
        let room_id = Room::create(self.matrix_api, alias, name, creator_id, invited_user_id)?;
        let matrix_room_alias_id = self.build_room_alias_id()?;
        let alias_id = Some(matrix_room_alias_id);
        self.matrix_api.put_canonical_room_alias(room_id.clone(), alias_id)?;

        let room = Room::new(self.config, self.logger, self.matrix_api, room_id.clone());
        room.join_all_rocketchat_users(rocketchat_api, userlist, self.server_id)?;

        Ok(room_id)
    }

    /// Indicates if the channel or group is bridged for a given user.
    pub fn is_bridged_for_user(&self, user_id: &UserId) -> Result<bool> {
        let room_alias_id = self.build_room_alias_id()?;

        match self.matrix_api.get_room_alias(room_alias_id)? {
            Some(room_id) => {
                let room = Room::new(self.config, self.logger, self.matrix_api, room_id);
                let is_user_in_room = room.user_ids(None)?.iter().any(|id| id == user_id);
                Ok(is_user_in_room)
            }
            None => Ok(false),
        }
    }

    /// Build the Matrix room alias id for a room that is bridged to a Rocket.Chat server.
    pub fn build_room_alias_id(&self) -> Result<RoomAliasId> {
        let alias_name = self.build_room_alias_name();
        let alias_id = format!("#{}:{}", alias_name, self.config.hs_domain);
        let room_alias_id =
            RoomAliasId::try_from(alias_id.as_ref()).chain_err(|| ErrorKind::InvalidRoomAliasId(alias_id.clone()))?;
        Ok(room_alias_id)
    }

    /// Build the Matrix room alias local part for a room that is bridged to a Rocket.Chat server.
    pub fn build_room_alias_name(&self) -> String {
        format!("{}#{}#{}", self.config.sender_localpart, self.server_id, self.id)
    }

    /// Gets the Matrix room ID for a Rocket.Chat channel or group ID and a Rocket.Chat server.
    pub fn matrix_id(&self) -> Result<Option<RoomId>> {
        let room_alias_id = self.build_room_alias_id()?;
        self.matrix_api.get_room_alias(room_alias_id)
    }
}
