//! Application service to bridge Matrix <-> Rocket.Chat.

#![feature(try_from, nll)]
#![deny(missing_docs)]
#![recursion_limit = "256"]

#[macro_use]
extern crate diesel;
#[macro_use]
extern crate diesel_migrations;
#[macro_use]
extern crate error_chain;
extern crate hyper_native_tls;
extern crate iron;
#[macro_use]
extern crate lazy_static;
extern crate persistent;
extern crate pulldown_cmark;
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
#[macro_use]
extern crate serde_json;
extern crate serde_yaml;
#[macro_use]
extern crate slog;
extern crate slog_term;
extern crate url;
extern crate yaml_rust;

embed_migrations!();

/// The maximum number of characters that can be used for a Rocket.Chat server ID
pub const MAX_ROCKETCHAT_SERVER_ID_LENGTH: usize = 16;

/// Translations
#[macro_use]
pub mod i18n;
/// Application service errors
#[macro_use]
pub mod errors;

/// REST APIs
pub mod api;
/// Helpers to interact with the application service configuration.
pub mod config;
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
