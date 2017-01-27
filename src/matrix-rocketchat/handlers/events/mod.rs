//! Event handlers

/// Handles commands from the admin room
pub mod command_handler;
/// Event dispatcher
pub mod event_dispatcher;
/// Handles room events
pub mod room_handler;

pub use self::command_handler::CommandHandler;
pub use self::event_dispatcher::EventDispatcher;
pub use self::room_handler::RoomHandler;
