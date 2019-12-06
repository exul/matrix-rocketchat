use diesel::sqlite::SqliteConnection;
use ruma_events::collections::all::Event;
use ruma_identifiers::RoomId;
use slog::Logger;

use api::MatrixApi;
use config::Config;
use errors::*;
use handlers::matrix::{MembershipHandler, MessageHandler};
use handlers::ErrorNotifier;
use log;
use models::Room;

/// Dispatches events to the corresponding handler.
pub struct Dispatcher<'a> {
    config: &'a Config,
    connection: &'a SqliteConnection,
    logger: &'a Logger,
    matrix_api: Box<dyn MatrixApi>,
}

impl<'a> Dispatcher<'a> {
    /// Create a new `Dispatcher` with an SQLite connection
    pub fn new(
        config: &'a Config,
        connection: &'a SqliteConnection,
        logger: &'a Logger,
        matrix_api: Box<dyn MatrixApi>,
    ) -> Dispatcher<'a> {
        Dispatcher { config, connection, logger, matrix_api }
    }

    /// Processes the events that are passed to the method by forwarding them to the
    /// corresponding handler.
    pub fn process(&self, events: Vec<Box<Event>>) -> Result<()> {
        for event in events {
            match *event {
                Event::RoomMember(member_event) => {
                    let room_id = match &member_event.room_id {
                        Some(room_id) => room_id,
                        None => {
                            debug!(self.logger, "Skipping event, no room is specified");
                            return Ok(());
                        }
                    };
                    let room = Room::new(self.config, self.logger, self.matrix_api.as_ref(), room_id.clone());
                    let handler =
                        MembershipHandler::new(self.config, self.connection, self.logger, self.matrix_api.as_ref(), &room);
                    if let Err(err) = handler.process(&member_event) {
                        return self.handle_error(&err, room_id);
                    }
                }
                Event::RoomMessage(message_event) => {
                    let room_id = match &message_event.room_id {
                        Some(room_id) => room_id,
                        None => {
                            debug!(self.logger, "Skipping event, no room is specified");
                            return Ok(());
                        }
                    };
                    let handler = MessageHandler::new(self.config, self.connection, self.logger, self.matrix_api.clone());
                    if let Err(err) = handler.process(&message_event) {
                        return self.handle_error(&err, room_id);
                    }
                }
                _ => debug!(self.logger, "Skipping event, because the event type is not known"),
            }
        }
        Ok(())
    }

    /// Forward the error to the notifier to send the corresponding message to the user
    /// The error message can only the sent to the user if the bot user has joined the channel.
    /// If the error cannot be sent to the user or the error doesn't contain a readable user
    /// message it is returned to the caller so that the caller can take care of the error.
    pub fn handle_error(&self, err: &Error, room_id: &RoomId) -> Result<()> {
        debug!(self.logger, "Sending error message to room {}", &room_id);

        let error_notifier = ErrorNotifier { config: self.config, logger: self.logger, matrix_api: self.matrix_api.as_ref() };

        if let Err(send_err) = error_notifier.send_message_to_user(err, room_id.clone()) {
            warn!(self.logger, "Unable to send an error message to the user in room {}", room_id);
            log::log_debug(self.logger, err);
            return Err(send_err);
        }

        Ok(())
    }
}
