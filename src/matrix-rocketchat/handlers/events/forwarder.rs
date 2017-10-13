use diesel::sqlite::SqliteConnection;
use ruma_events::room::message::{MessageEvent, MessageEventContent};
use ruma_identifiers::RoomId;
use slog::Logger;

use api::{MatrixApi, RocketchatApi};
use db::{Room, UserOnRocketchatServer};
use errors::*;

/// Forwards messages
pub struct Forwarder<'a> {
    connection: &'a SqliteConnection,
    logger: &'a Logger,
    matrix_api: &'a MatrixApi,
}

impl<'a> Forwarder<'a> {
    /// Create a new `Forwarder`.
    pub fn new(connection: &'a SqliteConnection, logger: &'a Logger, matrix_api: &'a MatrixApi) -> Forwarder<'a> {
        Forwarder {
            connection: connection,
            logger: logger,
            matrix_api: matrix_api,
        }
    }

    /// Forwards messages to Rocket.Chat
    pub fn process(&self, event: &MessageEvent, matrix_room_id: RoomId, rocketchat_channel_id: String) -> Result<()> {
        match Room::rocketchat_server(self.connection, self.matrix_api, matrix_room_id.clone())? {
            Some(rocketchat_server) => {
                let mut user_on_rocketchat_server =
                    UserOnRocketchatServer::find(self.connection, &event.user_id, rocketchat_server.id)?;

                if user_on_rocketchat_server.is_virtual_user {
                    debug!(self.logger, "Skipping event, because it was sent by a virtual user");
                    return Ok(());
                }

                match event.content {
                    MessageEventContent::Text(ref text_content) => {
                        let rocketchat_api = RocketchatApi::new(rocketchat_server.rocketchat_url, self.logger.clone())?
                            .with_credentials(
                                user_on_rocketchat_server.rocketchat_user_id.clone().unwrap_or_default(),
                                user_on_rocketchat_server.rocketchat_auth_token.clone().unwrap_or_default(),
                            );
                        rocketchat_api.post_chat_message(&text_content.body, &rocketchat_channel_id)?;
                    }
                    _ => info!(self.logger, "Forwarding the type {} is not implemented.", event.event_type),
                }

                user_on_rocketchat_server.set_last_message_sent(self.connection)?;
            }

            None => debug!(self.logger, "Skipping event, because the room is not bridged"),
        }

        Ok(())
    }
}
