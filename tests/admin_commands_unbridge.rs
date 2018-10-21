#![feature(try_from)]

extern crate iron;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_events;
extern crate ruma_identifiers;
extern crate serde_json;

use std::convert::TryFrom;

use iron::status;
use matrix_rocketchat::api::rocketchat::WebhookMessage;
use matrix_rocketchat::api::MatrixApi;
use matrix_rocketchat_test::{default_timeout, handlers, helpers, MessageForwarder, Test, DEFAULT_LOGGER, RS_TOKEN};
use ruma_client_api::r0::alias::delete_alias::Endpoint as DeleteAliasEndpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_client_api::r0::sync::get_state_events_for_empty_key::{self, Endpoint as GetStateEventsForEmptyKey};
use ruma_client_api::Endpoint;
use ruma_events::EventType;
use ruma_identifiers::{RoomAliasId, RoomId, UserId};
use serde_json::to_string;

#[test]
fn successfully_unbridge_a_rocketchat_room() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("bridged_channel", vec!["spec_user"]))
        .run();

    // send message to create a virtual user
    let message = WebhookMessage {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "bridged_channel_id".to_string(),
        channel_name: Some("bridged_channel".to_string()),
        user_id: "new_user_id".to_string(),
        user_name: "new_user".to_string(),
        text: "spec_message".to_string(),
    };
    let payload = to_string(&message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);

    helpers::leave_room(
        &test.config,
        RoomId::try_from("!bridged_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "unbridge bridged_channel".to_string(),
    );

    // discard welcome message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard bridged message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard message from virtual user
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("bridged_channel is now unbridged."));
}

#[test]
fn successfully_unbridge_a_private_group() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_group(("bridged_group", vec!["spec_user"]))
        .run();

    // send message to create a virtual user
    let message = WebhookMessage {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "bridged_group_id".to_string(),
        channel_name: Some("bridged_group".to_string()),
        user_id: "new_user_id".to_string(),
        user_name: "new_user".to_string(),
        text: "spec_message".to_string(),
    };
    let payload = to_string(&message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);

    helpers::leave_room(
        &test.config,
        RoomId::try_from("!bridged_group_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "unbridge bridged_group".to_string(),
    );

    // discard welcome message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard bridged message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard message from virtual user
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("bridged_group is now unbridged."));
}

#[test]
fn do_not_allow_to_unbridge_a_channel_with_other_matrix_users() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let admin_room_creator_handler = handlers::RoomStateCreate { creator: UserId::try_from("@other_user:localhost").unwrap() };
    let admin_room_creator_params = get_state_events_for_empty_key::PathParams {
        room_id: RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        event_type: EventType::RoomCreate.to_string(),
    };
    matrix_router.get(
        GetStateEventsForEmptyKey::request_path(admin_room_creator_params),
        admin_room_creator_handler,
        "get_room_creator_admin_room",
    );

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("bridged_channel", vec!["spec_user"]))
        .run();

    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    matrix_api.register("other_user".to_string()).unwrap();

    // create other admin room
    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    matrix_api
        .create_room(Some("other_admin_room".to_string()), None, &UserId::try_from("@other_user:localhost").unwrap())
        .unwrap();
    helpers::invite(
        &test.config,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
    );

    // connect other admin room
    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
        format!("connect {}", test.rocketchat_mock_url.clone().unwrap()),
    );

    // login other user
    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
        "login other_user secret".to_string(),
    );

    // bridge bridged_channel for other user
    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
        "bridge bridged_channel".to_string(),
    );

    // other_user accepts invite from bot user
    helpers::join(
        &test.config,
        RoomId::try_from("!bridged_channel_id:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
    );

    // discard welcome message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard bridged message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();

    // discard welcome message for other user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message for other user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message for other user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard bridged message for other user
    receiver.recv_timeout(default_timeout()).unwrap();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "unbridge bridged_channel".to_string(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Cannot unbdrige channel or group bridged_channel, because Matrix users"));
    assert!(message_received_by_matrix.contains("@spec_user:localhost"));
    assert!(message_received_by_matrix.contains("@other_user:localhost"));
    assert!(
        message_received_by_matrix
            .contains("are still using the room. All Matrix users have to leave a room before the room can be unbridged.",)
    );
}

#[test]
fn do_not_allow_to_unbridge_a_private_group_with_other_matrix_users() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let admin_room_creator_handler = handlers::RoomStateCreate { creator: UserId::try_from("@other_user:localhost").unwrap() };
    let admin_room_creator_params = get_state_events_for_empty_key::PathParams {
        room_id: RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        event_type: EventType::RoomCreate.to_string(),
    };
    matrix_router.get(
        GetStateEventsForEmptyKey::request_path(admin_room_creator_params),
        admin_room_creator_handler,
        "get_room_creator_admin_room",
    );

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_group(("bridged_group", vec!["spec_user"]))
        .run();

    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    matrix_api.register("other_user".to_string()).unwrap();

    // create other admin room
    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    matrix_api
        .create_room(Some("other_admin_room".to_string()), None, &UserId::try_from("@other_user:localhost").unwrap())
        .unwrap();
    helpers::invite(
        &test.config,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
    );

    // connect other admin room
    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
        format!("connect {}", test.rocketchat_mock_url.clone().unwrap()),
    );

    // login other user
    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
        "login other_user secret".to_string(),
    );

    // bridge bridged_group for other user
    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
        "bridge bridged_group".to_string(),
    );

    // other_user accepts invite from bot user
    helpers::join(
        &test.config,
        RoomId::try_from("!bridged_group_id:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
    );

    // discard welcome message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard bridged message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();

    // discard welcome message for other user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message for other user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message for other user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard bridged message for other user
    receiver.recv_timeout(default_timeout()).unwrap();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "unbridge bridged_group".to_string(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Cannot unbdrige channel or group bridged_group, because Matrix users"));
    assert!(message_received_by_matrix.contains("@spec_user:localhost"));
    assert!(message_received_by_matrix.contains("@other_user:localhost"));
    assert!(
        message_received_by_matrix
            .contains("are still using the room. All Matrix users have to leave a room before the room can be unbridged.",)
    );
}

#[test]
fn do_not_allow_to_unbridge_a_channel_with_remaining_room_aliases() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("bridged_channel", vec!["spec_user"]))
        .run();

    // send message to create a virtual user
    let message = WebhookMessage {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "bridged_channel_id".to_string(),
        channel_name: Some("bridged_channel".to_string()),
        user_id: "new_user_id".to_string(),
        user_name: "new_user".to_string(),
        text: "spec_message".to_string(),
    };
    let payload = to_string(&message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);

    helpers::add_room_alias_id(
        &test.config,
        RoomId::try_from("!bridged_channel_id:localhost").unwrap(),
        RoomAliasId::try_from("#spec_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "user_access_token",
    );

    helpers::leave_room(
        &test.config,
        RoomId::try_from("!bridged_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "unbridge bridged_channel".to_string(),
    );

    // discard welcome message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard bridged message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard message from virtual user
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains(
        "Cannot unbdrige room bridged_channel, because aliases (#spec_id:localhost) are still associated with the room. \
         All aliases have to be removed before the room can be unbridged."
    ));
}

#[test]
fn attempting_to_unbridge_a_non_existing_channel_returns_an_error() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let channels = test.channel_list();
    channels.lock().unwrap().insert("normal_channel", Vec::new());
    let test =
        test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_connected_admin_room().with_logged_in_user().run();

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
        "unbridge nonexisting_channel".to_string(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(
        message_received_by_matrix.contains("The channel or group nonexisting_channel is not bridged, cannot unbridge it.")
    );
}

#[test]
fn attempting_to_unbridge_an_channel_that_is_not_bridged_returns_an_error() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let channels = test.channel_list();
    channels.lock().unwrap().insert("normal_channel", Vec::new());
    let test =
        test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_connected_admin_room().with_logged_in_user().run();

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
        "unbridge normal_channel".to_string(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("The channel or group normal_channel is not bridged, cannot unbridge it."));
}

#[test]
fn room_is_not_unbridged_when_deleting_the_room_alias_failes() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.delete(
        DeleteAliasEndpoint::router_path(),
        handlers::MatrixErrorResponder {
            status: status::InternalServerError,
            message: "Could not delete room alias".to_string(),
        },
        "delete_room_alias",
    );
    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("bridged_channel", vec!["spec_user"]))
        .run();

    helpers::leave_room(
        &test.config,
        RoomId::try_from("!bridged_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

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
        "unbridge bridged_channel".to_string(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}
