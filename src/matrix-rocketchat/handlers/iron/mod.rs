//! Iron handlers

/// Process requests from the Rocket.Chat server
pub mod rocketchat;
/// Process login request for Rocket.Chat
pub mod rocketchat_login;
/// Processes requests from the Matrix homeserver
pub mod transactions;
/// Sends a welcome message to the caller
pub mod welcome;

pub use self::rocketchat::Rocketchat;
pub use self::rocketchat_login::RocketchatLogin;
pub use self::transactions::Transactions;
pub use self::welcome::Welcome;
