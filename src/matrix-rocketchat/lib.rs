//! Application service to bridge Matrix <-> Rocket.Chat.

#![feature(try_from)]

#![deny(missing_docs)]

#![recursion_limit = "128"]

#[macro_use]
extern crate diesel;
#[macro_use]
extern crate diesel_codegen;
#[macro_use]
extern crate error_chain;
extern crate iron;
#[macro_use]
extern crate lazy_static;
extern crate persistent;
extern crate r2d2;
extern crate r2d2_diesel;
extern crate reqwest;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_events;
extern crate ruma_identifiers;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate serde_yaml;
#[macro_use]
extern crate slog;
extern crate slog_term;
extern crate yaml_rust;

embed_migrations!();

/// Translations
#[macro_use]
pub mod i18n;
/// REST APIs
pub mod api;
/// Helpers to interact with the application service configuration.
pub mod config;
/// Helpers to interact with the database.
pub mod db;
/// Application service errors
pub mod errors;
/// Iron handlers
pub mod handlers;
/// Logging helpers
pub mod log;
/// Iron middleware
pub mod middleware;
/// Models used by the application service
pub mod models;
/// The server that runs the application service.
pub mod server;

pub use config::Config;
pub use server::Server;
