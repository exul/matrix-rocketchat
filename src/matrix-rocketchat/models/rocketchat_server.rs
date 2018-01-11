use diesel;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use iron::typemap::Key;
use ruma_identifiers::{RoomId, UserId};
use slog::Logger;

use api::{MatrixApi, RocketchatApi};
use config::Config;
use errors::*;
use handlers::matrix::CommandHandler;
use models::schema::{rocketchat_servers, users_on_rocketchat_servers};
use models::{Room, UserOnRocketchatServer};

/// A Rocket.Chat server.
#[derive(Associations, Debug, Identifiable, Queryable)]
#[table_name = "rocketchat_servers"]
pub struct RocketchatServer {
    /// The unique identifier for the Rocket.Chat server
    pub id: String,
    /// The URL to connect to the Rocket.Chat server
    pub rocketchat_url: String,
    /// The token to identify requests from the Rocket.Chat server
    pub rocketchat_token: Option<String>,
    /// created timestamp
    pub created_at: String,
    /// updated timestamp
    pub updated_at: String,
}

/// A new `Room`, not yet saved.
#[derive(Insertable)]
#[table_name = "rocketchat_servers"]
pub struct NewRocketchatServer<'a> {
    /// The unique identifier for the Rocket.Chat server
    pub id: &'a str,
    /// The URL to connect to the Rocket.Chat server
    pub rocketchat_url: &'a str,
    /// The token to identify reuqests from the Rocket.Chat server
    pub rocketchat_token: Option<&'a str>,
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

impl RocketchatServer {
    /// Insert a `RocketchatServer`.
    pub fn insert(connection: &SqliteConnection, new_rocketchat_server: &NewRocketchatServer) -> Result<RocketchatServer> {
        diesel::insert_into(rocketchat_servers::table)
            .values(new_rocketchat_server)
            .execute(connection)
            .chain_err(|| ErrorKind::DBInsertError)?;

        let server = RocketchatServer::find(connection, new_rocketchat_server.rocketchat_url)?;
        Ok(server)
    }

    /// Find a `RocketchatServer` by its URL, return an error if the `RocketchatServer` is not
    /// found.
    pub fn find(connection: &SqliteConnection, url: &str) -> Result<RocketchatServer> {
        let server = rocketchat_servers::table
            .filter(rocketchat_servers::rocketchat_url.eq(url))
            .first(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;
        Ok(server)
    }

    /// Find a `RocketchatServer` by its ID.
    pub fn find_by_id(connection: &SqliteConnection, id: &str) -> Result<Option<RocketchatServer>> {
        let rocketchat_servers = rocketchat_servers::table
            .filter(rocketchat_servers::id.eq(id))
            .load(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;
        Ok(rocketchat_servers.into_iter().next())
    }

    /// Find a `RocketchatServer` by its URL.
    pub fn find_by_url(connection: &SqliteConnection, url: &str) -> Result<Option<RocketchatServer>> {
        let rocketchat_servers = rocketchat_servers::table
            .filter(rocketchat_servers::rocketchat_url.eq(url))
            .load(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;
        Ok(rocketchat_servers.into_iter().next())
    }

    /// Find a `RocketchatServer` bit its token.
    pub fn find_by_token(connection: &SqliteConnection, token: &str) -> Result<Option<RocketchatServer>> {
        let rocketchat_servers = rocketchat_servers::table
            .filter(rocketchat_servers::rocketchat_token.eq(Some(token)))
            .load(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;
        Ok(rocketchat_servers.into_iter().next())
    }

    /// Get all connected servers.
    pub fn find_connected_servers(connection: &SqliteConnection) -> Result<Vec<RocketchatServer>> {
        let rocketchat_servers = rocketchat_servers::table
            .filter(rocketchat_servers::rocketchat_token.is_not_null())
            .load::<RocketchatServer>(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;
        Ok(rocketchat_servers)
    }

    /// Perform a login request on the Rocket.Chat server.
    /// Stores the credentials if the login is successful and an error if it failes.
    pub fn login(
        &self,
        config: &Config,
        connection: &SqliteConnection,
        logger: &Logger,
        matrix_api: &MatrixApi,
        credentials: &Credentials,
        admin_room_id: Option<RoomId>,
    ) -> Result<()> {
        let mut user_on_rocketchat_server = UserOnRocketchatServer::find(connection, &credentials.user_id, self.id.clone())?;
        let rocketchat_api = RocketchatApi::new(self.rocketchat_url.clone(), logger.clone())?;

        let (user_id, auth_token) = rocketchat_api.login(&credentials.rocketchat_username, &credentials.password)?;
        user_on_rocketchat_server.set_credentials(connection, Some(user_id.clone()), Some(auth_token.clone()))?;

        if let Some(room_id) = admin_room_id {
            let room = Room::new(config, logger, matrix_api, room_id.clone());
            let bot_user_id = config.matrix_bot_user_id()?;
            let as_url = config.as_url.clone();
            let message = CommandHandler::build_help_message(connection, &room, as_url, &credentials.user_id)?;
            matrix_api.send_text_message_event(room_id, bot_user_id, message)?;
        }

        Ok(info!(
            logger,
            "Successfully executed login command for user {} on Rocket.Chat server {}",
            credentials.rocketchat_username,
            self.rocketchat_url
        ))
    }

    /// Get all users that are connected to this Rocket.Chat server.
    pub fn logged_in_users_on_rocketchat_server(&self, connection: &SqliteConnection) -> Result<Vec<UserOnRocketchatServer>> {
        let users_on_rocketchat_server: Vec<UserOnRocketchatServer> = users_on_rocketchat_servers::table
            .filter(users_on_rocketchat_servers::rocketchat_server_id.eq(&self.id))
            .filter(users_on_rocketchat_servers::rocketchat_auth_token.is_not_null())
            .load(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;
        Ok(users_on_rocketchat_server)
    }
}

impl Key for RocketchatServer {
    type Value = RocketchatServer;
}
