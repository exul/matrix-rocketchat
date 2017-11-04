use diesel::sqlite::SqliteConnection;
use ruma_identifiers::{RoomId, UserId};
use slog::Logger;

use api::{MatrixApi, RocketchatApi};
use api::rocketchat::Channel;
use config::Config;
use errors::*;
use handlers::events::MembershipHandler;
use models::{RocketchatServer, Room};

/// The `RoomHandler` creates new rooms and bridges them
pub struct RoomHandler<'a> {
    config: &'a Config,
    connection: &'a SqliteConnection,
    logger: &'a Logger,
    matrix_api: &'a MatrixApi,
    creator_id: &'a UserId,
    invited_user_id: &'a UserId,
}

impl<'a> RoomHandler<'a> {
    /// Create a new `MembershipHandler`.
    pub fn new(
        config: &'a Config,
        connection: &'a SqliteConnection,
        logger: &'a Logger,
        matrix_api: &'a MatrixApi,
        creator_id: &'a UserId,
        invited_user_id: &'a UserId,
    ) -> RoomHandler<'a> {
        RoomHandler {
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
        let alias = Some(matrix_room_alias);
        let room_id = Room::create(self.matrix_api, alias, channel.name.clone(), &self.creator_id, &self.invited_user_id)?;
        let matrix_room_alias_id = Room::build_room_alias_id(self.config, &server.id, &channel.id)?;
        let alias_id = Some(matrix_room_alias_id);
        self.matrix_api.put_canonical_room_alias(room_id.clone(), alias_id)?;

        let room = Room::new(self.config, self.logger, self.matrix_api, room_id.clone());
        let membership_handler = MembershipHandler::new(self.config, self.connection, self.logger, self.matrix_api, &room);
        membership_handler.add_virtual_users_to_room(rocketchat_api, channel, server.id.clone())?;

        Ok(room_id)
    }
}
