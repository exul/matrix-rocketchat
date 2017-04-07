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
use matrix_rocketchat::api::rocketchat::v1::{LOGIN_PATH, ME_PATH, USERS_INFO_PATH};
use matrix_rocketchat::db::Room;
use matrix_rocketchat_test::{MessageForwarder, Test, default_timeout, handlers, helpers};
use router::Router;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::membership::invite_user::Endpoint as InviteEndpoint;
use ruma_client_api::r0::room::create_room::Endpoint as CreateRoomEndpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_client_api::r0::send::send_state_event_for_empty_key::Endpoint as SendStateEventForEmptyKeyEndpoint;
use ruma_client_api::r0::sync::get_member_events::{self, Endpoint as GetMemberEventsEndpoint};
use ruma_identifiers::{RoomId, UserId};


#[test]
fn successfully_bridge_a_rocketchat_room() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let (invite_forwarder, invite_receiver) = MessageForwarder::new();
    let (state_forwarder, state_receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(CreateRoomEndpoint::router_path(),
                       handlers::MatrixCreateRoom { room_id: RoomId::try_from("!joined_channel_id:localhost").unwrap() },
                       "create_room");
    matrix_router.put(SendStateEventForEmptyKeyEndpoint::router_path(), state_forwarder, "send_state_event_for_key");
    matrix_router.post(InviteEndpoint::router_path(), invite_forwarder, "invite_user");
    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user", "user_1", "user_2", "user_3"]);

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

    let invite_spec_user = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(invite_spec_user.contains("@spec_user:localhost"));
    let invite_virtual_spec_user = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(invite_virtual_spec_user.contains("rocketchat_spec_user_id_1:localhost"));
    let invite_user_1 = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(invite_user_1.contains("@rocketchat_user_1_id_1:localhost"));
    let invite_user_2 = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(invite_user_2.contains("@rocketchat_user_2_id_1:localhost"));
    let invite_user_3 = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(invite_user_3.contains("@rocketchat_user_3_id_1:localhost"));

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("joined_channel is now bridged."));

    let set_room_name_received_by_matrix = state_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(set_room_name_received_by_matrix.contains("Admin Room (Rocket.Chat)"));

    // only moderators and admins can invite other users
    let power_levels_received_by_matrix = state_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(power_levels_received_by_matrix.contains("\"invite\":50"));
    assert!(power_levels_received_by_matrix.contains("\"kick\":50"));
    assert!(power_levels_received_by_matrix.contains("\"ban\":50"));
    assert!(power_levels_received_by_matrix.contains("\"redact\":50"));
    assert!(power_levels_received_by_matrix.contains("@rocketchat:localhost"));

    // users accepts invite from bot user
    helpers::join(&test.config.as_url,
                  RoomId::try_from("!joined_channel_id:localhost").unwrap(),
                  UserId::try_from("@spec_user:localhost").unwrap());

    helpers::join(&test.config.as_url,
                  RoomId::try_from("!joined_channel_id:localhost").unwrap(),
                  UserId::try_from("@rocketchat_spec_user_id_1:localhost").unwrap());

    helpers::join(&test.config.as_url,
                  RoomId::try_from("!joined_channel_id:localhost").unwrap(),
                  UserId::try_from("@rocketchat_user_1_id_1:localhost").unwrap());

    helpers::join(&test.config.as_url,
                  RoomId::try_from("!joined_channel_id:localhost").unwrap(),
                  UserId::try_from("@rocketchat_user_2_id_1:localhost").unwrap());

    helpers::join(&test.config.as_url,
                  RoomId::try_from("!joined_channel_id:localhost").unwrap(),
                  UserId::try_from("@rocketchat_user_3_id_1:localhost").unwrap());

    let connection = test.connection_pool.get().unwrap();
    let room = Room::find(&connection, &RoomId::try_from("!joined_channel_id:localhost").unwrap()).unwrap();
    assert_eq!(room.display_name, "joined_channel");

    let users = room.users(&connection).unwrap();
    assert!(users.iter().any(|u| u.matrix_user_id == UserId::try_from("@rocketchat:localhost").unwrap()));
    assert!(users.iter().any(|u| u.matrix_user_id == UserId::try_from("@spec_user:localhost").unwrap()));
    assert!(users.iter().any(|u| u.matrix_user_id == UserId::try_from("@rocketchat_spec_user_id_1:localhost").unwrap()));
    assert!(users.iter().any(|u| u.matrix_user_id == UserId::try_from("@rocketchat_user_1_id_1:localhost").unwrap()));
    assert!(users.iter().any(|u| u.matrix_user_id == UserId::try_from("@rocketchat_user_2_id_1:localhost").unwrap()));
    assert!(users.iter().any(|u| u.matrix_user_id == UserId::try_from("@rocketchat_user_3_id_1:localhost").unwrap()));
}

#[test]
fn susccessfully_bridge_a_rocketchat_room_that_an_other_user_already_bridged() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let (invite_forwarder, invite_receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(CreateRoomEndpoint::router_path(),
                       handlers::MatrixCreateRoom { room_id: RoomId::try_from("!joined_channel_id:localhost").unwrap() },
                       "create_room");
    let other_room_members = handlers::RoomMembers {
        room_id: RoomId::try_from("!admin:localhost").unwrap(),
        members: vec![UserId::try_from("@other_user:localhost").unwrap(), UserId::try_from("@rocketchat:localhost").unwrap()],
    };
    let path_params = get_member_events::PathParams { room_id: RoomId::try_from("!other_admin:localhost").unwrap() };
    matrix_router.get(GetMemberEventsEndpoint::request_path(path_params), other_room_members, "other_room_members");
    matrix_router.post(InviteEndpoint::router_path(), invite_forwarder, "invite_user");

    let mut rocketchat_router = Router::new();
    rocketchat_router.post(LOGIN_PATH,
                           handlers::RocketchatLogin {
                               successful: true,
                               rocketchat_user_id: None,
                           },
                           "login");
    rocketchat_router.get(ME_PATH, handlers::RocketchatMe { username: "spec_user".to_string() }, "me");
    rocketchat_router.get(USERS_INFO_PATH, handlers::RocketchatUsersInfo {}, "users_info");

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
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "bridge joined_channel".to_string());

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

    // spec user received success message
    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("joined_channel is now bridged."));

    // other user received success message
    let other_message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(other_message_received_by_matrix.contains("joined_channel is now bridged."));

    // spec_user accepts invite from bot user
    helpers::join(&test.config.as_url,
                  RoomId::try_from("!joined_channel_id:localhost").unwrap(),
                  UserId::try_from("@spec_user:localhost").unwrap());

    // other_user accepts invite from bot user
    helpers::join(&test.config.as_url,
                  RoomId::try_from("!joined_channel_id:localhost").unwrap(),
                  UserId::try_from("@other_user:localhost").unwrap());

    let spec_user_invite_received_by_matrix = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(spec_user_invite_received_by_matrix.contains("@spec_user:localhost"));

    let virtual_spec_user_invite_received_by_matrix = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(virtual_spec_user_invite_received_by_matrix.contains("@rocketchat_spec_user_id_1:localhost"));

    let other_user_invite_received_by_matrix = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(other_user_invite_received_by_matrix.contains("@rocketchat_other_user_id_1:localhost"));

    let connection = test.connection_pool.get().unwrap();
    let room = Room::find(&connection, &RoomId::try_from("!joined_channel_id:localhost").unwrap()).unwrap();
    assert_eq!(room.display_name, "joined_channel");

    let users_in_room = room.users(&connection).unwrap();
    assert!(users_in_room.iter().any(|u| u.matrix_user_id == UserId::try_from("@rocketchat:localhost").unwrap()));
    assert!(users_in_room.iter().any(|u| u.matrix_user_id == UserId::try_from("@spec_user:localhost").unwrap()));
    assert!(users_in_room.iter().any(|u| u.matrix_user_id == UserId::try_from("@other_user:localhost").unwrap()));
}

#[test]
fn susccessfully_bridge_a_rocketchat_room_that_was_unbridged_before() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let (invite_forwarder, invite_receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(InviteEndpoint::router_path(), invite_forwarder, "invite_user");

    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("joined_channel", "spec_user"))
        .run();

    helpers::leave_room(&test.config.as_url,
                        RoomId::try_from("!joined_channel_id:localhost").unwrap(),
                        UserId::try_from("@spec_user:localhost").unwrap());

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "unbridge joined_channel".to_string());

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "bridge joined_channel".to_string());

    // spec_user accepts invite from bot user
    helpers::join(&test.config.as_url,
                  RoomId::try_from("!joined_channel_id:localhost").unwrap(),
                  UserId::try_from("@spec_user:localhost").unwrap());

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard bridge message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard unbridge message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("joined_channel is now bridged."));

    let invite_received_by_matrix = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(invite_received_by_matrix.contains("@spec_user:localhost"));

    let connection = test.connection_pool.get().unwrap();
    let room = Room::find(&connection, &RoomId::try_from("!joined_channel_id:localhost").unwrap()).unwrap();
    assert_eq!(room.display_name, "joined_channel");
    assert!(room.is_bridged);

    let users_in_room = room.users(&connection).unwrap();
    assert!(users_in_room.iter().any(|u| u.matrix_user_id == UserId::try_from("@rocketchat:localhost").unwrap()));
    assert!(users_in_room.iter().any(|u| u.matrix_user_id == UserId::try_from("@spec_user:localhost").unwrap()));
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

    // spec_user accepts invite from bot user
    helpers::join(&test.config.as_url,
                  RoomId::try_from("!joined_channel_id:localhost").unwrap(),
                  UserId::try_from("@spec_user:localhost").unwrap());

    // discard successful bridge message
    receiver.recv_timeout(default_timeout()).unwrap();

    helpers::send_room_message_from_matrix(&test.config.as_url,
                                           RoomId::try_from("!admin:localhost").unwrap(),
                                           UserId::try_from("@spec_user:localhost").unwrap(),
                                           "bridge joined_channel".to_string());

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
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
fn the_user_gets_a_message_when_setting_the_powerlevels_failes() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(CreateRoomEndpoint::router_path(),
                       handlers::MatrixCreateRoom { room_id: RoomId::try_from("!joined_channel_id:localhost").unwrap() },
                       "create_room");
    matrix_router.put(SendStateEventForEmptyKeyEndpoint::router_path(),
                      handlers::MatrixConditionalErrorResponder {
                          status: status::InternalServerError,
                          message: "Could not set power levels".to_string(),
                          conditional_content: "invite",
                      },
                      "set_power_levels");
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
fn the_user_gets_a_message_when_inviting_the_user_failes() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(CreateRoomEndpoint::router_path(),
                       handlers::MatrixCreateRoom { room_id: RoomId::try_from("!joined_channel_id:localhost").unwrap() },
                       "create_room");
    matrix_router.post(InviteEndpoint::router_path(),
                       handlers::MatrixErrorResponder {
                           status: status::InternalServerError,
                           message: "Could not invite user".to_string(),
                       },
                       "invite");
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
    matrix_router.post(CreateRoomEndpoint::router_path(), handlers::InvalidJsonResponse { status: status::Ok }, "create_room");
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
