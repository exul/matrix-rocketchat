use errors::*;
use super::user::User;

/// A room that is managed by the application service. This can be either a bridged room or an
/// admin room.
pub struct Room {
    /// A flag that indicates if the rooms is used as a admin room for the
    /// Rocket.Chat application service
    pub is_admin_room: bool,
}

impl Room {
    /// Find a room by its matrix room id. Returns an error if the room is not found.
    pub fn find(matrix_room_id: &str) -> Result<Room> {
        let room = Room { is_admin_room: true };
        Ok(room)
    }

    /// Returns all users in the room.
    pub fn users(&self) -> Result<Vec<User>> {
        let users: Vec<User> = Vec::new();
        Ok(users)
    }
}
