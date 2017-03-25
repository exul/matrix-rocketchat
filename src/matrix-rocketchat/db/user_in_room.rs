use diesel;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use ruma_identifiers::{RoomId, UserId};

use errors::*;
use super::room::Room;
use super::schema::users_in_rooms;

/// Join table for users that participate in a room.
#[derive(Associations, Debug, Identifiable, Queryable)]
#[belongs_to(Room, foreign_key = "matrix_room_id")]
#[primary_key(matrix_user_id, matrix_room_id)]
#[table_name="users_in_rooms"]
pub struct UserInRoom {
    /// The users unique id on the Matrix server.
    pub matrix_user_id: UserId,
    /// The rooms unique id on the matrix server.
    pub matrix_room_id: RoomId,
    /// created timestamp
    pub created_at: String,
    /// updated timestamp
    pub updated_at: String,
}

/// A new `UserInRoom`, not yet saved.
#[derive(Insertable)]
#[table_name="users_in_rooms"]
pub struct NewUserInRoom {
    /// The users unique id on the Matrix server.
    pub matrix_user_id: UserId,
    /// The rooms unique id on the matrix server.
    pub matrix_room_id: RoomId,
}

impl UserInRoom {
    /// Insert a new `UserInRoom` into the database.
    pub fn insert(connection: &SqliteConnection, user_in_room: &NewUserInRoom) -> Result<UserInRoom> {
        diesel::insert(user_in_room).into(users_in_rooms::table).execute(connection).chain_err(|| ErrorKind::DBInsertError)?;
        UserInRoom::find(connection, &user_in_room.matrix_user_id, &user_in_room.matrix_room_id)
    }

    /// Find a `UserInRoom` by its matrix user ID and its matrix room ID, return an error if the user is not found
    pub fn find(connection: &SqliteConnection, matrix_user_id: &UserId, matrix_room_id: &RoomId) -> Result<UserInRoom> {
        let user_in_room = users_in_rooms::table.find((matrix_user_id, matrix_room_id))
            .first(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;
        Ok(user_in_room)
    }

    /// Find a `UserInRoom` by its matrix user ID and its matrix room ID.
    pub fn find_by_matrix_user_id_and_matrix_room_id(connection: &SqliteConnection,
                                                     matrix_user_id: &UserId,
                                                     matrix_room_id: &RoomId)
                                                     -> Result<Option<UserInRoom>> {
        let user_in_room = users_in_rooms::table.find((matrix_user_id, matrix_room_id))
            .load(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;
        Ok(user_in_room.into_iter().next())
    }

    /// Delete a user_in_room.
    pub fn delete(&self, connection: &SqliteConnection) -> Result<()> {
        diesel::delete(
            users_in_rooms::table.filter(
                users_in_rooms::matrix_user_id.eq(&self.matrix_user_id).and(users_in_rooms::matrix_room_id.eq(&self.matrix_room_id))
            )
        ).execute(connection).chain_err(|| ErrorKind::DBDeleteError)?;

        Ok(())

    }
}
