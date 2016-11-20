//! Application service to bridge Matrix <-> Rocket.Chat.

#![deny(missing_docs)]

extern crate matrix_rocketchat;
#[macro_use]
extern crate slog;
extern crate slog_json;
extern crate slog_stream;
extern crate slog_term;

use std::fs::OpenOptions;

use matrix_rocketchat::{Config, Server};
use slog::{DrainExt, Record};

fn main() {
    let log_path = "matrix-rocketchat.log";
    let file = OpenOptions::new().create(true).write(true).truncate(true).open(log_path).expect("Setup log failed");

    let term_drain = slog_term::streamer().stderr().full().build();
    let file_drain = slog_stream::stream(file, slog_json::new().add_default_keys().build());
    let log = slog::Logger::root(slog::duplicate(term_drain, file_drain).fuse(),
                                 o!("version" => env!("CARGO_PKG_VERSION"),
                                    "place" => file_line_logger_format));

    let config = Config {};
    Server::new(&config, log).run();
}

fn file_line_logger_format(info: &Record) -> String {
    format!("{}:{} {}", info.file(), info.line(), info.module())
}
