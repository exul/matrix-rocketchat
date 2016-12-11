use diesel::sqlite::SqliteConnection;
use ruma_events::collections::all::Event;
use slog::Logger;

use errors::*;
use super::room_handler::RoomHandler;

/// Dispatches events to the corresponding handler.
pub struct EventDispatcher<'a> {
    connection: &'a SqliteConnection,
    logger: Logger,
}

impl<'a> EventDispatcher<'a> {
    /// Create a new `EventDispatcher` with an SQLite connection
    pub fn new(connection: &'a SqliteConnection, logger: Logger) -> EventDispatcher {
        EventDispatcher {
            connection: connection,
            logger: logger,
        }
    }

    /// Processes the events that are passed to the method by forwarding them to the
    /// corresponding handler.
    pub fn process(&self, events: Vec<Box<Event>>) -> Result<()> {
        for event in events {
            match *event {
                Event::RoomMember(member_event) => {
                    RoomHandler::new(self.connection, self.logger.clone()).process(&member_event)?;
                }
                _ => {
                    debug!(self.logger, "Skipping event, because the event type is not known");
                }
            }
        }

        Ok(())
    }
}
