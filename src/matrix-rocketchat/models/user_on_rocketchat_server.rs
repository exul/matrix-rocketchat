use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use diesel;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use ruma_identifiers::UserId;

use errors::*;
use models::schema::users_on_rocketchat_servers;

lazy_static! {
    /// User credentials
    static ref CREDENTIALS: RwLock<HashMap<(UserId, String), (String)>> = { RwLock::new(HashMap::new()) };
}

/// A user on a Rocket.Chat server.
#[derive(Associations, Debug, Identifiable, Queryable)]
#[primary_key(matrix_user_id, rocketchat_server_id)]
#[table_name = "users_on_rocketchat_servers"]
pub struct UserOnRocketchatServer {
    /// Time when the user sent the last message in seconds since UNIX_EPOCH
    pub last_message_sent: i64,
    /// The users unique id on the Rocket.Chat server.
    pub matrix_user_id: UserId,
    /// The unique id for the Rocket.Chat server
    pub rocketchat_server_id: String,
    /// The users unique id on the Rocket.Chat server.
    pub rocketchat_user_id: Option<String>,
    /// created timestamp
    pub created_at: String,
    /// updated timestamp
    pub updated_at: String,
}

/// A new `Room`, not yet saved.
#[derive(Insertable)]
#[table_name = "users_on_rocketchat_servers"]
pub struct NewUserOnRocketchatServer {
    /// The users unique id on the Rocket.Chat server.
    pub matrix_user_id: UserId,
    /// The unique id for the Rocket.Chat server
    pub rocketchat_server_id: String,
}

impl UserOnRocketchatServer {
    /// Insert or update a `UserOnRocketchatServer`.
    pub fn upsert(
        connection: &SqliteConnection,
        user_on_rocketchat_server: &NewUserOnRocketchatServer,
    ) -> Result<UserOnRocketchatServer> {
        let users_on_rocketchat_server: Vec<UserOnRocketchatServer> = users_on_rocketchat_servers::table
            .find((&user_on_rocketchat_server.matrix_user_id, &user_on_rocketchat_server.rocketchat_server_id))
            .load(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;

        if users_on_rocketchat_server.into_iter().next().is_none() {
            diesel::insert_into(users_on_rocketchat_servers::table)
                .values(user_on_rocketchat_server)
                .execute(connection)
                .chain_err(|| ErrorKind::DBInsertError)?;
        }

        UserOnRocketchatServer::find(
            connection,
            &user_on_rocketchat_server.matrix_user_id,
            user_on_rocketchat_server.rocketchat_server_id.clone(),
        )
    }

    /// Find a `UserOnRocketchatServer` by his matrix user ID and the Rocket.Chat server ID, return
    /// an error if the `UserOnRocketchatServer` is not found
    pub fn find(
        connection: &SqliteConnection,
        matrix_user_id: &UserId,
        rocketchat_server_id: String,
    ) -> Result<UserOnRocketchatServer> {
        let user_on_rocketchat_server = users_on_rocketchat_servers::table
            .find((matrix_user_id, rocketchat_server_id.clone()))
            .first(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;

        Ok(user_on_rocketchat_server)
    }

    /// Find a `UserOnRocketchatServer` by his matrix user ID and the Rocket.Chat server ID, return
    /// `None` if the `UserOnRocketchatServer` is not found
    pub fn find_by_matrix_user_id(
        connection: &SqliteConnection,
        matrix_user_id: &UserId,
        rocketchat_server_id: String,
    ) -> Result<Option<UserOnRocketchatServer>> {
        let user_on_rocketchat_server = users_on_rocketchat_servers::table
            .filter(
                users_on_rocketchat_servers::matrix_user_id
                    .eq(matrix_user_id)
                    .and(users_on_rocketchat_servers::rocketchat_server_id.eq(rocketchat_server_id.clone())),
            )
            .load(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;

        Ok(user_on_rocketchat_server.into_iter().next())
    }

    /// Find a list of `UserOnRocketchatServer` by their matrix user Id and the Rocket.Chat server ID.
    pub fn find_by_matrix_user_ids(
        connection: &SqliteConnection,
        matrix_user_ids: Vec<UserId>,
        rocketchat_server_id: String,
    ) -> Result<Vec<UserOnRocketchatServer>> {
        let users_on_rocketchat_server = users_on_rocketchat_servers::table
            .filter(
                users_on_rocketchat_servers::matrix_user_id
                    .eq_any(matrix_user_ids)
                    .and(users_on_rocketchat_servers::rocketchat_server_id.eq(rocketchat_server_id.clone())),
            )
            .load(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;

        Ok(users_on_rocketchat_server)
    }

    /// Find a `UserOnRocketchatServer` by his Rocket.Chat user ID. Returns `None`,
    /// if the `UserOnRocketchatServer` is not found.
    pub fn find_by_rocketchat_user_id(
        connection: &SqliteConnection,
        rocketchat_server_id: String,
        rocketchat_user_id: String,
    ) -> Result<Option<UserOnRocketchatServer>> {
        let users_on_rocketchat_servers: Vec<UserOnRocketchatServer> = users_on_rocketchat_servers::table
            .filter(
                users_on_rocketchat_servers::rocketchat_server_id
                    .eq(rocketchat_server_id.clone())
                    .and(users_on_rocketchat_servers::rocketchat_user_id.eq(rocketchat_user_id.clone())),
            )
            .load(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;

        Ok(users_on_rocketchat_servers.into_iter().next())
    }

    /// Set the users credentials.
    pub fn set_credentials(
        &mut self,
        connection: &SqliteConnection,
        rocketchat_user_id: Option<String>,
        rocketchat_auth_token: Option<String>,
    ) -> Result<()> {
        let mut credentials = CREDENTIALS.write().unwrap_or_else(|poisoned_lock| poisoned_lock.into_inner());

        match rocketchat_auth_token {
            Some(rocketchat_auth_token) => {
                credentials
                    .insert((self.matrix_user_id.clone(), self.rocketchat_server_id.clone()), rocketchat_auth_token.clone());
            }
            None => {
                credentials.remove(&(self.matrix_user_id.clone(), self.rocketchat_server_id.clone()));
            }
        }

        self.rocketchat_user_id = rocketchat_user_id.clone();

        diesel::update(users_on_rocketchat_servers::table.find((&self.matrix_user_id, self.rocketchat_server_id.clone())))
            .set(users_on_rocketchat_servers::rocketchat_user_id.eq(rocketchat_user_id))
            .execute(connection)
            .chain_err(|| ErrorKind::DBUpdateError)?;

        Ok(())
    }

    /// Returns the users Rocket.Chat authentication token.
    pub fn rocketchat_auth_token(&self) -> Option<String> {
        let credentials = CREDENTIALS.read().unwrap_or_else(|poisoned_lock| poisoned_lock.into_inner());
        credentials.get(&(self.matrix_user_id.clone(), self.rocketchat_server_id.clone())).map(|s| s.to_string())
    }

    /// Update last message sent.
    pub fn set_last_message_sent(&mut self, connection: &SqliteConnection) -> Result<()> {
        let last_message_sent =
            SystemTime::now().duration_since(UNIX_EPOCH).chain_err(|| ErrorKind::InternalServerError)?.as_secs() as i64;
        self.last_message_sent = last_message_sent;
        diesel::update(users_on_rocketchat_servers::table.find((&self.matrix_user_id, self.rocketchat_server_id.clone())))
            .set(users_on_rocketchat_servers::last_message_sent.eq(last_message_sent))
            .execute(connection)
            .chain_err(|| ErrorKind::DBUpdateError)?;
        Ok(())
    }

    /// Returns true if the user is logged in on the Rocket.Chat server via the application
    /// serivce, and false otherwise.
    pub fn is_logged_in(&self) -> bool {
        let credentials = CREDENTIALS.read().unwrap_or_else(|poisoned_lock| poisoned_lock.into_inner());
        let auth_token: Option<&String> = credentials.get(&(self.matrix_user_id.clone(), self.rocketchat_server_id.clone()));
        auth_token.is_some()
    }
}
