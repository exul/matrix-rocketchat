#![feature(try_from)]

extern crate matrix_rocketchat_test;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_identifiers;

use std::convert::TryFrom;

use matrix_rocketchat_test::{MessageForwarder, Test, default_timeout, helpers};
use ruma_client_api::Endpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_identifiers::{RoomId, UserId};


#[test]
fn unknown_commands_from_the_admin_room_are_ignored() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let test = test.with_matrix_routes(matrix_router).with_admin_room().run();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "bogus command".to_string(),
    );

    // we don't get a message, because the command is ignored and no error occurs
    receiver.recv_timeout(default_timeout()).is_err();
}

#[test]
fn unknown_content_types_from_the_admin_room_are_ignored() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let test = test.with_matrix_routes(matrix_router).with_admin_room().run();

    helpers::send_emote_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "emote message".to_string(),
    );

    // we don't get a message, because unknown content types are ignored and no error occurs
    receiver.recv_timeout(default_timeout()).is_err();
}

#[test]
fn messages_from_the_bot_user_are_ignored() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let test = test.with_matrix_routes(matrix_router).with_admin_room().run();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
        "bot message".to_string(),
    );

    // we don't get a message, because messages from the bot user are ignored and no error occurs
    receiver.recv_timeout(default_timeout()).is_err();
}
