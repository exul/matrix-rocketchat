#![feature(try_from)]

extern crate iron;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate reqwest;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_identifiers;

use std::collections::HashMap;
use std::convert::TryFrom;

use iron::status;
use matrix_rocketchat::api::RestApi;
use matrix_rocketchat_test::{HS_TOKEN, MessageForwarder, Test, default_timeout, handlers, helpers};
use reqwest::Method;
use router::Router;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::membership::leave_room::Endpoint as LeaveRoomEndpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_identifiers::{EventId, RoomId, UserId};

#[test]
fn error_message_language_falls_back_to_the_default_language_if_the_sender_is_not_a_bridge_user() {
    let mut matrix_router = Router::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder);
    let test = Test::new().with_custom_matrix_routes(matrix_router).run();

    helpers::join(&test.config.as_url,
                  RoomId::try_from("!admin:localhost").expect("Could not create room ID"),
                  UserId::try_from("@unkown_user:localhost").expect("Could not create user ID"));

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}
