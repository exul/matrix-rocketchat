//! Rocket.Chat handlers

/// Forwards message from Rocket.Chat to Matrix
pub mod forwarder;

pub use self::forwarder::Forwarder;
