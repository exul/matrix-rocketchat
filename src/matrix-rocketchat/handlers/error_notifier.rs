use diesel::sqlite::SqliteConnection;
use ruma_identifiers::RoomId;
use slog::Logger;

use i18n::*;
use api::MatrixApi;
use config::Config;
use errors::*;

/// Notifies the user about errors
pub struct ErrorNotifier<'a> {
    /// Application service configuration
    pub config: &'a Config,
    /// SQL database connection
    pub connection: &'a SqliteConnection,
    /// Logger context
    pub logger: &'a Logger,
    /// Matrix REST API
    pub matrix_api: &'a MatrixApi,
}

impl<'a> ErrorNotifier<'a> {
    /// Send the error message to the user if the error contains a user message. Otherwise just
    /// inform the user that an internal error happened.
    pub fn send_message_to_user(&self, err: &Error, room_id: RoomId) -> Result<()> {
        let mut msg = format!("{}", err);
        for err in err.error_chain.iter().skip(1) {
            msg = msg + " caused by: " + &format!("{}", err);
        }

        let matrix_bot_id = self.config.matrix_bot_user_id()?;
        let user_message = match err.user_message {
            Some(ref user_message) => {
                info!(self.logger, "{}", msg);
                user_message
            }
            None => {
                error!(self.logger, "{}", msg);
                let user_msg = t!(["defaults", "internal_error"]).l(DEFAULT_LANGUAGE);
                return self.matrix_api.send_text_message_event(room_id, matrix_bot_id, user_msg);
            }
        };

        self.matrix_api.send_text_message_event(room_id, matrix_bot_id, user_message.l(DEFAULT_LANGUAGE))
    }
}
