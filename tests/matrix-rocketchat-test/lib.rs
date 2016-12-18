#[macro_use]
extern crate diesel;
extern crate iron;
#[macro_use]
extern crate lazy_static;
extern crate matrix_rocketchat;
extern crate r2d2;
extern crate r2d2_diesel;
extern crate reqwest;
extern crate ruma_client_api;
extern crate ruma_events;
extern crate ruma_identifiers;
extern crate router;
extern crate serde_json;
#[macro_use]
extern crate slog;
extern crate slog_json;
extern crate slog_stream;
extern crate slog_term;

pub mod handlers;
pub mod helpers;

use std::mem;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

use diesel::sqlite::SqliteConnection;
use iron::{Iron, Listening};
use iron::Protocol::Http;
use matrix_rocketchat::{Config, Server};
use matrix_rocketchat::db::ConnectionPool;
use r2d2::Pool;
use r2d2_diesel::ConnectionManager;
use router::Router;
use slog::{DrainExt, Record};

/// Name of the temporary directory that is used for each test
pub const TEMP_DIR_NAME: &'static str = "matrix_rocketchat_test";
/// Application service token used in the tests
const AS_TOKEN: &'static str = "at";
/// Homeserver token used in the tests
pub const HS_TOKEN: &'static str = "ht";
/// Number of threads that iron uses when running tests
const IRON_THREADS: usize = 1;

lazy_static! {
    pub static ref DEFAULT_LOGGER: slog::Logger = {
        slog::Logger::root(slog_term::streamer().full().build().fuse(), o!("version" => env!("CARGO_PKG_VERSION"), "place" => file_line_logger_format))
    };
}

/// Helpers to forward messages from iron handlers
pub mod message_forwarder;

pub use message_forwarder::MessageForwarder;

/// A helper struct when running the tests that manages test resources and offers some helper methods.
pub struct Test {
    /// Configuration that is used during the test
    pub config: Config,
    /// Connection pool to get connection to the test database
    pub connection_pool: Pool<ConnectionManager<SqliteConnection>>,
    /// Flag to indicate if the test should start a matrix homeserver mock
    pub with_matrix_homeserver_mock: bool,
    /// Routes that the homeserver mock can handle
    pub matrix_homeserver_mock_router: Option<Router>,
    /// The matrix homeserver mock listening server
    pub hs_listening: Option<Listening>,
    /// The application service listening server
    pub as_listening: Option<Listening>,
}

impl Test {
    /// Create a new Test struct with helper methods that can be used for testing.
    pub fn new() -> Test {
        let config = build_test_config();
        let connection_pool = ConnectionPool::new(&config.database_url);
        Test {
            config: config,
            connection_pool: connection_pool,
            with_matrix_homeserver_mock: false,
            matrix_homeserver_mock_router: None,
            hs_listening: None,
            as_listening: None,
        }
    }

    /// Run the test with a matrix homeserver mock
    pub fn with_matrix_homeserver_mock(mut self) -> Test {
        self.with_matrix_homeserver_mock = true;
        self
    }

    /// Use custom routes when running the matrix homeserver mock instead of the default ones.
    pub fn with_custom_matrix_routes(mut self, router: Router) -> Test {
        self.matrix_homeserver_mock_router = Some(router);
        self
    }

    /// Run the application service so that a test can interact with it.
    pub fn run(mut self) -> Test {
        if self.with_matrix_homeserver_mock {
            self.run_matrix_homeserver_mock();
        }

        self.run_application_service();

        self
    }

    fn run_matrix_homeserver_mock(&mut self) {
        let (hs_tx, hs_rx) = channel::<Listening>();
        let hs_socket_addr = get_free_socket_addr();
        self.config.hs_url = format!("http://{}:{}", hs_socket_addr.ip(), hs_socket_addr.port());

        let mut router = match mem::replace(&mut self.matrix_homeserver_mock_router, None) {
            Some(router) => router,
            None => Router::new(),
        };

        router.get("/_matrix/client/versions", handlers::MatrixVersion {});

        thread::spawn(move || {
            let listening = Iron::new(router)
                .listen_with(&hs_socket_addr, IRON_THREADS, Http, None)
                .unwrap();
            hs_tx.send(listening).unwrap();
        });

        let hs_listening = hs_rx.recv_timeout(default_timeout()).unwrap();
        self.hs_listening = Some(hs_listening);
    }

    fn run_application_service(&mut self) {
        let server_config = self.config.clone();
        let (as_tx, as_rx) = channel::<Listening>();

        thread::spawn(move || {
            let log = slog::Logger::root(slog_term::streamer().full().build().fuse(),
                                         o!("version" => env!("CARGO_PKG_VERSION"),
                                            "place" => file_line_logger_format));
            let listening = Server::new(&server_config, log).run().expect("Could not start server");
            as_tx.send(listening).unwrap();
        });

        let as_listening = as_rx.recv_timeout(default_timeout()).unwrap();
        self.as_listening = Some(as_listening);
    }
}

impl Drop for Test {
    fn drop(&mut self) {
        if let Some(ref mut listening) = self.hs_listening {
            listening.close().expect("Could not shutdown matrix homeserver server mock")
        };

        if let Some(ref mut listening) = self.as_listening {
            listening.close().expect("Could not shutdown application server");
        };
    }
}

pub fn build_test_config() -> Config {
    let as_socket_addr = get_free_socket_addr();
    let as_url = format!("http://{}:{}", as_socket_addr.ip(), as_socket_addr.port());

    Config {
        as_token: AS_TOKEN.to_string(),
        hs_token: HS_TOKEN.to_string(),
        as_address: as_socket_addr,
        as_url: as_url,
        // is set if a homeserver mock is used in the test
        hs_url: "".to_string(),
        hs_domain: "localhost".to_string(),
        sender_localpart: "rocketchat".to_string(),
        database_url: ":memory:".to_string(),
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
