//! Rocket.Chat handlers

/// Forwards message from Rocket.Chat to Matrix
mod forwarder;

pub use self::forwarder::Forwarder;
