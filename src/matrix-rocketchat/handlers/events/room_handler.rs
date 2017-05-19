use std::convert::TryFrom;

use diesel::Connection;
use diesel::sqlite::SqliteConnection;
use iron::url::Host;
use ruma_events::room::member::{MemberEvent, MembershipState};
use ruma_identifiers::{RoomId, UserId};
use slog::Logger;

use api::MatrixApi;
use config::Config;
use db::{NewRoom, NewUserInRoom, Room, User, UserInRoom};
use errors::*;
use i18n::*;
use super::CommandHandler;

/// Handles room events
pub struct RoomHandler<'a> {
    config: &'a Config,
    connection: &'a SqliteConnection,
    logger: Logger,
    matrix_api: Box<MatrixApi>,
}

impl<'a> RoomHandler<'a> {
    /// Create a new `RoomHandler`.
    pub fn new(config: &'a Config,
               connection: &'a SqliteConnection,
               logger: Logger,
               matrix_api: Box<MatrixApi>)
               -> RoomHandler<'a> {
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
                let msg = format!("Bot {} joined room {}", matrix_bot_user_id, event.room_id);
                debug!(self.logger, msg);

                self.handle_bot_join(event.room_id.clone(), matrix_bot_user_id)?;
            }
            MembershipState::Join => {
                let msg = format!("User {} joined room {}", &state_key, &event.room_id);
                debug!(self.logger, msg);

                self.handle_user_join(state_key, event.room_id.clone())?;
            }
            MembershipState::Leave if !addressed_to_matrix_bot => {
                let msg = format!("User {} left room {}", event.user_id, event.room_id);
                debug!(self.logger, msg);

                self.handle_user_leave(&event.user_id, &event.room_id)?;
            }
            _ => {
                let msg = format!("Skipping event, don't know how to handle membership state `{}` with state key `{}`",
                                  event.content.membership,
                                  event.state_key);
                debug!(self.logger, msg);
            }
        }

        Ok(())
    }

    fn handle_bot_invite(&self, matrix_room_id: RoomId, invited_user_id: UserId, sender_id: UserId) -> Result<()> {
        if !self.config.accept_remote_invites && self.is_remote_invite(&matrix_room_id)? {
            info!(self.logger,
                  "Bot was invited by a user from another homeserver ({}). \
                  Ignoring the invite because remote invites are disabled.",
                  &matrix_room_id);
            return Ok(());
        }

        let room_creator_id = self.matrix_api.get_room_creator(matrix_room_id.clone())?;
        if sender_id != room_creator_id {
            self.handle_invalid_invite(matrix_room_id.clone(), invited_user_id.clone(), &sender_id, &room_creator_id)?;
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
                };
                Room::insert(self.connection, &room)?;
                let user_in_room = NewUserInRoom {
                    matrix_user_id: user.matrix_user_id,
                    matrix_room_id: room.matrix_room_id,
                };
                UserInRoom::insert(self.connection, &user_in_room)?;
                self.matrix_api.join(matrix_room_id, invited_user_id)
            })
            .map_err(Error::from)
    }

    fn handle_bot_join(&self, matrix_room_id: RoomId, matrix_bot_user_id: UserId) -> Result<()> {
        let room = Room::find(self.connection, &matrix_room_id)?;

        if room.is_admin_room {
            let users_in_room = room.users(self.connection)?;
            let invitation_submitter =
                users_in_room.first().expect("There is always a user in the room, because this user invited the bot");

            if !self.is_private_room(matrix_room_id.clone())? {
                return self.handle_non_private_room(&room, invitation_submitter, matrix_bot_user_id);
            }

            let body =
                CommandHandler::build_help_message(self.connection, self.config.as_url.clone(), &room, invitation_submitter)?;
            self.matrix_api.send_text_message_event(matrix_room_id.clone(), matrix_bot_user_id.clone(), body)?;

            let room_name = t!(["defaults", "admin_room_display_name"]).l(&invitation_submitter.language);
            self.matrix_api.set_room_name(matrix_room_id.clone(), room_name)?;
        }

        let user_in_room = NewUserInRoom {
            matrix_user_id: matrix_bot_user_id,
            matrix_room_id: room.matrix_room_id.clone(),
        };
        UserInRoom::insert(self.connection, &user_in_room)?;

        Ok(())
    }

    fn handle_user_join(&self, matrix_user_id: UserId, matrix_room_id: RoomId) -> Result<()> {
        let room = Room::find(self.connection, &matrix_room_id)?;

        if UserInRoom::find_by_matrix_user_id_and_matrix_room_id(self.connection, &matrix_user_id, &matrix_room_id)?.is_some() {
            let msg = format!("Skipping join event because the user {} is already in the room {} \
                              (join event triggered due to name change)",
                              &matrix_user_id,
                              &matrix_room_id);
            debug!(self.logger, msg);
            return Ok(());
        }

        if room.is_admin_room {
            info!(self.logger, "Another user join the admin room {}, bot user is leaving", matrix_room_id);
            let admin_room_language = self.admin_room_language(&room)?;
            let body = t!(["errors", "other_user_joined"]).l(&admin_room_language);
            self.matrix_api.send_text_message_event(matrix_room_id, self.config.matrix_bot_user_id()?, body)?;
            self.leave_and_forget_room(&room)?;
        } else {
            debug!(self.logger, format!("Adding user {} to room {}", &matrix_user_id, &matrix_room_id));
            let new_user_in_room = NewUserInRoom {
                matrix_user_id: matrix_user_id,
                matrix_room_id: matrix_room_id,
            };
            UserInRoom::insert(self.connection, &new_user_in_room)?;
        }
        Ok(())
    }

    fn handle_user_leave(&self, matrix_user_id: &UserId, matrix_room_id: &RoomId) -> Result<()> {
        let room = match Room::find_by_matrix_room_id(self.connection, matrix_room_id)? {
            Some(room) => room,
            None => return Ok(()),
        };

        if room.is_admin_room {
            return self.leave_and_forget_room(&room);
        }

        if let Some(user_in_room) =
            UserInRoom::find_by_matrix_user_id_and_matrix_room_id(self.connection, matrix_user_id, matrix_room_id)? {
            user_in_room.delete(self.connection)?;
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
        self.matrix_api.leave_room(room.matrix_room_id.clone())?;
        self.matrix_api.forget_room(room.matrix_room_id.clone())?;
        room.delete(self.connection)?;
        Ok(())
    }

    fn leave_and_forget_room(&self, room: &Room) -> Result<()> {
        self.matrix_api.leave_room(room.matrix_room_id.clone())?;
        self.matrix_api.forget_room(room.matrix_room_id.clone())?;
        room.delete(self.connection)
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
        let hs_hostname = Host::parse(&self.config.hs_domain)
            .chain_err(|| ErrorKind::InvalidHostname(self.config.hs_domain.clone()))?;
        Ok(matrix_room_id.hostname().ne(&hs_hostname))
    }

    fn handle_invalid_invite(&self,
                             matrix_room_id: RoomId,
                             invited_user_id: UserId,
                             sender_id: &UserId,
                             room_creator_id: &UserId)
                             -> Result<()> {
        self.matrix_api.join(matrix_room_id.clone(), invited_user_id)?;
        let body = t!(["admin_room", "only_room_creator_can_invite_bot_user"]).l(DEFAULT_LANGUAGE);
        self.matrix_api.send_text_message_event(matrix_room_id.clone(), self.config.matrix_bot_user_id()?, body)?;
        info!(self.logger,
              "The bot user was invited by the user {} but the room {} was created by {}, bot user is leaving",
              sender_id,
              &matrix_room_id,
              room_creator_id);
        self.matrix_api.leave_room(matrix_room_id.clone())?;
        self.matrix_api.forget_room(matrix_room_id)
    }
}
