//! Event handlers

/// Handles commands from the admin room
pub mod command_handler;
/// Event dispatcher
pub mod event_dispatcher;
/// Forwards messages to Rocket.Chat
pub mod forwarder;
/// Handles message events
pub mod message_handler;
/// Handles room events
pub mod room_handler;

pub use self::command_handler::CommandHandler;
pub use self::event_dispatcher::EventDispatcher;
pub use self::forwarder::Forwarder;
pub use self::message_handler::MessageHandler;
pub use self::room_handler::RoomHandler;
