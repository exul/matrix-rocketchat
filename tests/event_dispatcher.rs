#![feature(try_from)]

extern crate iron;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_identifiers;
extern crate serde_json;

use std::convert::TryFrom;

use iron::status;
use matrix_rocketchat::api::rocketchat::v1::POST_CHAT_MESSAGE_PATH;
use matrix_rocketchat_test::{MessageForwarder, Test, default_timeout, handlers, helpers};
use router::Router;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_identifiers::{RoomId, UserId};

#[test]
fn error_message_language_falls_back_to_the_default_language_if_the_sender_is_not_found() {
    let test = Test::new();
    let mut matrix_router = test.default_matrix_routes();
    let (message_forwarder, receiver) = MessageForwarder::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = Router::new();
    rocketchat_router.post(POST_CHAT_MESSAGE_PATH,
                           handlers::RocketchatErrorResponder {
                               message: "Rocketh.Chat chat.postMessage error".to_string(),
                               status: status::InternalServerError,
                           },
                           "post_chat_message");

    let test = test.with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", "spec_user"))
        .run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!spec_channel_id:localhost").unwrap(),
                                           UserId::try_from("@unknown_user:localhost").unwrap(),
                                           "spec message".to_string());


    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard room bridged message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}
