use diesel::sqlite::SqliteConnection;
use ruma_events::collections::all::Event;
use slog::Logger;

use api::MatrixApi;
use config::Config;
use errors::*;
use super::room_handler::RoomHandler;

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
                    RoomHandler::new(self.config, self.connection, self.logger.clone(), self.matrix_api.clone()).process(&member_event)?;
                }
                _ => {
                    debug!(self.logger, "Skipping event, because the event type is not known");
                }
            }
        }

        Ok(())
    }
}
