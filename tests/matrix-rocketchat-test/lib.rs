#![feature(try_from)]

extern crate diesel;
extern crate iron;
#[macro_use]
extern crate lazy_static;
extern crate matrix_rocketchat;
extern crate persistent;
extern crate r2d2;
extern crate r2d2_diesel;
extern crate rand;
extern crate reqwest;
extern crate ruma_client_api;
extern crate ruma_events;
extern crate ruma_identifiers;
extern crate router;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
#[macro_use]
extern crate slog;
extern crate slog_json;
extern crate slog_stream;
extern crate slog_term;
extern crate tempdir;

pub mod handlers;
pub mod helpers;

use std::collections::HashMap;
use std::convert::TryFrom;
use std::mem;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

use diesel::sqlite::SqliteConnection;
use iron::{Chain, Iron, Listening, status};
use iron::typemap::Key;
use matrix_rocketchat::{Config, Server};
use matrix_rocketchat::api::rocketchat::v1::{CHANNELS_LIST_PATH, LOGIN_PATH, ME_PATH, USERS_INFO_PATH};
use matrix_rocketchat::db::ConnectionPool;
use persistent::Write;
use r2d2::Pool;
use r2d2_diesel::ConnectionManager;
use router::Router;
use ruma_identifiers::{RoomId, UserId};
use ruma_client_api::Endpoint;
use ruma_client_api::r0::account::register::Endpoint as RegisterEndpoint;
use ruma_client_api::r0::room::create_room::Endpoint as CreateRoomEndpoint;
use ruma_client_api::r0::sync::get_member_events::Endpoint as GetMemberEventsEndpoint;
use slog::{DrainExt, Record};
use tempdir::TempDir;

/// Name of the temporary directory that is used for each test
pub const TEMP_DIR_NAME: &'static str = "matrix_rocketchat_test";
/// Name of the database file
pub const DATABASE_NAME: &'static str = "test.db";
/// Application service token used in the tests
const AS_TOKEN: &'static str = "at";
/// Homeserver token used in the tests
pub const HS_TOKEN: &'static str = "ht";
/// Rocket.Chat token used in the tests
pub const RS_TOKEN: &'static str = "rt";
/// Number of threads that iron uses when running tests
pub const IRON_THREADS: usize = 1;
/// The version the mock Rocket.Chat server announces
pub const DEFAULT_ROCKETCHAT_VERSION: &'static str = "0.49.0";

lazy_static! {
    /// Default logger
    pub static ref DEFAULT_LOGGER: slog::Logger = {
        slog::Logger::root(slog_term::streamer().full().build().fuse(), o!("version" => env!("CARGO_PKG_VERSION"), "place" => file_line_logger_format))
    };
}

#[macro_export]
macro_rules! assert_error_kind {
    ($err:expr, $kind:pat) => (match *$err.error_chain.kind() {
        $kind => assert!(true, "{:?} is of kind {:?}", $err, stringify!($kind)),
        _     => assert!(false, "{:?} is NOT of kind {:?}", $err, stringify!($kind))
    });
}

/// Helpers to forward messages from iron handlers
pub mod message_forwarder;

pub use message_forwarder::MessageForwarder;

/// Keep track of users that are registered on the Matrix server mock
#[derive(Copy, Clone)]
pub struct UsernameList;

impl Key for UsernameList {
    type Value = Vec<String>;
}

/// A helper struct when running the tests that manages test resources and offers some helper methods.
pub struct Test {
    /// The application service listening server
    pub as_listening: Option<Listening>,
    /// Room that is bridged and the user that bridged it
    pub bridged_room: Option<(&'static str, &'static str)>,
    /// A list of Rocket.Chat channels that are returned when querying the Rocket.Chat mock
    /// channels.list endpoint
    pub channels: Option<HashMap<&'static str, Vec<&'static str>>>,
    /// Configuration that is used during the test
    pub config: Config,
    /// Connection pool to get connection to the test database
    pub connection_pool: Pool<ConnectionManager<SqliteConnection>>,
    /// The matrix homeserver mock listening server
    pub hs_listening: Option<Listening>,
    /// Routes that the homeserver mock can handle
    pub matrix_homeserver_mock_router: Option<Router>,
    /// Router that the Rocket.Chat mock can handle
    pub rocketchat_mock_router: Option<Router>,
    /// The Rocket.Chat mock listening server
    pub rocketchat_listening: Option<Listening>,
    /// The URL of the Rocket.Chat mock server
    pub rocketchat_mock_url: Option<String>,
    /// Temp directory to store data during the test, it has to be part of the struct so that it
    /// does not get dropped until the test is over
    pub temp_dir: TempDir,
    /// Flag to indicate if the test should create an admin room
    pub with_admin_room: bool,
    /// Flag to indicate if a connected admin room should be created
    pub with_connected_admin_room: bool,
    /// Flag to indicate that the user should be logged in when the test starts
    pub with_logged_in_user: bool,
    /// Flag to indicate if a Rocket.Chat mock server should be started
    pub with_rocketchat_mock: bool,
}

impl Test {
    /// Create a new Test struct with helper methods that can be used for testing.
    pub fn new() -> Test {
        let temp_dir = TempDir::new(TEMP_DIR_NAME).unwrap();
        let config = build_test_config(&temp_dir);
        let connection_pool = ConnectionPool::create(&config.database_url).unwrap();
        Test {
            as_listening: None,
            bridged_room: None,
            channels: None,
            config: config,
            connection_pool: connection_pool,
            hs_listening: None,
            with_logged_in_user: false,
            matrix_homeserver_mock_router: None,
            rocketchat_mock_router: None,
            rocketchat_listening: None,
            rocketchat_mock_url: None,
            temp_dir: temp_dir,
            with_admin_room: false,
            with_connected_admin_room: false,
            with_rocketchat_mock: false,
        }
    }

    /// Use custom routes when running the matrix homeserver mock instead of the default ones.
    pub fn with_matrix_routes(mut self, router: Router) -> Test {
        self.matrix_homeserver_mock_router = Some(router);
        self
    }

    /// Create an admin room when starting the test.
    pub fn with_admin_room(mut self) -> Test {
        self.with_admin_room = true;
        self
    }

    /// Creates an admin room that is connected to the Rocket.Chat mock
    pub fn with_connected_admin_room(mut self) -> Test {
        self.with_connected_admin_room = true;
        self
    }

    /// Login the user on the Rocket.Chat server
    pub fn with_logged_in_user(mut self) -> Test {
        self.with_logged_in_user = true;
        self
    }

    /// Run a Rocket.Chat mock server.
    pub fn with_rocketchat_mock(mut self) -> Test {
        self.with_rocketchat_mock = true;
        self
    }

    /// Use custom routes when running the Rocket.Chat mock server instead of the default ones.
    pub fn with_custom_rocketchat_routes(mut self, router: Router) -> Test {
        self.rocketchat_mock_router = Some(router);
        self
    }

    /// Rooms that are bridged when running the tests.
    pub fn with_bridged_room(mut self, bridged_room: (&'static str, &'static str)) -> Test {
        self.bridged_room = Some(bridged_room);
        self
    }

    /// Set a list of Rocket.Chat channels that are returned when querying the Rocket.Chat mock
    /// channels.list edpoint
    pub fn with_custom_channel_list(mut self, channels: HashMap<&'static str, Vec<&'static str>>) -> Test {
        self.channels = Some(channels);
        self
    }

    /// Run the application service so that a test can interact with it.
    pub fn run(mut self) -> Test {
        self.run_matrix_homeserver_mock();

        if self.with_rocketchat_mock {
            self.run_rocketchat_server_mock()
        }

        self.run_application_service();

        if self.with_admin_room {
            self.create_admin_room();
        }

        if self.with_connected_admin_room {
            self.create_connected_admin_room();
        }

        if self.with_logged_in_user {
            self.login_user();
        }

        if let Some(bridged_room) = self.bridged_room {
            let (room_name, _) = bridged_room;
            self.bridge_room(room_name);
        }

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

        router.get("/_matrix/client/versions", handlers::MatrixVersion { versions: default_matrix_api_versions() }, "versions");
        router.post("*", handlers::EmptyJson {}, "default_post");
        router.put("*", handlers::EmptyJson {}, "default_put");
        if self.with_admin_room || self.with_connected_admin_room {
            let room_members = handlers::RoomMembers {
                room_id: RoomId::try_from("!admin:localhost").unwrap(),
                members: vec![UserId::try_from("@spec_user:localhost").unwrap(),
                              UserId::try_from("@rocketchat:localhost").unwrap()],
            };
            router.get(GetMemberEventsEndpoint::router_path(), room_members, "room_members");
            router.post(RegisterEndpoint::router_path(), handlers::MatrixRegister {}, "register");
        }

        if self.bridged_room.is_some() {
            router.post(CreateRoomEndpoint::router_path(), handlers::MatrixCreateRoom {}, "create_room");
        }

        thread::spawn(move || {
                          let mut chain = Chain::new(router);
                          chain.link_before(Write::<UsernameList>::one(Vec::new()));
                          let mut server = Iron::new(chain);
                          server.threads = IRON_THREADS;
                          let listening = server.http(&hs_socket_addr).unwrap();
                          hs_tx.send(listening).unwrap();
                      });

        let hs_listening = hs_rx.recv_timeout(default_timeout()).unwrap();
        self.hs_listening = Some(hs_listening);
    }

    fn run_rocketchat_server_mock(&mut self) {
        let (tx, rx) = channel::<Listening>();
        let socket_addr = get_free_socket_addr();

        let mut router = match mem::replace(&mut self.rocketchat_mock_router, None) {
            Some(router) => router,
            None => Router::new(),
        };

        router.get("/api/info", handlers::RocketchatInfo { version: DEFAULT_ROCKETCHAT_VERSION }, "info");

        if self.with_logged_in_user {
            router.post(LOGIN_PATH,
                        handlers::RocketchatLogin {
                            successful: true,
                            rocketchat_user_id: Some("spec_user_id".to_string()),
                        },
                        "login");
            router.get(ME_PATH, handlers::RocketchatMe { username: "spec_user".to_string() }, "me");
            router.get(USERS_INFO_PATH, handlers::RocketchatUsersInfo {}, "users_info");
        }

        let mut channels = match self.channels.clone() {
            Some(channels) => channels,
            None => HashMap::new(),
        };

        if let Some(bridged_room) = self.bridged_room {
            let (room_name, matrix_user_id) = bridged_room;
            channels.insert(room_name, vec![matrix_user_id]);
        }

        if channels.len() > 0 {
            router.get(CHANNELS_LIST_PATH,
                       handlers::RocketchatChannelsList {
                           status: status::Ok,
                           channels: channels,
                       },
                       "channels_list");
        }

        thread::spawn(move || {
                          let mut server = Iron::new(router);
                          server.threads = IRON_THREADS;
                          let listening = server.http(&socket_addr).unwrap();
                          tx.send(listening).unwrap();
                      });
        let listening = rx.recv_timeout(default_timeout() * 2).unwrap();
        self.rocketchat_listening = Some(listening);
        self.rocketchat_mock_url = Some(format!("http://{}", socket_addr));
    }

    fn run_application_service(&mut self) {
        let server_config = self.config.clone();
        let (as_tx, as_rx) = channel::<Listening>();

        thread::spawn(move || {
            let log = slog::Logger::root(slog_term::streamer().full().build().fuse(),
                                         o!("version" => env!("CARGO_PKG_VERSION"),
                                            "place" => file_line_logger_format));
            debug!(DEFAULT_LOGGER, "config: {:?}", server_config);
            let listening = match Server::new(&server_config, log).run(IRON_THREADS) {
                Ok(listening) => listening,
                Err(err) => {
                    error!(DEFAULT_LOGGER, "error: {}", err);
                    for err in err.error_chain.iter().skip(1) {
                        error!(DEFAULT_LOGGER, "caused by: {}", err);
                    }
                    return;
                }
            };
            as_tx.send(listening).unwrap()
        });

        let as_listening = as_rx.recv_timeout(default_timeout() * 2).unwrap();
        self.as_listening = Some(as_listening);
    }

    fn create_admin_room(&self) {
        helpers::create_admin_room(&self.config.as_url,
                                   RoomId::try_from("!admin:localhost").unwrap(),
                                   UserId::try_from("@spec_user:localhost").unwrap(),
                                   UserId::try_from("@rocketchat:localhost").unwrap());
    }

    fn create_connected_admin_room(&self) {
        self.create_admin_room();
        match self.rocketchat_mock_url {
            Some(ref rocketchat_mock_url) => {
                helpers::send_room_message_from_matrix(&self.config.as_url,
                                                       RoomId::try_from("!admin:localhost").unwrap(),
                                                       UserId::try_from("@spec_user:localhost").unwrap(),
                                                       format!("connect {} {} rc_id", rocketchat_mock_url, RS_TOKEN));
            }
            None => panic!("No Rocket.Chat mock present to connect to"),
        }
    }

    fn login_user(&self) {
        helpers::send_room_message_from_matrix(&self.config.as_url,
                                               RoomId::try_from("!admin:localhost").unwrap(),
                                               UserId::try_from("@spec_user:localhost").unwrap(),
                                               "login spec_user secret".to_string());
    }

    fn bridge_room(&self, room_name: &'static str) {
        helpers::send_room_message_from_matrix(&self.config.as_url,
                                               RoomId::try_from("!admin:localhost").unwrap(),
                                               UserId::try_from("@spec_user:localhost").unwrap(),
                                               format!("bridge {}", room_name));

        // users accept invite
        helpers::join(&self.config.as_url,
                      RoomId::try_from(&format!("!{}_id:localhost", room_name)).unwrap(),
                      UserId::try_from("@rocketchat:localhost").unwrap());

        helpers::join(&self.config.as_url,
                      RoomId::try_from(&format!("!{}_id:localhost", room_name)).unwrap(),
                      UserId::try_from("@spec_user:localhost").unwrap());
    }
}

impl Drop for Test {
    fn drop(&mut self) {
        if let Some(ref mut listening) = self.hs_listening {
            listening.close().unwrap()
        };

        if let Some(ref mut listening) = self.rocketchat_listening {
            listening.close().unwrap()
        };

        if let Some(ref mut listening) = self.as_listening {
            listening.close().unwrap()
        };
    }
}

pub fn build_test_config(temp_dir: &TempDir) -> Config {
    let as_socket_addr = get_free_socket_addr();
    let as_url = format!("http://{}:{}", as_socket_addr.ip(), as_socket_addr.port());
    let database_path = temp_dir.path().join(DATABASE_NAME);
    let database_url = database_path.to_str().unwrap();
    debug!(DEFAULT_LOGGER, format!("Database URL is: {}", database_url));

    Config {
        as_token: AS_TOKEN.to_string(),
        hs_token: HS_TOKEN.to_string(),
        as_address: as_socket_addr,
        as_url: as_url,
        // is set if a homeserver mock is used in the test
        hs_url: "".to_string(),
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
    Duration::from_millis(2000)
}

pub fn default_matrix_api_versions() -> Vec<&'static str> {
    vec!["r0.0.1", "r0.1.0", "r0.2.0"]
}

/// Returns a free socket address on localhost (by randomly choosing a free port).
/// The listener is not really needed, but when using to_socket_addrs the port stays at 0
/// until it is actually used.
pub fn get_free_socket_addr() -> SocketAddr {
    let address = "127.0.0.1:0";
    let listener = TcpListener::bind(address).unwrap();
    listener.local_addr().unwrap()
}

fn file_line_logger_format(info: &Record) -> String {
    format!("{}:{} {}", info.file(), info.line(), info.module())
}
