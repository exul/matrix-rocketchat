//! Event handlers

/// Handles commands from the admin room
pub mod command_handler;
/// Dispatches incomming events to the correct component
pub mod dispatcher;
/// Forwards messages to Rocket.Chat
pub mod forwarder;
/// Handles membership changes in bridge rooms
pub mod membership_handler;
/// Handles message events
pub mod message_handler;
/// Creates and bridge rooms
pub mod room_handler;

pub use self::command_handler::CommandHandler;
pub use self::dispatcher::Dispatcher;
pub use self::forwarder::Forwarder;
pub use self::message_handler::MessageHandler;
pub use self::membership_handler::MembershipHandler;
pub use self::room_handler::RoomHandler;
