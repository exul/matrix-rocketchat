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
use matrix_rocketchat::api::rocketchat::v1::{LOGIN_PATH, ME_PATH};
use matrix_rocketchat::db::Room;
use matrix_rocketchat_test::{MessageForwarder, Test, default_timeout, handlers, helpers};
use router::Router;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::room::create_room::Endpoint as CreateRoomEndpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_client_api::r0::sync::get_member_events::{self, Endpoint as GetMemberEventsEndpoint};
use ruma_identifiers::{RoomId, UserId};


#[test]
fn successfully_bridge_a_rocketchat_room() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(CreateRoomEndpoint::router_path(),
                       handlers::MatrixCreateRoom { room_id: RoomId::try_from("!joined_channel_id:localhost").unwrap() },
                       "create_room");
    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user"]);

    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_custom_channel_list(channels)
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
                                           "bridge joined_channel".to_string());

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("joined_channel is now bridged."));

    let connection = test.connection_pool.get().unwrap();
    let room = Room::find(&connection, &RoomId::try_from("!joined_channel_id:localhost").unwrap()).unwrap();
    assert_eq!(room.display_name, "joined_channel");

    let users_in_room = room.users(&connection).unwrap();
    assert!(users_in_room.iter().any(|u| u.matrix_user_id == UserId::try_from("@rocketchat:localhost").unwrap()));
    assert!(users_in_room.iter().any(|u| u.matrix_user_id == UserId::try_from("@spec_user:localhost").unwrap()));
}

#[test]
fn successfully_bridge_a_rocketchat_room_that_an_other_user_already_bridged() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(CreateRoomEndpoint::router_path(),
                       handlers::MatrixCreateRoom { room_id: RoomId::try_from("!joined_channel_id:localhost").unwrap() },
                       "create_room");
    let other_room_members = handlers::RoomMembers {
        room_id: RoomId::try_from("!admin:localhost").unwrap(),
        members: vec![UserId::try_from("@other_user:localhost").unwrap(),
                      UserId::try_from("@rocketchat:localhost").unwrap()],
    };
    let path_params = get_member_events::PathParams { room_id: RoomId::try_from("!other_admin:localhost").unwrap() };
    matrix_router.get(GetMemberEventsEndpoint::request_path(path_params),
                      other_room_members,
                      "other_room_members");

    let mut rocketchat_router = Router::new();
    rocketchat_router.post(LOGIN_PATH,
                           handlers::RocketchatLogin {
                               successful: true,
                               rocketchat_user_id: None,
                           },
                           "login");
    rocketchat_router.get(ME_PATH, handlers::RocketchatMe { username: "spec_user".to_string() }, "me");

    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user", "other_user"]);

    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_custom_channel_list(channels)
        .run();

    // login spec user
    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "login spec_user secret".to_string());

    // create other admin room
    helpers::create_admin_room(&test.config.as_url,
                               RoomId::try_from("!other_admin:localhost").unwrap(),
                               UserId::try_from("@other_user:localhost").unwrap(),
                               UserId::try_from("@rocketchat:localhost").unwrap());

    // connect other admin room
    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!other_admin:localhost").unwrap(),
                                           UserId::try_from("@other_user:localhost").unwrap(),
                                           format!("connect {}", test.rocketchat_mock_url.clone().unwrap()));


    // login other user
    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!other_admin:localhost").unwrap(),
                                           UserId::try_from("@other_user:localhost").unwrap(),
                                           "login other_user secret".to_string());

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!other_admin:localhost").unwrap(),
                                           UserId::try_from("@other_user:localhost").unwrap(),
                                           "bridge joined_channel".to_string());


    // discard welcome message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message for spec user
    receiver.recv_timeout(default_timeout()).unwrap();

    // discard welcome message for other user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message for other user
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message for other user
    receiver.recv_timeout(default_timeout()).unwrap();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "bridge joined_channel".to_string());

    // spec user received success message
    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("joined_channel is now bridged."));

    // other user received success message
    let other_message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(other_message_received_by_matrix.contains("joined_channel is now bridged."));

    let connection = test.connection_pool.get().unwrap();
    let room = Room::find(&connection, &RoomId::try_from("!joined_channel_id:localhost").unwrap()).unwrap();
    assert_eq!(room.display_name, "joined_channel");

    let users_in_room = room.users(&connection).unwrap();
    assert!(users_in_room.iter().any(|u| u.matrix_user_id == UserId::try_from("@rocketchat:localhost").unwrap()));
    assert!(users_in_room.iter().any(|u| u.matrix_user_id == UserId::try_from("@spec_user:localhost").unwrap()));
    assert!(users_in_room.iter().any(|u| u.matrix_user_id == UserId::try_from("@other_user:localhost").unwrap()));
}

#[test]
fn do_not_allow_to_bridge_channels_that_the_user_has_not_joined_on_the_rocketchat_server() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut channels = HashMap::new();
    channels.insert("normal_channel", Vec::new());

    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_custom_channel_list(channels)
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
                                           "bridge normal_channel".to_string());

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("You have to join the channel normal_channel on the Rocket.Chat server before you can bridge it."));
}

#[test]
fn attempting_to_bridge_a_non_existing_channel_returns_an_error() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut channels = HashMap::new();
    channels.insert("normal_channel", Vec::new());
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_custom_channel_list(channels)
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
                                           "bridge nonexisting_channel".to_string());

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("No channel with the name nonexisting_channel found."));
}

#[test]
fn attempting_to_bridge_an_already_bridged_channel_returns_an_error() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(CreateRoomEndpoint::router_path(),
                       handlers::MatrixCreateRoom { room_id: RoomId::try_from("!joined_channel_id:localhost").unwrap() },
                       "create_room");
    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user"]);
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_custom_channel_list(channels)
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
                                           "bridge joined_channel".to_string());

    // discard successful bridge message
    receiver.recv_timeout(default_timeout()).unwrap();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "bridge joined_channel".to_string());

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    println!("{}", message_received_by_matrix);
    assert!(message_received_by_matrix.contains("The channel joined_channel is already bridged."));
}

#[test]
fn the_user_gets_a_message_when_creating_the_room_failes() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(CreateRoomEndpoint::router_path(),
                       handlers::MatrixErrorResponder {
                           status: status::InternalServerError,
                           message: "Could not create room".to_string(),
                       },
                       "create_room");
    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user"]);
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_custom_channel_list(channels)
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
                                           "bridge joined_channel".to_string());

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn the_user_gets_a_message_when_the_create_room_response_cannot_be_deserialized() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(CreateRoomEndpoint::router_path(),
                       handlers::InvalidJsonResponse { status: status::Ok },
                       "create_room");
    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user"]);
    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_custom_channel_list(channels)
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
                                           "bridge joined_channel".to_string());

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}
