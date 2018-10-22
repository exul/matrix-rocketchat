use std::convert::TryFrom;

use ruma_identifiers::UserId;
use slog::Logger;

use api::MatrixApi;
use config::Config;
use errors::*;

/// Provides helper methods to manage virtual users.
pub struct VirtualUser<'a> {
    /// Application service configuration
    config: &'a Config,
    /// Logger context
    logger: &'a Logger,
    /// API to call the Matrix homeserver
    matrix_api: &'a MatrixApi,
}

impl<'a> VirtualUser<'a> {
    /// Create a new virtual users model, to interact with Matrix virtual users.
    pub fn new(config: &'a Config, logger: &'a Logger, matrix_api: &'a MatrixApi) -> VirtualUser<'a> {
        VirtualUser { config, logger, matrix_api }
    }

    /// Register a virtual user on the Matrix server and assign it to a Rocket.Chat server.
    pub fn find_or_register(
        &self,
        rocketchat_server_id: &str,
        rocketchat_user_id: &str,
        rocketchat_user_name: &str,
    ) -> Result<UserId> {
        let user_id = self.build_user_id(rocketchat_user_id, rocketchat_server_id)?;

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

        if let Err(err) = self.matrix_api.set_display_name(user_id.clone(), rocketchat_user_name.to_string()) {
            warn!(self.logger, "Setting display name `{}`, for user `{}` failed with {}", &user_id, rocketchat_user_name, err);
        }

        Ok(user_id)
    }

    /// Build the matrix user ID based on the Rocket.Chat user ID and the Rocket.Chat server ID.
    pub fn build_user_id(&self, rocketchat_user_id: &str, rocketchat_server_id: &str) -> Result<UserId> {
        let user_id_local_part = format!("{}_{}_{}", self.config.sender_localpart, rocketchat_server_id, rocketchat_user_id);
        let user_id = format!("@{}:{}", user_id_local_part, self.config.hs_domain);
        Ok(UserId::try_from(user_id.as_ref()).chain_err(|| ErrorKind::InvalidUserId(user_id))?)
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
