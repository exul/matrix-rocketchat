use diesel;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use ruma_identifiers::UserId;

use errors::*;
use super::schema::users_on_rocketchat_servers;

/// A user on a Rocket.Chat server.
#[derive(Debug, Queryable)]
pub struct UserOnRocketchatServer {
    /// The users unique id on the Rocket.Chat server.
    pub matrix_user_id: UserId,
    /// The unique id for the Rocket.Chat server
    pub rocketchat_server_id: i32,
    /// The token to identify reuqests from the Rocket.Chat server
    pub rocketchat_auth_token: Option<String>,
    /// created timestamp
    pub created_at: String,
    /// updated timestamp
    pub updated_at: String,
}

/// A new `Room`, not yet saved.
#[derive(Insertable)]
#[table_name="users_on_rocketchat_servers"]
pub struct NewUserOnRocketchatServer {
    /// The users unique id on the Rocket.Chat server.
    pub matrix_user_id: UserId,
    /// The unique id for the Rocket.Chat server
    pub rocketchat_server_id: i32,
    /// The token to identify reuqests from the Rocket.Chat server
    pub rocketchat_auth_token: Option<String>,
}

impl UserOnRocketchatServer {
    /// Insert a new `UserOnRocketchatServer` into the database.
    pub fn insert(connection: &SqliteConnection,
                  user_on_rocketchat_server: &NewUserOnRocketchatServer)
                  -> Result<UserOnRocketchatServer> {
        diesel::insert(user_on_rocketchat_server).into(users_on_rocketchat_servers::table)
            .execute(connection)
            .chain_err(|| ErrorKind::DBInsertError)?;
        UserOnRocketchatServer::find(connection,
                                     &user_on_rocketchat_server.matrix_user_id,
                                     user_on_rocketchat_server.rocketchat_server_id)
    }

    /// Find a `UserOnRocketchatServer` by his matrix user ID and the Rocket.Chat server ID, return
    /// an error if the `UserOnRocketchatServer` is not found
    pub fn find(connection: &SqliteConnection,
                matrix_user_id: &UserId,
                rocketchat_server_id: i32)
                -> Result<UserOnRocketchatServer> {
        users_on_rocketchat_servers::table.find((matrix_user_id, rocketchat_server_id))
            .first(connection)
            .chain_err(|| ErrorKind::DBSelectError)
            .map_err(Error::from)
    }
}
