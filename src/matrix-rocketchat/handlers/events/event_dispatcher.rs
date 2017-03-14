use diesel::sqlite::SqliteConnection;
use ruma_events::collections::all::Event;
use ruma_identifiers::{RoomId, UserId};
use slog::Logger;

use api::MatrixApi;
use config::Config;
use db::User;
use errors::*;
use super::room_handler::RoomHandler;
use super::command_handler::CommandHandler;
use i18n::*;

/// Dispatches events to the corresponding handler.
pub struct EventDispatcher<'a> {
    config: &'a Config,
    connection: &'a SqliteConnection,
    logger: Logger,
    matrix_api: Box<MatrixApi>,
}

impl<'a> EventDispatcher<'a> {
    /// Create a new `EventDispatcher` with an SQLite connection
    pub fn new(config: &'a Config,
               connection: &'a SqliteConnection,
               logger: Logger,
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
                    if let Err(err) = CommandHandler::new(self.config,
                                                          self.connection,
                                                          self.logger.clone(),
                                                          self.matrix_api.clone())
                        .process(&message_event) {
                        return self.handle_error(err, message_event.room_id, &message_event.user_id);
                    }
                }
                _ => debug!(self.logger, "Skipping event, because the event type is not known"),
            }
        }
        Ok(())
    }

    fn handle_error(&self, err: Error, room_id: RoomId, user_id: &UserId) -> Result<()> {
        let matrix_bot_id = self.config.matrix_bot_user_id()?;
        let language = match User::find_by_matrix_user_id(self.connection, user_id)? {
            Some(user) => user.language,
            None => DEFAULT_LANGUAGE.to_string(),
        };

        let user_message = match err.user_message {
            Some(ref user_message) => user_message,
            None => {
                let user_msg = t!(["defaults", "internal_error"]).l(&language);
                self.matrix_api.send_text_message_event(room_id, matrix_bot_id, user_msg)?;
                return Err(err);
            }
        };

        let mut msg = format!("{}", &err);
        for err in err.error_chain.iter().skip(1) {
            msg = msg + " caused by: " + &format!("{}", err);
        }

        debug!(self.logger, msg);
        self.matrix_api.send_text_message_event(room_id, matrix_bot_id, user_message.l(&language))
    }
}
