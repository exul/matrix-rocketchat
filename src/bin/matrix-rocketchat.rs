//! Application service to bridge Matrix <-> Rocket.Chat.

#![deny(missing_docs)]

extern crate clap;
extern crate iron;
extern crate matrix_rocketchat;
extern crate num_cpus;
#[macro_use]
extern crate slog;
extern crate slog_json;
extern crate slog_stream;
extern crate slog_term;

use std::fs::OpenOptions;
use std::path::Path;

use clap::{App, Arg};
use iron::Listening;
use matrix_rocketchat::{Config, Server};
use matrix_rocketchat::errors::*;
use slog::{DrainExt, Level, LevelFilter, Record};

fn main() {
    if let Err(ref e) = run() {
        println!("error: {}", e);

        for e in e.error_chain.iter().skip(1) {
            println!("caused by: {}", e);
        }
    }
}

fn run() -> Result<Listening> {
    let matches = App::new("matrix-rocketchat")
        .version("0.1")
        .author("Andreas Brönnimann <foss@exul.org>")
        .about("An application service to bridge Matrix and Rocket.Chat.")
        .arg(Arg::with_name("config").short("c").long("config").help("Path to config file").takes_value(true))
        .arg(Arg::with_name("log-file").short("l").long("log-file").help("Path to log file").takes_value(true))
        .get_matches();

    let config_path = matches.value_of("config").unwrap_or("config.yaml").to_string();
    let config = Config::read_from_file(&config_path).chain_err(|| ErrorKind::ReadFileError(config_path))?;
    let log_file_path = matches.value_of("log_file").unwrap_or("matrix-rocketchat.log");
    let log_level = matches.value_of("log_level").unwrap_or("info");
    let log = build_logger(log_file_path, log_level);
    let threads = num_cpus::get() * 8;
    Server::new(&config, log).run(threads)
}

fn build_logger(log_file_path: &str, log_level: &str) -> slog::Logger {
    let log_level = match log_level {
        "info" => Level::Info,
        "warning" => Level::Warning,
        _ => Level::Debug,
    };
    let path = Path::new(&log_file_path).to_path_buf();
    let file = OpenOptions::new().create(true).write(true).truncate(true).open(path).expect("Log file creation failed");
    let file_drain = slog_stream::stream(file, slog_json::new().add_default_keys().build());
    let term_drain = slog_term::streamer().stderr().full().build();
    slog::Logger::root(
        LevelFilter::new(slog::duplicate(term_drain, file_drain), log_level).fuse(),
        o!("version" => env!("CARGO_PKG_VERSION"),
                          "place" => file_line_logger_format),
    )
}

fn file_line_logger_format(info: &Record) -> String {
    format!("{}:{} {}", info.file(), info.line(), info.module())
}
