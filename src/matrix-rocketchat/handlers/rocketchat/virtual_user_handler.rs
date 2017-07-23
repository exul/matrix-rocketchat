use std::convert::TryFrom;

use diesel::sqlite::SqliteConnection;
use ruma_identifiers::{RoomId, UserId};
use slog::Logger;

use api::MatrixApi;
use config::Config;
use db::{NewUser, NewUserOnRocketchatServer, User, UserOnRocketchatServer};
use errors::*;
use i18n::*;

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
        info!(self.logger, "Adding virtual user {} to room {}", receiver_matrix_user_id, matrix_room_id);
        self.matrix_api.invite(matrix_room_id.clone(), receiver_matrix_user_id.clone(), sender_matrix_user_id)?;
        self.matrix_api.join(matrix_room_id, receiver_matrix_user_id)?;
        Ok(())
    }

    /// Register a virtual user on the Matrix server and assign it to a Rocket.Chat server.
    pub fn find_or_register(
        &self,
        rocketchat_server_id: String,
        rocketchat_user_id: String,
        rocketchat_user_name: String,
    ) -> Result<UserOnRocketchatServer> {
        debug!(
            self.logger,
            "Trying to find user with Rocket.Chat user ID {} and Rocket.Chat Server ID {}",
            &rocketchat_user_id,
            &rocketchat_server_id
        );
        let user_id_local_part =
            format!("{}_{}_{}", self.config.sender_localpart, &rocketchat_user_id, rocketchat_server_id.clone());
        let user_id = format!("@{}:{}", user_id_local_part, self.config.hs_domain);
        let matrix_user_id = UserId::try_from(&user_id).chain_err(|| ErrorKind::InvalidUserId(user_id))?;

        if let Some(user_on_rocketchat_server) =
            UserOnRocketchatServer::find_by_rocketchat_user_id(
                self.connection,
                rocketchat_server_id.clone(),
                rocketchat_user_id.clone(),
                true,
            )?
        {
            debug!(self.logger, "Found user with matrix_id {}", user_on_rocketchat_server.matrix_user_id);
            return Ok(user_on_rocketchat_server);
        }

        debug!(self.logger, "No user found, registring a new user with the matrix ID {}", &matrix_user_id);
        let new_user = NewUser {
            language: DEFAULT_LANGUAGE,
            matrix_user_id: matrix_user_id.clone(),
        };
        User::insert(self.connection, &new_user)?;

        let new_user_on_rocketchat_server = NewUserOnRocketchatServer {
            is_virtual_user: true,
            matrix_user_id: matrix_user_id,
            rocketchat_auth_token: None,
            rocketchat_server_id: rocketchat_server_id,
            rocketchat_user_id: Some(rocketchat_user_id.clone()),
            rocketchat_username: Some(rocketchat_user_name.clone()),
        };
        let user_on_rocketchat_server = UserOnRocketchatServer::upsert(self.connection, &new_user_on_rocketchat_server)?;

        self.matrix_api.register(user_id_local_part.clone())?;
        debug!(self.logger, "Successfully registred user {}", user_on_rocketchat_server.matrix_user_id);
        if let Err(err) = self.matrix_api.set_display_name(
            user_on_rocketchat_server.matrix_user_id.clone(),
            rocketchat_user_name.clone(),
        )
        {
            info!(
                self.logger,
                format!(
                    "Setting display name `{}`, for user `{}` failed with {}",
                    &user_on_rocketchat_server.matrix_user_id,
                    &rocketchat_user_name,
                    err
                )
            );
        }

        Ok(user_on_rocketchat_server)
    }
}
