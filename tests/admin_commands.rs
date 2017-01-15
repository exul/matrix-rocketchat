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
fn successfully_connect_rocketchat_server() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test = Test::new().with_custom_matrix_routes(matrix_router).with_rocketchat_mock().run();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           format!("connect {} spec_token", test.rocketchat_mock_url.clone().unwrap()));

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains(&format!("You are connected to {}",test.rocketchat_mock_url.clone().unwrap())));
}

#[test]
fn connect_to_incompatible_rocketchat_server_version() {}

#[test]
fn attempt_to_create_to_non_rocketchat_server() {}

#[test]
fn attempt_to_connect_to_non_existing_server() {}
