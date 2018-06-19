use diesel::sqlite::SqliteConnection;
use reqwest::mime::{self, Mime};
use ruma_events::room::message::{MessageEvent, MessageEventContent};
use slog::Logger;
use url::Url;

use api::{MatrixApi, RocketchatApi};
use errors::*;
use i18n::*;
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
            connection,
            logger,
            matrix_api,
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

        let rocketchat_api = RocketchatApi::new(server.rocketchat_url, self.logger.clone())?.with_credentials(
            user_on_rocketchat_server.rocketchat_user_id.clone().unwrap_or_default(),
            user_on_rocketchat_server.rocketchat_auth_token.clone().unwrap_or_default(),
        );

        match event.content {
            MessageEventContent::Text(ref content) => {
                rocketchat_api.chat_post_message(&content.body, channel_id)?;
            }
            MessageEventContent::Image(ref content) => {
                let mimetype = content.clone().info.chain_err(|| ErrorKind::MissingMimeType)?.mimetype;
                self.forward_file_to_rocketchat(rocketchat_api.as_ref(), &content.url, mimetype, &content.body, channel_id)?;
            }
            MessageEventContent::File(ref content) => {
                let mimetype = content.clone().info.chain_err(|| ErrorKind::MissingMimeType)?.mimetype;
                self.forward_file_to_rocketchat(rocketchat_api.as_ref(), &content.url, mimetype, &content.body, channel_id)?;
            }
            MessageEventContent::Audio(ref content) => {
                let mimetype = content.clone().info.chain_err(|| ErrorKind::MissingMimeType)?.mimetype;
                self.forward_file_to_rocketchat(rocketchat_api.as_ref(), &content.url, mimetype, &content.body, channel_id)?;
            }
            MessageEventContent::Video(ref content) => {
                let mimetype = content.clone().info.chain_err(|| ErrorKind::MissingMimeType)?.mimetype;
                self.forward_file_to_rocketchat(rocketchat_api.as_ref(), &content.url, mimetype, &content.body, channel_id)?;
            }
            MessageEventContent::Emote(_) | MessageEventContent::Location(_) | MessageEventContent::Notice(_) => {
                info!(self.logger, "Not forwarding message, forwarding emote, location or notice messages is not implemented.")
            }
        }

        user_on_rocketchat_server.set_last_message_sent(self.connection)
    }

    fn forward_file_to_rocketchat(
        &self,
        rocketchat_api: &RocketchatApi,
        url: &str,
        mimetype: Option<String>,
        body: &str,
        channel_id: &str,
    ) -> Result<()> {
        let url = Url::parse(url).chain_err(|| ErrorKind::InternalServerError)?;
        let host = url.host_str().unwrap_or_default();
        let file_id = url.path().trim_left_matches('/');
        let file = self.matrix_api.get_content(host.to_string(), file_id.to_string())?;

        let mime: Mime = self.parse_mimetype(mimetype)?;

        if let Err(err) = rocketchat_api.rooms_upload(file, body, mime, channel_id) {
            bail_error!(
                ErrorKind::RocketchatUploadFailed(url.to_string(), err.to_string()),
                t!(["errors", "rocketchat_server_upload_failed"])
                    .with_vars(vec![("url", url.to_string()), ("err", err.to_string())])
            );
        }

        Ok(())
    }

    fn parse_mimetype(&self, mimetype: Option<String>) -> Result<Mime> {
        let mime = mimetype
            .clone()
            .unwrap_or_else(|| mime::APPLICATION_OCTET_STREAM.to_string())
            .parse()
            .chain_err(|| ErrorKind::UnknownMimeType(mimetype.unwrap_or_default()))?;

        Ok(mime)
    }
}
