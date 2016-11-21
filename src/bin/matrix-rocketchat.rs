//! Application service to bridge Matrix <-> Rocket.Chat.

#![deny(missing_docs)]

#[macro_use]
extern crate clap;
extern crate matrix_rocketchat;
#[macro_use]
extern crate slog;
extern crate slog_json;
extern crate slog_stream;
extern crate slog_term;

use std::fs::OpenOptions;
use std::path::Path;

use clap::App;
use matrix_rocketchat::{Config, Server};
use slog::{DrainExt, Record};

fn main() {
    let cli_yaml = load_yaml!("../../assets/cli.yaml");
    let matches = App::from_yaml(cli_yaml).get_matches();

    let config_path = matches.value_of("config").expect("Could not find config path").to_string();
    let config = Config::read_from_file(&config_path).expect("Reading config file failed");
    let log_file_path = matches.value_of("log_file").expect("Could not find log file path").to_string();
    let log = build_logger(&log_file_path);

    Server::new(&config, log).run();
}

fn build_logger(log_file_path: &str) -> slog::Logger {
    let path = Path::new(&log_file_path).to_path_buf();
    let file = OpenOptions::new().create(true).write(true).truncate(true).open(path).expect("Log file creation failed");
    let file_drain = slog_stream::stream(file, slog_json::new().add_default_keys().build());
    let term_drain = slog_term::streamer().stderr().full().build();
    slog::Logger::root(slog::duplicate(term_drain, file_drain).fuse(),
                       o!("version" => env!("CARGO_PKG_VERSION"),
                                    "place" => file_line_logger_format))
}

fn file_line_logger_format(info: &Record) -> String {
    format!("{}:{} {}", info.file(), info.line(), info.module())
}
