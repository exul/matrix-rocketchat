//! Event handlers

/// Event dispatcher
pub mod event_dispatcher;
/// Handles room events
pub mod room_handler;

pub use self::event_dispatcher::EventDispatcher;
pub use self::room_handler::RoomHandler;
