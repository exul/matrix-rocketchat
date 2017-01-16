//! Models that manage data and logic used by the application service.

/// A list of Events that are received from the Matirx homeserver.
mod events;

pub use self::events::Events;
