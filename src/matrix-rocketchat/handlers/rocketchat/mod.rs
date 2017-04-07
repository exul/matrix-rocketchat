//! Rocket.Chat handlers

/// Forwards message from Rocket.Chat to Matrix
pub mod forwarder;
/// Provides helper methods to manage virtual users.
pub mod virtual_user_handler;

pub use self::forwarder::Forwarder;
pub use self::virtual_user_handler::VirtualUserHandler;
