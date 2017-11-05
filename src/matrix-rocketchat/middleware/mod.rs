//! Iron middleware

/// Middleware for requests from the Matrix homeserver
mod matrix;
/// Middleware for requests from the Rocket.Chat server
mod rocketchat;

pub use self::matrix::AccessToken;
pub use self::rocketchat::RocketchatToken;
