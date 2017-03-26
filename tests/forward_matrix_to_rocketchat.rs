#![feature(try_from)]

extern crate iron;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate reqwest;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_identifiers;
extern crate serde_json;

use std::convert::TryFrom;

use matrix_rocketchat::api::rocketchat::v1::POST_CHAT_MESSAGE_PATH;
use matrix_rocketchat::api::rocketchat::Message;
use matrix_rocketchat_test::{MessageForwarder, RS_TOKEN, Test, default_timeout, helpers};
use router::Router;
use ruma_identifiers::{RoomId, UserId};
use serde_json::to_string;

#[test]
fn successfully_forwards_a_text_message_from_matrix_to_rocketchat() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(POST_CHAT_MESSAGE_PATH, message_forwarder, "post_chat_message");

    let test = Test::new()
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", "spec_user"))
        .run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!spec_channel_id:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "spec message".to_string());

    let message_received_by_rocketchat = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_rocketchat.contains("spec message"));
    assert!(message_received_by_rocketchat.contains("spec_channel"));
}

#[test]
fn do_not_forward_messages_from_the_bot_user_to_avoid_loops() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(POST_CHAT_MESSAGE_PATH, message_forwarder, "post_chat_message");

    let test = Test::new()
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", "spec_user"))
        .run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!spec_channel_id:localhost").unwrap(),
                                           UserId::try_from("@rocketchat:localhost").unwrap(),
                                           "spec message".to_string());

    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn do_not_forward_messages_from_virtual_user_to_avoid_loops() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(POST_CHAT_MESSAGE_PATH, message_forwarder, "post_chat_message");

    let test = Test::new()
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", "spec_user"))
        .run();

    // create the virtual user by simulating a message from Rocket.Chat
    let message = Message {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: "spec_channel".to_string(),
        user_id: "virtual_spec_user_id".to_string(),
        user_name: "virtual_spec_user".to_string(),
        text: "spec_message".to_string(),
    };
    let payload = to_string(&message).unwrap();
    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!spec_channel_id:localhost").unwrap(),
                                           UserId::try_from("@rocketchat_virtual_spec_user_id_1:localhost").unwrap(),
                                           "spec message".to_string());

    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn ignore_messages_from_unbridged_rooms() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(POST_CHAT_MESSAGE_PATH, message_forwarder, "post_chat_message");

    let test = Test::new()
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!not_bridged_channel_id:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "spec message".to_string());

    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn ignore_messages_with_a_message_type_that_is_not_supported() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(POST_CHAT_MESSAGE_PATH, message_forwarder, "post_chat_message");

    let test = Test::new()
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", "spec_user"))
        .run();

    // create the virtual user by simulating a message from Rocket.Chat
    let message = Message {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: "spec_channel".to_string(),
        user_id: "virtual_spec_user_id".to_string(),
        user_name: "virtual_spec_user".to_string(),
        text: "spec_message".to_string(),
    };
    let payload = to_string(&message).unwrap();
    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);

    helpers::send_emote_message_from_matrix(&test.config.as_url,
                                            RoomId::try_from("!spec_channel_id:localhost").unwrap(),
                                            UserId::try_from("@spec_user:localhost").unwrap(),
                                            "emote message".to_string());

    assert!(receiver.recv_timeout(default_timeout()).is_err());
}
