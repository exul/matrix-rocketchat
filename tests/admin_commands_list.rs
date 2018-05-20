#![feature(try_from)]

extern crate iron;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_identifiers;

use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::{Arc, Mutex};

use iron::status;
use matrix_rocketchat::api::rocketchat::v1::{CHANNELS_LIST_JOINED_PATH, CHANNELS_LIST_PATH, LOGIN_PATH, ME_PATH};
use matrix_rocketchat::api::MatrixApi;
use matrix_rocketchat_test::{default_timeout, handlers, helpers, MessageForwarder, Test, DEFAULT_LOGGER};
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_client_api::Endpoint;
use ruma_identifiers::{RoomId, UserId};

#[test]
fn sucessfully_list_rocketchat_rooms() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = test.default_rocketchat_routes();
    let channels = test.channel_list();
    channels.lock().unwrap().insert("normal_channel", Vec::new());
    channels.lock().unwrap().insert("joined_channel", vec!["spec_user"]);
    channels.lock().unwrap().insert("bridged_channel", vec!["spec_user"]);
    let mut users_in_rooms = HashMap::new();
    users_in_rooms.insert("spec_user_id", vec!["joined_channel", "bridged_channel"]);
    rocketchat_router.get(
        CHANNELS_LIST_JOINED_PATH,
        handlers::RocketchatJoinedRooms {
            users_in_rooms: users_in_rooms,
        },
        "joined_channels",
    );

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("bridged_channel", vec!["spec_user"]))
        .run();

    // the listing has to work even when the Matrix user's display name is different from the one on
    // Rocket.Chat
    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    matrix_api.set_display_name(UserId::try_from("@spec_user:localhost").unwrap(), "something different".to_string()).unwrap();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard bridge message
    receiver.recv_timeout(default_timeout()).unwrap();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "list".to_string(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("normal_channel"));
    assert!(message_received_by_matrix.contains("*joined_channel*"));
    assert!(message_received_by_matrix.contains("**bridged_channel**"));
}

#[test]
fn the_user_gets_a_message_when_getting_room_list_failes() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.get(
        CHANNELS_LIST_PATH,
        handlers::RocketchatErrorResponder {
            message: "List Error".to_string(),
            status: status::InternalServerError,
        },
        "channels_list",
    );
    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "list".to_string(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn the_user_gets_a_message_when_the_room_list_cannot_be_deserialized() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.get(
        CHANNELS_LIST_PATH,
        handlers::InvalidJsonResponse {
            status: status::Ok,
        },
        "channels_list",
    );
    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "list".to_string(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn attempt_to_list_rooms_when_the_admin_room_is_not_connected() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test = test.with_matrix_routes(matrix_router).with_admin_room().run();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "list".to_string(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("This room is not connected to a Rocket.Chat server"));
}

#[test]
fn the_user_gets_a_message_when_the_me_endpoint_returns_an_error() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.post(
        LOGIN_PATH,
        handlers::RocketchatLogin {
            successful: true,
            rocketchat_user_id: Arc::new(Mutex::new(None)),
        },
        "login",
    );
    rocketchat_router.get(
        ME_PATH,
        handlers::RocketchatErrorResponder {
            status: status::InternalServerError,
            message: "Spec Error".to_string(),
        },
        "me",
    );

    let channels = test.channel_list();
    channels.lock().unwrap().insert("spec_channel", Vec::new());

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .run();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "login spec_user secret".to_string(),
    );

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "list".to_string(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn the_user_gets_a_message_when_the_me_response_cannot_be_deserialized() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.post(
        LOGIN_PATH,
        handlers::RocketchatLogin {
            successful: true,
            rocketchat_user_id: Arc::new(Mutex::new(None)),
        },
        "login",
    );
    rocketchat_router.get(
        ME_PATH,
        handlers::InvalidJsonResponse {
            status: status::Ok,
        },
        "me",
    );

    let channels = test.channel_list();
    channels.lock().unwrap().insert("spec_channel", Vec::new());

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .run();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "login spec_user secret".to_string(),
    );

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "list".to_string(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}
