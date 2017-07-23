use diesel;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use ruma_events::room::member::MembershipState;
use ruma_identifiers::{RoomId, UserId};

use api::MatrixApi;
use errors::*;
use super::schema::{rocketchat_servers, rooms};
use super::RocketchatServer;

/// A room that is managed by the application service. This can be either a bridged room or an
/// admin room.
#[derive(Associations, Debug, Identifiable, Queryable)]
#[belongs_to(RocketchatServer, foreign_key = "rocketchat_server_id")]
#[primary_key(matrix_room_id)]
#[table_name = "rooms"]
pub struct Room {
    /// The rooms unique id on the matrix server.
    pub matrix_room_id: RoomId,
    /// The rooms display name.
    pub display_name: String,
    /// The Rocket.Chat server the room is connected to.
    pub rocketchat_server_id: Option<String>,
    /// The rooms unique id on the Rocket.Chat server.
    pub rocketchat_room_id: Option<String>,
    /// A flag that indicates if the rooms is used as a admin room for the
    /// Rocket.Chat application service
    pub is_admin_room: bool,
    /// A flag to indicate if the room is bridged to Rocket.Chat
    pub is_bridged: bool,
    /// A flag to indicate if the room is used to send direct messages between two users.
    pub is_direct_message_room: bool,
    /// created timestamp
    pub created_at: String,
    /// updated timestamp
    pub updated_at: String,
}

/// A new `Room`, not yet saved.
#[derive(Debug, Insertable)]
#[table_name = "rooms"]
pub struct NewRoom {
    /// The rooms unique id on the matrix server.
    pub matrix_room_id: RoomId,
    /// The rooms display name.
    pub display_name: String,
    /// The Rocket.Chat server the room is connected to.
    pub rocketchat_server_id: Option<String>,
    /// The rooms unique id on the rocketchat server.
    pub rocketchat_room_id: Option<String>,
    /// A flag that indicates if the rooms is used as a admin room for the
    /// Rocket.Chat application service
    pub is_admin_room: bool,
    /// A flag to indicate if the room is bridged to Rocket.Chat
    pub is_bridged: bool,
    /// A flag to indicate if the room is used to send direct messages between two users.
    pub is_direct_message_room: bool,
}

impl Room {
    /// Insert a new `Room` into the database.
    pub fn insert(connection: &SqliteConnection, room: &NewRoom) -> Result<Room> {
        diesel::insert(room).into(rooms::table).execute(connection).chain_err(|| ErrorKind::DBInsertError)?;
        Room::find(connection, &room.matrix_room_id)
    }

    /// Find a `Room` by its matrix room ID. Returns an error if the room is not found.
    pub fn find(connection: &SqliteConnection, matrix_room_id: &RoomId) -> Result<Room> {
        rooms::table.find(matrix_room_id).first(connection).chain_err(|| ErrorKind::DBSelectError).map_err(Error::from)
    }

    /// Find a `Room` by its matrix room ID. Returns `None`, if the room is not found.
    pub fn find_by_matrix_room_id(connection: &SqliteConnection, matrix_room_id: &RoomId) -> Result<Option<Room>> {
        let rooms = rooms::table.find(matrix_room_id).load(connection).chain_err(|| ErrorKind::DBSelectError)?;
        Ok(rooms.into_iter().next())
    }

    /// Find a `Room` by its Rocket.Chat room ID. It also requires the server id, because the
    /// Rocket.Chat room ID might not be unique across servers.
    /// Returns `None`, if the room is not found.
    pub fn find_by_rocketchat_room_id(
        connection: &SqliteConnection,
        rocketchat_server_id: String,
        rocketchat_room_id: String,
    ) -> Result<Option<Room>> {
        let rooms = rooms::table
            .filter(rooms::rocketchat_server_id.eq(rocketchat_server_id).and(rooms::rocketchat_room_id.eq(rocketchat_room_id)))
            .load(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;
        Ok(rooms.into_iter().next())
    }

    /// Find a `Room` by its display name. It also requires the server id, because the
    /// display name might not be unique across servers.
    /// Returns `None`, if the room is not found.
    pub fn find_by_display_name(
        connection: &SqliteConnection,
        rocketchat_server_id: String,
        display_name: String,
    ) -> Result<Option<Room>> {
        let rooms = rooms::table
            .filter(rooms::rocketchat_server_id.eq(rocketchat_server_id).and(rooms::display_name.eq(display_name)))
            .load(connection)
            .chain_err(|| ErrorKind::DBSelectError)?;
        Ok(rooms.into_iter().next())
    }

    /// Indicates if the room is bridged for a given user.
    pub fn is_bridged_for_user(
        connection: &SqliteConnection,
        matrix_api: &MatrixApi,
        rocketchat_server_id: String,
        rocketchat_room_id: String,
        matrix_user_id: &UserId,
    ) -> Result<bool> {
        if let Some(room) = Room::find_by_rocketchat_room_id(connection, rocketchat_server_id, rocketchat_room_id)? {
            Ok(room.is_bridged && room.user_ids(matrix_api)?.iter().any(|id| id == matrix_user_id))
        } else {
            Ok(false)
        }
    }

    /// Indicates if a room is bridged.
    pub fn is_bridged(connection: &SqliteConnection, rocketchat_server_id: String, rocketchat_room_id: String) -> Result<bool> {
        match Room::find_by_rocketchat_room_id(connection, rocketchat_server_id, rocketchat_room_id)? {
            Some(room) => Ok(room.is_bridged),
            None => Ok(false),
        }
    }

    /// Get the Rocket.Chat server this room is connected to, if any.
    pub fn rocketchat_server(&self, connection: &SqliteConnection) -> Result<Option<RocketchatServer>> {
        match self.rocketchat_server_id.clone() {
            Some(rocketchat_server_id) => {
                let rocketchat_server = rocketchat_servers::table
                    .find(rocketchat_server_id)
                    .first::<RocketchatServer>(connection)
                    .chain_err(|| ErrorKind::DBSelectError)?;
                Ok(Some(rocketchat_server))
            }
            None => Ok(None),
        }
    }

    /// Get the URL of the connected Rocket.Chat server, if any.
    pub fn rocketchat_url(&self, connection: &SqliteConnection) -> Result<Option<String>> {
        match self.rocketchat_server(connection)? {
            Some(rocketchat_server) => Ok(Some(rocketchat_server.rocketchat_url)),
            None => Ok(None),
        }
    }

    /// Set the Rocket.Chat id for a room.
    pub fn set_rocketchat_server_id(&mut self, connection: &SqliteConnection, rocketchat_server_id: String) -> Result<()> {
        self.rocketchat_server_id = Some(rocketchat_server_id.clone());
        diesel::update(rooms::table.find(&self.matrix_room_id))
            .set(rooms::rocketchat_server_id.eq(rocketchat_server_id))
            .execute(connection)
            .chain_err(|| ErrorKind::DBUpdateError)?;
        Ok(())
    }

    /// Indicate if the room is connected to a Rocket.Chat server
    pub fn is_connected(&self) -> bool {
        self.rocketchat_server_id.is_some()
    }

    /// Users that are currently in the room.
    pub fn user_ids(&self, matrix_api: &MatrixApi) -> Result<Vec<UserId>> {
        let member_events = matrix_api.get_room_members(self.matrix_room_id.clone())?;
        let user_ids = member_events
            .into_iter()
            .filter_map(|member_event| if member_event.content.membership == MembershipState::Join {
                Some(member_event.user_id)
            } else {
                None
            })
            .collect();

        Ok(user_ids)
    }

    /// Update the is_bridged flag for the room.
    pub fn set_is_bridged(&mut self, connection: &SqliteConnection, is_bridged: bool) -> Result<()> {
        self.is_bridged = is_bridged;
        diesel::update(rooms::table.find(&self.matrix_room_id))
            .set(rooms::is_bridged.eq(is_bridged))
            .execute(connection)
            .chain_err(|| ErrorKind::DBUpdateError)?;
        Ok(())
    }

    /// Delete a room, this also deletes any users_in_rooms relations for that room.
    pub fn delete(&self, connection: &SqliteConnection) -> Result<()> {
        diesel::delete(rooms::table.filter(rooms::matrix_room_id.eq(self.matrix_room_id.clone())))
            .execute(connection)
            .chain_err(|| ErrorKind::DBDeleteError)?;
        Ok(())
    }
}
