use diesel::sqlite::SqliteConnection;
use ruma_events::room::message::{MessageEvent, MessageEventContent};
use slog::Logger;

use api::RocketchatApi;
use errors::*;
use models::{RocketchatServer, UserOnRocketchatServer};

/// Forwards messages
pub struct Forwarder<'a> {
    connection: &'a SqliteConnection,
    logger: &'a Logger,
}

impl<'a> Forwarder<'a> {
    /// Create a new `Forwarder`.
    pub fn new(connection: &'a SqliteConnection, logger: &'a Logger) -> Forwarder<'a> {
        Forwarder {
            connection: connection,
            logger: logger,
        }
    }

    /// Forwards messages to Rocket.Chat
    pub fn process(&self, event: &MessageEvent, server: RocketchatServer, channel_id: &str) -> Result<()> {
        let mut user_on_rocketchat_server =
            match UserOnRocketchatServer::find_by_matrix_user_id(self.connection, &event.user_id, server.id)? {
                Some(user_on_rocketchat_server) => user_on_rocketchat_server,
                None => {
                    debug!(self.logger, "Skipping event, because it was sent by a virtual user");
                    return Ok(());
                }
            };

        match event.content {
            MessageEventContent::Text(ref text_content) => {
                let rocketchat_api = RocketchatApi::new(server.rocketchat_url, self.logger.clone())?.with_credentials(
                    user_on_rocketchat_server.rocketchat_user_id.clone().unwrap_or_default(),
                    user_on_rocketchat_server.rocketchat_auth_token.clone().unwrap_or_default(),
                );
                rocketchat_api.post_chat_message(&text_content.body, channel_id)?;
            }
            _ => info!(self.logger, "Forwarding the type {} is not implemented.", event.event_type),
        }

        user_on_rocketchat_server.set_last_message_sent(self.connection)
    }
}
