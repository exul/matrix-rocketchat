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
use matrix_rocketchat_test::{MessageForwarder, Test, default_timeout, helpers};
use router::Router;
use ruma_identifiers::{RoomId, UserId};

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
