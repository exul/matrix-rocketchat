use diesel::sqlite::SqliteConnection;
use ruma_identifiers::{RoomId, UserId};
use slog::Logger;

use api::{MatrixApi, RocketchatApi};
use api::rocketchat::Channel;
use config::Config;
use errors::*;
use handlers::events::RoomHandler;
use i18n::*;
use models::{RocketchatServer, Room};

/// The `RoomCreator` creates new rooms and bridges them
pub struct RoomCreator<'a> {
    config: &'a Config,
    connection: &'a SqliteConnection,
    logger: &'a Logger,
    matrix_api: &'a MatrixApi,
    creator_id: &'a UserId,
    invited_user_id: &'a UserId,
}

impl<'a> RoomCreator<'a> {
    /// Create a new `RoomHandler`.
    pub fn new(
        config: &'a Config,
        connection: &'a SqliteConnection,
        logger: &'a Logger,
        matrix_api: &'a MatrixApi,
        creator_id: &'a UserId,
        invited_user_id: &'a UserId,
    ) -> RoomCreator<'a> {
        RoomCreator {
            config: config,
            connection: connection,
            logger: logger,
            matrix_api: matrix_api,
            creator_id: creator_id,
            invited_user_id: invited_user_id,
        }
    }

    /// Bridges a new room between Rocket.Chat and Matrix. It creates the room on the Matrix
    /// homeserver and manages the rooms virtual users.
    pub fn bridge_new_room(
        &self,
        rocketchat_api: Box<RocketchatApi>,
        server: &RocketchatServer,
        channel: &Channel,
    ) -> Result<RoomId> {
        debug!(self.logger, "Briding new room, Rocket.Chat channel: {}", channel.name.clone().unwrap_or_default());

        let matrix_room_alias = Room::build_room_alias_name(self.config, &server.id, &channel.id);
        let room_id = self.create_room(Some(matrix_room_alias), channel.name.clone())?;
        let matrix_room_alias_id = Room::build_room_alias_id(self.config, &server.id, &channel.id)?;
        self.matrix_api.put_canonical_room_alias(room_id.clone(), Some(matrix_room_alias_id))?;

        let room = Room::new(self.config, self.logger, self.matrix_api, room_id.clone());
        let room_handler = RoomHandler::new(self.config, self.connection, self.logger, self.matrix_api, &room);
        room_handler.add_virtual_users_to_room(rocketchat_api, channel, server.id.clone())?;

        Ok(room_id)
    }

    /// Create a room on the Matrix homeserver with the power levels for a bridged room.
    pub fn create_room(&self, room_alias: Option<String>, room_display_name: Option<String>) -> Result<RoomId> {
        let room_id = self.matrix_api.create_room(room_display_name.clone(), room_alias, self.creator_id)?;
        debug!(self.logger, "Successfully created room, room_id is {}", &room_id);

        self.matrix_api.set_default_powerlevels(room_id.clone(), self.creator_id.clone())?;
        debug!(self.logger, "Successfully set powerlevels for room {}", &room_id);

        self.matrix_api.invite(room_id.clone(), self.invited_user_id.clone(), self.creator_id.clone())?;
        debug!(self.logger, "{} successfully invited {} into room {}", &self.creator_id, &self.invited_user_id, &room_id);

        Ok(room_id)
    }


    /// Bridges a room that is already bridged (for other users) for a new user.
    pub fn bridge_existing_room(&self, room: Room, user_id: UserId, rocketchat_channel_name: String) -> Result<()> {
        debug!(self.logger, "Briding existing room, Rocket.Chat channel: {}", rocketchat_channel_name);

        if room.user_ids(None)?.iter().any(|id| id == &user_id) {
            bail_error!(
                ErrorKind::RocketchatChannelAlreadyBridged(rocketchat_channel_name.clone()),
                t!(["errors", "rocketchat_channel_already_bridged"]).with_vars(vec![("channel_name", rocketchat_channel_name)])
            );
        }

        let bot_user_id = self.config.matrix_bot_user_id()?;
        self.matrix_api.invite(room.id.clone(), user_id, bot_user_id)
    }
}
