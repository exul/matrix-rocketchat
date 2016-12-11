use diesel::sqlite::SqliteConnection;
use ruma_events::room::member::{MemberEvent, MembershipState};
use slog::Logger;

use config::Config;
use errors::*;

/// Handles room events
pub struct RoomHandler<'a> {
    config: &'a Config,
    connection: &'a SqliteConnection,
    logger: Logger,
}

impl<'a> RoomHandler<'a> {
    /// Create a new `RoomHandler` with an SQLite connection
    pub fn new(config: &'a Config, connection: &'a SqliteConnection, logger: Logger) -> RoomHandler<'a> {
        RoomHandler {
            config: config,
            connection: connection,
            logger: logger,
        }

    }

    /// Handles room membership changes
    pub fn process(&self, event: &MemberEvent) -> Result<()> {
        let matrix_bot_user_id = format!("{}", &self.config.matrix_bot_user_id()?);
        let addressed_to_matrix_bot = &event.state_key == &matrix_bot_user_id;

        match event.content.membership {
            MembershipState::Invite if addressed_to_matrix_bot => {
                debug!(self.logger,
                       format!("Creating admin room for user `{}` with bot user `{}`",
                               event.user_id,
                               matrix_bot_user_id));
            }
            MembershipState::Join if addressed_to_matrix_bot => {
                debug!(self.logger,
                       format!("Bot user `{}` is joining room `{}`", matrix_bot_user_id, event.room_id));
            }
            _ => {
                info!(self.logger,
                      format!("Skipping event, don't know how to handle membership state `{}` with state key `{}`",
                              event.content.membership,
                              event.state_key));
            }
        }
        Ok(())
    }
}
