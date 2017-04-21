use diesel::sqlite::SqliteConnection;
use ruma_events::collections::all::Event;
use ruma_identifiers::{RoomId, UserId};
use slog::Logger;

use api::MatrixApi;
use config::Config;
use errors::*;
use handlers::ErrorNotifier;
use super::{MessageHandler, RoomHandler};

/// Dispatches events to the corresponding handler.
pub struct EventDispatcher<'a> {
    config: &'a Config,
    connection: &'a SqliteConnection,
    logger: &'a Logger,
    matrix_api: Box<MatrixApi>,
}

impl<'a> EventDispatcher<'a> {
    /// Create a new `EventDispatcher` with an SQLite connection
    pub fn new(config: &'a Config,
               connection: &'a SqliteConnection,
               logger: &'a Logger,
               matrix_api: Box<MatrixApi>)
               -> EventDispatcher<'a> {
        EventDispatcher {
            config: config,
            connection: connection,
            logger: logger,
            matrix_api: matrix_api,
        }
    }

    /// Processes the events that are passed to the method by forwarding them to the
    /// corresponding handler.
    pub fn process(&self, events: Vec<Box<Event>>) -> Result<()> {
        for event in events {
            match *event {
                Event::RoomMember(member_event) => {
                    if let Err(err) = RoomHandler::new(self.config,
                                                       self.connection,
                                                       self.logger.clone(),
                                                       self.matrix_api.clone())
                               .process(&member_event) {
                        return self.handle_error(err, member_event.room_id, &member_event.user_id);
                    }
                }
                Event::RoomMessage(message_event) => {
                    if let Err(err) = MessageHandler::new(self.config, self.connection, self.logger, self.matrix_api.clone())
                           .process(&message_event) {
                        return self.handle_error(err, message_event.room_id, &message_event.user_id);
                    }
                }
                _ => debug!(self.logger, "Skipping event, because the event type is not known"),
            }
        }
        Ok(())
    }

    /// Forward the error to the notifier to send the corresponding message to the user
    pub fn handle_error(&self, err: Error, room_id: RoomId, user_id: &UserId) -> Result<()> {
        let error_notifier = ErrorNotifier {
            config: self.config,
            connection: self.connection,
            logger: &self.logger,
            matrix_api: &self.matrix_api,
        };
        error_notifier.send_message_to_user(&err, room_id, user_id)?;
        if err.user_message.is_none() {
            return Err(err);
        }
        Ok(())
    }
}
