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
#[derive(Associations, Identifiable, Queryable)]
#[primary_key(matrix_room_id)]
#[table_name="rooms"]
pub struct Room {
    /// The rooms unique id on the matrix server.
    pub matrix_room_id: String,
    /// The rooms display name.
    pub display_name: String,
    /// The rooms unique id on the rocketchat server.
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

/// A new room, not yet saved.
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
    /// Insert a new room into the database.
    pub fn insert(connection: &SqliteConnection, room: &NewRoom) -> Result<Room> {
        diesel::insert(room).into(rooms::table).execute(connection).chain_err(|| ErrorKind::DBInsertFailed)?;
        Room::find(connection, &room.matrix_room_id)
    }

    /// Find a room by its matrix room id. Returns an error if the room is not found.
    pub fn find(connection: &SqliteConnection, matrix_room_id: &RoomId) -> Result<Room> {
        let room = rooms::table.find(matrix_room_id).first(connection).chain_err(|| "Room not found")?;
        Ok(room)
    }

    /// Returns all users in the room.
    pub fn users(&self, connection: &SqliteConnection) -> Result<Vec<User>> {
        let users: Vec<User>=
            users::table.filter(users::matrix_user_id.eq_any(UserInRoom::belonging_to(self).select(users_in_rooms::matrix_user_id)))
                .load(connection)
                .unwrap();
        Ok(users)
    }
}
