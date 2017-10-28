use std::convert::TryFrom;

use diesel::sqlite::SqliteConnection;
use ruma_identifiers::{RoomId, UserId};
use slog::Logger;

use api::MatrixApi;
use config::Config;
use db::Room;
use errors::*;

/// Provides helper methods to manage virtual users.
pub struct VirtualUserHandler<'a> {
    /// Application service configuration
    pub config: &'a Config,
    /// SQL database connection
    pub connection: &'a SqliteConnection,
    /// Logger context
    pub logger: &'a Logger,
    /// Matrix REST API
    pub matrix_api: &'a MatrixApi,
}

impl<'a> VirtualUserHandler<'a> {
    /// Add a virtual user to a Matrix room
    pub fn add_to_room(
        &self,
        receiver_matrix_user_id: UserId,
        sender_matrix_user_id: UserId,
        matrix_room_id: RoomId,
    ) -> Result<()> {
        let user_joined_already = Room::user_ids(self.matrix_api, matrix_room_id.clone(), Some(sender_matrix_user_id.clone()))?
            .iter()
            .any(|id| id == &receiver_matrix_user_id);

        if !user_joined_already {
            info!(self.logger, "Adding virtual user {} to room {}", receiver_matrix_user_id, matrix_room_id);
            self.matrix_api.invite(matrix_room_id.clone(), receiver_matrix_user_id.clone(), sender_matrix_user_id)?;

            if receiver_matrix_user_id.to_string().starts_with(&format!("@{}", self.config.sender_localpart)) {
                self.matrix_api.join(matrix_room_id, receiver_matrix_user_id)?;
            }
        }

        Ok(())
    }

    /// Register a virtual user on the Matrix server and assign it to a Rocket.Chat server.
    pub fn find_or_register(
        &self,
        rocketchat_server_id: String,
        rocketchat_user_id: String,
        rocketchat_user_name: String,
    ) -> Result<UserId> {
        let matrix_user_id = self.build_matrix_user_id(&rocketchat_user_id, &rocketchat_server_id)?;

        debug!(
            self.logger,
            "Trying to find user with Rocket.Chat user ID {}, Rocket.Chat Server ID {} and Matrix ID: {}",
            &rocketchat_user_id,
            &rocketchat_server_id,
            &matrix_user_id,
        );

        if self.matrix_api.get_display_name(matrix_user_id.clone())?.is_some() {
            debug!(self.logger, "Found user with matrix_id {}", matrix_user_id);
            return Ok(matrix_user_id);
        }

        debug!(self.logger, "No user found, registring a new user with the matrix ID {}", &matrix_user_id);
        self.matrix_api.register(matrix_user_id.localpart().to_string())?;
        debug!(self.logger, "Successfully registred user {}", &matrix_user_id);

        if let Err(err) = self.matrix_api.set_display_name(matrix_user_id.clone(), rocketchat_user_name.clone()) {
            warn!(
                self.logger,
                "Setting display name `{}`, for user `{}` failed with {}",
                &matrix_user_id,
                &rocketchat_user_name,
                err
            );
        }

        Ok(matrix_user_id)
    }

    /// Build the matrix user ID based on the Rocket.Chat user ID and the Rocket.Chat server ID.
    pub fn build_matrix_user_id(&self, rocketchat_user_id: &str, rocketchat_server_id: &str) -> Result<UserId> {
        let user_id_local_part = format!("{}_{}_{}", self.config.sender_localpart, rocketchat_server_id, rocketchat_user_id);
        let user_id = format!("@{}:{}", user_id_local_part, self.config.hs_domain);
        Ok(UserId::try_from(&user_id).chain_err(|| ErrorKind::InvalidUserId(user_id))?)
    }
}
