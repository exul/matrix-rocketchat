extern crate http;
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
use std::sync::{Arc, Mutex};

use http::Method;
use iron::status;
use matrix_rocketchat::api::rocketchat::v1::LOGIN_PATH;
use matrix_rocketchat::api::{MatrixApi, RequestData, RestApi};
use matrix_rocketchat::models::Credentials;
use matrix_rocketchat::models::{RocketchatServer, UserOnRocketchatServer};
use matrix_rocketchat_test::{default_timeout, handlers, helpers, MessageForwarder, Test, DEFAULT_LOGGER};
use reqwest::StatusCode;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_client_api::Endpoint;
use ruma_identifiers::{RoomId, UserId};
use serde_json::to_string;

#[test]
fn sucessfully_login_via_chat_mesage() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_connected_admin_room().run();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "login spec_user secret".to_string(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    let connection = test.connection_pool.get().unwrap();
    let rocketchat_server = RocketchatServer::find(&connection, &test.rocketchat_mock_url.clone().unwrap()).unwrap();
    let user_on_rocketchat_server =
        UserOnRocketchatServer::find(&connection, &UserId::try_from("@spec_user:localhost").unwrap(), rocketchat_server.id)
            .unwrap();
    assert_eq!(user_on_rocketchat_server.rocketchat_auth_token.unwrap(), "spec_auth_token");

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("You are logged in."));
}

#[test]
fn wrong_password_when_logging_in_via_chat_message() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.post(
        LOGIN_PATH,
        handlers::RocketchatLogin { successful: false, rocketchat_user_id: Arc::new(Mutex::new(None)) },
        "login",
    );
    let test = test
        .with_matrix_routes(matrix_router)
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .run();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "login spec_user wrong_password".to_string(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Authentication failed!"));
}

#[test]
fn login_multiple_times_via_chat_message() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_connected_admin_room().run();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    for _ in 0..2 {
        helpers::send_room_message_from_matrix(
            &test.config.as_url,
            RoomId::try_from("!admin_room_id:localhost").unwrap(),
            UserId::try_from("@spec_user:localhost").unwrap(),
            "login spec_user secret".to_string(),
        );

        let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
        assert!(message_received_by_matrix.contains("You are logged in."));
    }
}

#[test]
fn sucessfully_login_via_rest_api() {
    let test = Test::new().with_rocketchat_mock().with_connected_admin_room().run();

    let login_request = Credentials {
        user_id: UserId::try_from("@spec_user:localhost").unwrap(),
        rocketchat_username: "spec_user".to_string(),
        password: "secret".to_string(),
        rocketchat_url: test.rocketchat_mock_url.clone().unwrap(),
    };
    let payload = to_string(&login_request).unwrap();
    let (response, status_code) = RestApi::call(
        &Method::POST,
        &format!("http://{}/rocketchat/login", test.as_listening.as_ref().unwrap().socket),
        RequestData::Body(payload),
        &HashMap::new(),
        None,
    )
    .unwrap();

    assert!(response.contains(
        "You are logged in. Return to your Matrix client and \
         enter help in the admin room for more instructions.",
    ));
    assert!(status_code.is_success());

    let connection = test.connection_pool.get().unwrap();
    let rocketchat_server = RocketchatServer::find(&connection, &test.rocketchat_mock_url.clone().unwrap()).unwrap();
    let user_on_rocketchat_server =
        UserOnRocketchatServer::find(&connection, &UserId::try_from("@spec_user:localhost").unwrap(), rocketchat_server.id)
            .unwrap();
    assert_eq!(user_on_rocketchat_server.rocketchat_auth_token.unwrap(), "spec_auth_token");
}

#[test]
fn wrong_password_when_logging_in_via_rest_api() {
    let test = Test::new();

    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.post(
        LOGIN_PATH,
        handlers::RocketchatLogin { successful: false, rocketchat_user_id: Arc::new(Mutex::new(None)) },
        "login",
    );
    let test = test.with_custom_rocketchat_routes(rocketchat_router).with_rocketchat_mock().with_connected_admin_room().run();

    let login_request = Credentials {
        user_id: UserId::try_from("@spec_user:localhost").unwrap(),
        rocketchat_username: "spec_user".to_string(),
        password: "wrong_password".to_string(),
        rocketchat_url: test.rocketchat_mock_url.clone().unwrap(),
    };
    let payload = to_string(&login_request).unwrap();
    let (response, status_code) = RestApi::call(
        &Method::POST,
        &format!("http://{}/rocketchat/login", test.as_listening.as_ref().unwrap().socket),
        RequestData::Body(payload),
        &HashMap::new(),
        None,
    )
    .unwrap();
    assert!(response.contains("Authentication failed!"));
    assert_eq!(status_code, StatusCode::UNAUTHORIZED);
}

#[test]
fn login_multiple_times_via_rest_message() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_connected_admin_room().run();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    let login_request = Credentials {
        user_id: UserId::try_from("@spec_user:localhost").unwrap(),
        rocketchat_username: "spec_user".to_string(),
        password: "secret".to_string(),
        rocketchat_url: test.rocketchat_mock_url.clone().unwrap(),
    };
    let payload = to_string(&login_request).unwrap();

    for _ in 0..2 {
        let (response, status_code) = RestApi::call(
            &Method::POST,
            &format!("http://{}/rocketchat/login", test.as_listening.as_ref().unwrap().socket),
            RequestData::Body(payload.clone()),
            &HashMap::new(),
            None,
        )
        .unwrap();
        assert!(response.contains(
            "You are logged in. Return to your Matrix client and enter help in the admin room for more instructions.",
        ));
        assert!(status_code.is_success());
    }
}

#[test]
fn login_via_rest_api_with_invalid_payload() {
    let test = Test::new();
    let test = test.with_rocketchat_mock().with_connected_admin_room().run();

    let (response, status_code) = RestApi::call(
        &Method::POST,
        &format!("http://{}/rocketchat/login", test.as_listening.as_ref().unwrap().socket),
        RequestData::Body("not json".to_string()),
        &HashMap::new(),
        None,
    )
    .unwrap();
    assert!(response.contains("Could not process request, the submitted data is not valid"));
    assert_eq!(status_code, StatusCode::UNPROCESSABLE_ENTITY);
}

#[test]
fn login_via_rest_api_with_a_non_existing_rocketchat_server() {
    let test = Test::new().run();

    let login_request = Credentials {
        user_id: UserId::try_from("@spec_user:localhost").unwrap(),
        rocketchat_username: "spec_user".to_string(),
        password: "secret".to_string(),
        rocketchat_url: "http://nonexisting.foo".to_string(),
    };
    let payload = to_string(&login_request).unwrap();

    let (response, status_code) = RestApi::call(
        &Method::POST,
        &format!("http://{}/rocketchat/login", test.as_listening.as_ref().unwrap().socket),
        RequestData::Body(payload),
        &HashMap::new(),
        None,
    )
    .unwrap();
    let expected_respones = "Rocket.Chat server http://nonexisting.foo not found, it is probably not connected.";
    assert!(response.contains(expected_respones));
    assert_eq!(status_code, StatusCode::NOT_FOUND);
}

#[test]
fn the_user_can_login_again_on_the_same_server_with_a_new_admin_room() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let test =
        test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_connected_admin_room().with_logged_in_user().run();

    helpers::leave_room(
        &test.config,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    // create other admin room
    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    matrix_api
        .create_room(Some("other_admin_room".to_string()), None, &UserId::try_from("@spec_user:localhost").unwrap())
        .unwrap();

    helpers::invite(
        &test.config,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        format!("connect {}", test.rocketchat_mock_url.clone().unwrap()),
    );

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "login spec_user secret".to_string(),
    );

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
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.post(LOGIN_PATH, handlers::InvalidJsonResponse { status: status::Ok }, "login");
    let test = test
        .with_matrix_routes(matrix_router)
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .run();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "login spec_user secret".to_string(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn the_user_gets_a_message_when_the_login_response_cannot_be_deserialized() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.post(LOGIN_PATH, handlers::InvalidJsonResponse { status: status::Ok }, "login");
    let test = test
        .with_matrix_routes(matrix_router)
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .run();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "login spec_user secret".to_string(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn the_user_gets_a_message_when_the_login_returns_an_error() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.post(
        LOGIN_PATH,
        handlers::RocketchatErrorResponder { status: status::InternalServerError, message: "Spec Error".to_string() },
        "login",
    );
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

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn attempt_to_login_when_the_admin_room_is_not_connected() {
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
