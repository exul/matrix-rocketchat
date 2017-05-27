#![feature(try_from)]

extern crate iron;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_events;
extern crate ruma_identifiers;

use std::convert::TryFrom;
use std::sync::mpsc::channel;
use std::thread;

use iron::{Iron, Listening, status};
use matrix_rocketchat::db::{RocketchatServer, UserOnRocketchatServer};
use matrix_rocketchat_test::{DEFAULT_ROCKETCHAT_VERSION, IRON_THREADS, MessageForwarder, RS_TOKEN, Test, default_timeout,
                             get_free_socket_addr, handlers, helpers};
use router::Router;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_client_api::r0::sync::get_state_events_for_empty_key::{self, Endpoint as GetStateEventsForEmptyKey};
use ruma_events::EventType;
use ruma_identifiers::{RoomId, UserId};

#[test]
fn successfully_connect_rocketchat_server() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_connected_admin_room().run();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    let expected_message = format!("You are connected to {}", test.rocketchat_mock_url.clone().unwrap());
    assert!(message_received_by_matrix.contains(&expected_message));

    let connection = test.connection_pool.get().unwrap();
    let rocketchat_server =
        RocketchatServer::find_by_url(&connection, test.rocketchat_mock_url.clone().unwrap()).unwrap().unwrap();
    assert_eq!(rocketchat_server.rocketchat_token.unwrap(), RS_TOKEN.to_string());

    let users_on_rocketchat_server =
        UserOnRocketchatServer::find(&connection, &UserId::try_from("@spec_user:localhost").unwrap(), rocketchat_server.id);
    assert!(users_on_rocketchat_server.is_ok())
}

#[test]
fn attempt_to_connect_to_an_incompatible_rocketchat_server_version() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let (tx, rx) = channel::<Listening>();
    let socket_addr = get_free_socket_addr();

    thread::spawn(move || {
                      let mut rocketchat_router = Router::new();
                      rocketchat_router.get("/api/info", handlers::RocketchatInfo { version: "0.1.0" }, "info");
                      let mut server = Iron::new(rocketchat_router);
                      server.threads = IRON_THREADS;
                      let listening = server.http(&socket_addr).unwrap();
                      tx.send(listening).unwrap();
                  });
    let mut listening = rx.recv_timeout(default_timeout() * 2).unwrap();
    let rocketchat_mock_url = format!("http://{}", socket_addr);

    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_admin_room().run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           format!("connect {} {} rc_id", rocketchat_mock_url.clone(), RS_TOKEN));

    listening.close().unwrap();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("No supported API version (>= 0.49) found for the Rocket.Chat server, \
                                                found version: 0.1.0"));

    let connection = test.connection_pool.get().unwrap();
    let rocketchat_server = RocketchatServer::find_by_url(&connection, test.rocketchat_mock_url.clone().unwrap()).unwrap();
    assert!(rocketchat_server.is_none());
}

#[test]
fn attempt_to_connect_to_a_non_rocketchat_server() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let (tx, rx) = channel::<Listening>();
    let socket_addr = get_free_socket_addr();

    thread::spawn(move || {
                      let rocketchat_router = Router::new();
                      let mut server = Iron::new(rocketchat_router);
                      server.threads = IRON_THREADS;
                      let listening = server.http(&socket_addr).unwrap();
                      tx.send(listening).unwrap();
                  });
    let mut listening = rx.recv_timeout(default_timeout() * 2).unwrap();
    let rocketchat_mock_url = format!("http://{}", socket_addr);

    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_admin_room().run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           format!("connect {} {} rc_id", rocketchat_mock_url.clone(), RS_TOKEN));

    listening.close().unwrap();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    let expected_message = format!("No Rocket.Chat server found when querying {}", rocketchat_mock_url);
    assert!(message_received_by_matrix.contains(&expected_message));

    let connection = test.connection_pool.get().unwrap();
    let rocketchat_server = RocketchatServer::find_by_url(&connection, test.rocketchat_mock_url.clone().unwrap()).unwrap();
    assert!(rocketchat_server.is_none());
}

#[test]
fn attempt_to_connect_to_a_server_with_the_correct_endpoint_but_an_incompatible_response() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let (tx, rx) = channel::<Listening>();
    let socket_addr = get_free_socket_addr();

    thread::spawn(move || {
                      let mut rocketchat_router = Router::new();
                      rocketchat_router.get("/api/info", handlers::InvalidJsonResponse { status: status::Ok }, "info");
                      let mut server = Iron::new(rocketchat_router);
                      server.threads = IRON_THREADS;
                      let listening = server.http(&socket_addr).unwrap();
                      tx.send(listening).unwrap();
                  });
    let mut listening = rx.recv_timeout(default_timeout() * 2).unwrap();
    let rocketchat_mock_url = format!("http://{}", socket_addr);

    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_admin_room().run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           format!("connect {} spec_token rc_id", rocketchat_mock_url.clone()));

    listening.close().unwrap();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    let expected_message = format!("No Rocket.Chat server found when querying {}/api/info \
                                   (version information is missing from the response)",
                                   rocketchat_mock_url);
    assert!(message_received_by_matrix.contains(&expected_message));

    let connection = test.connection_pool.get().unwrap();
    let rocketchat_server = RocketchatServer::find_by_url(&connection, test.rocketchat_mock_url.clone().unwrap()).unwrap();
    assert!(rocketchat_server.is_none());
}

#[test]
fn attempt_to_connect_to_non_existing_server() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let socket_addr = get_free_socket_addr();
    let rocketchat_mock_url = format!("http://{}", socket_addr);

    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_admin_room().run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           format!("connect {} spec_token rc_id", rocketchat_mock_url.clone()));

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    let expected_message = format!("Could not reach Rocket.Chat server {}", rocketchat_mock_url);
    assert!(message_received_by_matrix.contains(&expected_message));

    let connection = test.connection_pool.get().unwrap();
    let rocketchat_server = RocketchatServer::find_by_url(&connection, test.rocketchat_mock_url.clone().unwrap()).unwrap();
    assert!(rocketchat_server.is_none());
}

#[test]
fn attempt_to_connect_without_a_rocketchat_server_id() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_admin_room().run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           format!("connect {} spec_token", &test.rocketchat_mock_url.clone().unwrap()));

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("You have to provide an id to connect to a Rocket.Chat server. \
                                                It can contain any alphanumeric character and `_`. \
                                                For example \
                                                `connect https://rocketchat.example.com my_token rocketchat_example`"));

    let connection = test.connection_pool.get().unwrap();
    let rocketchat_server = RocketchatServer::find_by_url(&connection, test.rocketchat_mock_url.clone().unwrap()).unwrap();
    assert!(rocketchat_server.is_none());
}

#[test]
fn attempt_to_connect_with_an_incompatible_rocketchat_server_id() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_admin_room().run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           format!("connect {} spec_token invalid$id",
                                                   &test.rocketchat_mock_url.clone().unwrap()));

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("The provided Rocket.Chat server ID `invalid$id` is not valid, \
                      it can only contain lowercase alphanumeric characters and `_`. \
                      The maximum length is 16 characters."));

    let connection = test.connection_pool.get().unwrap();
    let rocketchat_server = RocketchatServer::find_by_url(&connection, test.rocketchat_mock_url.clone().unwrap()).unwrap();
    assert!(rocketchat_server.is_none());
}

#[test]
fn attempt_to_connect_with_a_rocketchat_server_id_that_is_already_in_use() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_admin_room().run();

    let (tx, rx) = channel::<Listening>();
    let socket_addr = get_free_socket_addr();

    thread::spawn(move || {
        let mut rocketchat_router = Router::new();
        rocketchat_router.get("/api/info", handlers::RocketchatInfo { version: DEFAULT_ROCKETCHAT_VERSION }, "info");
        let mut server = Iron::new(rocketchat_router);
        server.threads = IRON_THREADS;
        let listening = server.http(&socket_addr).unwrap();
        tx.send(listening).unwrap();
    });
    let mut listening = rx.recv_timeout(default_timeout() * 2).unwrap();
    let other_rocketchat_mock_url = format!("http://{}", socket_addr);

    helpers::invite(&test.config.as_url,
                    RoomId::try_from("!other_admin:localhost").unwrap(),
                    UserId::try_from("@spec_user:localhost").unwrap(),
                    UserId::try_from("@rocketchat:localhost").unwrap());

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!other_admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           format!("connect {} other_token rc_id", &other_rocketchat_mock_url));

    listening.close().unwrap();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           format!("connect {} spec_token rc_id", &test.rocketchat_mock_url.clone().unwrap()));

    // discard first welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    // discard first connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    // discard second welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("The provided ID `rc_id` is already in use, please choose another one."));

    let connection = test.connection_pool.get().unwrap();
    let rocketchat_server = RocketchatServer::find(&connection, other_rocketchat_mock_url).unwrap();
    assert_eq!(rocketchat_server.id, "rc_id".to_string());
}

#[test]
fn connect_an_existing_server() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let admin_room_creator_handler = handlers::RoomStateCreate { creator: UserId::try_from("@other_user:localhost").unwrap() };
    let admin_room_creator_params = get_state_events_for_empty_key::PathParams {
        room_id: RoomId::try_from("!other_admin:localhost").unwrap(),
        event_type: EventType::RoomCreate.to_string(),
    };
    matrix_router.get(GetStateEventsForEmptyKey::request_path(admin_room_creator_params),
                      admin_room_creator_handler,
                      "get_room_creator_admin_room");

    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_connected_admin_room().run();

    helpers::invite(&test.config.as_url,
                    RoomId::try_from("!other_admin:localhost").unwrap(),
                    UserId::try_from("@other_user:localhost").unwrap(),
                    UserId::try_from("@rocketchat:localhost").unwrap());

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!other_admin:localhost").unwrap(),
                                           UserId::try_from("@other_user:localhost").unwrap(),
                                           format!("connect {}", test.rocketchat_mock_url.clone().unwrap()));
    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    // discard other welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    let expected_message = format!("You are connected to {}", test.rocketchat_mock_url.clone().unwrap());
    assert!(message_received_by_matrix.contains(&expected_message));
}

#[test]
fn attempt_to_connect_to_an_existing_server_with_a_token() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let admin_room_creator_handler = handlers::RoomStateCreate { creator: UserId::try_from("@other_user:localhost").unwrap() };
    let admin_room_creator_params = get_state_events_for_empty_key::PathParams {
        room_id: RoomId::try_from("!other_admin:localhost").unwrap(),
        event_type: EventType::RoomCreate.to_string(),
    };
    matrix_router.get(GetStateEventsForEmptyKey::request_path(admin_room_creator_params),
                      admin_room_creator_handler,
                      "get_room_creator_admin_room");

    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_connected_admin_room().run();

    helpers::invite(&test.config.as_url,
                    RoomId::try_from("!other_admin:localhost").unwrap(),
                    UserId::try_from("@other_user:localhost").unwrap(),
                    UserId::try_from("@rocketchat:localhost").unwrap());


    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!other_admin:localhost").unwrap(),
                                           UserId::try_from("@other_user:localhost").unwrap(),
                                           format!("connect {} my_token other_id", test.rocketchat_mock_url.clone().unwrap()));
    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    // discard other welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    let expected_message = format!("The Rocket.Chat server {} is already connected, \
                                   connect without a token if you want to connect to the server",
                                   test.rocketchat_mock_url.clone().unwrap());
    assert!(message_received_by_matrix.contains(&expected_message));
}

#[test]
fn attempt_to_connect_an_already_connected_room() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_connected_admin_room().run();

    let (tx, rx) = channel::<Listening>();
    let socket_addr = get_free_socket_addr();

    thread::spawn(move || {
                      let mut rocketchat_router = Router::new();
                      rocketchat_router.get("/api/info", handlers::RocketchatInfo { version: "0.49.0" }, "info");
                      let mut server = Iron::new(rocketchat_router);
                      server.threads = IRON_THREADS;
                      let listening = server.http(&socket_addr).unwrap();
                      tx.send(listening).unwrap();
                  });

    let mut listening = rx.recv_timeout(default_timeout() * 2).unwrap();
    let other_rocketchat_url = format!("http://{}", socket_addr);

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           format!("connect {} other_token", other_rocketchat_url.clone()));

    listening.close().unwrap();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard first connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("This room is already connected"));
}

#[test]
fn attempt_to_connect_a_server_with_a_token_that_is_already_in_use() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let admin_room_creator_handler = handlers::RoomStateCreate { creator: UserId::try_from("@other_user:localhost").unwrap() };
    let admin_room_creator_params = get_state_events_for_empty_key::PathParams {
        room_id: RoomId::try_from("!other_admin:localhost").unwrap(),
        event_type: EventType::RoomCreate.to_string(),
    };
    matrix_router.get(GetStateEventsForEmptyKey::request_path(admin_room_creator_params),
                      admin_room_creator_handler,
                      "get_room_creator_admin_room");

    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_connected_admin_room().run();

    let socket_addr = get_free_socket_addr();
    let other_rocketchat_url = format!("http://{}", socket_addr);

    helpers::invite(&test.config.as_url,
                    RoomId::try_from("!other_admin:localhost").unwrap(),
                    UserId::try_from("@other_user:localhost").unwrap(),
                    UserId::try_from("@rocketchat:localhost").unwrap());


    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!other_admin:localhost").unwrap(),
                                           UserId::try_from("@other_user:localhost").unwrap(),
                                           format!("connect {} {} other_id", other_rocketchat_url.clone(), RS_TOKEN));

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard other welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("The token rt is already in use, please use another token"));
}

#[test]
fn attempt_to_connect_to_a_new_server_without_a_token() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_admin_room().run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           format!("connect {}", test.rocketchat_mock_url.clone().unwrap()));

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("A token is needed to connect new Rocket.Chat servers"));
}
