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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use iron::{status, Chain};
use matrix_rocketchat::api::rocketchat::v1::{Attachment, Message as RocketchatMessage, UserInfo, CHAT_GET_MESSAGE_PATH};
use matrix_rocketchat::api::rocketchat::WebhookMessage;
use matrix_rocketchat::api::{MatrixApi, RequestData, RestApi};
use matrix_rocketchat::models::Room;
use matrix_rocketchat_test::{default_timeout, handlers, helpers, MessageForwarder, Test, DEFAULT_LOGGER, RS_TOKEN};
use reqwest::{Method, StatusCode};
use ruma_client_api::r0::account::register::Endpoint as RegisterEndpoint;
use ruma_client_api::r0::media::create_content::Endpoint as CreateContentEndpoint;
use ruma_client_api::r0::membership::invite_user::Endpoint as InviteUserEndpoint;
use ruma_client_api::r0::membership::join_room_by_id::Endpoint as JoinRoomByIdEndpoint;
use ruma_client_api::r0::profile::set_display_name::Endpoint as SetDisplayNameEndpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_client_api::Endpoint;
use ruma_identifiers::{RoomId, UserId};
use serde_json::to_string;

#[test]
fn successfully_forwards_a_text_message_from_rocketchat_to_matrix_when_the_user_is_not_registered_on_matrix() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let (register_forwarder, register_receiver) = handlers::MatrixRegister::with_forwarder();
    let (invite_forwarder, invite_receiver) = handlers::MatrixInviteUser::with_forwarder(test.config.as_url.clone());
    let (join_forwarder, join_receiver) = handlers::MatrixJoinRoom::with_forwarder(test.config.as_url.clone(), true);
    let (set_display_name_forwarder, set_display_name_receiver) = handlers::MatrixSetDisplayName::with_forwarder();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(RegisterEndpoint::router_path(), register_forwarder, "register");
    matrix_router.post(InviteUserEndpoint::router_path(), invite_forwarder, "invite_user");
    matrix_router.post(JoinRoomByIdEndpoint::router_path(), join_forwarder, "join_room");
    matrix_router.put(SetDisplayNameEndpoint::router_path(), set_display_name_forwarder, "set_display_name");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    let message = WebhookMessage {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: Some("spec_channel".to_string()),
        user_id: "new_user_id".to_string(),
        user_name: "new_spec_user".to_string(),
        text: "spec_message".to_string(),
    };
    let payload = to_string(&message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);

    // discard bot registration
    register_receiver.recv_timeout(default_timeout()).unwrap();

    // discard spec user registration
    register_receiver.recv_timeout(default_timeout()).unwrap();

    // virtual user was registered
    let register_message = register_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(register_message.contains("\"username\":\"rocketchat_rcid_spec_user_id\""));

    // discard admin room invite
    invite_receiver.recv_timeout(default_timeout()).unwrap();
    // receive the invite messages
    let spec_user_invite_message = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(spec_user_invite_message.contains("@spec_user:localhost"));
    let virtual_spec_user_invite_message = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(virtual_spec_user_invite_message.contains("@rocketchat_rcid_spec_user_id:localhost"));
    let new_user_invite_message = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(new_user_invite_message.contains("@rocketchat_rcid_new_user_id:localhost"));

    // receive the join message
    assert!(join_receiver.recv_timeout(default_timeout()).is_ok());

    // receive set display
    let set_display_name_spec_user = set_display_name_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(set_display_name_spec_user.contains("spec_user"));
    let set_display_name_virtual_spec_user = set_display_name_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(set_display_name_virtual_spec_user.contains("new_spec_user"));

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

    // the bot, the user who bridged the channel and two virtual user are in the channel
    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    let room_id = RoomId::try_from("!spec_channel_id:localhost").unwrap();
    let room = Room::new(&test.config, &DEFAULT_LOGGER, &(*matrix_api), room_id);
    let user_ids = room.user_ids(None).unwrap();

    assert_eq!(user_ids.len(), 4);

    let second_message = WebhookMessage {
        message_id: "spec_id_2".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: Some("spec_channel".to_string()),
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
fn successfully_forwards_a_text_message_from_rocketchat_to_matrix_when_the_user_is_registered_on_matrix() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let (register_forwarder, register_receiver) = handlers::MatrixRegister::with_forwarder();
    let (invite_forwarder, invite_receiver) = handlers::MatrixInviteUser::with_forwarder(test.config.as_url.clone());
    let (join_forwarder, join_receiver) = handlers::MatrixJoinRoom::with_forwarder(test.config.as_url.clone(), true);
    let (set_display_name_forwarder, set_display_name_receiver) = handlers::MatrixSetDisplayName::with_forwarder();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(RegisterEndpoint::router_path(), register_forwarder, "register");
    matrix_router.post(InviteUserEndpoint::router_path(), invite_forwarder, "invite_user");
    matrix_router.post(JoinRoomByIdEndpoint::router_path(), join_forwarder, "join_room");
    matrix_router.put(SetDisplayNameEndpoint::router_path(), set_display_name_forwarder, "set_display_name");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    let message = WebhookMessage {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: Some("spec_channel".to_string()),
        user_id: "spec_user_id".to_string(),
        user_name: "spec_user".to_string(),
        text: "spec_message".to_string(),
    };
    let payload = to_string(&message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);

    // discard admin room invite
    invite_receiver.recv_timeout(default_timeout()).unwrap();
    // receive the invite messages
    let spec_user_invite_message = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(spec_user_invite_message.contains("@spec_user:localhost"));
    let virtual_user_invite_message = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(virtual_user_invite_message.contains("@rocketchat_rcid_spec_user_id:localhost"));

    // discard admin room join
    join_receiver.recv_timeout(default_timeout()).unwrap();
    // receive the join message
    assert!(join_receiver.recv_timeout(default_timeout()).is_ok());

    // receive set display name
    let set_display_name_spec_user = set_display_name_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(set_display_name_spec_user.contains("spec_user"));

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

    // the bot, the user who bridged the channel and the virtual user are in the channel
    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    let room_id = RoomId::try_from("!spec_channel_id:localhost").unwrap();
    let room = Room::new(&test.config, &DEFAULT_LOGGER, &(*matrix_api), room_id);
    let user_ids = room.user_ids(None).unwrap();
    assert_eq!(user_ids.len(), 3);

    // discard bot registration
    register_receiver.recv_timeout(default_timeout()).unwrap();

    // discard spec user registration
    register_receiver.recv_timeout(default_timeout()).unwrap();

    // the virtual user was create with the Rocket.Chat user ID because the exiting matrix user
    // cannot be used since the application service can only impersonate virtual users.
    let register_message = register_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(register_message.contains("\"username\":\"rocketchat_rcid_spec_user_id\""));

    let second_message = WebhookMessage {
        message_id: "spec_id_2".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: Some("spec_channel".to_string()),
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
fn successfully_forwards_a_image_from_rocketchat_to_matrix_when_the_user_is_not_registered_on_matrix() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let uploaded_files = Arc::new(Mutex::new(Vec::new()));
    let (create_content_forwarder, create_content_receiver) =
        handlers::MatrixCreateContentHandler::with_forwarder(Arc::clone(&uploaded_files));
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(CreateContentEndpoint::router_path(), create_content_forwarder, "create_content");

    let attachments = vec![Attachment {
        description: "Spec image".to_string(),
        image_size: Some(100),
        image_type: Some("image/png".to_string()),
        image_url: Some("/file-upload/image.png".to_string()),
        title: "Spec titel".to_string(),
    }];
    let rocketchat_message = Arc::new(Mutex::new(Some(RocketchatMessage {
        id: "spec_id".to_string(),
        rid: "spec_rid".to_string(),
        msg: "".to_string(),
        ts: "2017-12-12 11:11".to_string(),
        attachments: Some(attachments),
        u: UserInfo {
            id: "spec_user_id".to_string(),
            username: "spec_sender".to_string(),
            name: "spec sender".to_string(),
        },
        mentions: Vec::new(),
        channels: Vec::new(),
        updated_at: "2017-12-12 11:11".to_string(),
    })));
    let rocketchat_message_responder = handlers::RocketchatMessageResponder {
        message: rocketchat_message,
    };
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.get(CHAT_GET_MESSAGE_PATH, rocketchat_message_responder, "get_chat_message");
    let mut files = HashMap::new();
    files.insert("image.png".to_string(), b"image".to_vec());
    rocketchat_router.get(
        "/file-upload/:filename",
        handlers::RocketchatFileResponder {
            files: files,
        },
        "get_file",
    );

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard room bridged message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message = WebhookMessage {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: Some("spec_channel".to_string()),
        user_id: "new_user_id".to_string(),
        user_name: "new_spec_user".to_string(),
        text: "Uploaded an image".to_string(),
    };
    let payload = to_string(&message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);

    let file = create_content_receiver.recv_timeout(default_timeout()).unwrap();
    // this would contain the image data, but for the test this was just a string converted to bytes.
    assert_eq!(file, "image");

    let message = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message.contains("Spec titel"));
    let files = uploaded_files.lock().unwrap();
    let file_id = files.first().unwrap();
    assert!(message.contains(&format!("mxc://localhost/{}", file_id)));
}

#[test]
fn do_not_forward_an_image_message_when_there_are_no_attachments() {
    let test = Test::new();
    let uploaded_files = Arc::new(Mutex::new(Vec::new()));
    let (message_forwarder, receiver) = MessageForwarder::new();
    let (create_content_forwarder, create_content_receiver) =
        handlers::MatrixCreateContentHandler::with_forwarder(Arc::clone(&uploaded_files));
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(CreateContentEndpoint::router_path(), create_content_forwarder, "create_content");

    let rocketchat_message = Arc::new(Mutex::new(Some(RocketchatMessage {
        id: "spec_id".to_string(),
        rid: "spec_rid".to_string(),
        msg: "".to_string(),
        ts: "2017-12-12 11:11".to_string(),
        attachments: None,
        u: UserInfo {
            id: "spec_user_id".to_string(),
            username: "spec_sender".to_string(),
            name: "spec sender".to_string(),
        },
        mentions: Vec::new(),
        channels: Vec::new(),
        updated_at: "2017-12-12 11:11".to_string(),
    })));
    let rocketchat_message_responder = handlers::RocketchatMessageResponder {
        message: rocketchat_message,
    };
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.get(CHAT_GET_MESSAGE_PATH, rocketchat_message_responder, "get_chat_message");
    let mut files = HashMap::new();
    files.insert("image.png".to_string(), b"image".to_vec());
    rocketchat_router.get(
        "/file-upload/:filename",
        handlers::RocketchatFileResponder {
            files: files,
        },
        "get_file",
    );

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard room bridged message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message = WebhookMessage {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: Some("spec_channel".to_string()),
        user_id: "new_user_id".to_string(),
        user_name: "new_spec_user".to_string(),
        text: "Uploaded an image".to_string(),
    };
    let payload = to_string(&message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);

    assert!(create_content_receiver.recv_timeout(default_timeout()).is_err());
    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn do_not_forward_an_image_message_when_there_are_is_an_error_when_getting_the_message() {
    let test = Test::new();
    let uploaded_files = Arc::new(Mutex::new(Vec::new()));
    let (message_forwarder, receiver) = MessageForwarder::new();
    let (create_content_forwarder, create_content_receiver) =
        handlers::MatrixCreateContentHandler::with_forwarder(Arc::clone(&uploaded_files));
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(CreateContentEndpoint::router_path(), create_content_forwarder, "create_content");

    let rocketchat_error_responder = handlers::RocketchatErrorResponder {
        message: "Could not get image".to_string(),
        status: status::InternalServerError,
    };
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.get(CHAT_GET_MESSAGE_PATH, rocketchat_error_responder, "get_chat_message");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard room bridged message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message = WebhookMessage {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: Some("spec_channel".to_string()),
        user_id: "new_user_id".to_string(),
        user_name: "new_spec_user".to_string(),
        text: "Uploaded an image".to_string(),
    };
    let payload = to_string(&message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);

    assert!(create_content_receiver.recv_timeout(default_timeout()).is_err());
    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn do_not_forward_an_image_message_when_there_are_is_an_error_when_getting_the_file() {
    let test = Test::new();
    let uploaded_files = Arc::new(Mutex::new(Vec::new()));
    let (message_forwarder, receiver) = MessageForwarder::new();
    let (create_content_forwarder, create_content_receiver) =
        handlers::MatrixCreateContentHandler::with_forwarder(Arc::clone(&uploaded_files));
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(CreateContentEndpoint::router_path(), create_content_forwarder, "create_content");

    let attachments = vec![Attachment {
        description: "Spec image".to_string(),
        image_size: Some(100),
        image_type: Some("image/png".to_string()),
        image_url: Some("/file-upload/image.png".to_string()),
        title: "Spec titel".to_string(),
    }];
    let rocketchat_message = Arc::new(Mutex::new(Some(RocketchatMessage {
        id: "spec_id".to_string(),
        rid: "spec_rid".to_string(),
        msg: "".to_string(),
        ts: "2017-12-12 11:11".to_string(),
        attachments: Some(attachments),
        u: UserInfo {
            id: "spec_user_id".to_string(),
            username: "spec_sender".to_string(),
            name: "spec sender".to_string(),
        },
        mentions: Vec::new(),
        channels: Vec::new(),
        updated_at: "2017-12-12 11:11".to_string(),
    })));
    let rocketchat_message_responder = handlers::RocketchatMessageResponder {
        message: rocketchat_message,
    };
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.get(CHAT_GET_MESSAGE_PATH, rocketchat_message_responder, "get_chat_message");
    let mut files = HashMap::new();
    files.insert("image.png".to_string(), b"image".to_vec());
    rocketchat_router.get(
        "/file-upload/:filename",
        handlers::RocketchatErrorResponder {
            message: "Could not get file".to_string(),
            status: status::InternalServerError,
        },
        "get_file",
    );

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard room bridged message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message = WebhookMessage {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: Some("spec_channel".to_string()),
        user_id: "new_user_id".to_string(),
        user_name: "new_spec_user".to_string(),
        text: "Uploaded an image".to_string(),
    };
    let payload = to_string(&message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);

    assert!(create_content_receiver.recv_timeout(default_timeout()).is_err());
    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn update_the_display_name_when_the_user_changed_it_on_the_rocketchat_server() {
    let test = Test::new();
    let (set_display_name_forwarder, set_display_name_receiver) = handlers::MatrixSetDisplayName::with_forwarder();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SetDisplayNameEndpoint::router_path(), set_display_name_forwarder, "set_display_name");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    let message = WebhookMessage {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: Some("spec_channel".to_string()),
        user_id: "other_virtual_user_id".to_string(),
        user_name: "other virtual user".to_string(),
        text: "spec_message".to_string(),
    };
    let payload = to_string(&message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);

    let spec_user_display_name_message = set_display_name_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(spec_user_display_name_message.contains("spec_user"));
    let other_virtual_user_display_name_message = set_display_name_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(other_virtual_user_display_name_message.contains("other virtual user"));

    let second_message_with_new_username = WebhookMessage {
        message_id: "spec_id_2".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: Some("spec_channel".to_string()),
        user_id: "other_virtual_user_id".to_string(),
        user_name: "other virtual user new".to_string(),
        text: "spec_message 2".to_string(),
    };
    let second_payload_with_new_username = to_string(&second_message_with_new_username).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &second_payload_with_new_username);

    let new_display_name_message = set_display_name_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(new_display_name_message.contains("other virtual user new"));

    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    let virtual_spec_user_id = UserId::try_from("@rocketchat_rcid_other_virtual_user_id:localhost").unwrap();
    let displayname = matrix_api.get_display_name(virtual_spec_user_id).unwrap().unwrap();

    assert_eq!(displayname, "other virtual user new".to_string());
}

#[test]
fn message_is_forwarded_even_if_setting_the_display_name_failes() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let error_responder_active = Arc::new(AtomicBool::new(false));
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let set_display_name = handlers::MatrixSetDisplayName {};
    let error_responder = handlers::MatrixActivatableErrorResponder {
        status: status::InternalServerError,
        message: "Could not set display name".to_string(),
        active: Arc::clone(&error_responder_active),
    };
    let mut set_display_name_with_error = Chain::new(set_display_name);
    set_display_name_with_error.link_before(error_responder);
    matrix_router.put(SetDisplayNameEndpoint::router_path(), set_display_name_with_error, "set_display_name");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    error_responder_active.store(true, Ordering::Relaxed);

    let message = WebhookMessage {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: Some("spec_channel".to_string()),
        user_id: "virtual_spec_user_id".to_string(),
        user_name: "virtual_spec_user".to_string(),
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
    let (_, status_code) = RestApi::call(&Method::Post, &url, RequestData::Body(payload), &params, None).unwrap();

    assert_eq!(status_code, StatusCode::UnprocessableEntity)
}

#[test]
fn no_message_is_forwarded_when_inviting_the_user_failes() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let invite_handler = handlers::MatrixInviteUser {
        as_url: test.config.as_url.clone(),
    };
    let conditional_error = handlers::MatrixConditionalErrorResponder {
        status: status::InternalServerError,
        message: "Could not invite user".to_string(),
        conditional_content: "new_user",
    };
    let mut invite_with_error = Chain::new(invite_handler);
    invite_with_error.link_before(conditional_error);
    matrix_router.post(InviteUserEndpoint::router_path(), invite_with_error, "invite_user_spec_channel");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    let message = WebhookMessage {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: Some("spec_channel".to_string()),
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
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let test =
        test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_connected_admin_room().with_logged_in_user().run();

    let message = WebhookMessage {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "not_bridged_channel_id".to_string(),
        channel_name: Some("not_bridged_channel".to_string()),
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
fn ignore_messages_forwarded_from_rocketchat_if_the_non_virtual_user_just_sent_a_message_on_matrix_to_avoid_loops() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!spec_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "message from Matrix".to_string(),
    );

    let message = WebhookMessage {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: Some("spec_channel".to_string()),
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

    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn do_not_forward_messages_when_the_channel_was_bridged_but_is_unbridged_now() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    helpers::leave_room(
        &test.config,
        RoomId::try_from("!spec_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "unbridge spec_channel".to_string(),
    );

    let message = WebhookMessage {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: Some("spec_channel".to_string()),
        user_id: "virtual_spec_user_id".to_string(),
        user_name: "virtual_spec_user".to_string(),
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
    // discard room unbridged message
    receiver.recv_timeout(default_timeout()).unwrap();

    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn returns_unauthorized_when_the_rs_token_is_missing() {
    let test = Test::new().run();
    let message = WebhookMessage {
        message_id: "spec_id".to_string(),
        token: None,
        channel_id: "spec_channel_id".to_string(),
        channel_name: Some("spec_channel".to_string()),
        user_id: "virtual_spec_user_id".to_string(),
        user_name: "virtual_spec_user".to_string(),
        text: "spec_message".to_string(),
    };
    let payload = to_string(&message).unwrap();

    let url = format!("{}/rocketchat", &test.config.as_url);

    let params = HashMap::new();
    let (_, status_code) = RestApi::call(&Method::Post, &url, RequestData::Body(payload), &params, None).unwrap();

    assert_eq!(status_code, StatusCode::Unauthorized)
}

#[test]
fn returns_forbidden_when_the_rs_token_does_not_match_a_server() {
    let test = Test::new().run();
    let message = WebhookMessage {
        message_id: "spec_id".to_string(),
        token: Some("wrong_token".to_string()),
        channel_id: "spec_channel_id".to_string(),
        channel_name: Some("spec_channel".to_string()),
        user_id: "virtual_spec_user_id".to_string(),
        user_name: "virtual_spec_user".to_string(),
        text: "spec_message".to_string(),
    };
    let payload = to_string(&message).unwrap();

    let url = format!("{}/rocketchat", &test.config.as_url);

    let params = HashMap::new();
    let (_, status_code) = RestApi::call(&Method::Post, &url, RequestData::Body(payload), &params, None).unwrap();

    assert_eq!(status_code, StatusCode::Forbidden)
}
