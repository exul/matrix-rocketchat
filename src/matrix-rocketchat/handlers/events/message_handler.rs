use diesel::sqlite::SqliteConnection;
use ruma_events::room::message::MessageEvent;
use slog::Logger;

use api::MatrixApi;
use config::Config;
use db::Room;
use errors::*;
use super::{CommandHandler, Forwarder};

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

        let matrix_room_id = event.room_id.clone();
        let matrix_api = self.matrix_api.as_ref();
        if Room::is_admin_room(self.config, self.matrix_api.as_ref(), matrix_room_id.clone())? {
            CommandHandler::new(self.config, self.connection, self.logger, matrix_api).process(event, matrix_room_id)?;
        } else if let Some(channel_id) = Room::rocketchat_channel_id(matrix_api, matrix_room_id.clone())? {
            Forwarder::new(self.connection, self.logger, matrix_api).process(event, matrix_room_id, channel_id)?;
        } else {
            debug!(self.logger, "Skipping event, because the room {} is not bridged", matrix_room_id);
        }

        Ok(())
    }
}
