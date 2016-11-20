//! Application service to bridge Matrix <-> Rocket.Chat.

#![deny(missing_docs)]

#[macro_use]
extern crate slog;
extern crate slog_term;

/// Helpers to interact with the application service configuration.
pub mod config;
/// The server that runs the application service.
pub mod server;

pub use config::Config;
pub use server::Server;
