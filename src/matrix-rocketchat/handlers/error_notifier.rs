use regex::{Captures, Regex};
use ruma_identifiers::RoomId;
use slog::Logger;

use api::MatrixApi;
use config::Config;
use errors::*;
use i18n::*;

lazy_static! {
    static ref MATRIX_ID_REGEX: Regex = Regex::new("@([0-9A-Za-z.-_]+):([0-9A-Za-z.-]+)").expect("compiling regex failed");
}

/// Notifies the user about errors
pub struct ErrorNotifier<'a> {
    /// Application service configuration
    pub config: &'a Config,
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
                if self.config.log_level != "debug" {
                    msg = obfuscate_mxid(&msg);
                }
                info!(self.logger, "{}", msg);
                user_message
            }
            None => {
                if self.config.log_level != "debug" {
                    msg = obfuscate_mxid(&msg);
                }
                error!(self.logger, "{}", msg);
                let user_msg = t!(["defaults", "internal_error"]).l(DEFAULT_LANGUAGE);
                return self.matrix_api.send_text_message(room_id, matrix_bot_id, user_msg);
            }
        };

        self.matrix_api.send_text_message(room_id, matrix_bot_id, user_message.l(DEFAULT_LANGUAGE))
    }
}

fn obfuscate_mxid(msg: &str) -> String {
    MATRIX_ID_REGEX
        .replace_all(&msg, |caps: &Captures| {
            let a: String = caps[1].chars().map(|_| '*').collect();
            let b: String = caps[2].chars().map(|_| '*').collect();
            format!("@{}:{}", a, b)
        })
        .into_owned()
}
