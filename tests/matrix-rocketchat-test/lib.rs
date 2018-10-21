#![feature(try_from)]

extern crate diesel;
extern crate http;
extern crate iron;
#[macro_use]
extern crate lazy_static;
extern crate matrix_rocketchat;
extern crate persistent;
extern crate r2d2;
extern crate r2d2_diesel;
extern crate rand;
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
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_json;
extern crate slog_stream;
extern crate slog_term;
extern crate tempdir;

pub mod handlers;
pub mod helpers;

use std::collections::HashMap;
use std::convert::TryFrom;
use std::error::Error;
use std::fmt::{self, Debug};
use std::io::Read;
use std::mem;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use diesel::sqlite::SqliteConnection;
use iron::prelude::*;
use iron::typemap::Key;
use iron::{status, Chain, Iron, Listening};
use matrix_rocketchat::api::rocketchat::v1::{
    CHANNELS_LIST_PATH, CHANNELS_MEMBERS_PATH, GROUPS_LIST_PATH, GROUPS_MEMBERS_PATH, LOGIN_PATH, ME_PATH, USERS_INFO_PATH,
};
use matrix_rocketchat::api::MatrixApi;
use matrix_rocketchat::models::ConnectionPool;
use matrix_rocketchat::{Config, Server};
use persistent::Write;
use r2d2::Pool;
use r2d2_diesel::ConnectionManager;
use router::Router;
use ruma_client_api::r0::account::register::Endpoint as RegisterEndpoint;
use ruma_client_api::r0::alias::delete_alias::Endpoint as DeleteAliasEndpoint;
use ruma_client_api::r0::alias::get_alias::Endpoint as GetAliasEndpoint;
use ruma_client_api::r0::membership::invite_user::Endpoint as InviteUserEndpoint;
use ruma_client_api::r0::membership::join_room_by_id::Endpoint as JoinRoomByIdEndpoint;
use ruma_client_api::r0::membership::leave_room::Endpoint as LeaveRoomEndpoint;
use ruma_client_api::r0::profile::get_display_name::Endpoint as GetDisplaynameEndpoint;
use ruma_client_api::r0::profile::set_display_name::Endpoint as SetDisplaynameEndpoint;
use ruma_client_api::r0::room::create_room::Endpoint as CreateRoomEndpoint;
use ruma_client_api::r0::send::send_state_event_for_empty_key::Endpoint as SendStateEventForEmptyKeyEndpoint;
use ruma_client_api::r0::sync::get_member_events::Endpoint as GetMemberEventsEndpoint;
use ruma_client_api::r0::sync::get_state_events::Endpoint as GetStateEventsEndpoint;
use ruma_client_api::r0::sync::get_state_events_for_empty_key::Endpoint as GetStateEventsForEmptyKeyEndpoint;
use ruma_client_api::r0::sync::sync_events::Endpoint as SyncEventsEndpoint;
use ruma_client_api::Endpoint;
use ruma_events::room::member::MembershipState;
use ruma_identifiers::{RoomAliasId, RoomId, UserId};
use slog::{Drain, FnValue, Level, LevelFilter, Record};
use tempdir::TempDir;

/// Name of the temporary directory that is used for each test
pub const TEMP_DIR_NAME: &str = "matrix_rocketchat_test";
/// Name of the database file
pub const DATABASE_NAME: &str = "test.db";
/// Application service token used in the tests
const AS_TOKEN: &str = "at";
/// Homeserver token used in the tests
pub const HS_TOKEN: &str = "ht";
/// Rocket.Chat token used in the tests
pub const RS_TOKEN: &str = "rt";
/// Number of threads that iron uses when running tests
pub const IRON_THREADS: usize = 4;
/// The version the mock Rocket.Chat server announces
pub const DEFAULT_ROCKETCHAT_VERSION: &str = "0.70.4";

lazy_static! {
    /// Default logger
    pub static ref DEFAULT_LOGGER: slog::Logger = {
        let log_level = match option_env!("TEST_LOG_LEVEL"){
            Some("error") => Level::Error,
            Some("warning") => Level::Warning,
            Some("debug") => Level::Debug,
            _ => Level::Info,
        };

        let decorator = slog_term::TermDecorator::new().build();
        let drain = slog_term::FullFormat::new(decorator).build().fuse();
        let drain = LevelFilter::new(slog_async::Async::new(drain).build(), log_level).fuse();

        slog::Logger::root(drain, o!("version" => env!("CARGO_PKG_VERSION"), "place" => FnValue(file_line_logger_format)))
    };
}

#[macro_export]
macro_rules! assert_error_kind {
    ($err:expr, $kind:pat) => {
        match *$err.error_chain.kind() {
            $kind => assert!(true, "{:?} is of kind {:?}", $err, stringify!($kind)),
            _ => assert!(false, "{:?} is NOT of kind {:?}", $err, stringify!($kind)),
        }
    };
}

/// Helpers to forward messages from iron handlers
pub mod message_forwarder;

pub use message_forwarder::{Message, MessageForwarder};

/// Keep track of users that are registered on the Matrix server mock
#[derive(Copy, Clone)]
pub struct UserList;

#[derive(Copy, Clone)]
pub struct UsersInRooms;

#[derive(Copy, Clone)]
pub struct RoomsStatesMap;

#[derive(Copy, Clone)]
pub struct RoomAliasMap;

#[derive(Copy, Clone)]
pub struct PendingInvites;

impl Key for UserList {
    type Value = HashMap<UserId, Option<String>>;
}

impl Key for UsersInRooms {
    type Value = HashMap<RoomId, HashMap<UserId, (MembershipState, Vec<(UserId, MembershipState)>)>>;
}

impl Key for RoomsStatesMap {
    type Value = HashMap<RoomId, HashMap<UserId, HashMap<String, String>>>;
}

impl Key for RoomAliasMap {
    type Value = HashMap<RoomId, Vec<RoomAliasId>>;
}

impl Key for PendingInvites {
    type Value = HashMap<RoomId, HashMap<UserId, UserId>>;
}

#[derive(Debug)]
struct TestError(String);

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Debug::fmt(self, f)
    }
}

impl Error for TestError {
    fn description(&self) -> &str {
        &*self.0
    }
}

/// A helper struct when running the tests that manages test resources and offers some helper methods.
pub struct Test {
    /// The application service listening server
    pub as_listening: Option<Listening>,
    /// Group that is bridged and the user that are in the group
    pub bridged_group: Option<(&'static str, Vec<&'static str>)>,
    /// Room that is bridged and the user that are in the room
    pub bridged_room: Option<(&'static str, Vec<&'static str>)>,
    /// Direct message that is bridged and the users that are participating
    pub bridged_dm: Option<(RoomId, Vec<UserId>)>,
    /// A list of Rocket.Chat channels that are returned when querying the Rocket.Chat mock
    /// channels.list endpoint
    pub channels: Arc<Mutex<HashMap<&'static str, Vec<&'static str>>>>,
    /// Configuration that is used during the test
    pub config: Config,
    /// Connection pool to get connection to the test database
    pub connection_pool: Pool<ConnectionManager<SqliteConnection>>,
    /// A list of Rocket.Chat groups that are returned when querying the Rocket.Chat mock
    /// groups.list endpoint
    pub groups: Arc<Mutex<HashMap<&'static str, Vec<&'static str>>>>,
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
            bridged_group: None,
            bridged_room: None,
            bridged_dm: None,
            channels: Arc::new(Mutex::new(HashMap::new())),
            config: config,
            connection_pool: connection_pool,
            groups: Arc::new(Mutex::new(HashMap::new())),
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

    /// Creates a test with a custom configuration
    pub fn with_custom_config(mut self, config: Config) -> Test {
        self.config = config;
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
    pub fn with_bridged_room(mut self, bridged_room: (&'static str, Vec<&'static str>)) -> Test {
        self.bridged_room = Some(bridged_room);
        self
    }

    pub fn with_bridged_group(mut self, bridged_group: (&'static str, Vec<&'static str>)) -> Test {
        self.bridged_group = Some(bridged_group);
        self
    }

    pub fn with_bridge_dm(mut self, bridged_dm: (RoomId, Vec<UserId>)) -> Test {
        self.bridged_dm = Some(bridged_dm);
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
            helpers::send_room_message_from_matrix(
                &self.config.as_url,
                RoomId::try_from("!admin_room_id:localhost").unwrap(),
                UserId::try_from("@spec_user:localhost").unwrap(),
                "login spec_user secret".to_string(),
            );
        }

        if let Some(bridged_room) = self.bridged_room.clone() {
            let (room_name, _) = bridged_room;
            self.bridge_room(room_name);
        }

        if let Some(bridged_group) = self.bridged_group.clone() {
            let (group_name, _) = bridged_group;
            self.bridge_room(group_name);
        }

        self
    }

    fn run_matrix_homeserver_mock(&mut self) {
        let (hs_tx, hs_rx) = channel::<Listening>();
        let hs_socket_addr = get_free_socket_addr();
        self.config.hs_url = format!("http://{}:{}", hs_socket_addr.ip(), hs_socket_addr.port());

        let router = match mem::replace(&mut self.matrix_homeserver_mock_router, None) {
            Some(router) => router,
            None => self.default_matrix_routes(),
        };

        let mut user_list = HashMap::new();
        let mut users_in_rooms = HashMap::new();
        let mut room_state_map = HashMap::new();
        if let Some(ref bridged_dm) = self.bridged_dm {
            self.create_dm_room(&mut user_list, &mut users_in_rooms, &mut room_state_map, bridged_dm);
        }

        thread::spawn(move || {
            let mut chain = Chain::new(router);
            chain.link_before(Write::<UserList>::one(user_list));
            chain.link_before(Write::<UsersInRooms>::one(users_in_rooms));
            chain.link_before(Write::<RoomsStatesMap>::one(room_state_map));
            chain.link_before(Write::<RoomAliasMap>::one(HashMap::new()));
            chain.link_before(Write::<PendingInvites>::one(HashMap::new()));
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

        let router = match mem::replace(&mut self.rocketchat_mock_router, None) {
            Some(router) => router,
            None => self.default_rocketchat_routes(),
        };

        if let Some(bridged_room) = self.bridged_room.clone() {
            let (room_name, user_ids) = bridged_room;
            self.channels.lock().unwrap().insert(room_name, user_ids);
        }

        if let Some(bridged_group) = self.bridged_group.clone() {
            let (group_name, user_ids) = bridged_group;
            self.groups.lock().unwrap().insert(group_name, user_ids);
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
            debug!(DEFAULT_LOGGER, "config: {:?}", server_config);
            let listening = match Server::new(&server_config, DEFAULT_LOGGER.clone()).run(IRON_THREADS) {
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

        let matrix_api = MatrixApi::new(&self.config, DEFAULT_LOGGER.clone()).unwrap();
        matrix_api
            .register("spec_user".to_string())
            .unwrap_or_else(|e| warn!(DEFAULT_LOGGER, "Can't register spec user {}", e));

        self.as_listening = Some(as_listening);
    }

    fn create_admin_room(&self) {
        let matrix_api = MatrixApi::new(&self.config, DEFAULT_LOGGER.clone()).unwrap();
        let spec_user_id = UserId::try_from("@spec_user:localhost").unwrap();
        let rocketchat_user_id = UserId::try_from("@rocketchat:localhost").unwrap();
        matrix_api.create_room(Some("admin_room".to_string()), None, &spec_user_id).unwrap();

        helpers::invite(&self.config, RoomId::try_from("!admin_room_id:localhost").unwrap(), rocketchat_user_id, spec_user_id);
    }

    fn create_connected_admin_room(&self) {
        self.create_admin_room();
        match self.rocketchat_mock_url {
            Some(ref rocketchat_mock_url) => {
                helpers::send_room_message_from_matrix(
                    &self.config.as_url,
                    RoomId::try_from("!admin_room_id:localhost").unwrap(),
                    UserId::try_from("@spec_user:localhost").unwrap(),
                    format!("connect {} {} rcid", rocketchat_mock_url, RS_TOKEN),
                );
            }
            None => panic!("No Rocket.Chat mock present to connect to"),
        }
    }

    fn bridge_room(&self, room_name: &'static str) {
        helpers::send_room_message_from_matrix(
            &self.config.as_url,
            RoomId::try_from("!admin_room_id:localhost").unwrap(),
            UserId::try_from("@spec_user:localhost").unwrap(),
            format!("bridge {}", room_name),
        );

        helpers::join(
            &self.config,
            RoomId::try_from(format!("!{}_id:localhost", room_name).as_ref()).unwrap(),
            UserId::try_from("@spec_user:localhost").unwrap(),
        );
    }

    /// create_dm_room simulates the creation of a DM room on the matrix mock server by adding all relevant users and states to
    /// the various maps that handle the matrix mock server's state.
    fn create_dm_room(
        &self,
        user_list: &mut HashMap<UserId, Option<String>>,
        users_in_rooms: &mut HashMap<RoomId, HashMap<UserId, (MembershipState, Vec<(UserId, MembershipState)>)>>,
        room_state_map: &mut HashMap<RoomId, HashMap<UserId, HashMap<String, String>>>,
        bridged_dm: &(RoomId, Vec<UserId>),
    ) {
        let bot_user_id = self.config.matrix_bot_user_id().unwrap();
        let (room_id, user_ids) = bridged_dm;
        // a dm room is created by the virtual user, so that the AS can control it
        let creator_id = user_ids.iter().filter(|id| self.config.is_application_service_user(id)).next().unwrap().to_string();
        let mut room_states = HashMap::new();
        room_states.insert("creator".to_string(), creator_id);

        users_in_rooms.insert(room_id.clone(), HashMap::new());
        room_state_map.insert(room_id.clone(), HashMap::new());

        // in a dm room both regular members joined the room
        let mut user_in_room_list: Vec<(UserId, MembershipState)> =
            user_ids.clone().into_iter().map(|id| (id, MembershipState::Join)).collect();

        // the bot user joins the room but leaves it immediately, it's only needed to have access
        // to the room state. The following membership states mimic this.
        user_in_room_list.push((bot_user_id.clone(), MembershipState::Leave));
        let bot_user_state = (MembershipState::Leave, user_in_room_list.clone());
        users_in_rooms.get_mut(room_id).unwrap().insert(bot_user_id.clone(), bot_user_state);
        room_state_map.get_mut(room_id).unwrap().insert(bot_user_id, room_states.clone());

        // room state for regular members
        for user_id in user_ids {
            user_list.insert(user_id.clone(), None);

            let user_state = (MembershipState::Join, user_in_room_list.clone());
            users_in_rooms.get_mut(room_id).unwrap().insert(user_id.clone(), user_state);

            room_state_map.get_mut(room_id).unwrap().insert(user_id.clone(), room_states.clone());
        }
    }

    /// The default matrix routes that the matrix mock server needs to work. They can be used a
    /// a staring point to add more routes.
    pub fn default_matrix_routes(&self) -> Router {
        let mut router = Router::new();

        router.get(SyncEventsEndpoint::router_path(), handlers::MatrixSync {}, "sync");

        let join_room_handler = handlers::MatrixJoinRoom { as_url: self.config.as_url.clone(), send_inviter: true };
        router.post(JoinRoomByIdEndpoint::router_path(), join_room_handler, "join_room");

        let leave_room_handler = handlers::MatrixLeaveRoom { as_url: self.config.as_url.clone() };
        router.post(LeaveRoomEndpoint::router_path(), leave_room_handler, "leave_room");

        router.get("/_matrix/client/versions", handlers::MatrixVersion { versions: default_matrix_api_versions() }, "versions");

        let mut get_state_events_for_empty_key = Chain::new(handlers::GetRoomState {});
        get_state_events_for_empty_key.link_before(handlers::PermissionCheck {});
        router.get(
            GetStateEventsForEmptyKeyEndpoint::router_path(),
            get_state_events_for_empty_key,
            "get_state_events_for_empty_key",
        );

        let mut get_state_events = Chain::new(handlers::MatrixState {});
        get_state_events.link_before(handlers::PermissionCheck {});
        router.get(GetStateEventsEndpoint::router_path(), get_state_events, "get_state_events");

        let mut get_members = Chain::new(handlers::RoomMembers {});
        get_members.link_before(handlers::PermissionCheck {});
        router.get(GetMemberEventsEndpoint::router_path(), get_members, "room_members");

        router.post(RegisterEndpoint::router_path(), handlers::MatrixRegister {}, "register");

        router.get(GetDisplaynameEndpoint::router_path(), handlers::MatrixGetDisplayName {}, "get_displayname");

        router.put(SetDisplaynameEndpoint::router_path(), handlers::MatrixSetDisplayName {}, "set_displayname");

        router.post(
            CreateRoomEndpoint::router_path(),
            handlers::MatrixCreateRoom { as_url: self.config.as_url.clone() },
            "create_room",
        );

        let invite_user_handler = handlers::MatrixInviteUser { as_url: self.config.as_url.clone() };
        router.post(InviteUserEndpoint::router_path(), invite_user_handler, "invite_user");

        let mut send_room_state = Chain::new(handlers::SendRoomState {});
        send_room_state.link_before(handlers::PermissionCheck {});
        router.put(SendStateEventForEmptyKeyEndpoint::router_path(), send_room_state, "send_room_state");

        let mut get_room_alias = Chain::new(handlers::GetRoomAlias {});
        get_room_alias.link_before(handlers::PermissionCheck {});
        router.get(GetAliasEndpoint::router_path(), get_room_alias, "get_room_alias");

        router.delete(DeleteAliasEndpoint::router_path(), handlers::DeleteRoomAlias {}, "delete_room_alias");

        router.post("*", handlers::EmptyJson {}, "default_post");
        router.put("*", handlers::EmptyJson {}, "default_put");

        router
    }

    /// The default Rocket.Chat routes that the Rocket.Chat mock server needs to work. They can be used a
    /// a staring point to add more routes.
    pub fn default_rocketchat_routes(&self) -> Router {
        let mut router = Router::new();

        router.get("/api/info", handlers::RocketchatInfo { version: DEFAULT_ROCKETCHAT_VERSION }, "info");

        let login_user_id = Arc::new(Mutex::new(Some("spec_user_id".to_string())));
        router.post(
            LOGIN_PATH,
            handlers::RocketchatLogin { successful: true, rocketchat_user_id: Arc::clone(&login_user_id) },
            "login",
        );

        let me_username = Arc::new(Mutex::new("spec_user".to_string()));
        router.get(ME_PATH, handlers::RocketchatMe { username: Arc::clone(&me_username) }, "me");
        router.get(USERS_INFO_PATH, handlers::RocketchatUsersInfo {}, "users_info");

        router.get(
            CHANNELS_LIST_PATH,
            handlers::RocketchatChannelsList { status: status::Ok, channels: Arc::clone(&self.channels) },
            "channels_list",
        );

        router.get(
            CHANNELS_MEMBERS_PATH,
            handlers::RocketchatRoomMembers { status: status::Ok, channels: Arc::clone(&self.channels) },
            "get_room_members",
        );

        router.get(
            GROUPS_LIST_PATH,
            handlers::RocketchatGroupsList { status: status::Ok, groups: Arc::clone(&self.groups) },
            "groups_list",
        );

        router.get(
            GROUPS_MEMBERS_PATH,
            handlers::RocketchatRoomMembers { status: status::Ok, channels: Arc::clone(&self.groups) },
            "get_group_members",
        );

        router.post("*", handlers::EmptyJson {}, "default_post");
        router.put("*", handlers::EmptyJson {}, "default_put");

        router
    }

    /// Get a list of channels that is used by the Rocket.Chat mock.
    pub fn channel_list(&self) -> Arc<Mutex<HashMap<&'static str, Vec<&'static str>>>> {
        Arc::clone(&self.channels)
    }

    /// Get a list of groups that is used by the Rocket.Chat mock.
    pub fn group_list(&self) -> Arc<Mutex<HashMap<&'static str, Vec<&'static str>>>> {
        Arc::clone(&self.groups)
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
    debug!(DEFAULT_LOGGER, "Database URL is: {}", database_url);

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
        accept_remote_invites: false,
        log_level: "info".to_string(),
        log_to_console: true,
        log_to_file: false,
        log_file_path: "".to_string(),
        use_https: false,
        pkcs12_path: None,
        pkcs12_password: None,
    }
}

/// The default timeout that is used when executing functions/methods with a timeout.
pub fn default_timeout() -> Duration {
    Duration::from_millis(2000)
}

/// The default versions that are returned by the matrix versions endpoint.
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

/// Get the payload from an incomming request. First try to get the payload from the request
/// body. If that one is empty, try to get it from the Message struct in case a middleware
/// already extracted the content and stored the message in the struct.
pub fn extract_payload(request: &mut Request) -> String {
    let mut payload = String::new();
    request.body.read_to_string(&mut payload).unwrap();

    // if the request payload is empty, try to get it from the middleware
    if payload.is_empty() {
        if let Some(message) = request.extensions.get::<Message>() {
            payload = message.payload.clone()
        }
    } else {
        let message = Message { payload: payload.clone() };
        request.extensions.insert::<Message>(message);
    }

    payload
}

fn file_line_logger_format(info: &Record) -> String {
    format!("{}:{} {}", info.file(), info.line(), info.module())
}
