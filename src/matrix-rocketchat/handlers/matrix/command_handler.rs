use diesel::sqlite::SqliteConnection;
use diesel::Connection;
use ruma_events::room::message::MessageEvent;
use ruma_events::room::message::MessageEventContent;
use ruma_identifiers::{RoomAliasId, UserId};
use slog::Logger;

use api::rocketchat::Channel;
use api::{MatrixApi, RocketchatApi};
use config::Config;
use errors::*;
use i18n::*;
use models::{
    Credentials, NewRocketchatServer, NewUserOnRocketchatServer, RocketchatRoom, RocketchatServer, Room, UserOnRocketchatServer,
};
use MAX_ROCKETCHAT_SERVER_ID_LENGTH;

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
            config,
            connection,
            logger,
            matrix_api,
            admin_room,
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
            self.list_rocketchat_rooms(event, &server)?;
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
                    None => self.get_existing_rocketchat_server(rocketchat_url)?,
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

                info!(
                    self.logger,
                    "Successfully executed connect command for user {} and Rocket.Chat server {}",
                    event.user_id,
                    rocketchat_url
                );
                Ok(())
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
                    t!(["errors", "rocketchat_server_already_connected"])
                        .with_vars(vec![("rocketchat_url", rocketchat_url.to_owned()), ("user_id", user_id.to_string())])
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
            rocketchat_url,
            rocketchat_token: Some(token),
        };

        RocketchatServer::insert(self.connection, &new_rocketchat_server)
    }

    fn help(&self, event: &MessageEvent) -> Result<()> {
        let help_message =
            CommandHandler::build_help_message(self.connection, self.admin_room, self.config.as_url.clone(), &event.user_id)?;
        let bot_user_id = self.config.matrix_bot_user_id()?;
        self.matrix_api.send_text_message_event(self.admin_room.id.clone(), bot_user_id, help_message)?;

        info!(self.logger, "Successfully executed help command for user {}", event.user_id);
        Ok(())
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

        let admin_room_id = Some(self.admin_room.id.clone());
        server.login(self.config, self.connection, self.logger, self.matrix_api, &credentials, admin_room_id)
    }

    fn list_rocketchat_rooms(&self, event: &MessageEvent, server: &RocketchatServer) -> Result<()> {
        let user_on_rocketchat_server = UserOnRocketchatServer::find(self.connection, &event.user_id, server.id.clone())?;
        let rocketchat_api = RocketchatApi::new(server.rocketchat_url.clone(), self.logger.clone())?.with_credentials(
            user_on_rocketchat_server.rocketchat_user_id.unwrap_or_default(),
            user_on_rocketchat_server.rocketchat_auth_token.unwrap_or_default(),
        );
        let bot_user_id = self.config.matrix_bot_user_id()?;
        let list = self.build_rocketchat_rooms_list(rocketchat_api.as_ref(), &server.id, &event.user_id)?;
        let message = t!(["admin_room", "list_rocketchat_rooms"]).with_vars(vec![("list", list)]);
        self.matrix_api.send_text_message_event(event.room_id.clone(), bot_user_id, message.l(DEFAULT_LANGUAGE))?;

        info!(self.logger, "Successfully listed rooms for Rocket.Chat server {}", &server.rocketchat_url);
        Ok(())
    }

    fn bridge(&self, event: &MessageEvent, server: &RocketchatServer, message: &str) -> Result<()> {
        let bot_user_id = self.config.matrix_bot_user_id()?;
        let user_on_rocketchat_server = UserOnRocketchatServer::find(self.connection, &event.user_id, server.id.clone())?;
        let rocketchat_api = RocketchatApi::new(server.rocketchat_url.clone(), self.logger.clone())?.with_credentials(
            user_on_rocketchat_server.rocketchat_user_id.clone().unwrap_or_default(),
            user_on_rocketchat_server.rocketchat_auth_token.clone().unwrap_or_default(),
        );

        let channels = rocketchat_api.channels_list()?;
        let groups = rocketchat_api.groups_list()?;

        let mut command = message.split_whitespace().collect::<Vec<&str>>().into_iter();
        let rocketchat_room_name = command.by_ref().nth(1).unwrap_or_default();

        let (rocketchat_room_id, users) =
            match channels.iter().find(|channel| channel.name.clone().unwrap_or_default() == rocketchat_room_name) {
                Some(channel) => {
                    let users = rocketchat_api.channels_members(&channel.id)?;
                    (channel.id.clone(), users)
                }
                None => match groups.iter().find(|group| group.name.clone().unwrap_or_default() == rocketchat_room_name) {
                    Some(group) => {
                        let users = rocketchat_api.groups_members(&group.id)?;
                        (group.id.clone(), users)
                    }
                    None => {
                        bail_error!(
                            ErrorKind::RocketchatChannelOrGroupNotFound(rocketchat_room_name.to_string()),
                            t!(["errors", "rocketchat_channel_or_group_not_found"])
                                .with_vars(vec![("rocketchat_room_name", rocketchat_room_name.to_string())])
                        );
                    }
                },
            };

        let username = rocketchat_api.me()?.username;
        if !users.iter().any(|u| u.username == username) {
            bail_error!(
                ErrorKind::RocketchatJoinFirst(rocketchat_room_name.to_string()),
                t!(["errors", "rocketchat_join_first"])
                    .with_vars(vec![("rocketchat_room_name", rocketchat_room_name.to_string())])
            );
        }

        let rocketchat_room = RocketchatRoom::new(self.config, self.logger, self.matrix_api, rocketchat_room_id, &server.id);
        let room_id = match rocketchat_room.matrix_id()? {
            Some(room_id) => {
                let room = Room::new(self.config, self.logger, self.matrix_api, room_id.clone());
                room.bridge_for_user(event.user_id.clone(), rocketchat_room_name.to_string())?;
                room_id
            }
            None => {
                let usernames: Vec<String> = users.into_iter().map(|u| u.username).collect();
                rocketchat_room.bridge(
                    rocketchat_api.as_ref(),
                    &Some(rocketchat_room_name.to_string()),
                    &usernames,
                    &bot_user_id,
                    &event.user_id,
                )?
            }
        };

        let message = t!(["admin_room", "room_successfully_bridged"])
            .with_vars(vec![("rocketchat_room_name", rocketchat_room_name.to_string())]);
        self.matrix_api.send_text_message_event(event.room_id.clone(), bot_user_id.clone(), message.l(DEFAULT_LANGUAGE))?;

        info!(self.logger, "Successfully bridged room {} to {}", &rocketchat_room.id, &room_id);
        Ok(())
    }

    fn unbridge(&self, event: &MessageEvent, server: &RocketchatServer, message: &str) -> Result<()> {
        let mut command = message.split_whitespace().collect::<Vec<&str>>().into_iter();
        let name = command.nth(1).unwrap_or_default().to_string();

        let user_on_rocketchat_server = UserOnRocketchatServer::find(self.connection, &event.user_id, server.id.clone())?;
        let rocketchat_api = RocketchatApi::new(server.rocketchat_url.clone(), self.logger.clone())?.with_credentials(
            user_on_rocketchat_server.rocketchat_user_id.clone().unwrap_or_default(),
            user_on_rocketchat_server.rocketchat_auth_token.clone().unwrap_or_default(),
        );

        let rocketchat_room =
            RocketchatRoom::from_name(self.config, self.logger, self.matrix_api, &name, &server.id, rocketchat_api.as_ref())?;
        let rocketchat_room_id = match rocketchat_room.matrix_id()? {
            Some(rocketchat_room_id) => rocketchat_room_id,
            None => {
                bail_error!(
                    ErrorKind::UnbridgeOfNotBridgedRoom(name.to_string()),
                    t!(["errors", "unbridge_of_not_bridged_room"]).with_vars(vec![("rocketchat_room_name", name.clone())])
                );
            }
        };

        let room = Room::new(self.config, self.logger, self.matrix_api, rocketchat_room_id.clone());
        let user_ids = room.user_ids(None)?;
        // scope to drop non_virtual_user_ids
        {
            let non_virtual_user_ids: Vec<&UserId> =
                user_ids.iter().filter(|id| !self.config.is_application_service_user(id)).collect();
            if !non_virtual_user_ids.is_empty() {
                let non_virtual_user_ids =
                    non_virtual_user_ids.iter().map(|id| id.to_string()).collect::<Vec<String>>().join(", ");
                bail_error!(
                    ErrorKind::RoomNotEmpty(name.to_string(), non_virtual_user_ids.clone()),
                    t!(["errors", "room_not_empty"])
                        .with_vars(vec![("rocketchat_room_name", name.clone()), ("users", non_virtual_user_ids)])
                );
            }
        }

        let canonical_alias_id = rocketchat_room.build_room_alias_id()?;
        let user_aliases: Vec<RoomAliasId> = room.aliases()?.into_iter().filter(|alias| alias != &canonical_alias_id).collect();
        if !user_aliases.is_empty() {
            let user_aliases = user_aliases.iter().map(|a| a.to_string()).collect::<Vec<String>>().join(", ");
            bail_error!(
                ErrorKind::RoomAssociatedWithAliases(name.to_string(), user_aliases.clone()),
                t!(["errors", "room_assocaited_with_aliases"])
                    .with_vars(vec![("rocketchat_room_name", name.clone()), ("aliases", user_aliases)])
            );
        }

        self.matrix_api.delete_room_alias(canonical_alias_id)?;

        for user_id in user_ids {
            debug!(self.logger, "Leaving and forgetting room {} for user {}", room.id, user_id);
            room.forget(user_id)?;
        }

        let bot_user_id = self.config.matrix_bot_user_id()?;
        let message = t!(["admin_room", "room_successfully_unbridged"]).with_vars(vec![("rocketchat_room_name", name.clone())]);
        self.matrix_api.send_text_message_event(event.room_id.clone(), bot_user_id, message.l(DEFAULT_LANGUAGE))?;

        info!(self.logger, "Successfully unbridged room {}", name.clone());
        Ok(())
    }

    fn get_existing_rocketchat_server(&self, rocketchat_url: &str) -> Result<RocketchatServer> {
        let server: RocketchatServer = match RocketchatServer::find_by_url(self.connection, rocketchat_url)? {
            Some(server) => server,
            None => {
                bail_error!(ErrorKind::RocketchatTokenMissing, t!(["errors", "rocketchat_token_missing"]));
            }
        };

        Ok(server)
    }

    fn build_rocketchat_rooms_list(
        &self,
        rocketchat_api: &RocketchatApi,
        rocketchat_server_id: &str,
        user_id: &UserId,
    ) -> Result<String> {
        let channels = rocketchat_api.channels_list()?;
        let groups = rocketchat_api.groups_list()?;
        let mut joined_rocketchat_rooms = rocketchat_api.channels_list_joined()?;
        joined_rocketchat_rooms.extend(groups.iter().cloned());

        let channel_list =
            self.format_rocketchat_rooms_list(rocketchat_server_id, user_id, &channels, &joined_rocketchat_rooms)?;
        let groups_list = self.format_rocketchat_rooms_list(rocketchat_server_id, user_id, &groups, &joined_rocketchat_rooms)?;
        let channels_title = t!(["admin_room", "channels"]).l(DEFAULT_LANGUAGE);
        let groups_title = t!(["admin_room", "groups"]).l(DEFAULT_LANGUAGE);
        let list = format!("{}:\n{}\n{}:\n{}", channels_title, channel_list, groups_title, groups_list);

        Ok(list)
    }

    fn format_rocketchat_rooms_list(
        &self,
        rocketchat_server_id: &str,
        user_id: &UserId,
        rocketchat_rooms: &[Channel],
        joined_rocketchat_rooms: &[Channel],
    ) -> Result<String> {
        let mut list = "".to_string();
        for r in rocketchat_rooms {
            let rocketchat_room =
                RocketchatRoom::new(self.config, self.logger, self.matrix_api, r.id.clone(), rocketchat_server_id);
            let formatter = if rocketchat_room.is_bridged_for_user(user_id)? {
                "**"
            } else if joined_rocketchat_rooms.iter().any(|jc| jc.id == r.id) {
                "*"
            } else {
                ""
            };

            list = list + "*   " + formatter + &r.name.clone().unwrap_or(rocketchat_room.id) + formatter + "\n\n";
        }

        Ok(list)
    }

    fn get_rocketchat_server(&self) -> Result<RocketchatServer> {
        match self.admin_room.rocketchat_server_for_admin_room(self.connection)? {
            Some(server) => Ok(server),
            None => Err(user_error!(
                ErrorKind::RoomNotConnected(self.admin_room.id.to_string()),
                t!(["errors", "room_not_connected"])
            )),
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
