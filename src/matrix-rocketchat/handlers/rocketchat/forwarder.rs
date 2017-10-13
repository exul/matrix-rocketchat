use std::time::{SystemTime, UNIX_EPOCH};

use diesel::Connection;
use diesel::sqlite::SqliteConnection;
use slog::Logger;
use ruma_identifiers::RoomId;

use i18n::*;
use api::{MatrixApi, RocketchatApi};
use api::rocketchat::Message;
use config::Config;
use db::{RocketchatServer, Room, UserOnRocketchatServer};
use errors::*;
use handlers::events::RoomHandler;
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
    pub matrix_api: &'a MatrixApi,
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

        let mut user_on_rocketchat_server = self.connection.transaction(|| {
            virtual_user_handler.find_or_register(
                rocketchat_server.id.clone(),
                message.user_id.clone(),
                message.user_name.clone(),
            )
        })?;

        if !self.is_sendable_message(&user_on_rocketchat_server)? {
            debug!(
                self.logger,
                "Skipping message, because the message was just posted by the user Matrix and echoed back from Rocket.Chat"
            );
            return Ok(());
        }

        let is_direct_message_room = message.channel_id.contains(&message.user_id);
        let channel_id = if is_direct_message_room {
            format!("{}#dm", message.channel_id)
        } else {
            message.channel_id.clone()
        };

        let matrix_room_id = match Room::matrix_id_from_rocketchat_channel_id(
            self.config,
            self.matrix_api,
            &rocketchat_server.id,
            &channel_id,
        )? {
            Some(matrix_room_id) => matrix_room_id,
            None => {
                match self.auto_bridge_direct_message_channel(&virtual_user_handler, rocketchat_server, message)? {
                    Some(matrix_room_id) => matrix_room_id,
                    None => {
                        debug!(
                            self.logger,
                            "Ignoring message from Rocket.Chat channel `{}`, because the channel is not bridged.",
                            message.channel_id
                        );
                        return Ok(());
                    }
                }
            }
        };

        if is_direct_message_room {
            if Room::direct_message_room_matrix_user(
                self.config,
                self.matrix_api,
                matrix_room_id.clone(),
                Some(user_on_rocketchat_server.matrix_user_id.clone()),
            )?
                .is_none()
            {
                match self.find_matching_user_for_direct_message(rocketchat_server, message)? {
                    Some(other_user) => {
                        let invited_user_id = other_user.matrix_user_id.clone();
                        let inviting_user_id = user_on_rocketchat_server.matrix_user_id.clone();
                        virtual_user_handler.add_to_room(invited_user_id.clone(), inviting_user_id, matrix_room_id.clone())?;
                    }
                    None => {
                        debug!(
                            self.logger,
                            "Ignoring message, because not matching user for the direct chat message was found"
                        );
                        return Ok(());
                    }
                }
            };
        } else {
            let invited_user_id = user_on_rocketchat_server.matrix_user_id.clone();
            let inviting_user_id = self.config.matrix_bot_user_id()?;
            virtual_user_handler.add_to_room(invited_user_id, inviting_user_id, matrix_room_id.clone())?;
        };

        if Some(message.user_name.clone()) != user_on_rocketchat_server.rocketchat_username.clone() {
            self.connection.transaction(|| {
                user_on_rocketchat_server.set_rocketchat_username(self.connection, Some(message.user_name.clone()))?;
                self.matrix_api.set_display_name(user_on_rocketchat_server.matrix_user_id.clone(), message.user_name.clone())
            })?;
        }

        self.matrix_api.send_text_message_event(matrix_room_id, user_on_rocketchat_server.matrix_user_id, message.text.clone())
    }

    fn is_sendable_message(&self, user_on_rocketchat_server: &UserOnRocketchatServer) -> Result<bool> {
        match UserOnRocketchatServer::find_by_rocketchat_user_id(
            self.connection,
            user_on_rocketchat_server.rocketchat_server_id.clone(),
            user_on_rocketchat_server.rocketchat_user_id.clone().unwrap_or_default(),
            false,
        )? {
            Some(user_on_rocketchat_server) => {
                let now =
                    SystemTime::now().duration_since(UNIX_EPOCH).chain_err(|| ErrorKind::InternalServerError)?.as_secs() as i64;
                let last_sent = now - user_on_rocketchat_server.last_message_sent;
                Ok(last_sent > RESEND_THRESHOLD_IN_SECONDS)
            }
            None => Ok(true),
        }
    }

    fn auto_bridge_direct_message_channel(
        &self,
        virtual_user_handler: &VirtualUserHandler,
        rocketchat_server: &RocketchatServer,
        message: &Message,
    ) -> Result<Option<RoomId>> {
        debug!(
            self.logger,
            "Got a message for a room that is not bridged yet (channel_id `{}`), checking if it's a direct message",
            &message.channel_id
        );

        let user_on_rocketchat_server = match self.find_matching_user_for_direct_message(rocketchat_server, message)? {
            Some(user_on_rocketchat_server) => user_on_rocketchat_server,
            None => {
                debug!(self.logger, "No matching user found. Not bridging channel {} automatically", message.channel_id);
                return Ok(None);
            }
        };

        let rocketchat_api = RocketchatApi::new(rocketchat_server.rocketchat_url.clone(), self.logger.clone())?
            .with_credentials(
                user_on_rocketchat_server.rocketchat_user_id.clone().unwrap_or_default(),
                user_on_rocketchat_server.rocketchat_auth_token.clone().unwrap_or_default(),
            );

        if let Some(direct_message_channel) =
            rocketchat_api.direct_messages_list()?.iter().find(|dm| dm.id == message.channel_id)
        {
            let direct_message_sender = virtual_user_handler.find_or_register(
                rocketchat_server.id.clone(),
                message.user_id.clone(),
                message.user_name.clone(),
            )?;
            let room_handler = RoomHandler::new(self.config, self.connection, self.logger, self.matrix_api);

            let room_display_name_suffix = t!(["defaults", "direct_message_room_display_name_suffix"]).l(DEFAULT_LANGUAGE);
            let room_display_name = format!("{} {}", message.user_name, room_display_name_suffix);
            let dm_channel_id = format!("{}#dm", direct_message_channel.id.clone());
            let matrix_room_id = room_handler.create_room(
                dm_channel_id,
                rocketchat_server.id.clone(),
                direct_message_sender.matrix_user_id.clone(),
                user_on_rocketchat_server.matrix_user_id.clone(),
                Some(room_display_name),
            )?;

            // invite the bot user into the direct message room to be able to read the room members
            // the bot will leave as soon as the AS gets the join event
            let invitee_id = self.config.matrix_bot_user_id()?;
            self.matrix_api.invite(matrix_room_id.clone(), invitee_id.clone(), direct_message_sender.matrix_user_id.clone())?;
            debug!(self.logger, "Direct message room {} successfully created", &matrix_room_id);

            Ok(Some(matrix_room_id))
        } else {
            debug!(
                self.logger,
                "User {} matched the channel_id, but does not have access to the channel. \
                   Not bridging channel {} automatically",
                user_on_rocketchat_server.matrix_user_id,
                message.channel_id
            );
            Ok(None)
        }
    }

    // this is a pretty hacky way to find a Matrix user that could be the recipient for this
    // message. The message itself doesn't contain any information about the recipient so the
    // channel ID has to be checked against all users that use the application service and are
    // logged in on the sending Rocket.Chat server, because direct message channel IDs consist of
    // the `user_id`s of the two participants.
    fn find_matching_user_for_direct_message(
        &self,
        rocketchat_server: &RocketchatServer,
        message: &Message,
    ) -> Result<Option<UserOnRocketchatServer>> {
        for user_on_rocketchat_server in rocketchat_server.logged_in_users_on_rocketchat_server(self.connection)? {
            if let Some(rocketchat_user_id) = user_on_rocketchat_server.rocketchat_user_id.clone() {
                if message.channel_id.contains(&rocketchat_user_id) {
                    debug!(
                        self.logger,
                        "Matching user with rocketchat_user_id `{}` for channel_id `{}` found.",
                        rocketchat_user_id,
                        &message.channel_id
                    );
                    return Ok(Some(user_on_rocketchat_server));
                }
            }
        }

        Ok(None)
    }
}
