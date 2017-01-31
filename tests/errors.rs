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
use std::error::Error;

use iron::status;
use matrix_rocketchat::api::MatrixApi;
use matrix_rocketchat::db::UserInRoom;
use matrix_rocketchat_test::{DEFAULT_LOGGER, MessageForwarder, Test, default_timeout, handlers};
use router::Router;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_identifiers::{RoomId, UserId};

#[test]
fn error_descriptions_from_the_error_chain_are_passed_to_the_outer_error() {
    let test = Test::new().run();

    let connection = test.connection_pool.get().unwrap();
    let not_found_error = UserInRoom::find(&connection,
                                           &UserId::try_from("@nonexisting:localhost").unwrap(),
                                           &RoomId::try_from("!some_room:localhost").unwrap())
        .unwrap_err();

    assert_eq!(not_found_error.description(), "Error when selecting a record");
}

#[test]
fn errors_when_sending_a_message_are_handled_gracefully() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put("/_matrix/client/r0/rooms/!room:localhost/send/:event_type/:txn_id",
                      message_forwarder,
                      "send_message_event_success");
    matrix_router.put(SendMessageEventEndpoint::router_path(),
                      handlers::InvalidJsonResponse { status: status::InternalServerError },
                      "send_message_event_fail");
    let test = Test::new().with_matrix_routes(matrix_router).with_admin_room().run();

    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    matrix_api.send_text_message_event(RoomId::try_from("!room:localhost").unwrap(),
                                 UserId::try_from("@user:localhost").unwrap(),
                                 "Message after an error".to_string())
        .unwrap();

    // the welcome message fails, but the next message is received
    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Message after an error"));
}
