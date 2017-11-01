use diesel::sqlite::SqliteConnection;
use ruma_identifiers::{RoomId, UserId};
use slog::Logger;

use api::{MatrixApi, RocketchatApi};
use config::Config;
use errors::*;
use handlers::events::CommandHandler;
use models::{RocketchatServer, Room, UserOnRocketchatServer};

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

/// Credentials to perform a login on the Rocket.Chat server. The `user_id` is used to find
/// the corresponding matrix user.
#[derive(Serialize, Deserialize)]
pub struct Credentials {
    /// The users unique id on the Matrix homeserver
    pub user_id: UserId,
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
    pub fn call(&self, credentials: &Credentials, server: &RocketchatServer, admin_room_id: Option<RoomId>) -> Result<()> {
        let mut user_on_rocketchat_server =
            UserOnRocketchatServer::find(self.connection, &credentials.user_id, server.id.clone())?;
        let rocketchat_api = RocketchatApi::new(server.rocketchat_url.clone(), self.logger.clone())?;

        let (user_id, auth_token) = rocketchat_api.login(&credentials.rocketchat_username, &credentials.password)?;
        user_on_rocketchat_server.set_credentials(self.connection, Some(user_id.clone()), Some(auth_token.clone()))?;

        if let Some(room_id) = admin_room_id {
            let room = Room::new(self.config, self.logger, self.matrix_api, room_id.clone());
            let bot_user_id = self.config.matrix_bot_user_id()?;
            let as_url = self.config.as_url.clone();
            let message = CommandHandler::build_help_message(self.connection, &room, as_url, &credentials.user_id)?;
            self.matrix_api.send_text_message_event(room_id, bot_user_id, message)?;
        }

        Ok(info!(
            self.logger,
            "Successfully executed login command for user {} on Rocket.Chat server {}",
            credentials.rocketchat_username,
            server.rocketchat_url
        ))
    }
}
