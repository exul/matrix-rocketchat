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
use matrix_rocketchat::api::rocketchat::Message;
use matrix_rocketchat::api::rocketchat::v1::DIRECT_MESSAGES_LIST_PATH;
use matrix_rocketchat::db::Room;
use matrix_rocketchat_test::{MessageForwarder, RS_TOKEN, Test, default_timeout, handlers, helpers};
use router::Router;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::room::create_room::Endpoint as CreateRoomEndpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_identifiers::{RoomId, UserId};
use serde_json::to_string;


#[test]
fn successfully_forwards_a_direct_message() {
    let test = Test::new();
    let (create_room_forwarder, create_room_receiver) = handlers::MatrixCreateRoom::with_forwarder();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(CreateRoomEndpoint::router_path(), create_room_forwarder, "create_room");
    let mut rocketchat_router = Router::new();
    let mut direct_messages = HashMap::new();
    direct_messages.insert("spec_user_id_other_user_id", vec!["spec_user", "other_user"]);
    let direct_messages_list_handler = handlers::RocketchatDirectMessagesList {
        direct_messages: direct_messages,
        status: status::Ok,
    };
    rocketchat_router.get(DIRECT_MESSAGES_LIST_PATH, direct_messages_list_handler, "post_chat_message");

    let test = test.with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    let first_direct_message = Message {
        message_id: "spec_id_1".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey there".to_string(),
    };
    let first_direct_message_payload = to_string(&first_direct_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &first_direct_message_payload);

    // spec user accepts the invite
    helpers::join(&test.config.as_url,
                  RoomId::try_from("!1234_id:localhost").unwrap(),
                  UserId::try_from("@spec_user:localhost").unwrap());

    let create_room_message = create_room_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(create_room_message.contains("\"room_alias_name\":\"rocketchat_rc_id_spec_user_id_other_user_id\""));

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();

    let first_message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(first_message_received_by_matrix.contains("Hey there"));

    let connection = test.connection_pool.get().unwrap();
    let room = Room::find_by_rocketchat_room_id(&connection, "rc_id".to_string(), "spec_user_id_other_user_id".to_string())
        .unwrap()
        .unwrap();

    let users = room.users(&connection).unwrap();
    assert_eq!(users.len(), 3);
    assert!(users.iter().any(|u| u.matrix_user_id == UserId::try_from("@rocketchat_other_user_id_rc_id:localhost").unwrap()));
    assert!(users.iter().any(|u| u.matrix_user_id == UserId::try_from("@rocketchat_spec_user_id_rc_id:localhost").unwrap()));
    assert!(users.iter().any(|u| u.matrix_user_id == UserId::try_from("@spec_user:localhost").unwrap()));

    let second_direct_message = Message {
        message_id: "spec_id_2".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Yay".to_string(),
    };
    let second_direct_message_payload = to_string(&second_direct_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &second_direct_message_payload);

    let second_message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(second_message_received_by_matrix.contains("Yay"));
}
