#![feature(try_from)]

extern crate iron;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_identifiers;

use std::convert::TryFrom;

use iron::status;
use matrix_rocketchat::api::rocketchat::v1::{LOGIN_PATH, ME_PATH};
use matrix_rocketchat::db::{RocketchatServer, UserOnRocketchatServer};
use matrix_rocketchat_test::{MessageForwarder, Test, default_timeout, handlers, helpers};
use router::Router;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_identifiers::{RoomId, UserId};


#[test]
fn sucessfully_login_via_chat_mesage() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(LOGIN_PATH, handlers::RocketchatLogin { successful: true }, "login");
    rocketchat_router.get(ME_PATH, handlers::RocketchatMe { username: "Spec user".to_string() }, "me");
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "login spec_user secret".to_string());

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    let connection = test.connection_pool.get().unwrap();
    let rocketchat_server = RocketchatServer::find(&connection, test.rocketchat_mock_url.clone().unwrap()).unwrap();
    let user_on_rocketchat_server = UserOnRocketchatServer::find(&connection,
                                                                 &UserId::try_from("@spec_user:localhost").unwrap(),
                                                                 rocketchat_server.id)
        .unwrap();
    assert_eq!(user_on_rocketchat_server.rocketchat_auth_token.unwrap(), "spec_auth_token");
    assert_eq!(user_on_rocketchat_server.rocketchat_user_id.unwrap(), "spec_user_id");

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("You are logged in."));
}

#[test]
fn wrong_password_when_logging_in_via_chat_mesage() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(LOGIN_PATH, handlers::RocketchatLogin { successful: false }, "login");
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "login spec_user wrong_password".to_string());

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Authentication failed!"));
}

#[test]
fn login_multiple_times() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(LOGIN_PATH, handlers::RocketchatLogin { successful: true }, "login");
    rocketchat_router.get(ME_PATH, handlers::RocketchatMe { username: "Spec user".to_string() }, "me");
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .run();

    for _ in 0..2 {
        helpers::send_room_message_from_matrix(&test.config.as_url,
                                               RoomId::try_from("!admin:localhost").unwrap(),
                                               UserId::try_from("@spec_user:localhost").unwrap(),
                                               "login spec_user secret".to_string());
    }

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard first login message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("You are logged in."));
}

#[test]
fn the_user_can_login_again_on_the_same_server_with_a_new_admin_room() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(LOGIN_PATH, handlers::RocketchatLogin { successful: true }, "login");
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    helpers::leave_room(&test.config.as_url,
                        RoomId::try_from("!admin:localhost").unwrap(),
                        UserId::try_from("@spec_user:localhost").unwrap());

    helpers::create_admin_room(&test.config.as_url,
                               RoomId::try_from("!admin:localhost").unwrap(),
                               UserId::try_from("@spec_user:localhost").unwrap(),
                               UserId::try_from("@rocketchat:localhost").unwrap());

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           format!("connect {}", test.rocketchat_mock_url.clone().unwrap()));

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "login spec_user secret".to_string());

    // discard first welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard first connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard first login message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard second welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard second connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("You are logged in."));
}

#[test]
fn server_does_not_respond_when_logging_in_via_chat_mesage() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(LOGIN_PATH, handlers::RocketchatLogin { successful: true }, "login");
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "login spec_user secret".to_string());

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn the_user_gets_a_message_when_the_login_response_cannot_be_deserialized() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(LOGIN_PATH, handlers::InvalidJsonResponse { status: status::Ok }, "login");
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "login spec_user secret".to_string());

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn the_user_gets_a_message_when_the_login_returns_an_error() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(LOGIN_PATH,
                           handlers::RocketchatErrorResponder {
                               status: status::InternalServerError,
                               message: "Spec Error".to_string(),
                           },
                           "login");
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "login spec_user secret".to_string());

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn the_user_gets_a_message_when_the_me_response_cannot_be_deserialized() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(LOGIN_PATH, handlers::RocketchatLogin { successful: true }, "login");
    rocketchat_router.get(ME_PATH, handlers::InvalidJsonResponse { status: status::Ok }, "me");
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "login spec_user secret".to_string());


    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn attempt_to_login_when_the_admin_room_is_not_connected() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_admin_room()
        .run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "list".to_string());

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("This room is not connected to a Rocket.Chat server"));
}
