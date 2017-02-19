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
fn sucessfully_bridge_a_rocketchat_room() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = Router::new();
    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user"]);
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
                                           "connect joined_channel".to_string());

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("joined_channel is now bridged."));
}

#[test]
fn do_not_allow_to_bridge_channels_that_the_user_has_not_joined_on_the_rocketchat_server() {
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
                                           "connect normal_channel".to_string());

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("You have to join the channel on the Rocket.Chat server before you can bridge it."));
}

#[test]
fn attempting_to_bridge_a_non_existing_channel_returns_an_error() {
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
                                           "connect nonexisting_channel".to_string());

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("No channel with the name nonexisting_channel found."));
}

#[test]
fn attempting_to_bridge_an_already_bridged_channel_returns_an_error() {
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
                                           "connect joined_channel".to_string());

    // discard successful bridge message
    receiver.recv_timeout(default_timeout()).unwrap();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "connect joined_channel".to_string());

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("The channel joined_channel is already bridged."));
}
