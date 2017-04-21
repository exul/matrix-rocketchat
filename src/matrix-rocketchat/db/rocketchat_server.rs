use diesel;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use iron::typemap::Key;
use ruma_identifiers::UserId;

use i18n::*;
use errors::*;
use super::{Room, User, UserInRoom};
use super::schema::{rocketchat_servers, rooms, users_in_rooms};

/// A Rocket.Chat server.
#[derive(Associations, Debug, Identifiable, Queryable)]
#[table_name="rocketchat_servers"]
pub struct RocketchatServer {
    /// The unique id for the Rocket.Chat server
    pub id: i32,
    /// The URL to connect to the Rocket.Chat server
    pub rocketchat_url: String,
    /// The token to identify reuqests from the Rocket.Chat server
    pub rocketchat_token: Option<String>,
    /// created timestamp
    pub created_at: String,
    /// updated timestamp
    pub updated_at: String,
}

/// A new `Room`, not yet saved.
#[derive(Insertable)]
#[table_name="rocketchat_servers"]
pub struct NewRocketchatServer {
    /// The URL to connect to the Rocket.Chat server
    pub rocketchat_url: String,
    /// The token to identify reuqests from the Rocket.Chat server
    pub rocketchat_token: Option<String>,
}

impl RocketchatServer {
    /// Insert a `RocketchatServer`.
    pub fn insert(connection: &SqliteConnection, new_rocketchat_server: &NewRocketchatServer) -> Result<RocketchatServer> {
        diesel::insert(new_rocketchat_server).into(rocketchat_servers::table)
            .execute(connection)
            .chain_err(|| ErrorKind::DBInsertError)?;

        let rocketchat_server = RocketchatServer::find(connection, new_rocketchat_server.rocketchat_url.clone())?;
        Ok(rocketchat_server)
    }

    /// Find a `RocketchatServer` by its URL, return an error if the `RocketchatServer` is not
    /// found.
    pub fn find(connection: &SqliteConnection, url: String) -> Result<RocketchatServer> {
        let rocketchat_server = rocketchat_servers::table.filter(rocketchat_servers::rocketchat_url.eq(url))
            .first(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;
        Ok(rocketchat_server)
    }

    /// Find a `RocketchatServer` by its URL.
    pub fn find_by_url(connection: &SqliteConnection, url: String) -> Result<Option<RocketchatServer>> {
        let rocketchat_servers = rocketchat_servers::table.filter(rocketchat_servers::rocketchat_url.eq(url))
            .load(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;
        Ok(rocketchat_servers.into_iter().next())
    }

    /// Find a `RocketchatServer` bit its token.
    pub fn find_by_token(connection: &SqliteConnection, token: String) -> Result<Option<RocketchatServer>> {
        let rocketchat_servers = rocketchat_servers::table.filter(rocketchat_servers::rocketchat_token.eq(Some(token)))
            .load(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;
        Ok(rocketchat_servers.into_iter().next())
    }

    /// Get all connected servers.
    pub fn find_connected_servers(connection: &SqliteConnection) -> Result<Vec<RocketchatServer>> {
        let rocketchat_servers = rocketchat_servers::table.filter(rocketchat_servers::rocketchat_token.is_not_null())
            .load::<RocketchatServer>(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;
        Ok(rocketchat_servers)
    }

    /// Get the admin room for this Rocket.Chat server and a given user.
    pub fn admin_room_for_user(&self, connection: &SqliteConnection, matrix_user_id: &UserId) -> Result<Option<Room>> {
        let user = match User::find_by_matrix_user_id(connection, matrix_user_id)? {
            Some(user) => user,
            None => return Ok(None),
        };

        let rooms =
            rooms::table.filter(rooms::is_admin_room.eq(true)
                                    .and(rooms::matrix_room_id.eq_any(UserInRoom::belonging_to(&user)
                                                                          .select(users_in_rooms::matrix_room_id))))
                .load::<Room>(connection)
                .chain_err(|| ErrorKind::DBSelectError)?;
        Ok(rooms.into_iter().next())
    }

    /// Same as admin_room_for_user but returns an error if no room is found.
    pub fn admin_room_for_user_or_err(&self, connection: &SqliteConnection, matrix_user_id: &UserId) -> Result<Room> {
        match self.admin_room_for_user(connection, matrix_user_id)? {
            Some(room) => Ok(room),
            None => {
                Err(user_error!(ErrorKind::AdminRoomForRocketchatServerNotFound(self.rocketchat_url.clone()),
                                t!(["errors", "admin_room_for_rocketchat_server_not_found"])
                                    .with_vars(vec![("rocketchat_url", self.rocketchat_url.clone())])))
            }
        }
    }
}

impl Key for RocketchatServer {
    type Value = RocketchatServer;
}
