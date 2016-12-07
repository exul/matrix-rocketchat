use super::room::Room;
use super::schema::users_in_rooms;

/// Join table for users that participate in a room.
#[derive(Associations, Identifiable, Queryable)]
#[belongs_to(Room, foreign_key = "matrix_room_id")]
#[table_name="users_in_rooms"]
#[primary_key(matrix_user_id, matrix_room_id)]
pub struct UserInRoom {
    /// The users unique id on the Matrix server.
    matrix_user_id: String,
    /// The rooms unique id on the matrix server.
    matrix_room_id: String,
    /// created timestamp
    pub created_at: String,
    /// updated timestamp
    pub updated_at: String,
}
