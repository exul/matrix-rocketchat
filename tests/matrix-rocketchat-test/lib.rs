extern crate matrix_rocketchat;
extern crate tempdir;

use std::net::SocketAddr;
use std::net::TcpListener;

use matrix_rocketchat::Config;
use tempdir::TempDir;

/// Application service token used in the tests
const AS_TOKEN: &'static str = "at";
/// Homeserver token used in the tests
const HS_TOKEN: &'static str = "ht";
/// Name of the test database file
const DATABASE_NAME: &'static str = "test_database.sqlite3";

/// A helper struct when running the tests that manages test resources and offers some helper methods.
pub struct Test {
    pub config: Config,
}

impl Test {
    /// Create a new Test struct with helper methods that can be used for testing.
    pub fn new() -> Test {
        // create a temp directory with a database for each test to be able to run them in parallel
        let temp_dir = TempDir::new("matrix-rocketchat-tests").expect("Could not create temp dir");
        let config = build_test_config(&temp_dir);
        Test { config: config }
    }
}

fn build_test_config(temp_dir: &TempDir) -> Config {
    let as_socket_addr = get_free_socket_addr();
    let as_address = format!("{}:{}", as_socket_addr.ip(), as_socket_addr.port());
    let as_url = format!("http://{}", as_address);
    let hs_socket_addr = get_free_socket_addr();
    let hs_url = format!("http://{}:{}", hs_socket_addr.ip(), as_socket_addr.port());
    let database_path = temp_dir.path().join(DATABASE_NAME);
    let database_url = database_path.to_str().expect("could not build database url");

    Config {
        as_token: AS_TOKEN.to_string(),
        hs_token: HS_TOKEN.to_string(),
        as_address: as_address,
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

// We don't really need the listener, but with to to_socket_addrs the port stays at 0 so we
// use the listener to get the actual port that we can use as part of the homeserver url.
// This way we just get a random free port and can run the tests in parallel.
fn get_free_socket_addr() -> SocketAddr {
    let address = "127.0.0.1:0";
    let listener = TcpListener::bind(address).expect("Could not bind address");
    listener.local_addr().expect("Could not get local addr")
}
