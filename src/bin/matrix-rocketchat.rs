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
use std::process;

use clap::{App, Arg};
use iron::Listening;
use matrix_rocketchat::errors::*;
use matrix_rocketchat::server::StartupNotification;
use matrix_rocketchat::{Config, Server};
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
        .author("Andreas Studer <foss@exul.org>")
        .about("An application service to bridge Matrix and Rocket.Chat.")
        .arg(Arg::with_name("config").short("c").long("config").help("Path to config file").takes_value(true))
        .arg(
            Arg::with_name("skip-login-notification")
                .short("s")
                .long("skip-login-notification")
                .help("Do not notify users that they have to re-login")
                .takes_value(false),
        )
        .get_matches();

    let mut startup_notification = StartupNotification::On;
    let config_path = matches.value_of("config").unwrap_or("config.yaml").to_string();
    if matches.is_present("skip-login-notification") {
        startup_notification = StartupNotification::Off;
    }
    let config = Config::read_from_file(&config_path).chain_err(|| ErrorKind::ReadFileError(config_path))?;
    let log = build_logger(&config);
    let threads = num_cpus::get() * 8;
    Server::new(&config, log).run(threads, startup_notification)
}

fn build_logger(config: &Config) -> slog::Logger {
    let log_level = match &*config.log_level {
        "info" => Level::Info,
        "warning" => Level::Warning,
        "error" => Level::Error,
        _ => Level::Debug,
    };

    let mut file_drain = None;
    let mut console_drain = None;

    if config.log_to_file {
        let path = Path::new(&config.log_file_path).to_path_buf();
        let file = OpenOptions::new().create(true).write(true).truncate(true).open(path).expect("Log file creation failed");
        let file_decorator = slog_term::PlainDecorator::new(file);
        let formatted_drain = slog_term::FullFormat::new(file_decorator).build().fuse();
        file_drain = Some(slog_async::Async::new(formatted_drain).build());
    }

    if config.log_to_console {
        let console_decorator = slog_term::TermDecorator::new().build();
        let formatted_drain = slog_term::FullFormat::new(console_decorator).build().fuse();
        console_drain = Some(slog_async::Async::new(formatted_drain).build());
    }

    let drain;
    if config.log_to_file && config.log_to_console {
        let file_drain = file_drain.expect("Setting up logging to file failed");
        let console_drain = console_drain.expect("Setting up logging to console failed");
        let dup_drain = slog::Duplicate::new(console_drain.fuse(), file_drain.fuse()).fuse();
        drain = slog_async::Async::new(dup_drain).build();
    } else if config.log_to_file {
        drain = file_drain.expect("Setting up logging to file failed");
    } else if config.log_to_console {
        drain = console_drain.expect("Setting up logging to console failed");
    } else {
        println!("No logging selected, either log to file or log to console has to be enabled!");
        process::exit(-1);
    }

    slog::Logger::root(
        LevelFilter::new(drain.fuse(), log_level).fuse(),
        o!("version" => env!("CARGO_PKG_VERSION"), "place" => FnValue(logger_format)),
    )
}

fn logger_format(info: &Record) -> String {
    format!("{}:{} {}", info.file(), info.line(), info.module())
}
