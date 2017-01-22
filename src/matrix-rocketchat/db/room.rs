use diesel;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use ruma_identifiers::RoomId;

use errors::*;
use super::schema::{rooms, users, users_in_rooms};
use super::user::User;
use super::user_in_room::UserInRoom;

/// A room that is managed by the application service. This can be either a bridged room or an
/// admin room.
#[derive(Associations, Debug, Identifiable, Queryable)]
#[primary_key(matrix_room_id)]
#[table_name="rooms"]
pub struct Room {
    /// The rooms unique id on the matrix server.
    pub matrix_room_id: RoomId,
    /// The rooms display name.
    pub display_name: String,
    /// The Rocket.Chat server the room is connected to.
    pub rocketchat_server_id: Option<i32>,
    /// The rooms unique id on the Rocket.Chat server.
    pub rocketchat_room_id: Option<String>,
    /// A flag that indicates if the rooms is used as a admin room for the
    /// Rocket.Chat application service
    pub is_admin_room: bool,
    /// A flag to indicate if the room is bridged to Rocket.Chat
    pub is_bridged: bool,
    /// created timestamp
    pub created_at: String,
    /// updated timestamp
    pub updated_at: String,
}

/// A new `Room`, not yet saved.
#[derive(Insertable)]
#[table_name="rooms"]
pub struct NewRoom {
    /// The rooms unique id on the matrix server.
    pub matrix_room_id: RoomId,
    /// The rooms display name.
    pub display_name: String,
    /// The rooms unique id on the rocketchat server.
    pub rocketchat_room_id: Option<String>,
    /// A flag that indicates if the rooms is used as a admin room for the
    /// Rocket.Chat application service
    pub is_admin_room: bool,
    /// A flag to indicate if the room is bridged to Rocket.Chat
    pub is_bridged: bool,
}

impl Room {
    /// Insert a new `Room` into the database.
    pub fn insert(connection: &SqliteConnection, room: &NewRoom) -> Result<Room> {
        diesel::insert(room).into(rooms::table).execute(connection).chain_err(|| ErrorKind::DBInsertError)?;
        Room::find(connection, &room.matrix_room_id)
    }

    /// Find a `Room` by its matrix room ID. Returns an error if the room is not found.
    pub fn find(connection: &SqliteConnection, matrix_room_id: &RoomId) -> Result<Room> {
        rooms::table.find(matrix_room_id).first(connection).chain_err(|| ErrorKind::DBSelectError)
    }

    /// Find a `Room` by its matrix room ID. Returns `None`, if the room is not found.
    pub fn find_by_matrix_room_id(connection: &SqliteConnection, matrix_room_id: &RoomId) -> Result<Option<Room>> {
        let rooms = rooms::table.find(matrix_room_id).load(connection).chain_err(|| ErrorKind::DBSelectError)?;
        Ok(rooms.into_iter().next())
    }

    /// Indicate if the room is connected to a Rocket.Chat server
    pub fn is_connected(&self) -> bool {
        self.rocketchat_server_id.is_some()
    }

    /// Returns all `User`s in the room.
    pub fn users(&self, connection: &SqliteConnection) -> Result<Vec<User>> {
        let users: Vec<User>=
            users::table.filter(users::matrix_user_id.eq_any(UserInRoom::belonging_to(self).select(users_in_rooms::matrix_user_id)))
                .load(connection).chain_err(|| ErrorKind::DBSelectError)?;
        Ok(users)
    }

    /// Delete a room, this also deletes any users_in_rooms relations for that room.
    pub fn delete(&self, connection: &SqliteConnection) -> Result<()> {
        diesel::delete(UserInRoom::belonging_to(self)).execute(connection).chain_err(|| ErrorKind::DBDeleteError)?;
        diesel::delete(rooms::table.filter(rooms::matrix_room_id.eq(self.matrix_room_id.clone()))).execute(connection)
            .chain_err(|| ErrorKind::DBDeleteError)?;
        Ok(())
    }
}
