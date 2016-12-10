use diesel::sqlite::SqliteConnection;

use errors::*;
use models::Events;

/// Dispatches events to the corresponding handler.
pub struct EventDispatcher<'a> {
    connection: &'a SqliteConnection,
}

impl<'a> EventDispatcher<'a> {
    /// Create a new `EventDispatcher` with an Sqlite Connection
    pub fn new(connection: &'a SqliteConnection) -> EventDispatcher {
        EventDispatcher { connection: connection }
    }

    /// Processes the events that are passed to the method by forwarding them to the
    /// corresponding handler.
    pub fn process(&self, events: Events) -> Result<()> {
        Ok(())
    }
}
