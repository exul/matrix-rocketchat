#![feature(try_from)]

extern crate matrix_rocketchat_test;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_identifiers;

use std::convert::TryFrom;

use matrix_rocketchat_test::{MessageForwarder, Test, default_timeout, helpers};
use router::Router;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_identifiers::{RoomId, UserId};

#[test]
fn error_message_language_falls_back_to_the_default_language_if_the_sender_is_not_a_bridge_user() {
    let mut matrix_router = Router::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test = Test::new().with_custom_matrix_routes(matrix_router).run();

    helpers::join(&test.config.as_url,
                  RoomId::try_from("!admin:localhost").unwrap(),
                  UserId::try_from("@unkown_user:localhost").unwrap());

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}
