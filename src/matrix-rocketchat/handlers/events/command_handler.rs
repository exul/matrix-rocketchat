use diesel::Connection;
use diesel::sqlite::SqliteConnection;
use ruma_events::room::message::MessageEvent;
use ruma_events::room::message::MessageEventContent;
use ruma_identifiers::UserId;
use slog::Logger;

use MAX_ROCKETCHAT_SERVER_ID_LENGTH;
use api::{MatrixApi, RocketchatApi};
use config::Config;
use errors::*;
use handlers::rocketchat::{Credentials, Login};
use handlers::events::RoomHandler;
use i18n::*;
use models::{NewRocketchatServer, NewUserOnRocketchatServer, RocketchatServer, Room, UserOnRocketchatServer};

/// Handles command messages from the admin room
pub struct CommandHandler<'a> {
    config: &'a Config,
    connection: &'a SqliteConnection,
    logger: &'a Logger,
    matrix_api: &'a MatrixApi,
    admin_room: &'a Room<'a>,
}

impl<'a> CommandHandler<'a> {
    /// Create a new `CommandHandler`.
    pub fn new(
        config: &'a Config,
        connection: &'a SqliteConnection,
        logger: &'a Logger,
        matrix_api: &'a MatrixApi,
        admin_room: &'a Room<'a>,
    ) -> CommandHandler<'a> {
        CommandHandler {
            config: config,
            connection: connection,
            logger: logger,
            matrix_api: matrix_api,
            admin_room: admin_room,
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
            debug!(self.logger, "Received connect command: {}", message);

            self.connect(event, &message)?;
        } else if message == "help" {
            debug!(self.logger, "Received help command");

            self.help(event)?;
        } else if message.starts_with("login") {
            debug!(self.logger, "Received login command");

            let server = self.get_rocketchat_server()?;
            self.login(event, &server, &message)?;
        } else if message == "list" {
            debug!(self.logger, "Received list command");

            let server = self.get_rocketchat_server()?;
            self.list_channels(event, &server)?;
        } else if message.starts_with("bridge") {
            debug!(self.logger, "Received bridge command");

            let server = self.get_rocketchat_server()?;
            self.bridge(event, &server, &message)?;
        } else if message.starts_with("unbridge") {
            debug!(self.logger, "Received unbridge command");

            let server = self.get_rocketchat_server()?;
            self.unbridge(event, &server, &message)?;
        } else {
            debug!(self.logger, "Skipping event, don't know how to handle command `{}`", message);
        }

        Ok(())
    }

    fn connect(&self, event: &MessageEvent, message: &str) -> Result<()> {
        self.connection
            .transaction(|| {
                if self.admin_room.is_connected(self.connection)? {
                    bail_error!(
                        ErrorKind::RoomAlreadyConnected(self.admin_room.id.to_string()),
                        t!(["errors", "room_already_connected"])
                    );
                }

                let mut command = message.split_whitespace().collect::<Vec<&str>>().into_iter();
                let rocketchat_url = command.by_ref().nth(1).unwrap_or_default();

                debug!(self.logger, "Connecting to Rocket.Chat server {}", rocketchat_url);

                let server = match command.by_ref().next() {
                    Some(token) => {
                        let rocketchat_id = command.by_ref().next().unwrap_or_default();
                        self.connect_new_rocketchat_server(rocketchat_id, rocketchat_url, token, &event.user_id)?
                    }
                    None => self.get_existing_rocketchat_server(rocketchat_url.to_string())?,
                };

                let new_user_on_rocketchat_server = NewUserOnRocketchatServer {
                    matrix_user_id: event.user_id.clone(),
                    rocketchat_server_id: server.id,
                    rocketchat_user_id: None,
                    rocketchat_auth_token: None,
                };

                UserOnRocketchatServer::upsert(self.connection, &new_user_on_rocketchat_server)?;
                self.matrix_api.set_room_topic(self.admin_room.id.clone(), rocketchat_url.to_string())?;

                let body = CommandHandler::build_help_message(
                    self.connection,
                    self.admin_room,
                    self.config.as_url.clone(),
                    &event.user_id,
                )?;
                self.matrix_api.send_text_message_event(self.admin_room.id.clone(), self.config.matrix_bot_user_id()?, body)?;

                Ok(info!(
                    self.logger,
                    "Successfully executed connect command for user {} and Rocket.Chat server {}",
                    event.user_id,
                    rocketchat_url
                ))
            })
            .map_err(Error::from)
    }

    fn connect_new_rocketchat_server(
        &self,
        rocketchat_server_id: &str,
        rocketchat_url: &str,
        token: &str,
        user_id: &UserId,
    ) -> Result<RocketchatServer> {
        if rocketchat_server_id.is_empty() {
            bail_error!(ErrorKind::ConnectWithoutRocketchatServerId, t!(["errors", "connect_without_rocketchat_server_id"]));
        } else if rocketchat_server_id.len() > MAX_ROCKETCHAT_SERVER_ID_LENGTH
            || rocketchat_server_id.chars().any(|c| c.is_uppercase() || (!c.is_digit(36)))
        {
            bail_error!(
                ErrorKind::ConnectWithInvalidRocketchatServerId(rocketchat_server_id.to_owned()),
                t!(["errors", "connect_with_invalid_rocketchat_server_id"]).with_vars(vec![
                    ("rocketchat_server_id", rocketchat_server_id.to_owned()),
                    ("max_rocketchat_server_id_length", MAX_ROCKETCHAT_SERVER_ID_LENGTH.to_string()),
                ])
            );
        } else if RocketchatServer::find_by_id(self.connection, rocketchat_server_id)?.is_some() {
            bail_error!(
                ErrorKind::RocketchatServerIdAlreadyInUse(rocketchat_server_id.to_owned()),
                t!(["errors", "rocketchat_server_id_already_in_use"])
                    .with_vars(vec![("rocketchat_server_id", rocketchat_server_id.to_owned())])
            );
        }

        if let Some(server) = RocketchatServer::find_by_url(self.connection, rocketchat_url)? {
            if server.rocketchat_token.is_some() {
                bail_error!(
                    ErrorKind::RocketchatServerAlreadyConnected(rocketchat_url.to_owned()),
                    t!(["errors", "rocketchat_server_already_connected"]).with_vars(vec![
                        ("rocketchat_url", rocketchat_url.to_owned()),
                        ("user_id", user_id.to_string()),
                    ])
                );
            }
        }

        if RocketchatServer::find_by_token(self.connection, token)?.is_some() {
            bail_error!(
                ErrorKind::RocketchatTokenAlreadyInUse(token.to_owned()),
                t!(["errors", "token_already_in_use"]).with_vars(vec![("token", token.to_owned())])
            );
        }

        // see if we can reach the server and if the server has a supported API version
        RocketchatApi::new(rocketchat_url.to_owned(), self.logger.clone())?;

        let new_rocketchat_server = NewRocketchatServer {
            id: rocketchat_server_id,
            rocketchat_url: rocketchat_url,
            rocketchat_token: Some(token),
        };

        RocketchatServer::insert(self.connection, &new_rocketchat_server)
    }

    fn help(&self, event: &MessageEvent) -> Result<()> {
        let help_message =
            CommandHandler::build_help_message(self.connection, self.admin_room, self.config.as_url.clone(), &event.user_id)?;
        let bot_user_id = self.config.matrix_bot_user_id()?;
        self.matrix_api.send_text_message_event(self.admin_room.id.clone(), bot_user_id, help_message)?;

        Ok(info!(self.logger, "Successfully executed help command for user {}", event.user_id))
    }

    fn login(&self, event: &MessageEvent, server: &RocketchatServer, message: &str) -> Result<()> {
        let mut command = message.split_whitespace().collect::<Vec<&str>>().into_iter();
        let username = command.by_ref().nth(1).unwrap_or_default();
        let password = command.by_ref().fold("".to_string(), |acc, x| acc + x);

        let credentials = Credentials {
            user_id: event.user_id.clone(),
            rocketchat_username: username.to_string(),
            password: password.to_string(),
            rocketchat_url: server.rocketchat_url.clone(),
        };
        let login = Login {
            config: self.config,
            connection: self.connection,
            logger: self.logger,
            matrix_api: self.matrix_api,
        };
        login.call(&credentials, server, Some(self.admin_room.id.clone()))
    }

    fn list_channels(&self, event: &MessageEvent, server: &RocketchatServer) -> Result<()> {
        let user_on_rocketchat_server = UserOnRocketchatServer::find(self.connection, &event.user_id, server.id.clone())?;
        let rocketchat_api = RocketchatApi::new(server.rocketchat_url.clone(), self.logger.clone())?.with_credentials(
            user_on_rocketchat_server.rocketchat_user_id.unwrap_or_default(),
            user_on_rocketchat_server.rocketchat_auth_token.unwrap_or_default(),
        );
        let bot_user_id = self.config.matrix_bot_user_id()?;
        let channels_list = self.build_channels_list(rocketchat_api.as_ref(), &server.id, &event.user_id)?;
        let message = t!(["admin_room", "list_channels"]).with_vars(vec![("channel_list", channels_list)]);
        self.matrix_api.send_text_message_event(event.room_id.clone(), bot_user_id, message.l(DEFAULT_LANGUAGE))?;

        Ok(info!(self.logger, "Successfully listed channels for Rocket.Chat server {}", &server.rocketchat_url))
    }

    fn bridge(&self, event: &MessageEvent, server: &RocketchatServer, message: &str) -> Result<()> {
        let bot_user_id = self.config.matrix_bot_user_id()?;
        let user_on_rocketchat_server = UserOnRocketchatServer::find(self.connection, &event.user_id, server.id.clone())?;
        let rocketchat_api = RocketchatApi::new(server.rocketchat_url.clone(), self.logger.clone())?.with_credentials(
            user_on_rocketchat_server.rocketchat_user_id.clone().unwrap_or_default(),
            user_on_rocketchat_server.rocketchat_auth_token.clone().unwrap_or_default(),
        );

        let channels = rocketchat_api.channels_list()?;

        let mut command = message.split_whitespace().collect::<Vec<&str>>().into_iter();
        let channel_name = command.by_ref().nth(1).unwrap_or_default();

        let channel = match channels.iter().find(|channel| channel.name.clone().unwrap_or_default() == channel_name) {
            Some(channel) => channel,
            None => {
                bail_error!(
                    ErrorKind::RocketchatChannelNotFound(channel_name.to_string()),
                    t!(["errors", "rocketchat_channel_not_found"]).with_vars(vec![("channel_name", channel_name.to_string())])
                );
            }
        };

        let username = rocketchat_api.current_username()?;
        if !channel.usernames.iter().any(|u| u == &username) {
            bail_error!(
                ErrorKind::RocketchatJoinFirst(channel_name.to_string()),
                t!(["errors", "rocketchat_join_first"]).with_vars(vec![("channel_name", channel_name.to_string())])
            );
        }

        let room_handler =
            RoomHandler::new(self.config, self.connection, self.logger, self.matrix_api, &bot_user_id, &event.user_id);
        let room_id = match Room::matrix_id_from_rocketchat_channel_id(self.config, self.matrix_api, &server.id, &channel.id)? {
            Some(room_id) => {
                let room = Room::new(self.config, self.logger, self.matrix_api, room_id.clone());
                room_handler.bridge_existing_room(room, event.user_id.clone(), channel_name.to_string())?;
                room_id
            }
            None => room_handler.bridge_new_room(rocketchat_api, server, channel)?,
        };

        let matrix_room_alias_id = Room::build_room_alias_id(self.config, &server.id, &channel.id)?;
        self.matrix_api.put_canonical_room_alias(room_id.clone(), Some(matrix_room_alias_id))?;

        let message = t!(["admin_room", "room_successfully_bridged"]).with_vars(vec![
            ("channel_name", channel.name.clone().unwrap_or_else(|| channel.id.clone())),
        ]);
        self.matrix_api.send_text_message_event(event.room_id.clone(), bot_user_id.clone(), message.l(DEFAULT_LANGUAGE))?;

        Ok(info!(self.logger, "Successfully bridged room {} to {}", &channel.id, &room_id))
    }

    fn unbridge(&self, event: &MessageEvent, server: &RocketchatServer, message: &str) -> Result<()> {
        let mut command = message.split_whitespace().collect::<Vec<&str>>().into_iter();
        let channel_name = command.nth(1).unwrap_or_default().to_string();

        let user_on_rocketchat_server = UserOnRocketchatServer::find(self.connection, &event.user_id, server.id.clone())?;
        let rocketchat_api = RocketchatApi::new(server.rocketchat_url.clone(), self.logger.clone())?.with_credentials(
            user_on_rocketchat_server.rocketchat_user_id.clone().unwrap_or_default(),
            user_on_rocketchat_server.rocketchat_auth_token.clone().unwrap_or_default(),
        );

        let room_id = match Room::matrix_id_from_rocketchat_channel_name(
            self.config,
            self.matrix_api,
            rocketchat_api.as_ref(),
            &server.id,
            channel_name.clone(),
        )? {
            Some(room_id) => room_id,
            None => {
                bail_error!(
                    ErrorKind::UnbridgeOfNotBridgedRoom(channel_name.to_string()),
                    t!(["errors", "unbridge_of_not_bridged_room"]).with_vars(vec![("channel_name", channel_name)])
                );
            }
        };

        let room = Room::new(self.config, self.logger, self.matrix_api, room_id.clone());
        let virtual_user_prefix = format!("@{}", self.config.sender_localpart);
        let user_ids: Vec<UserId> =
            room.user_ids(None)?.into_iter().filter(|id| !id.to_string().starts_with(&virtual_user_prefix)).collect();
        if !user_ids.is_empty() {
            let user_ids = user_ids.iter().map(|id| id.to_string()).collect::<Vec<String>>().join(", ");
            bail_error!(
                ErrorKind::RoomNotEmpty(channel_name.to_string(), user_ids.clone()),
                t!(["errors", "room_not_empty"]).with_vars(vec![("channel_name", channel_name), ("users", user_ids)])
            );
        }

        let rocketchat_channel_id = room.rocketchat_channel_id()?.unwrap_or_default();
        let room_alias_id = Room::build_room_alias_id(self.config, &server.id, &rocketchat_channel_id)?;
        self.matrix_api.put_canonical_room_alias(room_id.clone(), None)?;
        self.matrix_api.delete_room_alias(room_alias_id)?;

        //TODO: Should we cleanup all the virtual users here?
        let bot_user_id = self.config.matrix_bot_user_id()?;
        let message = t!(["admin_room", "room_successfully_unbridged"]).with_vars(vec![("channel_name", channel_name.clone())]);
        self.matrix_api.send_text_message_event(event.room_id.clone(), bot_user_id, message.l(DEFAULT_LANGUAGE))?;

        Ok(info!(self.logger, "Successfully unbridged room {}", channel_name.clone()))
    }

    fn get_existing_rocketchat_server(&self, rocketchat_url: String) -> Result<RocketchatServer> {
        let server: RocketchatServer = match RocketchatServer::find_by_url(self.connection, &rocketchat_url)? {
            Some(server) => server,
            None => {
                bail_error!(ErrorKind::RocketchatTokenMissing, t!(["errors", "rocketchat_token_missing"]));
            }
        };

        Ok(server)
    }

    fn build_channels_list(
        &self,
        rocketchat_api: &RocketchatApi,
        rocketchat_server_id: &str,
        user_id: &UserId,
    ) -> Result<String> {
        let display_name = rocketchat_api.current_username()?;
        let channels = rocketchat_api.channels_list()?;

        let mut channel_list = "".to_string();
        for channel in channels {
            let formatter = if Room::is_bridged_for_user(
                self.config,
                self.logger,
                self.matrix_api,
                rocketchat_server_id,
                &channel.id,
                user_id,
            )? {
                "**"
            } else if channel.usernames.iter().any(|username| username == &display_name) {
                "*"
            } else {
                ""
            };

            channel_list = channel_list + "*   " + formatter + &channel.name.unwrap_or(channel.id) + formatter + "\n\n";
        }

        Ok(channel_list)
    }

    fn get_rocketchat_server(&self) -> Result<RocketchatServer> {
        match self.admin_room.rocketchat_server_for_admin_room(self.connection)? {
            Some(server) => Ok(server),
            None => Err(
                user_error!(ErrorKind::RoomNotConnected(self.admin_room.id.to_string()), t!(["errors", "room_not_connected"])),
            ),
        }
    }

    /// Build the help message depending on the status of the admin room (connected, user logged
    /// in, etc.).
    pub fn build_help_message(connection: &SqliteConnection, room: &Room, as_url: String, user_id: &UserId) -> Result<String> {
        let message = match room.rocketchat_server_for_admin_room(connection)? {
            Some(server) => if UserOnRocketchatServer::find(connection, user_id, server.id)?.is_logged_in() {
                t!(["admin_room", "usage_instructions"]).with_vars(vec![("rocketchat_url", server.rocketchat_url)])
            } else {
                t!(["admin_room", "login_instructions"]).with_vars(vec![
                    ("rocketchat_url", server.rocketchat_url),
                    ("as_url", as_url),
                    ("user_id", user_id.to_string()),
                ])
            },
            None => {
                let connected_servers = RocketchatServer::find_connected_servers(connection)?;
                let server_list = if connected_servers.is_empty() {
                    t!(["admin_room", "no_rocketchat_server_connected"]).l(DEFAULT_LANGUAGE)
                } else {
                    connected_servers.iter().fold("".to_string(), |init, rs| init + &format!("* {}\n", rs.rocketchat_url))
                };
                t!(["admin_room", "connection_instructions"]).with_vars(vec![("as_url", as_url), ("server_list", server_list)])
            }
        };

        Ok(message.l(DEFAULT_LANGUAGE))
    }
}
