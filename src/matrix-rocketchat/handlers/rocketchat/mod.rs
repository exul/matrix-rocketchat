//! Rocket.Chat handlers

/// Forwards message from Rocket.Chat to Matrix
pub mod forwarder;
/// Helper methods to login a user on the Rocket.Chat server
pub mod login;

pub use self::forwarder::Forwarder;
pub use self::login::{Credentials, Login};
