//! Models that manage data and logic used by the application service.

/// A Rocket.Chat channel
mod channel;
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
/// A virtual user on the Matrix homeserver that represents a Rocket.Chat user.
mod virtual_user;

pub use self::connection_pool::ConnectionPool;
pub use self::channel::Channel;
pub use self::events::Events;
pub use self::rocketchat_server::{Credentials, NewRocketchatServer, RocketchatServer};
pub use self::room::Room;
pub use self::user_on_rocketchat_server::{NewUserOnRocketchatServer, UserOnRocketchatServer};
pub use self::virtual_user::VirtualUser;
