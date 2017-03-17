#![feature(try_from)]

extern crate iron;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate reqwest;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_identifiers;
extern crate serde_json;

use std::collections::HashMap;
use std::convert::TryFrom;

use iron::status;
use matrix_rocketchat::api::RestApi;
use matrix_rocketchat::api::rocketchat::Message;
use matrix_rocketchat::db::{Room, UserOnRocketchatServer};
use matrix_rocketchat_test::{MessageForwarder, RS_TOKEN, Test, default_timeout, handlers, helpers};
use router::Router;
use reqwest::{Method, StatusCode};
use ruma_client_api::Endpoint;
use ruma_client_api::r0::membership::invite_user::Endpoint as InviteUserEndpoint;
use ruma_client_api::r0::membership::join_room_by_id::Endpoint as JoinRoomByIdEndpoint;
use ruma_client_api::r0::profile::set_display_name::Endpoint as SetDisplayNameEndpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_identifiers::{RoomId, UserId};
use serde_json::to_string;

#[test]
fn successfully_forwards_a_text_message_from_a_user_that_is_not_registered_on_matrix() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let (invite_forwarder, invite_receiver) = MessageForwarder::new();
    let (join_forwarder, join_receiver) = MessageForwarder::new();
    let (set_display_name_forwarder, set_display_name_receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(InviteUserEndpoint::router_path(), invite_forwarder, "invite_user");
    matrix_router.post(JoinRoomByIdEndpoint::router_path(), join_forwarder, "join_room_id");
    matrix_router.put(SetDisplayNameEndpoint::router_path(),
                      set_display_name_forwarder,
                      "set_display_name");

    let mut channels = HashMap::new();
    channels.insert("spec_channel", vec!["spec_user"]);

    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", "spec_user"))
        .run();

    let message = Message {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: "spec_channel".to_string(),
        user_id: "new_user_id".to_string(),
        user_name: "new_spec_user".to_string(),
        text: "spec_message".to_string(),
    };
    let payload = to_string(&message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);

    // receive the invite message
    let invite_message = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(invite_message.contains("@rocketchat_new_user_id_1:localhost"));

    // discard admin room join
    join_receiver.recv_timeout(default_timeout()).unwrap();
    // receive the join message
    assert!(join_receiver.recv_timeout(default_timeout()).is_ok());

    // receive set display name
    assert!(set_display_name_receiver.recv_timeout(default_timeout()).is_ok());

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard room bridged message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("spec_message"));

    let connection = test.connection_pool.get().unwrap();
    let admin_room = Room::find(&connection, &RoomId::try_from("!admin:localhost").unwrap()).unwrap();
    let rocketchat_server_id = admin_room.rocketchat_server_id.unwrap();
    let bridged_room = Room::find_by_rocketchat_room_id(&connection, rocketchat_server_id, "spec_channel_id".to_string())
        .unwrap()
        .unwrap();

    // the bot, the user who bridged the channel and the virtual user are in the channel
    let users = bridged_room.users(&connection).unwrap();
    assert_eq!(users.len(), 3);

    let bot_user_id = UserId::try_from("@rocketchat:localhost").unwrap();
    let spec_user_id = UserId::try_from("@spec_user:localhost").unwrap();
    let users_iter = users.iter();
    let user_ids = users_iter.filter_map(|u| if u.matrix_user_id != bot_user_id && u.matrix_user_id != spec_user_id {
                                             Some(u.matrix_user_id.clone())
                                         } else {
                                             None
                                         })
        .collect::<Vec<UserId>>();
    let new_user_id = user_ids.iter().next().unwrap();

    // the virtual user was create with the Rocket.Chat user ID
    let user_on_rocketchat = UserOnRocketchatServer::find(&connection, new_user_id, rocketchat_server_id).unwrap();
    assert_eq!(user_on_rocketchat.rocketchat_user_id.unwrap(), "new_user_id".to_string());

    let second_message = Message {
        message_id: "spec_id_2".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: "spec_channel".to_string(),
        user_id: "new_user_id".to_string(),
        user_name: "new_spec_user".to_string(),
        text: "spec_message 2".to_string(),
    };
    let second_payload = to_string(&second_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &second_payload);

    // make sure the display name is only set once
    assert!(set_display_name_receiver.recv_timeout(default_timeout()).is_err());

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("spec_message 2"));
}

#[test]
fn successfully_forwards_a_text_message_from_a_user_that_is_registered_on_matrix() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let (invite_forwarder, invite_receiver) = MessageForwarder::new();
    let (join_forwarder, join_receiver) = MessageForwarder::new();
    let (set_display_name_forwarder, set_display_name_receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(InviteUserEndpoint::router_path(), invite_forwarder, "invite_user");
    matrix_router.post(JoinRoomByIdEndpoint::router_path(), join_forwarder, "join_room_id");
    matrix_router.put(SetDisplayNameEndpoint::router_path(),
                      set_display_name_forwarder,
                      "set_display_name");


    let mut channels = HashMap::new();
    channels.insert("spec_channel", vec!["spec_user"]);

    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", "spec_user"))
        .run();

    let message = Message {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: "spec_channel".to_string(),
        user_id: "spec_user_id".to_string(),
        user_name: "spec_user".to_string(),
        text: "spec_message".to_string(),
    };
    let payload = to_string(&message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);

    // receive the invite message
    let invite_message = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(invite_message.contains("@rocketchat_spec_user_id_1:localhost"));

    // discard admin room join
    join_receiver.recv_timeout(default_timeout()).unwrap();
    // receive the join message
    assert!(join_receiver.recv_timeout(default_timeout()).is_ok());

    // receive set display name
    assert!(set_display_name_receiver.recv_timeout(default_timeout()).is_ok());

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard room bridged message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("spec_message"));

    let connection = test.connection_pool.get().unwrap();
    let admin_room = Room::find(&connection, &RoomId::try_from("!admin:localhost").unwrap()).unwrap();
    let rocketchat_server_id = admin_room.rocketchat_server_id.unwrap();
    let bridged_room = Room::find_by_rocketchat_room_id(&connection, rocketchat_server_id, "spec_channel_id".to_string())
        .unwrap()
        .unwrap();

    // the bot, the user who bridged the channel and the virtual user are in the channel
    let users = bridged_room.users(&connection).unwrap();
    assert_eq!(users.len(), 3);

    let bot_user_id = UserId::try_from("@rocketchat:localhost").unwrap();
    let spec_user_id = UserId::try_from("@spec_user:localhost").unwrap();
    let users_iter = users.iter();
    let user_ids = users_iter.filter_map(|u| if u.matrix_user_id != bot_user_id && u.matrix_user_id != spec_user_id {
                                             Some(u.matrix_user_id.clone())
                                         } else {
                                             None
                                         })
        .collect::<Vec<UserId>>();
    let new_user_id = user_ids.iter().next().unwrap();

    // the virtual user was create with the Rocket.Chat user ID because the exiting matrix user
    // cannot be used since the application service can only impersonate virtual users.
    let user_on_rocketchat = UserOnRocketchatServer::find(&connection, new_user_id, rocketchat_server_id).unwrap();
    assert_eq!(user_on_rocketchat.rocketchat_user_id.unwrap(), "spec_user_id".to_string());

    let second_message = Message {
        message_id: "spec_id_2".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: "spec_channel".to_string(),
        user_id: "spec_user_id".to_string(),
        user_name: "spec_user".to_string(),
        text: "spec_message 2".to_string(),
    };
    let second_payload = to_string(&second_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &second_payload);

    // make sure the display name is only set once
    assert!(set_display_name_receiver.recv_timeout(default_timeout()).is_err());

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("spec_message 2"));
}

#[test]
fn update_the_display_name_when_the_user_changed_it_on_the_rocketchat_server() {
    let (set_display_name_forwarder, set_display_name_receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SetDisplayNameEndpoint::router_path(),
                      set_display_name_forwarder,
                      "set_display_name");


    let mut channels = HashMap::new();
    channels.insert("spec_channel", vec!["spec_user"]);

    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", "spec_user"))
        .run();

    let message = Message {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: "spec_channel".to_string(),
        user_id: "spec_user_id".to_string(),
        user_name: "spec_user".to_string(),
        text: "spec_message".to_string(),
    };
    let payload = to_string(&message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);

    let display_name_message = set_display_name_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(display_name_message.contains("spec_user"));

    let second_message_with_new_username = Message {
        message_id: "spec_id_2".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: "spec_channel".to_string(),
        user_id: "spec_user_id".to_string(),
        user_name: "spec_user_new".to_string(),
        text: "spec_message 2".to_string(),
    };
    let second_payload_with_new_username = to_string(&second_message_with_new_username).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &second_payload_with_new_username);

    let new_display_name_message = set_display_name_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(new_display_name_message.contains("spec_user_new"));

    let connection = test.connection_pool.get().unwrap();
    let admin_room = Room::find(&connection, &RoomId::try_from("!admin:localhost").unwrap()).unwrap();
    let rocketchat_server_id = admin_room.rocketchat_server_id.unwrap();
    let user_on_rocketchat_server = UserOnRocketchatServer::find_by_rocketchat_user_id(&connection,
                                                                                       rocketchat_server_id,
                                                                                       "spec_user_id".to_string(),
                                                                                       true)
            .unwrap()
            .unwrap();
    assert_eq!(user_on_rocketchat_server.rocketchat_username.unwrap(),
               "spec_user_new".to_string());
}

#[test]
fn message_is_forwarded_even_if_setting_the_display_name_failes() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.put(SetDisplayNameEndpoint::router_path(),
                      handlers::MatrixErrorResponder {
                          status: status::InternalServerError,
                          message: "Could not set display name".to_string(),
                      },
                      "set_display_name");

    let mut channels = HashMap::new();
    channels.insert("spec_channel", vec!["spec_user"]);

    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", "spec_user"))
        .run();

    let message = Message {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: "spec_channel".to_string(),
        user_id: "spec_user_id".to_string(),
        user_name: "spec_user".to_string(),
        text: "spec_message".to_string(),
    };
    let payload = to_string(&message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard room bridged message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("spec_message"));
}

#[test]
fn rocketchat_sends_mal_formatted_json() {
    let test = Test::new().run();
    let payload = "bad_json";

    let url = format!("{}/rocketchat", &test.config.as_url);

    let params = HashMap::new();
    let (_, status_code) = RestApi::call(Method::Post, &url, payload, &params, None).unwrap();

    assert_eq!(status_code, StatusCode::UnprocessableEntity)
}

#[test]
fn no_message_is_forwarded_when_inviting_the_user_failes() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(InviteUserEndpoint::router_path(),
                       handlers::MatrixErrorResponder {
                           status: status::InternalServerError,
                           message: "Could not invite user".to_string(),
                       },
                       "invite_user");
    let mut channels = HashMap::new();
    channels.insert("spec_channel", vec!["spec_user"]);

    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", "spec_user"))
        .run();

    let message = Message {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: "spec_channel".to_string(),
        user_id: "new_user_id".to_string(),
        user_name: "new_spec_user".to_string(),
        text: "spec_message".to_string(),
    };
    let payload = to_string(&message).unwrap();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard room bridged message
    receiver.recv_timeout(default_timeout()).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);

    // no message is forwarded
    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn ignore_messages_to_a_room_that_is_not_bridged() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let mut channels = HashMap::new();
    channels.insert("not_bridged_channel", vec!["spec_user"]);

    let test = Test::new()
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    let message = Message {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "not_bridged_channel_id".to_string(),
        channel_name: "not_bridged_channel".to_string(),
        user_id: "new_user_id".to_string(),
        user_name: "new_spec_user".to_string(),
        text: "spec_message".to_string(),
    };
    let payload = to_string(&message).unwrap();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);

    // no message is forwarded
    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn returns_unauthorized_when_the_rs_token_is_missing() {
    let test = Test::new().run();
    let message = Message {
        message_id: "spec_id".to_string(),
        token: None,
        channel_id: "spec_channel_id".to_string(),
        channel_name: "spec_channel".to_string(),
        user_id: "spec_user_id".to_string(),
        user_name: "spec_user".to_string(),
        text: "spec_message".to_string(),
    };
    let payload = to_string(&message).unwrap();

    let url = format!("{}/rocketchat", &test.config.as_url);

    let params = HashMap::new();
    let (_, status_code) = RestApi::call(Method::Post, &url, &payload, &params, None).unwrap();

    assert_eq!(status_code, StatusCode::Unauthorized)
}

#[test]
fn returns_forbidden_when_the_rs_token_does_not_match_a_server() {
    let test = Test::new().run();
    let message = Message {
        message_id: "spec_id".to_string(),
        token: Some("wrong_token".to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: "spec_channel".to_string(),
        user_id: "spec_user_id".to_string(),
        user_name: "spec_user".to_string(),
        text: "spec_message".to_string(),
    };
    let payload = to_string(&message).unwrap();

    let url = format!("{}/rocketchat", &test.config.as_url);

    let params = HashMap::new();
    let (_, status_code) = RestApi::call(Method::Post, &url, &payload, &params, None).unwrap();

    assert_eq!(status_code, StatusCode::Forbidden)
}
