//! Application service to bridge Matrix <-> Rocket.Chat.

#![feature(proc_macro)]

#![deny(missing_docs)]

extern crate iron;
extern crate router;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_yaml;
#[macro_use]
extern crate slog;
extern crate slog_term;

/// Helpers to interact with the application service configuration.
pub mod config;
/// Application service errors
pub mod errors;
/// Iron handlers
pub mod handlers;
/// The server that runs the application service.
pub mod server;

pub use errors::ASError;
pub use config::Config;
pub use server::Server;
