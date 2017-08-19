use diesel;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use iron::typemap::Key;

use errors::*;
use super::UserOnRocketchatServer;
use super::schema::{rocketchat_servers, users_on_rocketchat_servers};

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
pub struct NewRocketchatServer {
    /// The unique identifier for the Rocket.Chat server
    pub id: String,
    /// The URL to connect to the Rocket.Chat server
    pub rocketchat_url: String,
    /// The token to identify reuqests from the Rocket.Chat server
    pub rocketchat_token: Option<String>,
}

impl RocketchatServer {
    /// Insert a `RocketchatServer`.
    pub fn insert(connection: &SqliteConnection, new_rocketchat_server: &NewRocketchatServer) -> Result<RocketchatServer> {
        diesel::insert(new_rocketchat_server).into(rocketchat_servers::table).execute(connection).chain_err(
            || ErrorKind::DBInsertError,
        )?;

        let rocketchat_server = RocketchatServer::find(connection, new_rocketchat_server.rocketchat_url.clone())?;
        Ok(rocketchat_server)
    }

    /// Find a `RocketchatServer` by its URL, return an error if the `RocketchatServer` is not
    /// found.
    pub fn find(connection: &SqliteConnection, url: String) -> Result<RocketchatServer> {
        let rocketchat_server = rocketchat_servers::table
            .filter(rocketchat_servers::rocketchat_url.eq(url))
            .first(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;
        Ok(rocketchat_server)
    }

    /// Find a `RocketchatServer` by its ID.
    pub fn find_by_id(connection: &SqliteConnection, id: &str) -> Result<Option<RocketchatServer>> {
        let rocketchat_servers =
            rocketchat_servers::table.filter(rocketchat_servers::id.eq(id)).load(connection).chain_err(
                || ErrorKind::DBSelectError,
            )?;
        Ok(rocketchat_servers.into_iter().next())
    }

    /// Find a `RocketchatServer` by its URL.
    pub fn find_by_url(connection: &SqliteConnection, url: String) -> Result<Option<RocketchatServer>> {
        let rocketchat_servers = rocketchat_servers::table
            .filter(rocketchat_servers::rocketchat_url.eq(url))
            .load(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;
        Ok(rocketchat_servers.into_iter().next())
    }

    /// Find a `RocketchatServer` bit its token.
    pub fn find_by_token(connection: &SqliteConnection, token: String) -> Result<Option<RocketchatServer>> {
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

    /// Get all users that are connected to this Rocket.Chat server.
    pub fn logged_in_users_on_rocketchat_server(&self, connection: &SqliteConnection) -> Result<Vec<UserOnRocketchatServer>> {
        let users_on_rocketchat_server: Vec<UserOnRocketchatServer> = UserOnRocketchatServer::belonging_to(self)
            .filter(users_on_rocketchat_servers::rocketchat_auth_token.is_not_null())
            .load(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;
        Ok(users_on_rocketchat_server)
    }
}

impl Key for RocketchatServer {
    type Value = RocketchatServer;
}
