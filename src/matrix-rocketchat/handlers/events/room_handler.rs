use std::convert::TryFrom;

use diesel::sqlite::SqliteConnection;
use diesel::Connection;
use iron::url::Host;
use ruma_events::room::member::{MemberEvent, MembershipState};
use ruma_identifiers::{RoomId, UserId};
use slog::Logger;

use api::{MatrixApi, RocketchatApi};
use api::rocketchat::Channel;
use config::Config;
use db::{RocketchatServer, Room, User};
use errors::*;
use handlers::ErrorNotifier;
use handlers::rocketchat::VirtualUserHandler;
use i18n::*;
use log;
use super::CommandHandler;

/// Handles room events
pub struct RoomHandler<'a> {
    config: &'a Config,
    connection: &'a SqliteConnection,
    logger: &'a Logger,
    matrix_api: &'a MatrixApi,
}

impl<'a> RoomHandler<'a> {
    /// Create a new `RoomHandler`.
    pub fn new(
        config: &'a Config,
        connection: &'a SqliteConnection,
        logger: &'a Logger,
        matrix_api: &'a MatrixApi,
    ) -> RoomHandler<'a> {
        RoomHandler {
            config: config,
            connection: connection,
            logger: logger,
            matrix_api: matrix_api,
        }

    }

    /// Handles room membership changes
    pub fn process(&self, event: &MemberEvent) -> Result<()> {
        let matrix_bot_user_id = self.config.matrix_bot_user_id()?;
        let state_key = UserId::try_from(&event.state_key).chain_err(|| ErrorKind::InvalidUserId(event.state_key.clone()))?;
        let addressed_to_matrix_bot = state_key == matrix_bot_user_id;

        match event.content.membership {
            MembershipState::Invite if addressed_to_matrix_bot => {
                let msg = format!("Bot `{}` got invite for room `{}`", matrix_bot_user_id, event.room_id);
                debug!(self.logger, msg);

                self.handle_bot_invite(event.room_id.clone(), matrix_bot_user_id)?;
            }
            MembershipState::Join if addressed_to_matrix_bot => {
                let msg = format!("Received join event for bot user {} and room {}", matrix_bot_user_id, event.room_id);
                debug!(self.logger, msg);

                self.handle_bot_join(event.room_id.clone(), matrix_bot_user_id)?;
            }
            MembershipState::Join => {
                let msg = format!("Received join event for user {} and room {}", &state_key, &event.room_id);
                debug!(self.logger, msg);

                self.handle_user_join(event.room_id.clone())?;
            }
            MembershipState::Leave if !addressed_to_matrix_bot => {
                let msg = format!("User {} left room {}", event.user_id, event.room_id);
                debug!(self.logger, msg);

                self.handle_user_leave(event.room_id.clone())?;
            }
            _ => {
                let msg = format!(
                    "Skipping event, don't know how to handle membership state `{}` with state key `{}`",
                    event.content.membership,
                    event.state_key
                );
                debug!(self.logger, msg);
            }
        }

        Ok(())
    }

    /// Bridges a new room between Rocket.Chat and Matrix. It creates the room on the Matrix
    /// homeserver and manages the rooms virtual users.
    pub fn bridge_new_room(
        &self,
        rocketchat_api: Box<RocketchatApi>,
        rocketchat_server: &RocketchatServer,
        channel: &Channel,
        room_creator_id: UserId,
        invited_user_id: UserId,
    ) -> Result<RoomId> {
        debug!(self.logger, "Briding new room, Rocket.Chat channel: {}", channel.name.clone().unwrap_or_default());
        let matrix_room_id = self.create_room(
            channel.id.clone(),
            rocketchat_server.id.clone(),
            room_creator_id,
            invited_user_id,
            channel.name.clone(),
        )?;
        let matrix_room_alias_id = Room::build_room_alias_id(self.config, &rocketchat_server.id, &channel.id)?;
        self.matrix_api.put_canonical_room_alias(matrix_room_id.clone(), Some(matrix_room_alias_id))?;
        self.add_virtual_users_to_room(rocketchat_api, channel, rocketchat_server.id.clone(), matrix_room_id.clone())?;
        Ok(matrix_room_id)
    }

    /// Bridges a room that is already bridged (for other users) for a new user.
    pub fn bridge_existing_room(
        &self,
        matrix_room_id: RoomId,
        matrix_user_id: UserId,
        rocketchat_channel_name: String,
    ) -> Result<()> {
        debug!(self.logger, "Briding existing room, Rocket.Chat channel: {}", rocketchat_channel_name);
        if User::is_in_room(self.matrix_api, &matrix_user_id, matrix_room_id.clone())? {
            bail_error!(
                ErrorKind::RocketchatChannelAlreadyBridged(rocketchat_channel_name.clone()),
                t!(["errors", "rocketchat_channel_already_bridged"]).with_vars(vec![("channel_name", rocketchat_channel_name)])
            );
        }

        let bot_matrix_user_id = self.config.matrix_bot_user_id()?;
        self.matrix_api.invite(matrix_room_id, matrix_user_id, bot_matrix_user_id)
    }

    fn handle_bot_invite(&self, matrix_room_id: RoomId, invited_user_id: UserId) -> Result<()> {
        if !self.config.accept_remote_invites && self.is_remote_invite(&matrix_room_id)? {
            info!(
                self.logger,
                "Bot was invited by a user from another homeserver ({}). \
                  Ignoring the invite because remote invites are disabled.",
                &matrix_room_id
            );
            return Ok(());
        }

        self.matrix_api.join(matrix_room_id, invited_user_id)
    }

    fn handle_bot_join(&self, matrix_room_id: RoomId, matrix_bot_user_id: UserId) -> Result<()> {
        let is_admin_room = match Room::is_admin_room(self.matrix_api, self.config, matrix_room_id.clone()) {
            Ok(is_admin_room) => is_admin_room,
            Err(err) => {
                warn!(
                    self.logger,
                    "Could not determine if the room that the bot user was invited to is an admin room or not, bot is leaving"
                );
                self.leave_and_forget_room(matrix_room_id, matrix_bot_user_id)?;
                return Err(err);
            }
        };

        if is_admin_room {
            let user_ids: Vec<UserId> = Room::user_ids(self.matrix_api, matrix_room_id.clone())?
                .into_iter()
                .filter(|id| id != &matrix_bot_user_id)
                .collect();
            let invitation_submitter_id =
                user_ids.first().expect("There is always another user in the room, because this user invited the bot");

            if let Err(err) = self.setup_admin_room(
                matrix_room_id.clone(),
                matrix_bot_user_id.clone(),
                invitation_submitter_id,
            )
            {
                info!(
                    self.logger,
                    "Could not setup admin room {}, bot user will leave and forget the room",
                    matrix_room_id,
                );
                let error_notifier = ErrorNotifier {
                    config: self.config,
                    connection: self.connection,
                    logger: self.logger,
                    matrix_api: self.matrix_api,
                };
                error_notifier.send_message_to_user(&err, matrix_room_id.clone(), invitation_submitter_id)?;
                if let Err(err) = self.leave_and_forget_room(matrix_room_id, matrix_bot_user_id) {
                    log::log_error(self.logger, &err);
                }
                return Ok(());
            }
        }

        Ok(())
    }

    fn setup_admin_room(
        &self,
        matrix_room_id: RoomId,
        matrix_bot_user_id: UserId,
        invitation_submitter_id: &UserId,
    ) -> Result<()> {
        debug!(self.logger, "Setting up a new admin room with id {}", matrix_room_id);

        if !self.is_admin_room_valid(matrix_room_id.clone(), invitation_submitter_id, matrix_bot_user_id.clone())? {
            return Ok(());
        }

        self.connection.transaction(|| {
            let invitation_submitter =
                User::find_or_create_by_matrix_user_id(self.connection, invitation_submitter_id.clone())?;
            match CommandHandler::build_help_message(
                self.connection,
                self.matrix_api,
                self.config.as_url.clone(),
                matrix_room_id.clone(),
                &invitation_submitter,
            ) {
                Ok(body) => {
                    self.matrix_api.send_text_message_event(matrix_room_id.clone(), matrix_bot_user_id, body)?;
                }
                Err(err) => {
                    log::log_info(self.logger, &err);
                }
            }


            let room_name = t!(["defaults", "admin_room_display_name"]).l(&invitation_submitter.language);
            if let Err(err) = self.matrix_api.set_room_name(matrix_room_id, room_name) {
                log::log_info(self.logger, &err);
            }

            Ok(())
        })
    }

    fn handle_user_join(&self, matrix_room_id: RoomId) -> Result<()> {
        if Room::is_admin_room(self.matrix_api, self.config, matrix_room_id.clone())? &&
            !self.is_private_room(matrix_room_id.clone())?
        {
            info!(self.logger, "Another user join the admin room {}, bot user is leaving", matrix_room_id);
            let admin_room_language = self.admin_room_language(matrix_room_id.clone())?;
            let body = t!(["errors", "other_user_joined"]).l(&admin_room_language);
            let bot_matrix_user_id = self.config.matrix_bot_user_id()?;
            self.matrix_api.send_text_message_event(matrix_room_id.clone(), bot_matrix_user_id.clone(), body)?;
            self.leave_and_forget_room(matrix_room_id, bot_matrix_user_id)?;
        }
        Ok(())
    }

    fn handle_user_leave(&self, matrix_room_id: RoomId) -> Result<()> {
        if Room::is_admin_room(self.matrix_api, self.config, matrix_room_id.clone())? {
            let bot_matrix_user_id = self.config.matrix_bot_user_id()?;
            return self.leave_and_forget_room(matrix_room_id, bot_matrix_user_id);
        }

        Ok(())
    }

    fn leave_and_forget_room(&self, matrix_room_id: RoomId, matrix_user_id: UserId) -> Result<()> {
        self.matrix_api.leave_room(matrix_room_id.clone(), matrix_user_id)?;
        self.matrix_api.forget_room(matrix_room_id)
    }

    fn admin_room_language(&self, matrix_room_id: RoomId) -> Result<String> {
        let matrix_bot_user_id = self.config.matrix_bot_user_id()?;
        let user_ids: Vec<UserId> =
            Room::user_ids(self.matrix_api, matrix_room_id)?.into_iter().filter(|id| id != &matrix_bot_user_id).collect();
        let user_id = user_ids.first().expect("An admin room always contains another user");
        let user = User::find(self.connection, user_id)?;
        Ok(user.language.clone())
    }

    fn is_remote_invite(&self, matrix_room_id: &RoomId) -> Result<bool> {
        let hs_hostname =
            Host::parse(&self.config.hs_domain).chain_err(|| ErrorKind::InvalidHostname(self.config.hs_domain.clone()))?;
        Ok(matrix_room_id.hostname().ne(&hs_hostname))
    }

    fn is_admin_room_valid(
        &self,
        matrix_room_id: RoomId,
        invitation_submitter_id: &UserId,
        matrix_bot_user_id: UserId,
    ) -> Result<bool> {
        debug!(self.logger, "Validating admin room");
        let room_creator_id = self.matrix_api.get_room_creator(matrix_room_id.clone())?;
        if invitation_submitter_id != &room_creator_id {
            self.handle_admin_room_not_created_by_inviter(matrix_room_id.clone(), invitation_submitter_id, &room_creator_id)?;
            return Ok(false);
        }

        if !self.is_private_room(matrix_room_id.clone())? {
            self.handle_non_private_room(matrix_room_id, invitation_submitter_id, matrix_bot_user_id)?;
            return Ok(false);
        }

        Ok(true)
    }

    fn is_private_room(&self, matrix_room_id: RoomId) -> Result<bool> {
        Ok(Room::user_ids(self.matrix_api, matrix_room_id)?.len() <= 2)
    }

    fn handle_non_private_room(
        &self,
        matrix_room_id: RoomId,
        invitation_submitter_id: &UserId,
        matrix_bot_user_id: UserId,
    ) -> Result<()> {
        info!(self.logger, format!("Room {} has more then two members and cannot be used as admin room", matrix_room_id));
        let invitation_submitter = User::find_or_create_by_matrix_user_id(self.connection, invitation_submitter_id.clone())?;
        let body = t!(["errors", "too_many_members_in_room"]).l(&invitation_submitter.language);
        self.matrix_api.send_text_message_event(matrix_room_id.clone(), matrix_bot_user_id.clone(), body)?;
        if let Err(err) = self.leave_and_forget_room(matrix_room_id, matrix_bot_user_id) {
            log::log_error(self.logger, &err);
        }
        Ok(())
    }

    fn handle_admin_room_not_created_by_inviter(
        &self,
        matrix_room_id: RoomId,
        sender_id: &UserId,
        room_creator_id: &UserId,
    ) -> Result<()> {
        let body = t!(["admin_room", "only_room_creator_can_invite_bot_user"]).l(DEFAULT_LANGUAGE);
        self.matrix_api.send_text_message_event(matrix_room_id.clone(), self.config.matrix_bot_user_id()?, body)?;
        info!(
            self.logger,
            "The bot user was invited by the user {} but the room {} was created by {}, bot user is leaving",
            sender_id,
            &matrix_room_id,
            room_creator_id
        );
        let matrix_bot_user_id = self.config.matrix_bot_user_id()?;
        if let Err(err) = self.leave_and_forget_room(matrix_room_id, matrix_bot_user_id) {
            log::log_error(self.logger, &err);
        };
        Ok(())
    }

    /// Create a room on the Matrix homeserver with the power levels for a bridged room.
    pub fn create_room(
        &self,
        rocketchat_channel_id: String,
        rocketchat_server_id: String,
        room_creator_id: UserId,
        invited_user_id: UserId,
        room_display_name: Option<String>,
    ) -> Result<RoomId> {
        let matrix_room_alias_id = Room::build_room_alias_name(self.config, &rocketchat_server_id, &rocketchat_channel_id);
        let matrix_room_id =
            self.matrix_api.create_room(room_display_name.clone(), Some(matrix_room_alias_id), &room_creator_id)?;
        debug!(self.logger, "Successfully created room, matrix_room_id is {}", &matrix_room_id);
        self.matrix_api.set_default_powerlevels(matrix_room_id.clone(), room_creator_id.clone())?;
        debug!(self.logger, "Successfully set powerlevels for room {}", &matrix_room_id);
        self.matrix_api.invite(matrix_room_id.clone(), invited_user_id.clone(), room_creator_id.clone())?;
        debug!(self.logger, "{} successfully invited {} into room {}", &room_creator_id, &invited_user_id, &matrix_room_id);

        Ok(matrix_room_id)
    }

    /// Add all users that are in a Rocket.Chat room to the Matrix room.
    pub fn add_virtual_users_to_room(
        &self,
        rocketchat_api: Box<RocketchatApi>,
        channel: &Channel,
        rocketchat_server_id: String,
        matrix_room_id: RoomId,
    ) -> Result<()> {
        debug!(self.logger, "Starting to add virtual users to room {}", matrix_room_id);

        let virtual_user_handler = VirtualUserHandler {
            config: self.config,
            connection: self.connection,
            logger: self.logger,
            matrix_api: self.matrix_api,
        };

        //TODO: Check if a max number of users per channel has to be defined to avoid problems when
        //there are several thousand users in a channel.
        let bot_matrix_user_id = self.config.matrix_bot_user_id()?;
        for username in &channel.usernames {
            let rocketchat_user = rocketchat_api.users_info(username)?;
            let user_on_rocketchat_server =
                virtual_user_handler.find_or_register(rocketchat_server_id.clone(), rocketchat_user.id, username.to_string())?;
            virtual_user_handler.add_to_room(
                user_on_rocketchat_server.matrix_user_id,
                bot_matrix_user_id.clone(),
                matrix_room_id.clone(),
            )?;
        }

        debug!(self.logger, "Successfully added {} virtual users to room {}", channel.usernames.len(), matrix_room_id);

        Ok(())
    }
}
