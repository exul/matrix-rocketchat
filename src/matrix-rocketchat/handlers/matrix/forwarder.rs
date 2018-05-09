use diesel::sqlite::SqliteConnection;
use reqwest::mime::Mime;
use ruma_events::room::message::{MessageEvent, MessageEventContent};
use slog::Logger;
use url::Url;

use api::{MatrixApi, RocketchatApi};
use errors::*;
use models::{RocketchatServer, UserOnRocketchatServer};

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
                rocketchat_api.chat_post_message(&text_content.body, channel_id)?;
            }
            MessageEventContent::Image(ref image_content) => {
                let url = Url::parse(&image_content.url).chain_err(|| ErrorKind::InternalServerError)?;
                let host = url.host_str().unwrap_or_default();
                let image_id = url.path().trim_left_matches('/');
                let image = self.matrix_api.get_content(host.to_string(), image_id.to_string())?;

                let rocketchat_api = RocketchatApi::new(server.rocketchat_url, self.logger.clone())?.with_credentials(
                    user_on_rocketchat_server.rocketchat_user_id.clone().unwrap_or_default(),
                    user_on_rocketchat_server.rocketchat_auth_token.clone().unwrap_or_default(),
                );

                let info = image_content.clone().info.chain_err(|| ErrorKind::MissingMimeType)?;
                let mime_type: Mime = info.mimetype.parse().chain_err(|| ErrorKind::UnknownMimeType(info.mimetype.clone()))?;
                rocketchat_api.rooms_upload(image, &image_content.body, mime_type, channel_id)?;
            }
            _ => info!(self.logger, "Forwarding the type {} is not implemented.", event.event_type),
        }

        user_on_rocketchat_server.set_last_message_sent(self.connection)
    }
}
