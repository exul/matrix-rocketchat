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
    pub fn add_to_room(&self, receiver_user_id: UserId, sender_user_id: UserId, room_id: RoomId) -> Result<()> {
        let user_joined_already = Room::user_ids(self.matrix_api, room_id.clone(), Some(sender_user_id.clone()))?
            .iter()
            .any(|id| id == &receiver_user_id);

        if !user_joined_already {
            info!(self.logger, "Adding virtual user {} to room {}", receiver_user_id, room_id);
            self.matrix_api.invite(room_id.clone(), receiver_user_id.clone(), sender_user_id)?;

            if receiver_user_id.to_string().starts_with(&format!("@{}", self.config.sender_localpart)) {
                self.matrix_api.join(room_id, receiver_user_id)?;
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
        let user_id = self.build_user_id(&rocketchat_user_id, &rocketchat_server_id)?;

        debug!(
            self.logger,
            "Trying to find user with Rocket.Chat user ID {}, Rocket.Chat Server ID {} and Matrix ID: {}",
            &rocketchat_user_id,
            &rocketchat_server_id,
            &user_id,
        );

        if self.matrix_api.get_display_name(user_id.clone())?.is_some() {
            debug!(self.logger, "Found user with matrix_id {}", user_id);
            return Ok(user_id);
        }

        debug!(self.logger, "No user found, registring a new user with the matrix ID {}", &user_id);
        self.matrix_api.register(user_id.localpart().to_string())?;
        debug!(self.logger, "Successfully registred user {}", &user_id);

        if let Err(err) = self.matrix_api.set_display_name(user_id.clone(), rocketchat_user_name.clone()) {
            warn!(self.logger, "Setting display name `{}`, for user `{}` failed with {}", &user_id, &rocketchat_user_name, err);
        }

        Ok(user_id)
    }

    /// Build the matrix user ID based on the Rocket.Chat user ID and the Rocket.Chat server ID.
    pub fn build_user_id(&self, rocketchat_user_id: &str, rocketchat_server_id: &str) -> Result<UserId> {
        let user_id_local_part = format!("{}_{}_{}", self.config.sender_localpart, rocketchat_server_id, rocketchat_user_id);
        let user_id = format!("@{}:{}", user_id_local_part, self.config.hs_domain);
        Ok(UserId::try_from(&user_id).chain_err(|| ErrorKind::InvalidUserId(user_id))?)
    }

    /// Extracts the Rocket.Chat server and the users Rocket.Chat user ID from the Matrix User ID.
    pub fn rocketchat_server_and_user_id_from_matrix_id(user_id: &UserId) -> (String, String) {
        let user_local_part = user_id.localpart().to_owned();
        let id_parts: Vec<&str> = user_local_part.splitn(2, '_').collect();
        let server_and_user_id: Vec<&str> = id_parts.into_iter().nth(1).unwrap_or_default().splitn(2, '_').collect();
        let server_id = server_and_user_id.clone().into_iter().nth(0).unwrap_or_default().to_string();
        let rocketchat_user_id = server_and_user_id.clone().into_iter().nth(1).unwrap_or_default();

        (server_id, rocketchat_user_id.to_owned())
    }
}
