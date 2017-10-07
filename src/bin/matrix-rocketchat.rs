//! Application service to bridge Matrix <-> Rocket.Chat.

#![deny(missing_docs)]

extern crate clap;
extern crate iron;
extern crate matrix_rocketchat;
extern crate num_cpus;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_json;
extern crate slog_stream;
extern crate slog_term;

use std::fs::OpenOptions;
use std::path::Path;

use clap::{App, Arg};
use iron::Listening;
use matrix_rocketchat::{Config, Server};
use matrix_rocketchat::errors::*;
use slog::{Drain, FnValue, Level, LevelFilter, Record};

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
        .arg(Arg::with_name("log-file").short("f").long("log-file").help("Path to log file").takes_value(true))
        .arg(Arg::with_name("log-level").short("l").long("log-level").help("Log level").takes_value(true))
        .get_matches();

    let config_path = matches.value_of("config").unwrap_or("config.yaml").to_string();
    let config = Config::read_from_file(&config_path).chain_err(|| ErrorKind::ReadFileError(config_path))?;
    let log_file_path = matches.value_of("log-file").unwrap_or("matrix-rocketchat.log");
    let log_level = matches.value_of("log-level").unwrap_or("info");
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
    let file_decorator = slog_term::PlainDecorator::new(file);
    let file_drain = slog_term::FullFormat::new(file_decorator).build().fuse();
    let file_drain = slog_async::Async::new(file_drain).build().fuse();
    let term_decorator = slog_term::TermDecorator::new().build();
    let term_drain = slog_term::FullFormat::new(term_decorator).build().fuse();
    let term_drain = slog_async::Async::new(term_drain).build().fuse();
    let dup_drain = LevelFilter::new(slog::Duplicate::new(term_drain, file_drain), log_level).fuse();
    slog::Logger::root(dup_drain, o!("version" => env!("CARGO_PKG_VERSION"), "place" => FnValue(file_line_logger_format)))
}

fn file_line_logger_format(info: &Record) -> String {
    format!("{}:{} {}", info.file(), info.line(), info.module())
}
