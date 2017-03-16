//! Iron and Event handlers

/// Iron handlers
pub mod iron;
/// Notifies the user about errors that appear in one of the handlers.
pub mod error_notifier;
/// Event handlers
pub mod events;
/// Rocket.Chat handlers
pub mod rocketchat;

pub use self::error_notifier::ErrorNotifier;
