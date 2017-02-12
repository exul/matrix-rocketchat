#![feature(try_from)]

extern crate iron;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_identifiers;

use std::collections::HashMap;
use std::convert::TryFrom;

use iron::status;
use matrix_rocketchat::api::rocketchat::v1::CHANNELS_LIST_PATH;
use matrix_rocketchat_test::{MessageForwarder, Test, default_timeout, handlers, helpers};
use router::Router;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_identifiers::{RoomId, UserId};


#[test]
fn sucessfully_list_rocketchat_rooms() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = Router::new();
    let mut channels = HashMap::new();
    channels.insert("normal_channel", Vec::new());
    rocketchat_router.get(CHANNELS_LIST_PATH,
                          handlers::RocketchatChannelsList {
                              status: status::Ok,
                              channels: channels,
                          },
                          "channels_list");
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
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
                                           "list".to_string());

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    println!("MSG: {}", message_received_by_matrix);
    assert!(message_received_by_matrix.contains("normal_channel"));
    assert!(message_received_by_matrix.contains("*joined_channel*"));
    assert!(message_received_by_matrix.contains("**bridged_channel**"));
    assert!(message_received_by_matrix.contains("***joined_bridged_room***"));
}
