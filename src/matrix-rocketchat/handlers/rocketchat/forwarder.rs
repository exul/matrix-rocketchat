use std::convert::TryFrom;

use diesel::sqlite::SqliteConnection;
use ruma_identifiers::UserId;
use slog::Logger;

use api::MatrixApi;
use api::rocketchat::Message;
use config::Config;
use db::{NewUser, NewUserOnRocketchatServer, RocketchatServer, Room, User, UserOnRocketchatServer};
use errors::*;
use i18n::DEFAULT_LANGUAGE;

/// Forwards messages from Rocket.Chat to Matrix
pub struct Forwarder<'a> {
    /// Application service configuration
    pub config: &'a Config,
    /// SQL database connection
    pub connection: &'a SqliteConnection,
    /// Logger context
    pub logger: &'a Logger,
    /// Matrix REST API
    pub matrix_api: &'a Box<MatrixApi>,
}

impl<'a> Forwarder<'a> {
    /// Send a message to the Matrix channel.
    pub fn send(&self, rocketchat_server: &RocketchatServer, message: &Message) -> Result<()> {
        let user_on_rocketchat_server = match UserOnRocketchatServer::find_by_rocketchat_user_id(self.connection,
                                                                                                 rocketchat_server.id,
                                                                                                 message.user_id
                                                                                                     .clone())? {
            Some(user_on_rocketchat_server) => user_on_rocketchat_server,
            None => self.create_virtual_user_on_rocketchat_server(rocketchat_server.id, message)?,
        };

        let room =
            match Room::find_by_rocketchat_room_id(self.connection, rocketchat_server.id, message.channel_id.clone())? {
                Some(room) => room,
                None => {
                    debug!(self.logger,
                           "Ignoring message from Rocket.Chat channel `{}`, because the channel is not bridged.",
                           message.channel_id);
                    return Ok(());
                }
            };

        self.matrix_api.send_text_message_event(room.matrix_room_id,
                                                user_on_rocketchat_server.matrix_user_id,
                                                message.text.clone())
    }

    fn create_virtual_user_on_rocketchat_server(&self,
                                                rocketchat_server_id: i32,
                                                message: &Message)
                                                -> Result<UserOnRocketchatServer> {
        let user_id_local_part = format!("@{}_{}_{}", self.config.sender_localpart, message.user_id, rocketchat_server_id);
        self.matrix_api.register(user_id_local_part.clone())?;
        let user_id = format!("{}:{}", user_id_local_part, self.config.hs_url);
        let matrix_user_id = UserId::try_from(&user_id).chain_err(|| ErrorKind::InvalidUserId(user_id))?;

        let new_user = NewUser {
            display_name: message.user_name.clone(),
            is_virtual_user: true,
            language: DEFAULT_LANGUAGE,
            matrix_user_id: matrix_user_id.clone(),
        };
        User::insert(self.connection, &new_user)?;

        let new_user_on_rocketchat_server = NewUserOnRocketchatServer {
            matrix_user_id: matrix_user_id,
            rocketchat_auth_token: None,
            rocketchat_server_id: rocketchat_server_id,
            rocketchat_user_id: Some(message.user_id.clone()),
            rocketchat_username: Some(message.user_name.clone()),
        };

        UserOnRocketchatServer::upsert(self.connection, &new_user_on_rocketchat_server)
    }
}
