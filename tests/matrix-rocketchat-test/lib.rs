extern crate iron;
#[macro_use]
extern crate lazy_static;
extern crate matrix_rocketchat;
extern crate reqwest;
#[macro_use]
extern crate slog;
extern crate slog_json;
extern crate slog_stream;
extern crate slog_term;
extern crate tempdir;

mod api;

pub use api::call_url;

use std::net::SocketAddr;
use std::net::TcpListener;
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

use iron::Listening;
use matrix_rocketchat::{Config, Server};
use slog::{DrainExt, Record};
use tempdir::TempDir;

/// Name of the temporary directory that is used for each test
pub const TEMP_DIR_NAME: &'static str = "matrix_rocketchat_test";
/// Application service token used in the tests
const AS_TOKEN: &'static str = "at";
/// Homeserver token used in the tests
const HS_TOKEN: &'static str = "ht";
/// Name of the test database file
const DATABASE_NAME: &'static str = "test_database.sqlite3";

lazy_static! {
    pub static ref DEFAULT_LOGGER: slog::Logger = {
        slog::Logger::root(slog_term::streamer().full().build().fuse(), o!("version" => env!("CARGO_PKG_VERSION"), "place" => file_line_logger_format))
    };
}

/// A helper struct when running the tests that manages test resources and offers some helper methods.
pub struct Test {
    /// Configuration that is used during the test
    pub config: Config,
    /// The application service listening server
    pub as_listening: Option<Listening>,
}

impl Test {
    /// Create a new Test struct with helper methods that can be used for testing.
    pub fn new() -> Test {
        // create a temp directory with a database for each test to be able to run them in parallel
        let temp_dir = TempDir::new("matrix-rocketchat-tests").expect("Could not create temp dir");
        let config = build_test_config(&temp_dir);
        Test {
            config: config,
            as_listening: None,
        }
    }

    /// Run the application service so that a test can interact with it.
    pub fn run(mut self) -> Test {
        let server_config = self.config.clone();
        let (tx, rx) = channel::<Listening>();

        thread::spawn(move || {
            let log = slog::Logger::root(slog_term::streamer().full().build().fuse(),
                                         o!("version" => env!("CARGO_PKG_VERSION"),
                                            "place" => file_line_logger_format));
            let listening = Server::new(&server_config, log).run().expect("Could not start server");
            tx.send(listening).unwrap();
        });
        let listening = rx.recv_timeout(default_timeout()).unwrap();
        self.as_listening = Some(listening);

        self
    }
}

impl Drop for Test {
    fn drop(&mut self) {
        if let Some(ref mut listening) = self.as_listening {
            listening.close().expect("Could not shutdown server");
        };
    }
}

pub fn build_test_config(temp_dir: &TempDir) -> Config {
    let as_socket_addr = get_free_socket_addr();
    let as_url = format!("http://{}:{}", as_socket_addr.ip(), as_socket_addr.port());
    let hs_socket_addr = get_free_socket_addr();
    let hs_url = format!("http://{}:{}", hs_socket_addr.ip(), as_socket_addr.port());
    let database_path = temp_dir.path().join(DATABASE_NAME);
    let database_url = database_path.to_str().expect("could not build database url");

    Config {
        as_token: AS_TOKEN.to_string(),
        hs_token: HS_TOKEN.to_string(),
        as_address: as_socket_addr,
        as_url: as_url,
        hs_url: hs_url,
        hs_domain: "localhost".to_string(),
        sender_localpart: "rocketchat".to_string(),
        database_url: database_url.to_string(),
        use_ssl: false,
        ssl_certificate_path: None,
        ssl_key_path: None,
    }
}

/// The default timeout that is used when executing functions/methods with a timeout.
pub fn default_timeout() -> Duration {
    Duration::from_millis(800)
}

// We don't really need the listener, but with to to_socket_addrs the port stays at 0 so we
// use the listener to get the actual port that we can use as part of the homeserver url.
// This way we just get a random free port and can run the tests in parallel.
fn get_free_socket_addr() -> SocketAddr {
    let address = "127.0.0.1:0";
    let listener = TcpListener::bind(address).expect("Could not bind address");
    listener.local_addr().expect("Could not get local addr")
}

fn file_line_logger_format(info: &Record) -> String {
    format!("{}:{} {}", info.file(), info.line(), info.module())
}
