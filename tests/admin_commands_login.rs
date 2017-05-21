#![feature(try_from)]

extern crate iron;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate reqwest;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_identifiers;
extern crate serde_json;

use std::collections::HashMap;
use std::convert::TryFrom;

use iron::status;
use matrix_rocketchat::api::RestApi;
use matrix_rocketchat::api::rocketchat::v1::{LOGIN_PATH, ME_PATH};
use matrix_rocketchat::db::{RocketchatServer, UserOnRocketchatServer};
use matrix_rocketchat::handlers::rocketchat::Credentials;
use matrix_rocketchat_test::{MessageForwarder, Test, default_timeout, handlers, helpers};
use reqwest::Method;
use router::Router;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::membership::leave_room::Endpoint as LeaveRoomEndpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_identifiers::{RoomId, UserId};
use serde_json::to_string;

#[test]
fn sucessfully_login_via_chat_mesage() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(LOGIN_PATH,
                           handlers::RocketchatLogin {
                               successful: true,
                               rocketchat_user_id: None,
                           },
                           "login");
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
    let user_on_rocketchat_server =
        UserOnRocketchatServer::find(&connection, &UserId::try_from("@spec_user:localhost").unwrap(), rocketchat_server.id)
            .unwrap();
    assert_eq!(user_on_rocketchat_server.rocketchat_auth_token.unwrap(), "spec_auth_token");

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("You are logged in."));
}

#[test]
fn wrong_password_when_logging_in_via_chat_message() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(LOGIN_PATH,
                           handlers::RocketchatLogin {
                               successful: false,
                               rocketchat_user_id: None,
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
                                           "login spec_user wrong_password".to_string());

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Authentication failed!"));
}

#[test]
fn login_multiple_times_via_chat_message() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(LOGIN_PATH,
                           handlers::RocketchatLogin {
                               successful: true,
                               rocketchat_user_id: None,
                           },
                           "login");
    rocketchat_router.get(ME_PATH, handlers::RocketchatMe { username: "Spec user".to_string() }, "me");
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .run();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    for _ in 0..2 {
        helpers::send_room_message_from_matrix(&test.config.as_url,
                                               RoomId::try_from("!admin:localhost").unwrap(),
                                               UserId::try_from("@spec_user:localhost").unwrap(),
                                               "login spec_user secret".to_string());

        let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
        assert!(message_received_by_matrix.contains("You are logged in."));
    }
}

#[test]
fn sucessfully_login_via_rest_api() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(LOGIN_PATH,
                           handlers::RocketchatLogin {
                               successful: true,
                               rocketchat_user_id: None,
                           },
                           "login");
    rocketchat_router.get(ME_PATH, handlers::RocketchatMe { username: "Spec user".to_string() }, "me");
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .run();

    let login_request = Credentials {
        matrix_user_id: UserId::try_from("@spec_user:localhost").unwrap(),
        rocketchat_username: "spec_user".to_string(),
        password: "secret".to_string(),
        rocketchat_url: test.rocketchat_mock_url.clone().unwrap(),
    };
    let payload = to_string(&login_request).unwrap();
    let (response, status_code) = RestApi::call(Method::Post,
                                                &format!("http://{}/rocketchat/login",
                                                        test.as_listening.as_ref().unwrap().socket),
                                                &payload,
                                                &HashMap::new(),
                                                None)
            .unwrap();
    assert!(response.contains("You are logged in. Return to your Matrix client and follow the instructions there."));
    assert!(status_code.is_success());

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    let connection = test.connection_pool.get().unwrap();
    let rocketchat_server = RocketchatServer::find(&connection, test.rocketchat_mock_url.clone().unwrap()).unwrap();
    let user_on_rocketchat_server =
        UserOnRocketchatServer::find(&connection, &UserId::try_from("@spec_user:localhost").unwrap(), rocketchat_server.id)
            .unwrap();
    assert_eq!(user_on_rocketchat_server.rocketchat_auth_token.unwrap(), "spec_auth_token");

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("You are logged in."));
}

#[test]
fn wrong_password_when_logging_in_via_rest_api() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(LOGIN_PATH,
                           handlers::RocketchatLogin {
                               successful: false,
                               rocketchat_user_id: None,
                           },
                           "login");
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .run();

    let login_request = Credentials {
        matrix_user_id: UserId::try_from("@spec_user:localhost").unwrap(),
        rocketchat_username: "spec_user".to_string(),
        password: "wrong_password".to_string(),
        rocketchat_url: test.rocketchat_mock_url.clone().unwrap(),
    };
    let payload = to_string(&login_request).unwrap();
    let (response, status_code) = RestApi::call(Method::Post,
                                                &format!("http://{}/rocketchat/login",
                                                        test.as_listening.as_ref().unwrap().socket),
                                                &payload,
                                                &HashMap::new(),
                                                None)
            .unwrap();
    assert!(response.contains("Authentication failed!"));
    assert_eq!(status_code, status::Unauthorized);

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Authentication failed!"));
}

#[test]
fn login_multiple_times_via_rest_message() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(LOGIN_PATH,
                           handlers::RocketchatLogin {
                               successful: true,
                               rocketchat_user_id: None,
                           },
                           "login");
    rocketchat_router.get(ME_PATH, handlers::RocketchatMe { username: "Spec user".to_string() }, "me");
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .run();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    let login_request = Credentials {
        matrix_user_id: UserId::try_from("@spec_user:localhost").unwrap(),
        rocketchat_username: "spec_user".to_string(),
        password: "secret".to_string(),
        rocketchat_url: test.rocketchat_mock_url.clone().unwrap(),
    };
    let payload = to_string(&login_request).unwrap();

    for _ in 0..2 {
        let (response, status_code) = RestApi::call(Method::Post,
                                                    &format!("http://{}/rocketchat/login",
                                                            test.as_listening.as_ref().unwrap().socket),
                                                    &payload,
                                                    &HashMap::new(),
                                                    None)
                .unwrap();
        assert!(response.contains("You are logged in. Return to your Matrix client and follow the instructions there."));
        assert!(status_code.is_success());
        let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
        assert!(message_received_by_matrix.contains("You are logged in."));
    }
}

#[test]
fn login_via_rest_api_with_invalid_payload() {
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(LOGIN_PATH,
                           handlers::RocketchatLogin {
                               successful: true,
                               rocketchat_user_id: None,
                           },
                           "login");
    rocketchat_router.get(ME_PATH, handlers::RocketchatMe { username: "Spec user".to_string() }, "me");
    let test =
        Test::new().with_rocketchat_mock().with_custom_rocketchat_routes(rocketchat_router).with_connected_admin_room().run();

    let (response, status_code) = RestApi::call(Method::Post,
                                                &format!("http://{}/rocketchat/login",
                                                        test.as_listening.as_ref().unwrap().socket),
                                                "not json",
                                                &HashMap::new(),
                                                None)
            .unwrap();
    assert!(response.contains("Could not process request, the submitted data is not valid"));
    assert_eq!(status_code, status::UnprocessableEntity);
}

#[test]
fn login_via_rest_api_with_a_non_existing_rocketchat_server() {
    let test = Test::new().run();

    let login_request = Credentials {
        matrix_user_id: UserId::try_from("@spec_user:localhost").unwrap(),
        rocketchat_username: "spec_user".to_string(),
        password: "secret".to_string(),
        rocketchat_url: "http://nonexisting.foo".to_string(),
    };
    let payload = to_string(&login_request).unwrap();

    let (response, status_code) = RestApi::call(Method::Post,
                                                &format!("http://{}/rocketchat/login",
                                                        test.as_listening.as_ref().unwrap().socket),
                                                &payload,
                                                &HashMap::new(),
                                                None)
            .unwrap();
    assert!(response.contains("No admin room found that is connected to the Rocket.Chat server http://nonexisting.foo"));
    assert_eq!(status_code, status::NotFound);
}

#[test]
fn login_via_rest_api_with_a_user_that_has_no_connected_admin_room_for_the_rocketchat_server() {
    // spec user has a conntected admin room, but other_user doesn't
    let test = Test::new().with_rocketchat_mock().with_connected_admin_room().run();

    let login_request = Credentials {
        matrix_user_id: UserId::try_from("@other_user:localhost").unwrap(),
        rocketchat_username: "other_user".to_string(),
        password: "secret".to_string(),
        rocketchat_url: test.rocketchat_mock_url.clone().unwrap(),
    };
    let payload = to_string(&login_request).unwrap();

    let (response, status_code) = RestApi::call(Method::Post,
                                                &format!("http://{}/rocketchat/login",
                                                        test.as_listening.as_ref().unwrap().socket),
                                                &payload,
                                                &HashMap::new(),
                                                None)
            .unwrap();
    let expected_respones = format!("No admin room found that is connected to the Rocket.Chat server {}",
                                    &test.rocketchat_mock_url.clone().unwrap());
    assert!(response.contains(&expected_respones));
    assert_eq!(status_code, status::NotFound);
}

#[test]
fn the_user_can_login_again_on_the_same_server_with_a_new_admin_room() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let leave_room = handlers::MatrixLeaveRoom {
        as_url: test.config.as_url.clone(),
        user_id: UserId::try_from("@rocketchat:localhost").unwrap(),
    };
    matrix_router.post(LeaveRoomEndpoint::router_path(), leave_room, "leave_room");

    let mut rocketchat_router = Router::new();
    rocketchat_router.post(LOGIN_PATH,
                           handlers::RocketchatLogin {
                               successful: true,
                               rocketchat_user_id: None,
                           },
                           "login");
    let test = test.with_matrix_routes(matrix_router)
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
    rocketchat_router.post(LOGIN_PATH,
                           handlers::RocketchatLogin {
                               successful: true,
                               rocketchat_user_id: None,
                           },
                           "login");
    let test = Test::new().with_matrix_routes(matrix_router).with_rocketchat_mock().with_connected_admin_room().run();

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
    rocketchat_router.post(LOGIN_PATH,
                           handlers::RocketchatLogin {
                               successful: true,
                               rocketchat_user_id: None,
                           },
                           "login");
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
fn the_user_gets_a_message_when_the_me_endpoint_returns_an_error() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(LOGIN_PATH,
                           handlers::RocketchatLogin {
                               successful: true,
                               rocketchat_user_id: None,
                           },
                           "login");
    rocketchat_router.get(ME_PATH,
                          handlers::RocketchatErrorResponder {
                              status: status::InternalServerError,
                              message: "Spec Error".to_string(),
                          },
                          "me");
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
    let test = Test::new().with_matrix_routes(matrix_router).with_admin_room().run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "list".to_string());

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("This room is not connected to a Rocket.Chat server"));
}
