use diesel::Connection;
use diesel::sqlite::SqliteConnection;
use ruma_events::room::message::MessageEvent;
use ruma_events::room::message::MessageEventContent;
use ruma_identifiers::UserId;
use slog::Logger;

use api::{MatrixApi, RocketchatApi};
use api::rocketchat::Channel;
use config::Config;
use db::{NewRocketchatServer, NewRoom, NewUserInRoom, NewUserOnRocketchatServer, RocketchatServer, Room, User, UserInRoom,
         UserOnRocketchatServer};
use errors::*;
use i18n::*;

/// Handles command messages from the admin room
pub struct CommandHandler<'a> {
    config: &'a Config,
    connection: &'a SqliteConnection,
    logger: &'a Logger,
    matrix_api: &'a Box<MatrixApi>,
}

impl<'a> CommandHandler<'a> {
    /// Create a new `CommandHandler`.
    pub fn new(config: &'a Config,
               connection: &'a SqliteConnection,
               logger: &'a Logger,
               matrix_api: &'a Box<MatrixApi>)
               -> CommandHandler<'a> {
        CommandHandler {
            config: config,
            connection: connection,
            logger: logger,
            matrix_api: matrix_api,
        }
    }

    /// Handles command messages from an admin room
    pub fn process(&self, event: &MessageEvent, room: &Room) -> Result<()> {
        let message = match event.content {
            MessageEventContent::Text(ref text_content) => text_content.body.clone(),
            _ => {
                debug!(self.logger, "Unknown event content type, skipping");
                return Ok(());
            }
        };

        if message.starts_with("connect") {
            debug!(self.logger, format!("Received connect command: {}", message));

            self.connect(event, &message)?;
        } else if message == "help" {
            debug!(self.logger, "Received help command");

            self.help(event)?;
        } else if message.starts_with("login") {
            debug!(self.logger, "Received login command");

            let rocketchat_server = self.get_rocketchat_server(room, &message)?;
            self.login(event, &rocketchat_server, &message)?;
        } else if message == "list" {
            debug!(self.logger, "Received list command");

            let rocketchat_server = self.get_rocketchat_server(room, &message)?;
            self.list_channels(event, &rocketchat_server)?;
        } else if message.starts_with("bridge") {
            debug!(self.logger, "Received bridge command");

            let rocketchat_server = self.get_rocketchat_server(room, &message)?;
            self.bridge(event, &rocketchat_server, &message)?;
        } else if event.user_id == self.config.matrix_bot_user_id()? {
            debug!(self.logger, "Skipping event from bot user");
        } else {
            let msg = format!("Skipping event, don't know how to handle command `{}`", message);
            debug!(self.logger, msg);
        }

        Ok(())
    }

    fn connect(&self, event: &MessageEvent, message: &str) -> Result<()> {
        self.connection
            .transaction(|| {
                let room = Room::find(self.connection, &event.room_id)?;
                if room.is_connected() {
                    bail_error!(ErrorKind::RoomAlreadyConnected(event.room_id.to_string()),
                                t!(["errors", "room_already_connected"]));
                }

                let mut command = message.split_whitespace().collect::<Vec<&str>>().into_iter();
                let rocketchat_url = command.by_ref().nth(1).unwrap_or_default();

                debug!(self.logger, "Attempting to connect to Rocket.Chat server {}", rocketchat_url);

                let rocketchat_server = match command.by_ref().next() {
                    Some(token) => {
                        self.connect_new_rocktechat_server(rocketchat_url.to_string(), token.to_string(), &event.user_id)?
                    }
                    None => self.get_existing_rocketchat_server(rocketchat_url.to_string())?,
                };

                room.set_rocketchat_server_id(self.connection, rocketchat_server.id)?;

                let new_user_on_rocketchat_server = NewUserOnRocketchatServer {
                    is_virtual_user: false,
                    matrix_user_id: event.user_id.clone(),
                    rocketchat_server_id: rocketchat_server.id,
                    rocketchat_user_id: None,
                    rocketchat_auth_token: None,
                    rocketchat_username: None,
                };

                UserOnRocketchatServer::upsert(self.connection, &new_user_on_rocketchat_server)?;

                let user = User::find(self.connection, &event.user_id)?;
                let body = t!(["admin_room", "login_instructions"])
                    .with_vars(vec![("rocketchat_url", rocketchat_url.to_string())])
                    .l(&user.language);
                self.matrix_api.send_text_message_event(event.room_id.clone(), self.config.matrix_bot_user_id()?, body)
            })
            .map_err(Error::from)
    }

    fn connect_new_rocktechat_server(&self,
                                     rocketchat_url: String,
                                     token: String,
                                     matrix_user_id: &UserId)
                                     -> Result<RocketchatServer> {
        if let Some(rocketchat_server) = RocketchatServer::find_by_url(self.connection, rocketchat_url.clone())? {
            if rocketchat_server.rocketchat_token.is_some() {
                bail_error!(ErrorKind::RocketchatServerAlreadyConnected(rocketchat_url.clone()),
                            t!(["errors", "rocketchat_server_already_connected"])
                                .with_vars(vec![("rocketchat_url", rocketchat_url),
                                                ("matrix_user_id", matrix_user_id.to_string())]));
            }
        }

        if RocketchatServer::find_by_token(self.connection, token.clone())?.is_some() {
            bail_error!(ErrorKind::RocketchatTokenAlreadyInUse(token.clone()),
                        t!(["errors", "token_already_in_use"]).with_vars(vec![("token", token)]));
        }

        // see if we can reach the server and if the server has a supported API version
        RocketchatApi::new(rocketchat_url.clone(), Some(token.clone()), self.logger.clone())?;

        let new_rocketchat_server = NewRocketchatServer {
            rocketchat_url: rocketchat_url,
            rocketchat_token: Some(token),
        };

        RocketchatServer::insert(self.connection, &new_rocketchat_server)
    }

    fn help(&self, event: &MessageEvent) -> Result<()> {
        let room = Room::find(self.connection, &event.room_id)?;
        let user = User::find(self.connection, &event.user_id)?;

        let body = match room.rocketchat_url(self.connection)? {
            Some(rocketchat_url) => {
                t!(["admin_room", "login_instructions"]).with_vars(vec![("rocketchat_url", rocketchat_url)])
            }
            None => t!(["admin_room", "connection_instructions"]).with_vars(vec![("as_url", self.config.as_url.clone())]),
        };

        self.matrix_api.send_text_message_event(event.room_id.clone(),
                                                self.config.matrix_bot_user_id()?,
                                                body.l(&user.language))
    }

    fn login(&self, event: &MessageEvent, rocketchat_server: &RocketchatServer, message: &str) -> Result<()> {
        let user = User::find(self.connection, &event.user_id)?;

        let user_on_rocketchat_server =
            UserOnRocketchatServer::find(self.connection, &event.user_id, rocketchat_server.id)?;

        let mut command = message.split_whitespace().collect::<Vec<&str>>().into_iter();
        let username = command.by_ref().nth(1).unwrap_or_default();
        let password = command.by_ref().fold("".to_string(), |acc, x| acc + x);

        let rocketchat_api = RocketchatApi::new(rocketchat_server.rocketchat_url.clone(),
                                                rocketchat_server.rocketchat_token.clone(),
                                                self.logger.clone())?;

        let (rocketchat_user_id, rocketchat_auth_token) = rocketchat_api.login(username, &password)?;
        user_on_rocketchat_server.set_credentials(self.connection,
                                                  Some(rocketchat_user_id.clone()),
                                                  Some(rocketchat_auth_token.clone()))?;

        let username = rocketchat_api.username(rocketchat_user_id, rocketchat_auth_token)?;
        user_on_rocketchat_server.set_rocketchat_username(self.connection, Some(username))?;

        let bot_matrix_user_id = self.config.matrix_bot_user_id()?;
        let message = t!(["admin_room", "bridge_instructions"]);
        self.matrix_api.send_text_message_event(event.room_id.clone(), bot_matrix_user_id, message.l(&user.language))
    }

    fn list_channels(&self, event: &MessageEvent, rocketchat_server: &RocketchatServer) -> Result<()> {
        let user = User::find(self.connection, &event.user_id)?;

        let user_on_rocketchat_server =
            UserOnRocketchatServer::find(self.connection, &event.user_id, rocketchat_server.id)?;
        let rocketchat_api = RocketchatApi::new(rocketchat_server.rocketchat_url.clone(),
                                                rocketchat_server.rocketchat_token.clone(),
                                                self.logger.clone())?;
        let channels =
            rocketchat_api.channels_list(user_on_rocketchat_server.rocketchat_user_id.unwrap_or_default(),
                                         user_on_rocketchat_server.rocketchat_auth_token.unwrap_or_default())?;

        let bot_matrix_user_id = self.config.matrix_bot_user_id()?;
        let channels_list = self.build_channels_list(rocketchat_server.id, &event.user_id, channels)?;
        let message = t!(["admin_room", "list_channels"]).with_vars(vec![("channel_list", channels_list)]);
        self.matrix_api.send_text_message_event(event.room_id.clone(), bot_matrix_user_id, message.l(&user.language))
    }

    fn bridge(&self, event: &MessageEvent, rocketchat_server: &RocketchatServer, message: &str) -> Result<()> {
        self.connection
            .transaction(|| {
                let user_on_rocketchat_server =
                    UserOnRocketchatServer::find(self.connection, &event.user_id, rocketchat_server.id)?;
                let rocketchat_api = RocketchatApi::new(rocketchat_server.rocketchat_url.clone(),
                                                        rocketchat_server.rocketchat_token.clone(),
                                                        self.logger.clone())?;

                let channels =
                    rocketchat_api.channels_list(user_on_rocketchat_server.rocketchat_user_id.clone().unwrap_or_default(),
                                                 user_on_rocketchat_server.rocketchat_auth_token
                                                     .clone()
                                                     .unwrap_or_default())?;

                let mut command = message.split_whitespace().collect::<Vec<&str>>().into_iter();
                let channel_name = command.by_ref().nth(1).unwrap_or_default();

                let channel = match channels.iter().find(|channel| channel.name == channel_name) {
                    Some(channel) => channel,
                    None => {
                        bail_error!(ErrorKind::RocketchatChannelNotFound(channel_name.to_string()),
                                    t!(["errors", "rocketchat_channel_not_found"])
                                        .with_vars(vec![("channel_name", channel_name.to_string())]));
                    }
                };

                if Room::is_bridged_for_user(self.connection, rocketchat_server.id, channel.id.clone(), &event.user_id)? {
                    bail_error!(ErrorKind::RocketchatChannelAlreadyBridged(channel_name.to_string()),
                                t!(["errors", "rocketchat_channel_already_bridged"])
                                    .with_vars(vec![("channel_name", channel_name.to_string())]));
                }

                let username = user_on_rocketchat_server.rocketchat_username.clone().unwrap_or_default();
                if !channel.usernames.iter().any(|u| u == &username) {
                    bail_error!(ErrorKind::RocketchatJoinFirst(channel_name.to_string()),
                                t!(["errors", "rocketchat_join_first"]).with_vars(vec![("channel_name",
                                                                                        channel_name.to_string())]));
                }

                let room =
                    match Room::find_by_rocketchat_room_id(self.connection, rocketchat_server.id, channel.id.clone())? {
                        Some(room) => room,
                        None => self.create_room(channel, rocketchat_server.id, event.user_id.clone())?,
                    };

                let user = user_on_rocketchat_server.user(self.connection)?;
                let new_user_in_room = NewUserInRoom {
                    matrix_user_id: user.matrix_user_id,
                    matrix_room_id: room.matrix_room_id.clone(),
                };
                UserInRoom::insert(self.connection, &new_user_in_room)?;

                let bot_matrix_user_id = self.config.matrix_bot_user_id()?;
                let message =
                    t!(["admin_room", "room_successfully_bridged"]).with_vars(vec![("channel_name", channel.name.clone())]);
                info!(self.logger,
                      "Successfully bridged room {} to {}",
                      &channel.id,
                      &room.matrix_room_id);
                self.matrix_api.send_text_message_event(event.room_id.clone(),
                                                        bot_matrix_user_id,
                                                        message.l(&user.language))
            })
            .map_err(Error::from)
    }

    fn get_existing_rocketchat_server(&self, rocketchat_url: String) -> Result<RocketchatServer> {
        let rocketchat_server: RocketchatServer = match RocketchatServer::find_by_url(self.connection, rocketchat_url)? {
            Some(rocketchat_server) => rocketchat_server,
            None => {
                bail_error!(ErrorKind::RocketchatTokenMissing, t!(["errors", "rocketchat_token_missing"]));
            }
        };

        Ok(rocketchat_server)
    }

    fn build_channels_list(&self,
                           rocketchat_server_id: i32,
                           matrix_user_id: &UserId,
                           channels: Vec<Channel>)
                           -> Result<String> {
        let user = UserOnRocketchatServer::find(self.connection, matrix_user_id, rocketchat_server_id)?;
        let mut channel_list = "".to_string();

        for channel in channels {
            let formatter =
                if Room::is_bridged_for_user(self.connection, rocketchat_server_id, channel.id.clone(), matrix_user_id)? {
                    "**"
                } else if channel.usernames.iter().any(|username| Some(username) == user.rocketchat_username.as_ref()) {
                    "*"
                } else {
                    ""
                };

            channel_list = channel_list + "*   " + formatter + &channel.name + formatter + "\n\n";
        }

        Ok(channel_list)
    }

    fn get_rocketchat_server(&self, room: &Room, message: &str) -> Result<RocketchatServer> {
        match room.rocketchat_server(self.connection)? {
            Some(rocketchat_server) => Ok(rocketchat_server),
            None => {
                Err(user_error!(ErrorKind::RoomNotConnected(room.matrix_room_id.to_string(), message.to_string()),
                                t!(["errors", "room_not_connected"])))
            }
        }
    }

    fn create_room(&self, channel: &Channel, rocketchat_server_id: i32, user_id: UserId) -> Result<Room> {
        let bot_matrix_user_id = self.config.matrix_bot_user_id()?;
        let matrix_room_id = self.matrix_api.create_room(channel.name.clone(), user_id)?;
        let new_room = NewRoom {
            matrix_room_id: matrix_room_id.clone(),
            display_name: channel.name.clone(),
            rocketchat_server_id: Some(rocketchat_server_id),
            rocketchat_room_id: Some(channel.id.clone()),
            is_admin_room: false,
            is_bridged: true,
        };
        let room = Room::insert(self.connection, &new_room)?;
        let new_user_in_room = NewUserInRoom {
            matrix_user_id: bot_matrix_user_id.clone(),
            matrix_room_id: matrix_room_id,
        };
        UserInRoom::insert(self.connection, &new_user_in_room)?;
        Ok(room)
    }
}
