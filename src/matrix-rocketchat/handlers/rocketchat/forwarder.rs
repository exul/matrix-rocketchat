use std::time::{SystemTime, UNIX_EPOCH};

use diesel::Connection;
use diesel::sqlite::SqliteConnection;
use slog::Logger;

use api::MatrixApi;
use api::rocketchat::Message;
use config::Config;
use db::{RocketchatServer, Room, UserInRoom, UserOnRocketchatServer};
use errors::*;
use handlers::rocketchat::VirtualUserHandler;

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
        let virtual_user_handler = VirtualUserHandler {
            config: self.config,
            connection: self.connection,
            logger: self.logger,
            matrix_api: self.matrix_api,
        };

        let mut user_on_rocketchat_server = self.connection
            .transaction(|| {
                             virtual_user_handler.find_or_register(rocketchat_server.id.clone(),
                                                                   message.user_id.clone(),
                                                                   message.user_name.clone())
                         })?;

        if !self.is_sendable_message(&user_on_rocketchat_server)? {
            debug!(self.logger,
                   "Skipping message, because the message was just posted by the user Matrix and echoed back from Rocket.Chat");
            return Ok(());
        }

        let matrix_room_id = match Room::find_by_rocketchat_room_id(self.connection,
                                                                    rocketchat_server.id.clone(),
                                                                    message.channel_id.clone())? {
            Some(ref room) if room.is_bridged => room.matrix_room_id.clone(),
            _ => {
                debug!(self.logger,
                       "Ignoring message from Rocket.Chat channel `{}`, because the channel is not bridged.",
                       message.channel_id);
                return Ok(());
            }
        };

        if Some(message.user_name.clone()) != user_on_rocketchat_server.rocketchat_username.clone() {
            self.connection
                .transaction(|| {
                                 user_on_rocketchat_server
                                     .set_rocketchat_username(self.connection, Some(message.user_name.clone()))?;
                                 self.matrix_api
                                     .set_display_name(user_on_rocketchat_server.matrix_user_id.clone(),
                                                       message.user_name.clone())
                             })?;
        }


        let user_in_room = UserInRoom::find_by_matrix_user_id_and_matrix_room_id(self.connection,
                                                                                 &user_on_rocketchat_server.matrix_user_id,
                                                                                 &matrix_room_id)?;
        if user_in_room.is_none() {
            virtual_user_handler.add_to_room(user_on_rocketchat_server.matrix_user_id.clone(), matrix_room_id.clone())?;
        }

        self.matrix_api.send_text_message_event(matrix_room_id, user_on_rocketchat_server.matrix_user_id, message.text.clone())
    }

    fn is_sendable_message(&self, virtual_user_on_rocketchat_server: &UserOnRocketchatServer) -> Result<bool> {
        match UserOnRocketchatServer::find_by_rocketchat_user_id(self.connection,
                                                                 virtual_user_on_rocketchat_server
                                                                     .rocketchat_server_id
                                                                     .clone(),
                                                                 virtual_user_on_rocketchat_server
                                                                     .rocketchat_user_id
                                                                     .clone()
                                                                     .unwrap_or_default(),
                                                                 false)? {
            Some(user_on_rocketchat_server) => {
                let user = user_on_rocketchat_server.user(self.connection)?;
                let now =
                    SystemTime::now().duration_since(UNIX_EPOCH).chain_err(|| ErrorKind::InternalServerError)?.as_secs() as i64;
                let last_sent = now - user.last_message_sent;
                Ok(last_sent > RESEND_THRESHOLD_IN_SECONDS)
            }
            None => Ok(true),
        }
    }
}
