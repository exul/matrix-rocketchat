//! Iron middleware

/// Middleware for requests from the Matrix homeserver
pub mod matrix;
/// Middleware for requests from the Rocket.Chat server
pub mod rocketchat;

pub use self::matrix::AccessToken;
pub use self::rocketchat::RocketchatToken;
