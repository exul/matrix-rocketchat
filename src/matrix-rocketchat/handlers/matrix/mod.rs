//! Event handlers

/// Handles commands from the admin room
mod command_handler;
/// Dispatches incomming events to the correct component
mod dispatcher;
/// Forwards messages to Rocket.Chat
mod forwarder;
/// Handles membership changes in bridge rooms
mod membership_handler;
/// Handles message events
mod message_handler;

pub use self::command_handler::CommandHandler;
pub use self::dispatcher::Dispatcher;
pub use self::forwarder::Forwarder;
pub use self::membership_handler::MembershipHandler;
pub use self::message_handler::MessageHandler;
