use std::convert::TryFrom;
use std::time::{SystemTime, UNIX_EPOCH};

use diesel::Connection;
use diesel::sqlite::SqliteConnection;
use ruma_identifiers::{RoomId, UserId};
use slog::Logger;

use api::MatrixApi;
use api::rocketchat::Message;
use config::Config;
use db::{NewUser, NewUserInRoom, NewUserOnRocketchatServer, RocketchatServer, Room, User, UserInRoom, UserOnRocketchatServer};
use errors::*;
use i18n::DEFAULT_LANGUAGE;

const RESEND_THRESHOLD_IN_SECONDS: i64 = 3;

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
                                                                     .clone(),
                                                                 true)? {
            Some(user_on_rocketchat_server) => user_on_rocketchat_server,
            None => {
                self.connection
                    .transaction(|| self.create_virtual_user_on_rocketchat_server(rocketchat_server.id, message))?
            }
        };

        if !self.is_sendable_message(&user_on_rocketchat_server)? {
            debug!(self.logger,
                   "Skipping message, because the message was just posted by the user Matrix and echoed back from Rocket.Chat");
            return Ok(());
        }

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

        if Some(message.user_name.clone()) != user_on_rocketchat_server.rocketchat_username.clone() {
            self.connection
                .transaction(|| {
                                 user_on_rocketchat_server.set_rocketchat_username(self.connection,
                                                                                   Some(message.user_name.clone()))?;
                                 self.matrix_api.set_display_name(user_on_rocketchat_server.matrix_user_id.clone(),
                                                                  message.user_name.clone())
                             })?;
        }


        let user_in_room =
            UserInRoom::find_by_matrix_user_id_and_matrix_room_id(self.connection,
                                                                  &user_on_rocketchat_server.matrix_user_id,
                                                                  &room.matrix_room_id)?;
        if user_in_room.is_none() {
            self.add_virtual_user_to_room(user_on_rocketchat_server.matrix_user_id.clone(), room.matrix_room_id.clone())?;
        }

        self.matrix_api.send_text_message_event(room.matrix_room_id,
                                                user_on_rocketchat_server.matrix_user_id,
                                                message.text.clone())
    }

    fn create_virtual_user_on_rocketchat_server(&self,
                                                rocketchat_server_id: i32,
                                                message: &Message)
                                                -> Result<UserOnRocketchatServer> {

        let user_id_local_part = format!("{}_{}_{}", self.config.sender_localpart, message.user_id, rocketchat_server_id);
        let user_id = format!("@{}:{}", user_id_local_part, self.config.hs_domain);
        let matrix_user_id = UserId::try_from(&user_id).chain_err(|| ErrorKind::InvalidUserId(user_id))?;

        let new_user = NewUser {
            language: DEFAULT_LANGUAGE,
            matrix_user_id: matrix_user_id.clone(),
        };
        User::insert(self.connection, &new_user)?;

        let new_user_on_rocketchat_server = NewUserOnRocketchatServer {
            is_virtual_user: true,
            matrix_user_id: matrix_user_id,
            rocketchat_auth_token: None,
            rocketchat_server_id: rocketchat_server_id,
            rocketchat_user_id: Some(message.user_id.clone()),
            rocketchat_username: Some(message.user_name.clone()),
        };
        let user_on_rocketchat_server = UserOnRocketchatServer::upsert(self.connection, &new_user_on_rocketchat_server)?;

        self.matrix_api.register(user_id_local_part.clone())?;
        if let Err(err) = self.matrix_api.set_display_name(user_on_rocketchat_server.matrix_user_id.clone(),
                                                           message.user_name.clone()) {
            info!(self.logger,
                  format!("Setting display name `{}`, for user `{}` failed with {}",
                          &user_on_rocketchat_server.matrix_user_id,
                          &message.user_name,
                          err));
        }

        Ok(user_on_rocketchat_server)
    }

    fn add_virtual_user_to_room(&self, matrix_user_id: UserId, matrix_room_id: RoomId) -> Result<()> {
        self.matrix_api.invite(matrix_room_id.clone(), matrix_user_id.clone())?;
        self.matrix_api.join(matrix_room_id.clone(), matrix_user_id.clone())?;
        let new_user_in_room = NewUserInRoom {
            matrix_user_id: matrix_user_id,
            matrix_room_id: matrix_room_id,
        };
        UserInRoom::insert(self.connection, &new_user_in_room)?;
        Ok(())
    }

    fn is_sendable_message(&self, virtual_user_on_rocketchat_server: &UserOnRocketchatServer) -> Result<bool> {
        match UserOnRocketchatServer::find_by_rocketchat_user_id(self.connection,
                                                                 virtual_user_on_rocketchat_server.rocketchat_server_id,
                                                                 virtual_user_on_rocketchat_server.rocketchat_user_id
                                                                     .clone()
                                                                     .unwrap_or_default(),
                                                                 false)? {
            Some(user_on_rocketchat_server) => {
                let user = user_on_rocketchat_server.user(self.connection)?;
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .chain_err(|| ErrorKind::InternalServerError)?
                    .as_secs() as i64;
                let last_sent = now - user.last_message_sent;
                Ok(last_sent > RESEND_THRESHOLD_IN_SECONDS)
            }
            None => Ok(true),
        }
    }
}
