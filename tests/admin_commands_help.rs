#![feature(try_from)]

extern crate iron;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_events;
extern crate ruma_identifiers;

use std::convert::TryFrom;

use matrix_rocketchat::api::MatrixApi;
use matrix_rocketchat_test::{default_timeout, handlers, helpers, MessageForwarder, Test, DEFAULT_LOGGER, RS_TOKEN};
use ruma_client_api::Endpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_client_api::r0::sync::get_state_events_for_empty_key::{self, Endpoint as GetStateEventsForEmptyKey};
use ruma_events::EventType;
use ruma_identifiers::{RoomId, UserId};

#[test]
fn help_command_when_not_connected_and_no_one_else_has_connected_a_server_yet() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_admin_room().run();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "help".to_string(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains(
        "You have to connect this room to a Rocket.Chat server. To do so you can \
         either use an already connected server (if there is one) or connect to a \
         new server.",
    ));
    assert!(message_received_by_matrix.contains("No Rocket.Chat server is connected yet."));
}

#[test]
fn help_command_when_not_connected_and_someone_else_has_connected_a_server_already() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let admin_room_creator_handler = handlers::RoomStateCreate {
        creator: UserId::try_from("@other_user:localhost").unwrap(),
    };
    let admin_room_creator_params = get_state_events_for_empty_key::PathParams {
        room_id: RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        event_type: EventType::RoomCreate.to_string(),
    };
    matrix_router.get(
        GetStateEventsForEmptyKey::request_path(admin_room_creator_params),
        admin_room_creator_handler,
        "get_room_creator_admin_room",
    );

    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_admin_room().run();

    // other user creates admin room
    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    let other_user_id = UserId::try_from("@other_user:localhost").unwrap();
    matrix_api.create_room(Some("other_admin_room".to_string()), None, &other_user_id).unwrap();

    helpers::invite(
        &test.config,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
    );

    // other user connects the Rocket.Chat server
    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
        format!("connect {} {} otherid", test.rocketchat_mock_url.clone().unwrap(), RS_TOKEN),
    );

    // spec user gets the already connected server list
    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "help".to_string(),
    );

    // discard other users welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard other users connect messsage
    receiver.recv_timeout(default_timeout()).unwrap();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains(
        "You have to connect this room to a Rocket.Chat server. To do so you can \
         either use an already connected server (if there is one) or connect to a \
         new server.",
    ));
    assert!(message_received_by_matrix.contains(&test.rocketchat_mock_url.clone().unwrap()));
}

#[test]
fn help_command_when_connected() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_connected_admin_room().run();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "help".to_string(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    let expected_curl_command = format!("curl http://{}", test.as_listening.as_ref().unwrap().socket);
    assert!(message_received_by_matrix.contains(
        "You have to login before you can use the application service, \
         there are two ways to do that",
    ));
    assert!(message_received_by_matrix.contains(&expected_curl_command));
}

#[test]
fn help_command_when_logged_in() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test =
        test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_connected_admin_room().with_logged_in_user().run();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "help".to_string(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("`list` Lists all public rooms from the Rocket.Chat server"));
    assert!(message_received_by_matrix.contains("`bridge rocketchatroomnname` Bridge a Rocket.Chat room"));
    assert!(message_received_by_matrix.contains(
        "`unbridge rocketchatroomnname` Unbridge a Rocket.Chat room \
         (messages are no longer forwarded)",
    ));
}
