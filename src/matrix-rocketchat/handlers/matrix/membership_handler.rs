use std::convert::TryFrom;

use diesel::sqlite::SqliteConnection;
use iron::url::Host;
use ruma_events::room::member::{MemberEvent, MembershipState};
use ruma_identifiers::UserId;
use slog::Logger;

use api::MatrixApi;
use config::Config;
use errors::*;
use handlers::ErrorNotifier;
use handlers::matrix::CommandHandler;
use i18n::*;
use log;
use models::Room;

/// Handles membership events for a specific room
pub struct MembershipHandler<'a> {
    config: &'a Config,
    conn: &'a SqliteConnection,
    logger: &'a Logger,
    matrix_api: &'a MatrixApi,
    room: &'a Room<'a>,
}

impl<'a> MembershipHandler<'a> {
    /// Create a new `MembershipHandler`.
    pub fn new(
        config: &'a Config,
        conn: &'a SqliteConnection,
        logger: &'a Logger,
        matrix_api: &'a MatrixApi,
        room: &'a Room<'a>,
    ) -> MembershipHandler<'a> {
        MembershipHandler {
            config: config,
            conn: conn,
            logger: logger,
            matrix_api: matrix_api,
            room: room,
        }
    }

    /// Handles room membership changes
    pub fn process(&self, event: &MemberEvent) -> Result<()> {
        let matrix_bot_user_id = self.config.matrix_bot_user_id()?;
        let state_key =
            UserId::try_from(event.state_key.as_ref()).chain_err(|| ErrorKind::InvalidUserId(event.state_key.clone()))?;
        let addressed_to_matrix_bot = state_key == matrix_bot_user_id;

        match event.content.membership {
            MembershipState::Invite if addressed_to_matrix_bot => {
                debug!(self.logger, "Bot `{}` got invite for room `{}`", matrix_bot_user_id, self.room.id);

                self.handle_bot_invite(matrix_bot_user_id)?;
            }
            MembershipState::Join if addressed_to_matrix_bot => {
                debug!(self.logger, "Received join event for bot user {} and room {}", matrix_bot_user_id, self.room.id);

                self.handle_bot_join(matrix_bot_user_id)?;
            }
            MembershipState::Join => {
                debug!(self.logger, "Received join event for user {} and room {}", &state_key, &event.room_id);

                self.handle_user_join()?;
            }
            MembershipState::Leave if !addressed_to_matrix_bot => {
                debug!(self.logger, "User {} left room {}", event.user_id, event.room_id);

                self.handle_user_leave()?;
            }
            _ => {
                let msg = format!(
                    "Skipping event, don't know how to handle membership state `{}` with state key `{}`",
                    event.content.membership, event.state_key
                );
                debug!(self.logger, "{}", msg);
            }
        }

        Ok(())
    }

    fn handle_bot_invite(&self, invited_user_id: UserId) -> Result<()> {
        if !self.config.accept_remote_invites && self.is_remote_invite()? {
            info!(
                self.logger,
                "Bot was invited by a user from another homeserver ({}). \
                 Ignoring the invite because remote invites are disabled.",
                &self.room.id
            );
            return Ok(());
        }

        self.matrix_api.join(self.room.id.clone(), invited_user_id)?;

        Ok(())
    }

    fn handle_bot_join(&self, matrix_bot_user_id: UserId) -> Result<()> {
        let is_admin_room = match self.room.is_admin_room() {
            Ok(is_admin_room) => is_admin_room,
            Err(err) => {
                warn!(
                    self.logger,
                    "Could not determine if the room that the bot user was invited to is an admin room or not, bot is leaving"
                );
                self.handle_admin_room_setup_error(&err, matrix_bot_user_id);
                return Err(err);
            }
        };

        if is_admin_room {
            self.setup_admin_room(matrix_bot_user_id.clone())?;
            return Ok(());
        }

        // leave direct message room, the bot only joined it to be able to read the room members
        if self.room.is_direct_message_room()? {
            self.matrix_api.leave_room(self.room.id.clone(), matrix_bot_user_id)?;
        }

        Ok(())
    }

    fn setup_admin_room(&self, matrix_bot_user_id: UserId) -> Result<()> {
        debug!(self.logger, "Setting up a new admin room with id {}", self.room.id);

        let room_creator_id = self.matrix_api.get_room_creator(self.room.id.clone())?;
        if let Err(err) = self.is_admin_room_valid() {
            info!(self.logger, "Admin room {} is not valid, bot will leave and forget the room", self.room.id);
            self.handle_admin_room_setup_error(&err, matrix_bot_user_id);
            return Ok(());
        }

        match CommandHandler::build_help_message(self.conn, self.room, self.config.as_url.clone(), &room_creator_id) {
            Ok(body) => {
                self.matrix_api.send_text_message_event(self.room.id.clone(), matrix_bot_user_id, body)?;
            }
            Err(err) => {
                log::log_info(self.logger, &err);
            }
        }

        let room_name = t!(["defaults", "admin_room_display_name"]).l(DEFAULT_LANGUAGE);
        if let Err(err) = self.matrix_api.set_room_name(self.room.id.clone(), room_name) {
            log::log_info(self.logger, &err);
        }

        Ok(())
    }

    fn handle_user_join(&self) -> Result<()> {
        if self.room.is_admin_room()? && !self.is_private_room()? {
            info!(self.logger, "Another user join the admin room {}, bot user is leaving", self.room.id);
            let bot_user_id = self.config.matrix_bot_user_id()?;
            let body = t!(["errors", "other_user_joined"]).l(DEFAULT_LANGUAGE);
            self.matrix_api.send_text_message_event(self.room.id.clone(), bot_user_id.clone(), body)?;
            self.room.forget(bot_user_id)?;
        }
        Ok(())
    }

    fn handle_user_leave(&self) -> Result<()> {
        if self.room.is_admin_room()? {
            let bot_user_id = self.config.matrix_bot_user_id()?;
            return self.room.forget(bot_user_id);
        }

        Ok(())
    }

    fn is_remote_invite(&self) -> Result<bool> {
        let hs_hostname =
            Host::parse(&self.config.hs_domain).chain_err(|| ErrorKind::InvalidHostname(self.config.hs_domain.clone()))?;
        Ok(self.room.id.hostname().ne(&hs_hostname))
    }

    fn is_admin_room_valid(&self) -> Result<()> {
        debug!(self.logger, "Validating admin room");

        if !self.is_private_room()? {
            bail_error!(ErrorKind::TooManyUsersInAdminRoom(self.room.id.clone()), t!(["errors", "too_many_members_in_room"]));
        }

        Ok(())
    }

    fn is_private_room(&self) -> Result<bool> {
        let room_members_events = self.matrix_api.get_room_members(self.room.id.clone(), None)?;
        let mut user_ids: Vec<&UserId> = room_members_events.iter().map(|m| &m.user_id).collect();
        user_ids.dedup();
        Ok(user_ids.len() <= 2)
    }

    fn handle_admin_room_setup_error(&self, err: &Error, matrix_bot_user_id: UserId) {
        let error_notifier = ErrorNotifier {
            config: self.config,
            logger: self.logger,
            matrix_api: self.matrix_api,
        };
        if let Err(err) = error_notifier.send_message_to_user(err, self.room.id.clone()) {
            log::log_error(self.logger, &err);
        }

        if let Err(err) = self.room.forget(matrix_bot_user_id) {
            log::log_error(self.logger, &err);
        }
    }
}
