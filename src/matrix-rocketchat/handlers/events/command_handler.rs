use std::collections::HashMap;

use diesel::Connection;
use diesel::sqlite::SqliteConnection;
use ruma_events::room::message::MessageEvent;
use ruma_events::room::message::MessageEventContent;
use slog::Logger;

use api::{MatrixApi, RocketchatApi};
use config::Config;
use db::{NewRocketchatServer, NewUserOnRocketchatServer, RocketchatServer, Room, User, UserOnRocketchatServer};
use errors::*;
use i18n::*;

/// Handles command messages from the admin room
pub struct CommandHandler<'a> {
    config: &'a Config,
    connection: &'a SqliteConnection,
    logger: Logger,
    matrix_api: Box<MatrixApi>,
}

impl<'a> CommandHandler<'a> {
    /// Create a new `CommandHandler`.
    pub fn new(config: &'a Config,
               connection: &'a SqliteConnection,
               logger: Logger,
               matrix_api: Box<MatrixApi>)
               -> CommandHandler<'a> {
        CommandHandler {
            config: config,
            connection: connection,
            logger: logger,
            matrix_api: matrix_api,
        }
    }

    /// Handles command messages from an admin room
    pub fn process(&self, event: &MessageEvent) -> Result<()> {
        let message = match event.content {
            MessageEventContent::Text(ref text_content) => text_content.body.clone(),
            _ => {
                debug!(self.logger, "Unknown event content type, skipping");
                return Ok(());
            }
        };

        if message.starts_with("connect") {
            let msg = format!("Received connect command: {}", message);
            debug!(self.logger, msg);

            self.handle_connect(event, &message)?;
        } else {
            let msg = format!("Skipping event, don't know how to handle command `{}`", message);
            debug!(self.logger, msg);
        }

        Ok(())
    }

    fn handle_connect(&self, event: &MessageEvent, message: &str) -> Result<()> {
        self.connection
            .transaction(|| {
                let room = Room::find(self.connection, &event.room_id)?;
                if room.is_connected() {
                    bail!(ErrorKind::RoomAlreadyConnected(event.room_id.to_string()));
                }

                let mut command = message.split_whitespace().collect::<Vec<&str>>().into_iter();
                let rocketchat_url = command.by_ref().nth(1).unwrap_or_default();

                debug!(self.logger, "Attempting to connect to Rocket.Chat server {}", rocketchat_url);

                let rocketchat_server = match command.by_ref().next() {
                    Some(token) => self.connect_new_rocktechat_server(rocketchat_url.to_string(), token.to_string())?,
                    None => self.get_existing_rocketchat_server(rocketchat_url.to_string())?,
                };

                room.set_rocketchat_server_id(self.connection, rocketchat_server.id)?;

                let new_user_on_rocketchat_server = NewUserOnRocketchatServer {
                    matrix_user_id: event.user_id.clone(),
                    rocketchat_server_id: rocketchat_server.id,
                    rocketchat_auth_token: None,
                };

                UserOnRocketchatServer::insert(self.connection, &new_user_on_rocketchat_server)?;

                let user = User::find(self.connection, &event.user_id)?;
                let mut vars = HashMap::new();
                vars.insert("rocketchat_url", rocketchat_url.to_string());
                let body = t!(["admin_command", "successfully_connected"]).l(&user.language, Some(vars));
                self.matrix_api.send_text_message_event(event.room_id.clone(), self.config.matrix_bot_user_id()?, body)
            })
            .map_err(Error::from)
    }

    fn connect_new_rocktechat_server(&self, rocketchat_url: String, token: String) -> Result<RocketchatServer> {
        if let Some(rocketchat_server) = RocketchatServer::find_by_url(self.connection, rocketchat_url.clone())? {
            if rocketchat_server.rocketchat_token.is_some() {
                bail!(ErrorKind::RocketchatServerAlreadyConnected(rocketchat_url));
            }
        }

        if RocketchatServer::find_by_token(self.connection, token.clone())?.is_some() {
            bail!(ErrorKind::RocketchatTokenAlreadyInUse(token));
        }

        // see if we can reach the server and if the server has a supported API version
        RocketchatApi::new(rocketchat_url.clone(), Some(token.clone()), self.logger.clone())?;

        let new_rocketchat_server = NewRocketchatServer {
            rocketchat_url: rocketchat_url,
            rocketchat_token: Some(token),
        };

        RocketchatServer::insert(self.connection, &new_rocketchat_server)
    }

    fn get_existing_rocketchat_server(&self, rocketchat_url: String) -> Result<RocketchatServer> {
        let rocketchat_server: RocketchatServer = match RocketchatServer::find_by_url(self.connection, rocketchat_url)? {
            Some(rocketchat_server) => rocketchat_server,
            None => {
                bail!(ErrorKind::RocketchatTokenMissing);
            }
        };

        Ok(rocketchat_server)
    }
}
