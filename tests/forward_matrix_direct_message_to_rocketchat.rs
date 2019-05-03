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
use matrix_rocketchat::api::rocketchat::v1::{CHAT_POST_MESSAGE_PATH, DM_LIST_PATH};
use matrix_rocketchat::api::rocketchat::WebhookMessage;
use matrix_rocketchat_test::{default_timeout, handlers, helpers, MessageForwarder, Test, RS_TOKEN};
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_client_api::Endpoint;
use ruma_identifiers::{RoomId, UserId};
use serde_json::to_string;

#[test]
fn successfully_forwards_a_direct_message_to_rocketchat() {
    let test = Test::new();
    let (matrix_message_forwarder, matrix_receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), matrix_message_forwarder, "send_message_event");
    let mut rocketchat_router = test.default_rocketchat_routes();
    let mut direct_messages = HashMap::new();
    direct_messages.insert("spec_user_id_other_user_id", vec!["spec_user", "other_user"]);
    let direct_messages_list_handler =
        handlers::RocketchatDirectMessagesList { direct_messages: direct_messages, status: status::Ok };
    rocketchat_router.get(DM_LIST_PATH, direct_messages_list_handler, "direct_messages_list");
    let (rocketchat_message_forwarder, rocketchat_receiver) = MessageForwarder::new();
    rocketchat_router.post(CHAT_POST_MESSAGE_PATH, rocketchat_message_forwarder, "post_text_message");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    // direct message from Rocket.Chat to trigger the room creation
    let direct_message_from_rocketchat = WebhookMessage {
        message_id: "spec_id_1".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey there".to_string(),
    };
    let direct_message_from_rocketchat_payload = to_string(&direct_message_from_rocketchat).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &direct_message_from_rocketchat_payload);

    helpers::join(
        &test.config,
        RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    // discard welcome message
    matrix_receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    matrix_receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    matrix_receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = matrix_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Hey there"));

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "It's so nice to hear from you after such a long time".to_string(),
    );

    let message_received_by_rocketchat = rocketchat_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_rocketchat.contains("It's so nice to hear from you after such a long time"));
}

#[test]
fn direct_messages_are_not_forwarded_if_no_matching_matrix_user_is_found() {
    let test = Test::new();
    let (matrix_message_forwarder, matrix_receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), matrix_message_forwarder, "send_message_event");
    let mut rocketchat_router = test.default_rocketchat_routes();
    let mut direct_messages = HashMap::new();
    direct_messages.insert("spec_user_id_other_user_id", vec!["spec_user", "other_user"]);
    let direct_messages_list_handler =
        handlers::RocketchatDirectMessagesList { direct_messages: direct_messages, status: status::Ok };
    rocketchat_router.get(DM_LIST_PATH, direct_messages_list_handler, "direct_messages_list");
    let (rocketchat_message_forwarder, rocketchat_receiver) = MessageForwarder::new();
    rocketchat_router.post(CHAT_POST_MESSAGE_PATH, rocketchat_message_forwarder, "post_text_message");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    // direct message from Rocket.Chat to trigger the room creation
    let direct_message_from_rocketchat = WebhookMessage {
        message_id: "spec_id_1".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey there".to_string(),
    };
    let direct_message_from_rocketchat_payload = to_string(&direct_message_from_rocketchat).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &direct_message_from_rocketchat_payload);

    // discard welcome message
    matrix_receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    matrix_receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    matrix_receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = matrix_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Hey there"));

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap(),
        UserId::try_from("@rocketchat_rcid_other_user_id:localhost").unwrap(),
        "It's so nice to hear from you after such a long time".to_string(),
    );

    let message_received_by_rocketchat = rocketchat_receiver.recv_timeout(default_timeout());
    // the message is not forwarded
    assert!(message_received_by_rocketchat.is_err());
}

#[test]
fn direct_messages_are_not_forwarded_if_no_matching_virtual_user_is_found() {
    let test = Test::new();
    let (matrix_message_forwarder, matrix_receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), matrix_message_forwarder, "send_message_event");
    let mut rocketchat_router = test.default_rocketchat_routes();
    let mut direct_messages = HashMap::new();
    direct_messages.insert("spec_user_id_other_user_id", vec!["spec_user", "other_user"]);
    let direct_messages_list_handler =
        handlers::RocketchatDirectMessagesList { direct_messages: direct_messages, status: status::Ok };
    rocketchat_router.get(DM_LIST_PATH, direct_messages_list_handler, "direct_messages_list");
    let (rocketchat_message_forwarder, rocketchat_receiver) = MessageForwarder::new();
    rocketchat_router.post(CHAT_POST_MESSAGE_PATH, rocketchat_message_forwarder, "post_text_message");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    // direct message from Rocket.Chat to trigger the room creation
    let direct_message_from_rocketchat = WebhookMessage {
        message_id: "spec_id_1".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey there".to_string(),
    };
    let direct_message_from_rocketchat_payload = to_string(&direct_message_from_rocketchat).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &direct_message_from_rocketchat_payload);

    helpers::join(
        &test.config,
        RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    // discard welcome message
    matrix_receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    matrix_receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    matrix_receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = matrix_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Hey there"));

    helpers::leave_room(
        &test.config,
        RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap(),
        UserId::try_from("@rocketchat_rcid_other_user_id:localhost").unwrap(),
    );

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "It's so nice to hear from you after such a long time".to_string(),
    );

    let message_received_by_rocketchat = rocketchat_receiver.recv_timeout(default_timeout());
    // the message is not forwarded
    assert!(message_received_by_rocketchat.is_err());
}
