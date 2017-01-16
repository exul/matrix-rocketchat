//! Helpers to interact with the database.

/// Database connection pool
pub mod connection_pool;
/// `Room` entry
pub mod room;
/// The database schema
pub mod schema;
/// `User` entry
pub mod user;
/// `UserInRoom` entry
pub mod user_in_room;

pub use self::connection_pool::ConnectionPool;
pub use self::room::Room;
pub use self::user::User;
pub use self::user_in_room::UserInRoom;
