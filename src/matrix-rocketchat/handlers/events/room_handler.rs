use diesel::sqlite::SqliteConnection;
use ruma_events::room::member::MemberEvent;
use slog::Logger;

use errors::*;

/// Handles room events
pub struct RoomHandler<'a> {
    connection: &'a SqliteConnection,
    logger: Logger,
}

impl<'a> RoomHandler<'a> {
    /// Create a new `RoomHandler` with an SQLite connection
    pub fn new(connection: &SqliteConnection, logger: Logger) -> RoomHandler {
        RoomHandler {
            connection: connection,
            logger: logger,
        }

    }

    /// Handles room membership changes
    pub fn process(&self, event: &MemberEvent) -> Result<()> {
        Ok(())
    }
}
