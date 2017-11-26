use std::time::{SystemTime, UNIX_EPOCH};

use diesel::sqlite::SqliteConnection;
use slog::Logger;

use i18n::*;
use api::{MatrixApi, RocketchatApi};
use api::rocketchat::Message;
use config::Config;
use errors::*;
use log;
use models::{Channel, RocketchatServer, Room, UserOnRocketchatServer, VirtualUser};

const RESEND_THRESHOLD_IN_SECONDS: i64 = 3;

/// Forwards messages from Rocket.Chat to Matrix
pub struct Forwarder<'a> {
    /// Application service configuration
    config: &'a Config,
    /// SQL database connection
    connection: &'a SqliteConnection,
    /// Logger context
    logger: &'a Logger,
    /// Matrix REST API
    matrix_api: &'a MatrixApi,
    /// Manages virtual users that the application service uses
    virtual_user: &'a VirtualUser<'a>,
}

impl<'a> Forwarder<'a> {
    /// Create a new `Forwarder`.
    pub fn new(
        config: &'a Config,
        connection: &'a SqliteConnection,
        logger: &'a Logger,
        matrix_api: &'a MatrixApi,
        virtual_user: &'a VirtualUser,
    ) -> Forwarder<'a> {
        Forwarder {
            config: config,
            connection: connection,
            logger: logger,
            matrix_api: matrix_api,
            virtual_user: virtual_user,
        }
    }

    /// Send a message to the Matrix channel.
    pub fn send(&self, server: &RocketchatServer, message: &Message) -> Result<()> {
        if !self.is_sendable_message(message.user_id.clone(), server.id.clone())? {
            debug!(
                self.logger,
                "Skipping message, because the message was just posted by the user Matrix and echoed back from Rocket.Chat"
            );
            return Ok(());
        }

        let room = match self.prepare_room(server, message)? {
            Some(room) => room,
            None => {
                debug!(
                    self.logger,
                    "Ignoring message from Rocket.Chat channel `{}`, because the channel is not bridged.",
                    message.channel_id
                );
                return Ok(());
            }
        };

        let sender_id =
            self.virtual_user.find_or_register(&server.id, &message.user_id, &message.user_name)?;
        let current_displayname = self.matrix_api.get_display_name(sender_id.clone())?.unwrap_or_default();
        if message.user_name != current_displayname {
            debug!(self.logger, "Display name changed from `{}` to `{}`, will update", current_displayname, message.user_name);
            if let Err(err) = self.matrix_api.set_display_name(sender_id.clone(), message.user_name.clone()) {
                log::log_error(self.logger, &err)
            }
        }

        self.matrix_api.send_text_message_event(room.id.clone(), sender_id, message.text.clone())
    }

    fn is_sendable_message(&self, rocketchat_user_id: String, server_id: String) -> Result<bool> {
        match UserOnRocketchatServer::find_by_rocketchat_user_id(self.connection, server_id, rocketchat_user_id)? {
            Some(user_on_rocketchatserver) => {
                let now =
                    SystemTime::now().duration_since(UNIX_EPOCH).chain_err(|| ErrorKind::InternalServerError)?.as_secs() as i64;
                let last_sent = now - user_on_rocketchatserver.last_message_sent;
                debug!(self.logger, "Found {}, last message sent {}s ago", user_on_rocketchatserver.matrix_user_id, last_sent);
                Ok(last_sent > RESEND_THRESHOLD_IN_SECONDS)
            }
            None => Ok(true),
        }
    }

    fn prepare_room(&self, server: &RocketchatServer, message: &Message) -> Result<Option<Room>> {
        let is_direct_message_room = message.channel_id.contains(&message.user_id);
        if is_direct_message_room {
            self.prepare_dm_room(server, message)
        } else {
            self.prepare_room_for_channel(server, message)
        }
    }

    fn prepare_dm_room(&self, server: &RocketchatServer, message: &Message) -> Result<Option<Room>> {
        let receiver = match self.find_matching_user_for_direct_message(server, message)? {
            Some(receiver) => receiver,
            None => {
                debug!(self.logger, "Ignoring message, because not matching user for the direct chat message was found");
                return Ok(None);
            }
        };

        let room = match self.try_to_find_or_create_direct_message_room(server, &receiver, message)? {
            Some(room) => room,
            None => return Ok(None),
        };

        Ok(Some(room))
    }

    fn try_to_find_or_create_direct_message_room(
        &self,
        server: &RocketchatServer,
        receiver: &UserOnRocketchatServer,
        message: &Message,
    ) -> Result<Option<Room>> {
        if let Some(room) = self.lookup_existing_direct_message_room(server, receiver, message)? {
            self.invite_user_into_direct_message_room(&room, receiver)?;
            return Ok(Some(room));
        }

        self.auto_bridge_direct_message_channel(server, receiver, message)
    }

    fn lookup_existing_direct_message_room(
        &self,
        server: &RocketchatServer,
        receiver: &UserOnRocketchatServer,
        message: &Message,
    ) -> Result<Option<Room>> {
        let sender_id = self.virtual_user.build_user_id(&message.user_id, &server.id)?;

        // If the user does not exist yet, there is no existing direct message room
        if self.matrix_api.get_display_name(sender_id.clone())?.is_none() {
            return Ok(None);
        }


        //TODO: This is highly inefficient and needs some kind of caching, but no persistent storage or alias is needed
        for room_id in self.matrix_api.get_joined_rooms(sender_id.clone())? {
            let room = Room::new(self.config, self.logger, self.matrix_api, room_id);
            let user_ids = room.user_ids(Some(sender_id.clone()))?;
            if user_ids.iter().all(|id| id == &sender_id || id == &receiver.matrix_user_id) {
                return Ok(Some(room));
            }
        }

        Ok(None)
    }

    fn invite_user_into_direct_message_room(&self, room: &Room, receiver: &UserOnRocketchatServer) -> Result<()> {
        let direct_message_recepient = room.direct_message_matrix_user()?;
        if direct_message_recepient.is_none() {
            let inviting_user_id = self.matrix_api.get_room_creator(room.id.clone())?;
            room.join_user(receiver.matrix_user_id.clone(), inviting_user_id)?;
        }

        Ok(())
    }

    fn prepare_room_for_channel(&self, server: &RocketchatServer, message: &Message) -> Result<Option<Room>> {
        let channel = Channel::new(self.config, self.logger, self.matrix_api, message.channel_id.clone(), &server.id);
        let room_id = match channel.matrix_id()? {
            Some(room_id) => room_id,
            None => return Ok(None),
        };

        let inviting_user_id = self.config.matrix_bot_user_id()?;
        let user_id = message.user_id.clone();
        let user_name = message.user_name.clone();
        let sender_id = self.virtual_user.find_or_register(&server.id, &user_id, &user_name)?;
        let room = Room::new(self.config, self.logger, self.matrix_api, room_id);
        room.join_user(sender_id, inviting_user_id)?;

        Ok(Some(room))
    }

    fn auto_bridge_direct_message_channel(
        &self,
        server: &RocketchatServer,
        receiver: &UserOnRocketchatServer,
        message: &Message,
    ) -> Result<Option<Room>> {
        debug!(
            self.logger,
            "Got a message for a room that is not bridged yet (channel_id `{}`), checking if it's a direct message",
            &message.channel_id
        );

        let rocketchat_api = RocketchatApi::new(server.rocketchat_url.clone(), self.logger.clone())?.with_credentials(
            receiver.rocketchat_user_id.clone().unwrap_or_default(),
            receiver.rocketchat_auth_token.clone().unwrap_or_default(),
        );

        if rocketchat_api.direct_messages_list()?.iter().any(|dm| dm.id == message.channel_id) {
            let sender_id =
                self.virtual_user.find_or_register(&server.id, &message.user_id, &message.user_name)?;

            let room_display_name_suffix = t!(["defaults", "direct_message_room_display_name_suffix"]).l(DEFAULT_LANGUAGE);
            let room_display_name = format!("{} {}", message.user_name, room_display_name_suffix);

            let display_name = Some(room_display_name);
            let room_id = Room::create(self.matrix_api, None, &display_name, &sender_id, &receiver.matrix_user_id)?;

            // invite the bot user into the direct message room to be able to read the room state
            // the bot will leave as soon as the AS gets the join event
            let invitee_id = self.config.matrix_bot_user_id()?;
            self.matrix_api.invite(room_id.clone(), invitee_id.clone(), sender_id.clone())?;
            debug!(self.logger, "Direct message room {} successfully created", &room_id);

            let room = Room::new(self.config, self.logger, self.matrix_api, room_id);
            Ok(Some(room))
        } else {
            debug!(
                self.logger,
                "User {} matched the channel_id, but does not have access to the channel. \
                 Not bridging channel {} automatically",
                receiver.matrix_user_id,
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
        server: &RocketchatServer,
        message: &Message,
    ) -> Result<Option<UserOnRocketchatServer>> {
        for user_on_rocketchatserver in server.logged_in_users_on_rocketchat_server(self.connection)? {
            if let Some(rocketchat_user_id) = user_on_rocketchatserver.rocketchat_user_id.clone() {
                if message.channel_id.contains(&rocketchat_user_id) {
                    debug!(
                        self.logger,
                        "Matching user with rocketchat_user_id `{}` for channel_id `{}` found.",
                        rocketchat_user_id,
                        &message.channel_id
                    );
                    return Ok(Some(user_on_rocketchatserver));
                }
            }
        }

        Ok(None)
    }
}
