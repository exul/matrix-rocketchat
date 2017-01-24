#![feature(try_from)]

extern crate iron;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_identifiers;

use std::convert::TryFrom;
use std::sync::mpsc::channel;
use std::thread;

use iron::{Iron, Listening, status};
use matrix_rocketchat::db::{RocketchatServer, UserOnRocketchatServer};
use matrix_rocketchat_test::{IRON_THREADS, MessageForwarder, Test, default_timeout, get_free_socket_addr, handlers,
                             helpers};
use router::Router;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_identifiers::{RoomId, UserId};

#[test]
fn successfully_connect_rocketchat_server() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test = Test::new().with_custom_matrix_routes(matrix_router).with_rocketchat_mock().with_admin_room().run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           format!("connect {} spec_token", test.rocketchat_mock_url.clone().unwrap()));

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains(&format!("You are connected to {}",test.rocketchat_mock_url.clone().unwrap())));

    let connection = test.connection_pool.get().unwrap();
    let rocketchat_server =
        RocketchatServer::find_by_url(&connection, test.rocketchat_mock_url.clone().unwrap()).unwrap().unwrap();
    assert_eq!(rocketchat_server.rocketchat_token.unwrap(), "spec_token".to_string());

    let users_on_rocketchat_server = UserOnRocketchatServer::find(&connection,
                                                                  &UserId::try_from("@spec_user:localhost").unwrap(),
                                                                  rocketchat_server.id);
    assert!(users_on_rocketchat_server.is_ok())
}

#[test]
fn attempt_to_connect_to_an_incompatible_rocketchat_server_version() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
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

    let test = Test::new()
        .with_custom_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_admin_room()
        .run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           format!("connect {} spec_token", rocketchat_mock_url.clone()));

    listening.close().unwrap();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("No supported API version found for the Rocket.Chat REST API, found versions: 0.1.0"));
}

#[test]
fn attempt_to_connect_to_a_non_rocketchat_server() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
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

    let test = Test::new()
        .with_custom_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_admin_room()
        .run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           format!("connect {} spec_token", rocketchat_mock_url.clone()));

    listening.close().unwrap();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains(&format!("No Rocket.Chat server found when querying {}", rocketchat_mock_url)));
}

#[test]
fn attempt_to_connect_to_a_server_with_the_correct_endpoint_but_an_incompatible_response() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
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

    let test = Test::new()
        .with_custom_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_admin_room()
        .run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           format!("connect {} spec_token", rocketchat_mock_url.clone()));

    listening.close().unwrap();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains(&format!("No Rocket.Chat server found when querying {}", rocketchat_mock_url)));
}


#[test]
fn attempt_to_connect_to_non_existing_server() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let socket_addr = get_free_socket_addr();
    let rocketchat_mock_url = format!("http://{}", socket_addr);

    let test = Test::new()
        .with_custom_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_admin_room()
        .run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           format!("connect {} spec_token", rocketchat_mock_url.clone()));

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains(&format!("Could not reach Rocket.Chat server {}", rocketchat_mock_url)));
}

#[test]
fn connect_an_existing_server() {}

#[test]
fn attempt_to_connect_to_an_existing_server_with_a_token() {}

#[test]
fn attempt_to_connect_an_already_connected_room() {}

#[test]
fn attempt_to_connect_a_server_with_a_token_that_is_already_in_use() {}
