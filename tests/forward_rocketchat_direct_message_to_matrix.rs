#![feature(try_from)]

extern crate iron;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate reqwest;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_events;
extern crate ruma_identifiers;
extern crate serde_json;

use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use iron::{status, Chain};
use matrix_rocketchat::api::rocketchat::v1::{
    Attachment, File, Message, UserInfo, CHAT_GET_MESSAGE_PATH, DM_LIST_PATH, LOGIN_PATH, ME_PATH,
};
use matrix_rocketchat::api::rocketchat::WebhookMessage;
use matrix_rocketchat::api::MatrixApi;
use matrix_rocketchat::models::{Room, UserOnRocketchatServer};
use matrix_rocketchat_test::{default_timeout, handlers, helpers, MessageForwarder, Test, DEFAULT_LOGGER, RS_TOKEN};
use ruma_client_api::r0::account::register::Endpoint as RegisterEndpoint;
use ruma_client_api::r0::media::create_content::Endpoint as CreateContentEndpoint;
use ruma_client_api::r0::membership::forget_room::Endpoint as ForgetRoomEndpoint;
use ruma_client_api::r0::membership::invite_user::Endpoint as InviteEndpoint;
use ruma_client_api::r0::membership::leave_room::Endpoint as LeaveRoomEndpoint;
use ruma_client_api::r0::profile::get_display_name::{self, Endpoint as GetDisplaynameEndpoint};
use ruma_client_api::r0::room::create_room::Endpoint as CreateRoomEndpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_client_api::r0::sync::get_state_events_for_empty_key::{self, Endpoint as GetStateEventsForEmptyKey};
use ruma_client_api::r0::sync::sync_events::Endpoint as SyncEventsEndpoint;
use ruma_client_api::Endpoint;
use ruma_events::EventType;
use ruma_identifiers::{RoomId, UserId};
use serde_json::to_string;

#[test]
fn successfully_forwards_a_direct_message_to_matrix() {
    let test = Test::new();
    let (create_room_forwarder, create_room_receiver) = handlers::MatrixCreateRoom::with_forwarder(test.config.as_url.clone());
    let (register_forwarder, register_receiver) = handlers::MatrixRegister::with_forwarder();
    let (invite_forwarder, invite_receiver) = handlers::MatrixInviteUser::with_forwarder(test.config.as_url.clone());
    let (message_forwarder, receiver) = MessageForwarder::with_path_filter("other_userDMRocketChat_id:localhost");
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(CreateRoomEndpoint::router_path(), create_room_forwarder, "create_room");
    matrix_router.post(InviteEndpoint::router_path(), invite_forwarder, "invite_user");
    matrix_router.post(RegisterEndpoint::router_path(), register_forwarder, "register");
    let mut rocketchat_router = test.default_rocketchat_routes();
    let mut direct_messages = HashMap::new();
    direct_messages.insert("spec_user_id_other_user_id", vec!["spec_user", "other_user"]);
    let direct_messages_list_handler = handlers::RocketchatDirectMessagesList {
        direct_messages: direct_messages,
        status: status::Ok,
    };
    rocketchat_router.get(DM_LIST_PATH, direct_messages_list_handler, "direct_messages_list");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    let first_direct_message = WebhookMessage {
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

    helpers::join(
        &test.config,
        RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    // discard admin room creation message
    create_room_receiver.recv_timeout(default_timeout()).unwrap();

    let create_room_message = create_room_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(create_room_message.contains("\"name\":\"other_user (DM Rocket.Chat)\""));

    // discard bot registration
    register_receiver.recv_timeout(default_timeout()).unwrap();

    // discard spec user registration
    register_receiver.recv_timeout(default_timeout()).unwrap();

    let register_message = register_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(register_message.contains("\"username\":\"rocketchat_rcid_other_user_id\""));

    // discard admin room invite
    invite_receiver.recv_timeout(default_timeout()).unwrap();

    let spec_user_invite = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(spec_user_invite.contains("\"user_id\":\"@spec_user:localhost\""));

    let first_message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(first_message_received_by_matrix.contains("Hey there"));

    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    let other_user_id = UserId::try_from("@rocketchat_rcid_other_user_id:localhost").unwrap();
    let spec_user_id = UserId::try_from("@spec_user:localhost").unwrap();
    let room_id = RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap();
    let room = Room::new(&test.config, &DEFAULT_LOGGER, &(*matrix_api), room_id);
    let user_ids = room.user_ids(Some(other_user_id.clone())).unwrap();
    assert_eq!(user_ids.len(), 2);
    assert!(user_ids.iter().any(|id| id == &other_user_id));
    assert!(user_ids.iter().any(|id| id == &spec_user_id));

    let second_direct_message = WebhookMessage {
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

#[test]
fn successfully_forwards_a_direct_message_to_an_existing_dm_room_on_matrix() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::with_path_filter("other_userDMRocketChat_id:localhost");
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = test.default_rocketchat_routes();
    let mut direct_messages = HashMap::new();
    direct_messages.insert("spec_user_id_other_user_id", vec!["spec_user", "other_user"]);
    let direct_messages_list_handler = handlers::RocketchatDirectMessagesList {
        direct_messages: direct_messages,
        status: status::Ok,
    };
    rocketchat_router.get(DM_LIST_PATH, direct_messages_list_handler, "direct_messages_list");
    let room_id = RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap();
    let spec_user_id = UserId::try_from("@spec_user:localhost").unwrap();
    let other_user_id = UserId::try_from("@rocketchat_rcid_other_user_id:localhost").unwrap();

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_bridge_dm((room_id, vec![spec_user_id, other_user_id]))
        .with_logged_in_user()
        .run();

    let first_direct_message = WebhookMessage {
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

    let first_message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(first_message_received_by_matrix.contains("Hey there"));
}

#[test]
fn successfully_forwards_an_image_in_a_direct_message_to_matrix() {
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
        mimetype: "image/png".to_string(),
        title: "Spec titel".to_string(),
        title_link: "/file-upload/image.png".to_string(),
    }];
    let rocketchat_message = Arc::new(Mutex::new(Some(Message {
        id: "spec_id".to_string(),
        rid: "spec_rid".to_string(),
        msg: "".to_string(),
        ts: "2017-12-12 11:11".to_string(),
        attachments: Some(attachments),
        file: Some(File {
            mimetype: "image/png".to_string(),
        }),
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
    let mut direct_messages = HashMap::new();
    direct_messages.insert("spec_user_id_other_user_id", vec!["spec_user", "other_user"]);
    let direct_messages_list_handler = handlers::RocketchatDirectMessagesList {
        direct_messages: direct_messages,
        status: status::Ok,
    };
    rocketchat_router.get(DM_LIST_PATH, direct_messages_list_handler, "direct_messages_list");

    let test = test
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

    // trigger room creation
    let message = WebhookMessage {
        message_id: "spec_id_1".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: Some("spec_channel".to_string()),
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "first message".to_string(),
    };
    let payload = to_string(&message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &payload);
    receiver.recv_timeout(default_timeout()).unwrap();
    helpers::join(
        &test.config,
        RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    // empty message, because the image was uploaded
    let message = WebhookMessage {
        message_id: "spec_id".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: Some("spec_channel".to_string()),
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
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
fn the_virtual_user_stays_in_the_direct_message_room_if_the_user_leaves() {
    let test = Test::new();

    let mut rocketchat_router = test.default_rocketchat_routes();
    let mut direct_messages = HashMap::new();
    direct_messages.insert("spec_user_id_other_user_id", vec!["spec_user", "other_user"]);
    let direct_messages_list_handler = handlers::RocketchatDirectMessagesList {
        direct_messages: direct_messages,
        status: status::Ok,
    };
    rocketchat_router.get(DM_LIST_PATH, direct_messages_list_handler, "direct_messages_list");

    let mut matrix_router = test.default_matrix_routes();
    let (forget_message_forwarder, forget_receiver) = MessageForwarder::new();
    let (leave_room, leave_receiver) = handlers::MatrixLeaveRoom::with_forwarder(test.config.as_url.clone());
    matrix_router.post(LeaveRoomEndpoint::router_path(), leave_room, "leave_room");
    matrix_router.post(ForgetRoomEndpoint::router_path(), forget_message_forwarder, "forget_room");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    let direct_message = WebhookMessage {
        message_id: "spec_id_1".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey there".to_string(),
    };
    let direct_message_payload = to_string(&direct_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &direct_message_payload);

    helpers::join(
        &test.config,
        RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    helpers::leave_room(
        &test.config,
        RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    // discard bot leave
    assert!(leave_receiver.recv_timeout(default_timeout()).is_ok());

    // discard spec user leave
    assert!(leave_receiver.recv_timeout(default_timeout()).is_ok());

    // no more calls to the leave and forget endpoints, because the virtual user stays in the room
    assert!(leave_receiver.recv_timeout(default_timeout()).is_err());
    assert!(forget_receiver.recv_timeout(default_timeout()).is_err());

    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    let other_user_id = UserId::try_from("@rocketchat_rcid_other_user_id:localhost").unwrap();
    let room_id = RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap();
    let room = Room::new(&test.config, &DEFAULT_LOGGER, &(*matrix_api), room_id);
    let user_ids = room.user_ids(Some(other_user_id.clone())).unwrap();
    assert_eq!(user_ids.len(), 1);
    assert!(user_ids.iter().any(|id| id == &other_user_id));
}

#[test]
fn successfully_forwards_a_direct_message_to_a_matrix_room_that_was_bridged_before() {
    let test = Test::new();

    let mut rocketchat_router = test.default_rocketchat_routes();
    let mut direct_messages = HashMap::new();
    direct_messages.insert("spec_user_id_other_user_id", vec!["spec_user", "other_user"]);
    let direct_messages_list_handler = handlers::RocketchatDirectMessagesList {
        direct_messages: direct_messages,
        status: status::Ok,
    };
    rocketchat_router.get(DM_LIST_PATH, direct_messages_list_handler, "direct_messages_list");

    let mut matrix_router = test.default_matrix_routes();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let (invite_forwarder, invite_receiver) = handlers::MatrixInviteUser::with_forwarder(test.config.as_url.clone());
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(InviteEndpoint::router_path(), invite_forwarder, "invite_user");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    let direct_message = WebhookMessage {
        message_id: "spec_id_1".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey there".to_string(),
    };
    let direct_message_payload = to_string(&direct_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &direct_message_payload);

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Hey there"));

    // discard admin room invite
    invite_receiver.recv_timeout(default_timeout()).unwrap();

    let initial_invite = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(initial_invite.contains("@spec_user:localhost"));

    helpers::join(
        &test.config,
        RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    helpers::leave_room(
        &test.config,
        RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    let direct_message = WebhookMessage {
        message_id: "spec_id_2".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey again".to_string(),
    };
    let direct_message_payload = to_string(&direct_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &direct_message_payload);

    // discard bot invite into direct message room
    invite_receiver.recv_timeout(default_timeout()).unwrap();

    let invite_to_rejoin = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(invite_to_rejoin.contains("@spec_user:localhost"));

    helpers::join(
        &test.config,
        RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Hey again"));
}

#[test]
fn do_use_two_different_dm_rooms_when_both_users_are_on_matrix() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let (create_room_forwarder, create_room_receiver) = handlers::MatrixCreateRoom::with_forwarder(test.config.as_url.clone());
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(CreateRoomEndpoint::router_path(), create_room_forwarder, "create_room");
    let admin_room_creator_handler = handlers::RoomStateCreate {
        creator: UserId::try_from("@other_user:localhost").unwrap(),
    };
    let admin_room_creator_params = get_state_events_for_empty_key::PathParams {
        room_id: RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        event_type: EventType::RoomCreate.to_string(),
    };
    matrix_router.get(
        GetStateEventsForEmptyKey::request_path(admin_room_creator_params),
        admin_room_creator_handler,
        "get_room_creator_admin_room",
    );

    let mut rocketchat_router = test.default_rocketchat_routes();
    let mut direct_messages = HashMap::new();
    direct_messages.insert("spec_user_id_other_user_id", vec!["spec_user", "other_user"]);
    let direct_messages_list_handler = handlers::RocketchatDirectMessagesList {
        direct_messages: direct_messages,
        status: status::Ok,
    };
    rocketchat_router.get(DM_LIST_PATH, direct_messages_list_handler, "direct_messages_list");
    let login_user_id = Arc::new(Mutex::new(Some("spec_user_id".to_string())));
    rocketchat_router.post(
        LOGIN_PATH,
        handlers::RocketchatLogin {
            successful: true,
            rocketchat_user_id: Arc::clone(&login_user_id),
        },
        "login",
    );
    let me_username = Arc::new(Mutex::new("spec_user".to_string()));
    rocketchat_router.get(
        ME_PATH,
        handlers::RocketchatMe {
            username: Arc::clone(&me_username),
        },
        "me",
    );

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    let other_user_sender_direct_message = WebhookMessage {
        message_id: "spec_id_1".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey there".to_string(),
    };
    let other_user_sender_direct_message_payload = to_string(&other_user_sender_direct_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &other_user_sender_direct_message_payload);

    // discard admin room creation message
    create_room_receiver.recv_timeout(default_timeout()).unwrap();

    let create_room_message = create_room_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(create_room_message.contains("\"name\":\"other_user (DM Rocket.Chat)\""));

    helpers::join(
        &test.config,
        RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();

    let other_user_sender_direct_message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(other_user_sender_direct_message_received_by_matrix.contains("Hey there"));

    {
        let mut new_login_user_id = login_user_id.lock().unwrap();
        *new_login_user_id = Some("other_user_id".to_string());
        let mut new_me_username = me_username.lock().unwrap();
        *new_me_username = "other_user".to_string();
    }

    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    matrix_api.register("other_user".to_string()).unwrap();

    // create other admin room
    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    matrix_api
        .create_room(Some("other_admin_room".to_string()), None, &UserId::try_from("@other_user:localhost").unwrap())
        .unwrap();
    helpers::invite(
        &test.config,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
    );

    // connect other admin room
    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
        format!("connect {}", test.rocketchat_mock_url.clone().unwrap()),
    );

    // login other user
    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
        "login other_user secret".to_string(),
    );

    let spec_user_sender_direct_message = WebhookMessage {
        message_id: "spec_id_2".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "spec_user_id".to_string(),
        user_name: "spec_user".to_string(),
        text: "Hey you".to_string(),
    };
    let spec_user_sender_direct_message_payload = to_string(&spec_user_sender_direct_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &spec_user_sender_direct_message_payload);

    // discard other admin room creation message
    create_room_receiver.recv_timeout(default_timeout()).unwrap();

    let create_room_message = create_room_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(create_room_message.contains("\"name\":\"spec_user (DM Rocket.Chat)\""));

    helpers::join(
        &test.config,
        RoomId::try_from("!spec_userDMRocketChat_id:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
    );

    // discard other welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard other connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard other login message
    receiver.recv_timeout(default_timeout()).unwrap();

    let spec_user_sender_direct_message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(spec_user_sender_direct_message_received_by_matrix.contains("Hey you"));
}

#[test]
fn do_not_forward_a_direct_message_if_the_receiver_is_the_senders_virtual_user() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::with_path_filter("other_userDMRocketChat_id:localhost");
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = test.default_rocketchat_routes();
    let mut direct_messages = HashMap::new();
    direct_messages.insert("spec_user_id_other_user_id", vec!["spec_user", "other_user"]);
    let direct_messages_list_handler = handlers::RocketchatDirectMessagesList {
        direct_messages: direct_messages,
        status: status::Ok,
    };
    rocketchat_router.get(DM_LIST_PATH, direct_messages_list_handler, "direct_messages_list");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    let first_direct_message = WebhookMessage {
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

    helpers::join(
        &test.config,
        RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    let first_message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(first_message_received_by_matrix.contains("Hey there"));

    let message_from_receiver_virtual_user = WebhookMessage {
        message_id: "spec_id_2".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "spec_user_id".to_string(),
        user_name: "spec_user".to_string(),
        text: "This will not be forwarded".to_string(),
    };
    let second_direct_message_payload = to_string(&message_from_receiver_virtual_user).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &second_direct_message_payload);

    // message is not forwarded, because the sender is the receivers virtual user
    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn do_not_forwards_a_direct_message_to_a_room_if_the_user_is_no_longer_logged_in_on_the_rocketchat_server() {
    let test = Test::new();

    let mut rocketchat_router = test.default_rocketchat_routes();
    let mut direct_messages = HashMap::new();
    direct_messages.insert("spec_user_id_other_user_id", vec!["spec_user", "other_user"]);
    let direct_messages_list_handler = handlers::RocketchatDirectMessagesList {
        direct_messages: direct_messages,
        status: status::Ok,
    };
    rocketchat_router.get(DM_LIST_PATH, direct_messages_list_handler, "direct_messages_list");

    let mut matrix_router = test.default_matrix_routes();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let (invite_forwarder, invite_receiver) = handlers::MatrixInviteUser::with_forwarder(test.config.as_url.clone());
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(InviteEndpoint::router_path(), invite_forwarder, "invite_user");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    let direct_message = WebhookMessage {
        message_id: "spec_id_1".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey there".to_string(),
    };
    let direct_message_payload = to_string(&direct_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &direct_message_payload);

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Hey there"));

    // discard admin room invite
    invite_receiver.recv_timeout(default_timeout()).unwrap();

    let initial_invite = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(initial_invite.contains("@spec_user:localhost"));

    helpers::join(
        &test.config,
        RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    helpers::leave_room(
        &test.config,
        RoomId::try_from("!other_userDMRocketChat_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    let connection = test.connection_pool.get().unwrap();
    let receier_user_id = UserId::try_from("@spec_user:localhost").unwrap();
    let user = UserOnRocketchatServer::find(&connection, &receier_user_id, "rcid".to_string()).unwrap();
    helpers::logout_user_from_rocketchat_server_on_bridge(&connection, "rcid".to_string(), &user.matrix_user_id);

    let direct_message = WebhookMessage {
        message_id: "spec_id_2".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey again".to_string(),
    };
    let direct_message_payload = to_string(&direct_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &direct_message_payload);

    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn no_room_is_created_when_the_user_doesn_not_have_access_to_the_matching_direct_message_channel() {
    let test = Test::new();
    let (create_room_forwarder, create_room_receiver) = handlers::MatrixCreateRoom::with_forwarder(test.config.as_url.clone());
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.post(CreateRoomEndpoint::router_path(), create_room_forwarder, "create_room");
    let mut rocketchat_router = test.default_rocketchat_routes();
    let direct_messages_list_handler = handlers::RocketchatDirectMessagesList {
        direct_messages: HashMap::new(),
        status: status::Ok,
    };
    rocketchat_router.get(DM_LIST_PATH, direct_messages_list_handler, "direct_messages_list");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    let direct_message = WebhookMessage {
        message_id: "spec_id_1".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey there".to_string(),
    };
    let direct_message_payload = to_string(&direct_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &direct_message_payload);

    // discard admin room creation
    create_room_receiver.recv_timeout(default_timeout()).unwrap();
    // no room is created on the Matrix server
    assert!(create_room_receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn no_room_is_created_when_no_matching_user_for_the_room_name_is_found() {
    let test = Test::new();
    let (create_room_forwarder, create_room_receiver) = handlers::MatrixCreateRoom::with_forwarder(test.config.as_url.clone());
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.post(CreateRoomEndpoint::router_path(), create_room_forwarder, "create_room");

    let test =
        test.with_matrix_routes(matrix_router).with_rocketchat_mock().with_connected_admin_room().with_logged_in_user().run();

    let direct_message = WebhookMessage {
        message_id: "spec_id_1".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "no_user_matches_this_channel_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey there".to_string(),
    };
    let direct_message_payload = to_string(&direct_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &direct_message_payload);

    // discard admin room creation
    create_room_receiver.recv_timeout(default_timeout()).unwrap();
    // no room is created on the Matrix server
    assert!(create_room_receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn no_room_is_created_when_getting_the_direct_message_list_failes() {
    let test = Test::new();
    let (create_room_forwarder, create_room_receiver) = handlers::MatrixCreateRoom::with_forwarder(test.config.as_url.clone());
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.post(CreateRoomEndpoint::router_path(), create_room_forwarder, "create_room");
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.get(
        DM_LIST_PATH,
        handlers::RocketchatErrorResponder {
            status: status::InternalServerError,
            message: "Getting DMs failed".to_string(),
        },
        "direct_messages_list",
    );

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    let direct_message = WebhookMessage {
        message_id: "spec_id_1".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey there".to_string(),
    };
    let direct_message_payload = to_string(&direct_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &direct_message_payload);

    // discard admin room creation
    create_room_receiver.recv_timeout(default_timeout()).unwrap();
    // no room is created on the Matrix server
    assert!(create_room_receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn no_additional_room_is_created_when_getting_the_initial_sync_failes() {
    let test = Test::new();
    let (create_room_forwarder, create_room_receiver) = handlers::MatrixCreateRoom::with_forwarder(test.config.as_url.clone());
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.post(CreateRoomEndpoint::router_path(), create_room_forwarder, "create_room");
    let error_responder = handlers::MatrixErrorResponder {
        status: status::InternalServerError,
        message: "Could get initial sync".to_string(),
    };
    matrix_router.get(SyncEventsEndpoint::router_path(), error_responder, "sync");

    let mut rocketchat_router = test.default_rocketchat_routes();
    let mut direct_messages = HashMap::new();
    direct_messages.insert("spec_user_id_other_user_id", vec!["spec_user", "other_user"]);
    let direct_messages_list_handler = handlers::RocketchatDirectMessagesList {
        direct_messages: direct_messages,
        status: status::Ok,
    };
    rocketchat_router.get(DM_LIST_PATH, direct_messages_list_handler, "direct_messages_list");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    let first_direct_message = WebhookMessage {
        message_id: "spec_id_1".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey there".to_string(),
    };

    let second_direct_message_payload = to_string(&first_direct_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &second_direct_message_payload);

    let second_direct_message = WebhookMessage {
        message_id: "spec_id_1".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey there".to_string(),
    };
    let first_direct_message_payload = to_string(&second_direct_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &first_direct_message_payload);

    // discard admin room creation
    create_room_receiver.recv_timeout(default_timeout()).unwrap();
    // discard first dm room creation
    create_room_receiver.recv_timeout(default_timeout()).unwrap();

    // no additional room is created on the Matrix server
    assert!(create_room_receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn no_room_is_created_when_getting_the_displayname_failes() {
    let test = Test::new();
    let (create_room_forwarder, create_room_receiver) = handlers::MatrixCreateRoom::with_forwarder(test.config.as_url.clone());
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.post(CreateRoomEndpoint::router_path(), create_room_forwarder, "create_room");
    let get_display_name = handlers::MatrixGetDisplayName {};
    let error_responder_active = Arc::new(AtomicBool::new(false));
    let error_responder = handlers::MatrixActivatableErrorResponder {
        status: status::InternalServerError,
        message: "Could get display name".to_string(),
        active: Arc::clone(&error_responder_active),
    };
    let mut get_display_name_with_error = Chain::new(get_display_name);
    get_display_name_with_error.link_before(error_responder);
    matrix_router.get(GetDisplaynameEndpoint::router_path(), get_display_name_with_error, "get_displayname");
    let mut rocketchat_router = test.default_rocketchat_routes();
    let mut direct_messages = HashMap::new();
    direct_messages.insert("spec_user_id_other_user_id", vec!["spec_user", "other_user"]);
    let direct_messages_list_handler = handlers::RocketchatDirectMessagesList {
        direct_messages: direct_messages,
        status: status::Ok,
    };
    rocketchat_router.get(DM_LIST_PATH, direct_messages_list_handler, "direct_messages_list");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    error_responder_active.store(true, Ordering::Relaxed);

    let direct_message = WebhookMessage {
        message_id: "spec_id_1".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey there".to_string(),
    };
    let direct_message_payload = to_string(&direct_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &direct_message_payload);

    // discard admin room creation
    create_room_receiver.recv_timeout(default_timeout()).unwrap();
    // no room is created on the Matrix server
    assert!(create_room_receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn no_room_is_created_when_the_direct_message_list_response_cannot_be_deserialized() {
    let test = Test::new();
    let (create_room_forwarder, create_room_receiver) = handlers::MatrixCreateRoom::with_forwarder(test.config.as_url.clone());
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.post(CreateRoomEndpoint::router_path(), create_room_forwarder, "create_room");
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.get(
        DM_LIST_PATH,
        handlers::InvalidJsonResponse {
            status: status::Ok,
        },
        "direct_messages_list",
    );

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    let direct_message = WebhookMessage {
        message_id: "spec_id_1".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey again".to_string(),
    };
    let direct_message_payload = to_string(&direct_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &direct_message_payload);

    // discard admin room creation
    create_room_receiver.recv_timeout(default_timeout()).unwrap();
    // no room is created on the Matrix server
    assert!(create_room_receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn no_additional_room_is_created_when_getting_the_initial_sync_response_cannot_be_deserialized() {
    let test = Test::new();
    let (create_room_forwarder, create_room_receiver) = handlers::MatrixCreateRoom::with_forwarder(test.config.as_url.clone());
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.post(CreateRoomEndpoint::router_path(), create_room_forwarder, "create_room");
    matrix_router.get(
        SyncEventsEndpoint::router_path(),
        handlers::InvalidJsonResponse {
            status: status::Ok,
        },
        "sync",
    );

    let mut rocketchat_router = test.default_rocketchat_routes();
    let mut direct_messages = HashMap::new();
    direct_messages.insert("spec_user_id_other_user_id", vec!["spec_user", "other_user"]);
    let direct_messages_list_handler = handlers::RocketchatDirectMessagesList {
        direct_messages: direct_messages,
        status: status::Ok,
    };
    rocketchat_router.get(DM_LIST_PATH, direct_messages_list_handler, "direct_messages_list");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    let first_direct_message = WebhookMessage {
        message_id: "spec_id_1".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey there".to_string(),
    };

    let second_direct_message_payload = to_string(&first_direct_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &second_direct_message_payload);

    let second_direct_message = WebhookMessage {
        message_id: "spec_id_1".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey again".to_string(),
    };
    let first_direct_message_payload = to_string(&second_direct_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &first_direct_message_payload);

    // discard admin room creation
    create_room_receiver.recv_timeout(default_timeout()).unwrap();
    // discard first dm room creation
    create_room_receiver.recv_timeout(default_timeout()).unwrap();

    // no additional room is created on the Matrix server
    assert!(create_room_receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn no_room_is_created_when_getting_the_displayname_respones_cannot_be_deserialized() {
    let test = Test::new();
    let (create_room_forwarder, create_room_receiver) = handlers::MatrixCreateRoom::with_forwarder(test.config.as_url.clone());
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.post(CreateRoomEndpoint::router_path(), create_room_forwarder, "create_room");
    let get_display_name_params = get_display_name::PathParams {
        user_id: UserId::try_from("@rocketchat_rcid_other_user_id:localhost").unwrap(),
    };
    let get_display_name_path = GetDisplaynameEndpoint::request_path(get_display_name_params);
    let invalid_json_responder = handlers::InvalidJsonResponse {
        status: status::Ok,
    };
    matrix_router.get(get_display_name_path, invalid_json_responder, "get_displayname_invalid_json");
    let mut rocketchat_router = test.default_rocketchat_routes();
    let mut direct_messages = HashMap::new();
    direct_messages.insert("spec_user_id_other_user_id", vec!["spec_user", "other_user"]);
    let direct_messages_list_handler = handlers::RocketchatDirectMessagesList {
        direct_messages: direct_messages,
        status: status::Ok,
    };
    rocketchat_router.get(DM_LIST_PATH, direct_messages_list_handler, "direct_messages_list");

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    let direct_message = WebhookMessage {
        message_id: "spec_id_1".to_string(),
        token: Some(RS_TOKEN.to_string()),
        channel_id: "spec_user_id_other_user_id".to_string(),
        channel_name: None,
        user_id: "other_user_id".to_string(),
        user_name: "other_user".to_string(),
        text: "Hey there".to_string(),
    };
    let direct_message_payload = to_string(&direct_message).unwrap();

    helpers::simulate_message_from_rocketchat(&test.config.as_url, &direct_message_payload);

    // discard admin room creation
    create_room_receiver.recv_timeout(default_timeout()).unwrap();
    // no room is created on the Matrix server
    assert!(create_room_receiver.recv_timeout(default_timeout()).is_err());
}
