use std::convert::TryFrom;

use ruma_identifiers::{RoomAliasId, RoomId, UserId};
use slog::Logger;

use api::{MatrixApi, RocketchatApi};
use config::Config;
use errors::*;
use models::Room;

/// A channel on a Rocket.Chat server.
pub struct Channel<'a> {
    /// The channels ID
    pub id: String,
    /// The application service config
    config: &'a Config,
    /// Logger context
    logger: &'a Logger,
    /// API to call the Matrix homeserver
    matrix_api: &'a MatrixApi,
    /// The ID of the channels Rocket.Chat server
    server_id: &'a str,
}

impl<'a> Channel<'a> {
    /// Create a channel room model, to interact with Rocket.chat channels.
    pub fn new(
        config: &'a Config,
        logger: &'a Logger,
        matrix_api: &'a MatrixApi,
        id: String,
        server_id: &'a str,
    ) -> Channel<'a> {
        Channel {
            config: config,
            logger: logger,
            matrix_api: matrix_api,
            id: id,
            server_id: server_id,
        }
    }

    /// Create a new channel model based on the Rocket.Chat channel name and server.
    pub fn from_name(
        config: &'a Config,
        logger: &'a Logger,
        matrix_api: &'a MatrixApi,
        name: String,
        server_id: &'a str,
        rocketchat_api: &'a RocketchatApi,
    ) -> Result<Channel<'a>> {
        let channel_id = rocketchat_api
            .channels_list()?
            .iter()
            .filter_map(|channel| if channel.name == Some(name.clone()) {
                Some(channel.id.clone())
            } else {
                None
            })
            .next()
            .unwrap_or_default();

        let channel = Channel::new(config, logger, matrix_api, channel_id, server_id);
        Ok(channel)
    }

    /// Bridges a new room between Rocket.Chat and Matrix. It creates the room on the Matrix
    /// homeserver and manages the rooms virtual users.
    pub fn bridge(
        &self,
        rocketchat_api: &RocketchatApi,
        name: Option<String>,
        userlist: &[String],
        creator_id: &UserId,
        invited_user_id: &UserId,
    ) -> Result<RoomId> {
        debug!(self.logger, "Briding new room, Rocket.Chat channel: {}", name.clone().unwrap_or_default());

        let matrix_room_alias = self.build_room_alias_name();
        let alias = Some(matrix_room_alias);
        let room_id = Room::create(self.matrix_api, alias, name.clone(), creator_id, invited_user_id)?;
        let matrix_room_alias_id = self.build_room_alias_id()?;
        let alias_id = Some(matrix_room_alias_id);
        self.matrix_api.put_canonical_room_alias(room_id.clone(), alias_id)?;

        let room = Room::new(self.config, self.logger, self.matrix_api, room_id.clone());
        room.join_all_rocketchat_users(rocketchat_api, userlist, self.server_id.to_owned())?;

        Ok(room_id)
    }

    /// Indicates if the channel is bridged for a given user.
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
        let room_alias_id = RoomAliasId::try_from(&alias_id).chain_err(|| ErrorKind::InvalidRoomAliasId(alias_id.clone()))?;
        Ok(room_alias_id)
    }

    /// Build the Matrix room alias local part for a room that is bridged to a Rocket.Chat server.
    pub fn build_room_alias_name(&self) -> String {
        format!("{}#{}#{}", self.config.sender_localpart, self.server_id, self.id)
    }

    /// Gets the Matrix room ID for a Rocket.Chat channel ID and a Rocket.Chat server.
    pub fn matrix_id(&self) -> Result<Option<RoomId>> {
        let room_alias_id = self.build_room_alias_id()?;
        self.matrix_api.get_room_alias(room_alias_id)
    }
}
