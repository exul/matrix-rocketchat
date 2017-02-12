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
    /// The users unique id on the Rocket.Chat server.
    pub rocketchat_user_id: Option<String>,
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
    /// The users unique id on the Rocket.Chat server.
    pub rocketchat_user_id: Option<String>,
    /// The token to identify reuqests from the Rocket.Chat server
    pub rocketchat_auth_token: Option<String>,
}

impl UserOnRocketchatServer {
    /// Insert or update a `UserOnRocketchatServer`.
    pub fn upsert(connection: &SqliteConnection,
                  user_on_rocketchat_server: &NewUserOnRocketchatServer)
                  -> Result<UserOnRocketchatServer> {
        let users_on_rocketchat_server: Vec<UserOnRocketchatServer> = users_on_rocketchat_servers::table
            .find((&user_on_rocketchat_server.matrix_user_id, &user_on_rocketchat_server.rocketchat_server_id))
            .load(connection).chain_err(|| ErrorKind::DBSelectError)?;

        match users_on_rocketchat_server.into_iter().next() {
            Some(existing_user_on_rocketchat_server) => {
                existing_user_on_rocketchat_server.set_credentials(connection,
                                     user_on_rocketchat_server.rocketchat_user_id.clone(),
                                     user_on_rocketchat_server.rocketchat_auth_token.clone())?;
            }
            None => {
                diesel::insert(user_on_rocketchat_server).into(users_on_rocketchat_servers::table)
                    .execute(connection)
                    .chain_err(|| ErrorKind::DBInsertError)?;
            }
        }


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
        let user_on_rocketchat_server = users_on_rocketchat_servers::table.find((matrix_user_id, rocketchat_server_id))
            .first(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;
        Ok(user_on_rocketchat_server)
    }

    /// Update the users credentials.
    pub fn set_credentials(&self,
                           connection: &SqliteConnection,
                           rocketchat_user_id: Option<String>,
                           rocketchat_auth_token: Option<String>)
                           -> Result<()> {
        diesel::update(users_on_rocketchat_servers::table.find((&self.matrix_user_id, self.rocketchat_server_id)))
                .set((users_on_rocketchat_servers::rocketchat_user_id.eq(rocketchat_user_id),
                      users_on_rocketchat_servers::rocketchat_auth_token.eq(rocketchat_auth_token))).execute(connection)
            .chain_err(|| ErrorKind::DBUpdateError)?;
        Ok(())
    }
}
