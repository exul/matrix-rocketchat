//! Event handlers

/// Handles commands from the admin room
pub mod command_handler;
/// Event dispatcher
pub mod event_dispatcher;
/// Forwards messages to Rocket.Chat
pub mod forwarder;
/// Handles membership changes in bridge rooms
pub mod membership_handler;
/// Handles message events
pub mod message_handler;
/// Create rooms
pub mod room_creator;

pub use self::command_handler::CommandHandler;
pub use self::event_dispatcher::EventDispatcher;
pub use self::forwarder::Forwarder;
pub use self::message_handler::MessageHandler;
pub use self::membership_handler::MembershipHandler;
pub use self::room_creator::RoomCreator;
