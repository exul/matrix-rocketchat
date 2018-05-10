//! Models that manage data and logic used by the application service.

/// The database connection pool
mod connection_pool;
/// A list of Events that are received from the Matirx homeserver.
mod events;
/// A Rocket.Chat channel or group
mod rocketchat_room;
/// `RocketchatServer` entry
mod rocketchat_server;
/// `Room` entry
mod room;
/// The database schema
mod schema;
/// `UserOnRocketchatServer` entry
mod user_on_rocketchat_server;
/// A virtual user on the Matrix homeserver that represents a Rocket.Chat user.
mod virtual_user;

pub use self::connection_pool::ConnectionPool;
pub use self::events::Events;
pub use self::rocketchat_room::RocketchatRoom;
pub use self::rocketchat_server::{Credentials, NewRocketchatServer, RocketchatServer};
pub use self::room::Room;
pub use self::user_on_rocketchat_server::{NewUserOnRocketchatServer, UserOnRocketchatServer};
pub use self::virtual_user::VirtualUser;
