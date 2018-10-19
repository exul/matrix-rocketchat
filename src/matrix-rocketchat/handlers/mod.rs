//! Iron and Event handlers

/// Notifies the user about errors that appear in one of the handlers.
pub mod error_notifier;
/// Iron handlers
pub mod iron;
/// Matrix handlers
pub mod matrix;
/// Rocket.Chat handlers
pub mod rocketchat;

pub use self::error_notifier::ErrorNotifier;
