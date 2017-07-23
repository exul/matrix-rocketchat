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

        match Room::find_by_matrix_room_id(self.connection, &event.room_id)? {
            Some(ref room) if room.is_admin_room => {
                CommandHandler::new(self.config, self.connection, self.logger, self.matrix_api.as_ref()).process(event, room)?;
            }
            Some(ref room) => {
                Forwarder::new(self.connection, self.logger).process(event, room)?;
            }
            None => debug!(self.logger, "Skipping event, because the room is not bridged"),
        }

        Ok(())
    }
}
