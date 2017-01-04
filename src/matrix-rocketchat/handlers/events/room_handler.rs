use std::convert::TryFrom;

use diesel::sqlite::SqliteConnection;
use ruma_events::room::member::{MemberEvent, MembershipState};
use ruma_identifiers::{RoomId, UserId};
use slog::Logger;

use api::MatrixApi;
use config::Config;
use db::room::{NewRoom, Room};
use db::user::User;
use db::user_in_room::{NewUserInRoom, UserInRoom};
use errors::*;
use i18n::*;

/// Handles room events
pub struct RoomHandler<'a> {
    config: &'a Config,
    connection: &'a SqliteConnection,
    logger: Logger,
    matrix_api: Box<MatrixApi>,
}

impl<'a> RoomHandler<'a> {
    /// Create a new `RoomHandler` with an SQLite connection
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
                let msg = format!("Bot `{}` got invitation for room `{}`", event.user_id, matrix_bot_user_id);
                debug!(self.logger, msg);

                self.handle_invite(event.room_id.clone(), matrix_bot_user_id, event.user_id.clone())?;
            }
            MembershipState::Join if addressed_to_matrix_bot => {
                let msg = format!("Bot {} joined room {}", matrix_bot_user_id, event.room_id);
                debug!(self.logger, msg);

                self.handle_join(event.room_id.clone())?;
            }
            _ => {
                let msg = format!("Skipping event, don't know how to handle membership state `{}` with state key `{}`",
                                  event.content.membership,
                                  event.state_key);
                info!(self.logger, msg);
            }
        }

        Ok(())
    }

    /// Process join invite for the bot user.
    pub fn handle_invite(&self, matrix_room_id: RoomId, invited_user_id: UserId, sender_id: UserId) -> Result<()> {
        let user = User::find_or_create_by_matrix_user_id(self.connection, sender_id)?;
        let display_name = t!(["defaults", "admin_room_display_name"]).l(&user.language);
        let room = NewRoom {
            matrix_room_id: matrix_room_id.clone(),
            display_name: display_name,
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
    }

    /// Process join events for the bot user.
    pub fn handle_join(&self, matrix_room_id: RoomId) -> Result<()> {
        if !self.is_private_room(matrix_room_id.clone())? {
            return self.handle_non_private_room(matrix_room_id);
        }

        // TODO: Add the bot user as member of the room

        // TODO: Send instructions

        Ok(())
    }

    fn is_private_room(&self, matrix_room_id: RoomId) -> Result<bool> {
        Ok(self.matrix_api.get_room_members(matrix_room_id)?.len() <= 2)
    }

    fn handle_non_private_room(&self, matrix_room_id: RoomId) -> Result<()> {
        info!(self.logger,
              format!("Room {} has more then two members and cannot be used as admin room",
                      matrix_room_id));
        let matrix_bot_user_id = self.config.matrix_bot_user_id()?;
        let room = Room::find(self.connection, &matrix_room_id)?;
        let users_in_room = room.users(self.connection)?;
        let invitation_submitter = users_in_room.first()
            .expect("There is always a user in the room, because this user invited the bot");
        let body = t!(["admin_room", "too_many_members_in_room"]).l(&invitation_submitter.language);
        self.matrix_api.send_text_message_event(matrix_room_id.clone(), matrix_bot_user_id, body)?;
        self.matrix_api.leave_room(matrix_room_id.clone())?;
        self.matrix_api.forget_room(matrix_room_id)?;
        room.delete(self.connection)?;
        Ok(())
    }
}
