use std::convert::TryFrom;

use diesel::sqlite::SqliteConnection;
use ruma_events::room::member::{MemberEvent, MembershipState};
use ruma_identifiers::{RoomId, UserId};
use slog::Logger;

use api::MatrixApi;
use config::Config;
use errors::*;

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

                self.join_room(event.room_id.clone(), matrix_bot_user_id)?;
            }
            MembershipState::Join if addressed_to_matrix_bot => {
                let msg = format!("Bot {} entered room {}", matrix_bot_user_id, event.room_id);
                debug!(self.logger, msg);

                self.send_instructions(event.room_id.clone())?;
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

    fn join_room(&self, matrix_room_id: RoomId, matrix_user_id: UserId) -> Result<()> {
        self.matrix_api.join(matrix_room_id, matrix_user_id)
    }

    fn send_instructions(&self, matrix_room_id: RoomId) -> Result<()> {
        if !self.is_private_room(matrix_room_id.clone())? {
            info!(self.logger,
                  format!("Room {} has more then two members and cannot be used as admin room",
                          matrix_room_id));
            // TODO: Send message to user
            return Ok(());
        }

        // TODO: Send instructions

        Ok(())
    }

    fn is_private_room(&self, matrix_room_id: RoomId) -> Result<bool> {
        Ok(self.matrix_api.get_room_members(matrix_room_id)?.len() <= 2)
    }
}
