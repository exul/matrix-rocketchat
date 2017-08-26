use diesel::sqlite::SqliteConnection;
use ruma_identifiers::{RoomId, UserId};
use slog::Logger;

use api::{MatrixApi, RocketchatApi};
use config::Config;
use db::{RocketchatServer, User, UserOnRocketchatServer};
use errors::*;
use handlers::events::CommandHandler;

/// Provides helper method to login a user on the Rocket.Chat server.
pub struct Login<'a> {
    /// Application service configuration
    pub config: &'a Config,
    /// SQL database connection
    pub connection: &'a SqliteConnection,
    /// Logger context
    pub logger: &'a Logger,
    /// Matrix REST API
    pub matrix_api: &'a MatrixApi,
}

/// Credentials to perform a login on the Rocket.Chat server. The `matrix_user_id` is used to find
/// the corresponding matrix user.
#[derive(Serialize, Deserialize)]
pub struct Credentials {
    /// The users unique id on the Matrix homeserver
    pub matrix_user_id: UserId,
    /// The username on the Rocket.Chat server
    pub rocketchat_username: String,
    /// The password on the Rocket.Chat server
    pub password: String,
    /// The URL of the Rocket.Chat server on which the user wants to login
    pub rocketchat_url: String,
}

impl<'a> Login<'a> {
    /// Perform a login request on the Rocket.Chat server.
    /// Stores the credentials if the login is successful.
    /// Returns an error if the login fails.
    pub fn call(
        &self,
        credentials: &Credentials,
        rocketchat_server: &RocketchatServer,
        admin_room_id: Option<RoomId>,
    ) -> Result<()> {
        let mut user_on_rocketchat_server =
            UserOnRocketchatServer::find(self.connection, &credentials.matrix_user_id, rocketchat_server.id.clone())?;
        let user = User::find(self.connection, &credentials.matrix_user_id)?;

        let mut rocketchat_api = RocketchatApi::new(rocketchat_server.rocketchat_url.clone(), self.logger.clone())?;

        let (rocketchat_user_id, rocketchat_auth_token) =
            rocketchat_api.login(&credentials.rocketchat_username, &credentials.password)?;
        user_on_rocketchat_server.set_credentials(
            self.connection,
            Some(rocketchat_user_id.clone()),
            Some(rocketchat_auth_token.clone()),
        )?;

        rocketchat_api = rocketchat_api.with_credentials(rocketchat_user_id, rocketchat_auth_token);
        let username = rocketchat_api.current_username()?;
        user_on_rocketchat_server.set_rocketchat_username(self.connection, Some(username.clone()))?;

        if let Some(matrix_room_id) = admin_room_id {
            let bot_matrix_user_id = self.config.matrix_bot_user_id()?;
            let message = CommandHandler::build_help_message(
                self.connection,
                self.matrix_api,
                self.config.as_url.clone(),
                matrix_room_id.clone(),
                &user,
            )?;
            self.matrix_api.send_text_message_event(matrix_room_id, bot_matrix_user_id, message)?;
        }

        Ok(info!(
            self.logger,
            "Successfully executed login command for user {} on Rocket.Chat server {}",
            credentials.rocketchat_username,
            rocketchat_server.rocketchat_url
        ))
    }
}
