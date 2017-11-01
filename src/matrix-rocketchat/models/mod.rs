//! Models that manage data and logic used by the application service.

/// The database connection pool
mod connection_pool;
/// A list of Events that are received from the Matirx homeserver.
mod events;
/// `RocketchatServer` entry
mod rocketchat_server;
/// `Room` entry
mod room;
/// The database schema
mod schema;
/// `UserOnRocketchatServer` entry
mod user_on_rocketchat_server;

pub use self::events::Events;
pub use self::rocketchat_server::{NewRocketchatServer, RocketchatServer};
pub use self::room::Room;
pub use self::user_on_rocketchat_server::{NewUserOnRocketchatServer, UserOnRocketchatServer};
pub use self::connection_pool::ConnectionPool;
