#![feature(try_from)]

extern crate ruma_client_api;
extern crate ruma_identifiers;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate router;
extern crate serde_json;

use std::collections::HashMap;
use std::convert::TryFrom;

use matrix_rocketchat::api::rocketchat::Message;
use matrix_rocketchat::api::rocketchat::v1::{LOGIN_PATH, ME_PATH};
use matrix_rocketchat::db::Room;
use matrix_rocketchat_test::{MessageForwarder, RS_TOKEN, Test, default_timeout, handlers, helpers};
use ruma_client_api::Endpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_identifiers::{RoomId, UserId};
use router::Router;
use serde_json::to_string;

#[test]
fn successfully_unbridge_a_rocketchat_room() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("bridged_channel", "spec_user"))
        .run();

    // send message to create a virtual user
    let message = Message {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "bridged_channel_id".to_string(),
        channel_name: "bridged_channel".to_string(),
        user_id: "new_user_id".to_string(),
        user_name: "new_user".to_string(),
        text: "spec_message".to_string(),
    };
    let payload = to_string(&message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);

    helpers::join(&test.config.as_url,
                  RoomId::try_from("!bridged_channel_id:localhost").unwrap(),
                  UserId::try_from("@rocketchat_new_user_id_1:localhost").unwrap());

    helpers::leave_room(&test.config.as_url,
                        RoomId::try_from("!bridged_channel_id:localhost").unwrap(),
                        UserId::try_from("@spec_user:localhost").unwrap());

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "unbridge bridged_channel".to_string());

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

    let connection = test.connection_pool.get().unwrap();
    let room = Room::find(&connection, &RoomId::try_from("!bridged_channel_id:localhost").unwrap()).unwrap();
    assert!(!room.is_bridged);

    let users_in_room = room.users(&connection).unwrap();
    assert!(users_in_room.iter().any(|u| u.matrix_user_id == UserId::try_from("@rocketchat:localhost").unwrap()));
    assert!(users_in_room.iter().any(|u| u.matrix_user_id == UserId::try_from("@rocketchat_new_user_id_1:localhost").unwrap()));
    assert!(!users_in_room.iter().any(|u| u.matrix_user_id == UserId::try_from("@spec_user:localhost").unwrap()));
}

#[test]
fn do_not_allow_to_unbridge_a_channel_with_other_matrix_users() {
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
    rocketchat_router.get(ME_PATH, handlers::RocketchatMe { username: "spec_user".to_string() }, "me");


    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("bridged_channel", "spec_user"))
        .run();

    // create other admin room
    helpers::create_admin_room(&test.config.as_url,
                               RoomId::try_from("!other_admin:localhost").unwrap(),
                               UserId::try_from("@other_user:localhost").unwrap(),
                               UserId::try_from("@rocketchat:localhost").unwrap());

    // connect other admin room
    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!other_admin:localhost").unwrap(),
                                           UserId::try_from("@other_user:localhost").unwrap(),
                                           format!("connect {}", test.rocketchat_mock_url.clone().unwrap()));

    // login other user
    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!other_admin:localhost").unwrap(),
                                           UserId::try_from("@other_user:localhost").unwrap(),
                                           "login other_user secret".to_string());

    // bridge bridged_channel for other user
    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!other_admin:localhost").unwrap(),
                                           UserId::try_from("@other_user:localhost").unwrap(),
                                           "bridge bridged_channel".to_string());

    // other_user accepts invite from bot user
    helpers::join(&test.config.as_url,
                  RoomId::try_from("!bridged_channel_id:localhost").unwrap(),
                  UserId::try_from("@other_user:localhost").unwrap());

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

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "unbridge bridged_channel".to_string());

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Cannot unbdrige room bridged_channel, because Matrix users (@other_user:localhost, @spec_user:localhost) are still using the room."));
}

#[test]
fn attempting_to_unbridge_a_non_existing_channel_returns_an_error() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut channels = HashMap::new();
    channels.insert("normal_channel", Vec::new());
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_custom_channel_list(channels)
        .run();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "unbridge nonexisting_channel".to_string());

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("The channel nonexisting_channel is not bridged, cannot unbridge it."));
}

#[test]
fn attempting_to_unbridge_an_channel_that_is_not_bridged_returns_an_error() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut channels = HashMap::new();
    channels.insert("normal_channel", Vec::new());
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_custom_channel_list(channels)
        .run();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "unbridge normal_channel".to_string());

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("The channel normal_channel is not bridged, cannot unbridge it."));
}
