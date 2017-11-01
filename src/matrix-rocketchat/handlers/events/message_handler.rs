use diesel::sqlite::SqliteConnection;
use ruma_events::room::message::MessageEvent;
use slog::Logger;

use api::MatrixApi;
use config::Config;
use errors::*;
use handlers::events::{CommandHandler, Forwarder};
use models::{RocketchatServer, Room};

/// Handles message events
pub struct MessageHandler<'a> {
    config: &'a Config,
    connection: &'a SqliteConnection,
    logger: &'a Logger,
    matrix_api: Box<MatrixApi>,
}

impl<'a> MessageHandler<'a> {
    /// Create a new `MessageHandler`.
    pub fn new(
        config: &'a Config,
        connection: &'a SqliteConnection,
        logger: &'a Logger,
        matrix_api: Box<MatrixApi>,
    ) -> MessageHandler<'a> {
        MessageHandler {
            config: config,
            connection: connection,
            logger: logger,
            matrix_api: matrix_api,
        }
    }

    /// Handles messages that are sent in a room
    pub fn process(&self, event: &MessageEvent) -> Result<()> {
        if event.user_id == self.config.matrix_bot_user_id()? {
            debug!(self.logger, "Skipping event, because it was sent by the bot user");
            return Ok(());
        }

        let matrix_api = self.matrix_api.as_ref();
        let room = Room::new(self.config, self.logger, self.matrix_api.as_ref(), event.room_id.clone());
        if room.is_admin_room()? {
            CommandHandler::new(self.config, self.connection, self.logger, matrix_api, &room).process(event)?;
        } else if let Some((server, channel_id)) = self.get_rocketchat_server_with_room(&room)? {
            Forwarder::new(self.connection, self.logger).process(event, server, channel_id)?;
        } else {
            debug!(self.logger, "Skipping event, because the room {} is not bridged", &event.room_id);
        }

        Ok(())
    }

    fn get_rocketchat_server_with_room(&self, room: &Room) -> Result<Option<(RocketchatServer, String)>> {
        // if it's a normal room, this will match
        if let Some(channel_id) = room.rocketchat_channel_id()? {
            if let Some(server) = room.rocketchat_server(self.connection)? {
                return Ok(Some((server, channel_id)));
            }
        }

        room.rocketchat_for_direct_room(self.connection)
    }
}
