use std::convert::TryFrom;

use diesel::Connection;
use diesel::sqlite::SqliteConnection;
use iron::url::Host;
use ruma_events::room::member::{MemberEvent, MembershipState};
use ruma_identifiers::{RoomId, UserId};
use slog::Logger;

use api::{MatrixApi, RocketchatApi};
use api::rocketchat::Channel;
use config::Config;
use db::{NewRoom, NewUserInRoom, RocketchatServer, Room, User, UserInRoom};
use errors::*;
use handlers::ErrorNotifier;
use handlers::rocketchat::VirtualUserHandler;
use i18n::*;
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
                let msg = format!("Bot `{}` got invitation for room `{}`", matrix_bot_user_id, event.room_id);
                debug!(self.logger, msg);

                self.handle_bot_invite(event.room_id.clone(), matrix_bot_user_id, event.user_id.clone())?;
            }
            MembershipState::Join if addressed_to_matrix_bot => {
                let msg = format!("Received join event for bot user {} and room {}", matrix_bot_user_id, event.room_id);
                debug!(self.logger, msg);

                self.handle_bot_join(event.room_id.clone(), matrix_bot_user_id)?;
            }
            MembershipState::Join => {
                let msg = format!("Received join event for user {} and room {}", &state_key, &event.room_id);
                debug!(self.logger, msg);

                self.handle_user_join(state_key, event.room_id.clone())?;
            }
            MembershipState::Leave if addressed_to_matrix_bot => {
                let msg = format!("Bot {} left room {}", event.user_id, event.room_id);
                debug!(self.logger, msg);

                self.handle_bot_leave(&event.room_id)?;
            }
            MembershipState::Leave if !addressed_to_matrix_bot => {
                let msg = format!("User {} left room {}", event.user_id, event.room_id);
                debug!(self.logger, msg);

                self.handle_user_leave(&event.user_id, &event.room_id)?;
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
    ) -> Result<Room> {
        let room = self.create_room(channel, rocketchat_server.id.clone(), room_creator_id, invited_user_id, false)?;
        self.add_virtual_users_to_room(rocketchat_api, channel, rocketchat_server.id.clone(), room.matrix_room_id.clone())?;
        Ok(room)
    }

    fn handle_bot_invite(&self, matrix_room_id: RoomId, invited_user_id: UserId, sender_id: UserId) -> Result<()> {
        if !self.config.accept_remote_invites && self.is_remote_invite(&matrix_room_id)? {
            info!(
                self.logger,
                "Bot was invited by a user from another homeserver ({}). \
                  Ignoring the invite because remote invites are disabled.",
                &matrix_room_id
            );
            return Ok(());
        }

        self.connection
            .transaction(|| {
                let user = User::find_or_create_by_matrix_user_id(self.connection, sender_id)?;
                let display_name = t!(["defaults", "admin_room_display_name"]).l(&user.language);
                let room = NewRoom {
                    matrix_room_id: matrix_room_id.clone(),
                    display_name: display_name,
                    rocketchat_server_id: None,
                    rocketchat_room_id: None,
                    is_admin_room: true,
                    is_bridged: false,
                    is_direct_message_room: false,
                };
                Room::insert(self.connection, &room)?;
                let user_in_room = NewUserInRoom {
                    matrix_user_id: user.matrix_user_id,
                    matrix_room_id: room.matrix_room_id,
                };
                UserInRoom::insert(self.connection, &user_in_room)
            })
            .map_err(Error::from)?;

        self.matrix_api.join(matrix_room_id, invited_user_id)
    }

    fn handle_bot_join(&self, matrix_room_id: RoomId, matrix_bot_user_id: UserId) -> Result<()> {
        let room = match self.insert_user_into_room(matrix_bot_user_id.clone(), matrix_room_id.clone())? {
            Some(room) => room,
            None => return Ok(()),
        };

        if room.is_admin_room {
            let users: Vec<User> =
                room.users(self.connection)?.into_iter().filter(|user| user.matrix_user_id != matrix_bot_user_id).collect();
            let invitation_submitter =
                users.first().expect("There is always another user in the room, because this user invited the bot");

            self.validate_admin_room(&room, invitation_submitter, matrix_bot_user_id.clone())?;

            let body =
                CommandHandler::build_help_message(self.connection, self.config.as_url.clone(), &room, invitation_submitter)?;
            self.matrix_api.send_text_message_event(matrix_room_id.clone(), matrix_bot_user_id, body)?;

            let room_name = t!(["defaults", "admin_room_display_name"]).l(&invitation_submitter.language);
            self.matrix_api.set_room_name(matrix_room_id.clone(), room_name)?;
        }

        Ok(())
    }

    fn handle_user_join(&self, matrix_user_id: UserId, matrix_room_id: RoomId) -> Result<()> {
        let room = match self.insert_user_into_room(matrix_user_id.clone(), matrix_room_id.clone())? {
            Some(room) => room,
            None => return Ok(()),
        };

        if room.is_admin_room && !room.users(self.connection)?.iter().any(|user| user.matrix_user_id == matrix_user_id) {
            info!(self.logger, "Another user join the admin room {}, bot user is leaving", matrix_room_id);
            let admin_room_language = self.admin_room_language(&room)?;
            let body = t!(["errors", "other_user_joined"]).l(&admin_room_language);
            self.matrix_api.send_text_message_event(matrix_room_id, self.config.matrix_bot_user_id()?, body)?;
            self.leave_and_forget_room(&room)?;
        }
        Ok(())
    }

    fn handle_bot_leave(&self, matrix_room_id: &RoomId) -> Result<()> {
        let room = Room::find(self.connection, matrix_room_id)?;
        room.delete(self.connection)
    }

    fn handle_user_leave(&self, matrix_user_id: &UserId, matrix_room_id: &RoomId) -> Result<()> {
        let mut room = match Room::find_by_matrix_room_id(self.connection, matrix_room_id)? {
            Some(room) => room,
            None => return Ok(()),
        };

        if room.is_admin_room {
            return self.leave_and_forget_room(&room);
        }

        if room.is_direct_message_room {
            room.set_is_bridged(self.connection, false)?;
        }

        if let Some(user_in_room) =
            UserInRoom::find_by_matrix_user_id_and_matrix_room_id(self.connection, matrix_user_id, matrix_room_id)?
        {
            user_in_room.delete(self.connection)?;
        }

        Ok(())
    }

    fn leave_and_forget_room(&self, room: &Room) -> Result<()> {
        self.matrix_api.leave_room(room.matrix_room_id.clone())?;
        self.matrix_api.forget_room(room.matrix_room_id.clone())
    }

    fn admin_room_language(&self, room: &Room) -> Result<String> {
        let matrix_bot_user_id = self.config.matrix_bot_user_id()?;
        let users: Vec<User> = room.non_virtual_users(self.connection)?
            .into_iter()
            .filter(|user| user.matrix_user_id != matrix_bot_user_id)
            .collect();
        let user = users.first().expect("An admin room always contains another user");
        Ok(user.language.clone())
    }

    fn is_remote_invite(&self, matrix_room_id: &RoomId) -> Result<bool> {
        let hs_hostname =
            Host::parse(&self.config.hs_domain).chain_err(|| ErrorKind::InvalidHostname(self.config.hs_domain.clone()))?;
        Ok(matrix_room_id.hostname().ne(&hs_hostname))
    }

    fn validate_admin_room(&self, room: &Room, invitation_submitter: &User, matrix_bot_user_id: UserId) -> Result<()> {
        match self.matrix_api.get_room_creator(room.matrix_room_id.clone()) {
            Ok(room_creator_id) => {
                if invitation_submitter.matrix_user_id != room_creator_id {
                    self.handle_admin_room_not_created_by_inviter(room,
                                                                  &invitation_submitter.matrix_user_id,
                                                                  &room_creator_id)?;
                    return Ok(());
                }
            }
            Err(err) => {
                let error_notifier = ErrorNotifier {
                    config: self.config,
                    connection: self.connection,
                    logger: self.logger,
                    matrix_api: self.matrix_api,
                };
                error_notifier.send_message_to_user(&err, room.matrix_room_id.clone(), &invitation_submitter.matrix_user_id)?;
                self.leave_and_forget_room(room)?;

                return Err(err);
            }
        };

        match self.is_private_room(room.matrix_room_id.clone()) {
            Ok(is_private_room) => {
                if !is_private_room {
                    return self.handle_non_private_room(room, invitation_submitter, matrix_bot_user_id);
                };
            }
            Err(err) => {
                let error_notifier = ErrorNotifier {
                    config: self.config,
                    connection: self.connection,
                    logger: self.logger,
                    matrix_api: self.matrix_api,
                };
                error_notifier.send_message_to_user(&err, room.matrix_room_id.clone(), &invitation_submitter.matrix_user_id)?;
                self.leave_and_forget_room(room)?;

                return Err(err);
            }
        }

        Ok(())
    }

    fn is_private_room(&self, matrix_room_id: RoomId) -> Result<bool> {
        Ok(self.matrix_api.get_room_members(matrix_room_id)?.len() <= 2)
    }

    fn handle_non_private_room(&self, room: &Room, invitation_submitter: &User, matrix_bot_user_id: UserId) -> Result<()> {
        info!(self.logger, format!("Room {} has more then two members and cannot be used as admin room", room.matrix_room_id));
        let body = t!(["errors", "too_many_members_in_room"]).l(&invitation_submitter.language);
        self.matrix_api.send_text_message_event(room.matrix_room_id.clone(), matrix_bot_user_id, body)?;
        self.leave_and_forget_room(room)?;
        Ok(())
    }

    fn handle_admin_room_not_created_by_inviter(
        &self,
        room: &Room,
        sender_id: &UserId,
        room_creator_id: &UserId,
    ) -> Result<()> {
        let body = t!(["admin_room", "only_room_creator_can_invite_bot_user"]).l(DEFAULT_LANGUAGE);
        self.matrix_api.send_text_message_event(room.matrix_room_id.clone(), self.config.matrix_bot_user_id()?, body)?;
        info!(
            self.logger,
            "The bot user was invited by the user {} but the room {} was created by {}, bot user is leaving",
            sender_id,
            &room.matrix_room_id,
            room_creator_id
        );
        self.leave_and_forget_room(room)
    }

    /// Create a room on the Matrix homeserver with the power levels for a bridged room.
    pub fn create_room(
        &self,
        channel: &Channel,
        rocketchat_server_id: String,
        room_creator_id: UserId,
        invited_user_id: UserId,
        is_direct_message_room: bool,
    ) -> Result<Room> {
        let bot_matrix_user_id = self.config.matrix_bot_user_id()?;
        let room_alias_name = format!("{}_{}_{}", self.config.sender_localpart, rocketchat_server_id, channel.id);
        let matrix_room_id = self.matrix_api.create_room(channel.name.clone(), Some(room_alias_name), &room_creator_id)?;
        self.matrix_api.set_default_powerlevels(matrix_room_id.clone(), bot_matrix_user_id.clone())?;
        self.matrix_api.invite(matrix_room_id.clone(), invited_user_id.clone())?;
        let new_room = NewRoom {
            matrix_room_id: matrix_room_id.clone(),
            display_name: channel.name.clone().unwrap_or_else(|| channel.id.clone()),
            rocketchat_server_id: Some(rocketchat_server_id),
            rocketchat_room_id: Some(channel.id.clone()),
            is_admin_room: false,
            is_bridged: true,
            is_direct_message_room: is_direct_message_room,
        };
        let room = Room::insert(self.connection, &new_room)?;

        // create the user for the room creator to avoid race conditions when the join event for
        // the room creator is triggered before the room is persisted in the database.
        let new_user_in_room = NewUserInRoom {
            matrix_user_id: room_creator_id,
            matrix_room_id: matrix_room_id,
        };
        UserInRoom::insert(self.connection, &new_user_in_room)?;

        Ok(room)
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
        for username in &channel.usernames {
            let rocketchat_user = rocketchat_api.users_info(username)?;
            let user_on_rocketchat_server =
                virtual_user_handler.find_or_register(rocketchat_server_id.clone(), rocketchat_user.id, username.to_string())?;
            virtual_user_handler.add_to_room(user_on_rocketchat_server.matrix_user_id, matrix_room_id.clone())?;
        }

        debug!(self.logger, "Successfully added {} virtual users to room {}", channel.usernames.len(), matrix_room_id);

        Ok(())
    }

    fn insert_user_into_room(&self, matrix_user_id: UserId, matrix_room_id: RoomId) -> Result<Option<Room>> {
        let room = match Room::find_by_matrix_room_id(self.connection, &matrix_room_id)? {
            Some(room) => room,
            None => {
                info!(
                    self.logger,
                    "Received join event for user {} and room {}, but the room wasn't persisted to the database. \
                      This is usually because the join event was finished before the room was stored in the database. \
                      That's not an issue, because room creators are saved to the database when the room is created.",
                    &matrix_user_id,
                    &matrix_room_id
                );
                return Ok(None);
            }
        };

        if UserInRoom::find_by_matrix_user_id_and_matrix_room_id(self.connection, &matrix_user_id, &matrix_room_id)?.is_some() {
            let msg = format!(
                "Skipping join event because the user {} is already in the room {} \
                              (join event triggered due to name change)",
                &matrix_user_id,
                &matrix_room_id
            );
            debug!(self.logger, msg);
            return Ok(Some(room));
        }

        let user_in_room = NewUserInRoom {
            matrix_user_id: matrix_user_id.clone(),
            matrix_room_id: matrix_room_id.clone(),
        };
        UserInRoom::insert(self.connection, &user_in_room)?;

        Ok(Some(room))
    }
}
