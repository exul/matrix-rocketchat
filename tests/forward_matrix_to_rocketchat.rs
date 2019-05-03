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
use matrix_rocketchat::api::rocketchat::v1::{CHAT_POST_MESSAGE_PATH, ROOMS_UPLOAD_PATH};
use matrix_rocketchat::api::rocketchat::WebhookMessage;
use matrix_rocketchat::api::MatrixApi;
use matrix_rocketchat_test::{default_timeout, handlers, helpers, MessageForwarder, Test, DEFAULT_LOGGER, RS_TOKEN};
use ruma_client_api::r0::media::get_content::Endpoint as GetContentEndpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_client_api::Endpoint;
use ruma_identifiers::{RoomId, UserId};
use serde_json::to_string;

#[test]
fn successfully_forwards_a_text_message_from_matrix_to_rocketchat() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.post(CHAT_POST_MESSAGE_PATH, message_forwarder, "post_text_message");

    let test = test
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!spec_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "spec message".to_string(),
    );

    let message_received_by_rocketchat = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_rocketchat.contains("spec message"));
    assert!(message_received_by_rocketchat.contains("spec_channel"));
}

#[test]
fn successfully_forwards_an_image_message_from_matrix_to_rocketchat() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    let mut files = HashMap::new();
    files.insert("spec_id".to_string(), b"image".to_vec());
    matrix_router.get(GetContentEndpoint::router_path(), handlers::MatrixGetContentHandler { files: files }, "get_file");
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.post(format!("{}{}", ROOMS_UPLOAD_PATH, "/:channel_id"), message_forwarder, "upload");

    let test = test
        .with_rocketchat_mock()
        .with_matrix_routes(matrix_router)
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    helpers::send_image_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!spec_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "spec_image.png".to_string(),
        "mxc://localhost/spec_id".to_string(),
    );

    let message_received_by_rocketchat = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_rocketchat.contains("image"));
}

#[test]
fn successfully_forwards_a_file_message_from_matrix_to_rocketchat() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    let mut files = HashMap::new();
    files.insert("spec_id".to_string(), b"file".to_vec());
    matrix_router.get(GetContentEndpoint::router_path(), handlers::MatrixGetContentHandler { files: files }, "get_file");
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.post(format!("{}{}", ROOMS_UPLOAD_PATH, "/:channel_id"), message_forwarder, "upload");

    let test = test
        .with_rocketchat_mock()
        .with_matrix_routes(matrix_router)
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    helpers::send_file_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!spec_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "spec_image.txt".to_string(),
        "mxc://localhost/spec_id".to_string(),
    );

    let message_received_by_rocketchat = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_rocketchat.contains("file"));
}

#[test]
fn successfully_forwards_an_audio_message_from_matrix_to_rocketchat() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    let mut files = HashMap::new();
    files.insert("spec_id".to_string(), b"audio".to_vec());
    matrix_router.get(GetContentEndpoint::router_path(), handlers::MatrixGetContentHandler { files: files }, "get_file");
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.post(format!("{}{}", ROOMS_UPLOAD_PATH, "/:channel_id"), message_forwarder, "upload");

    let test = test
        .with_rocketchat_mock()
        .with_matrix_routes(matrix_router)
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    helpers::send_audio_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!spec_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "spec_audio.wav".to_string(),
        "mxc://localhost/spec_id".to_string(),
    );

    let message_received_by_rocketchat = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_rocketchat.contains("audio"));
}

#[test]
fn successfully_forwards_a_video_message_from_matrix_to_rocketchat() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    let mut files = HashMap::new();
    files.insert("spec_id".to_string(), b"video".to_vec());
    matrix_router.get(GetContentEndpoint::router_path(), handlers::MatrixGetContentHandler { files: files }, "get_file");
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.post(format!("{}{}", ROOMS_UPLOAD_PATH, "/:channel_id"), message_forwarder, "upload");

    let test = test
        .with_rocketchat_mock()
        .with_matrix_routes(matrix_router)
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    helpers::send_video_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!spec_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "spec_video.webm".to_string(),
        "mxc://localhost/spec_id".to_string(),
    );

    let message_received_by_rocketchat = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_rocketchat.contains("video"));
}

#[test]
fn error_message_when_uploading_the_file_on_rocketchat_fails_when_forwarding_a_file_message_from_matrix_to_rocketchat() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut files = HashMap::new();
    files.insert("spec_id".to_string(), b"file".to_vec());
    matrix_router.get(GetContentEndpoint::router_path(), handlers::MatrixGetContentHandler { files: files }, "get_file");
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.post(
        format!("{}{}", ROOMS_UPLOAD_PATH, "/:channel_id"),
        handlers::RocketchatErrorResponder { message: "Spec error".to_string(), status: status::BadRequest },
        "upload",
    );

    let test = test
        .with_rocketchat_mock()
        .with_matrix_routes(matrix_router)
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    helpers::send_file_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!spec_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "spec_image.txt".to_string(),
        "mxc://localhost/spec_id".to_string(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard bridge message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(
        message_received_by_matrix
            .contains("Uploading file mxc://localhost/spec_id to Rocket.Chat failed with 'Rocket.Chat error: Spec error")
    );
}

#[test]
fn no_message_is_forwarded_when_the_image_cannot_be_found() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let (upload_forwarder, upload_receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.get(
        GetContentEndpoint::router_path(),
        handlers::MatrixGetContentHandler { files: HashMap::new() },
        "get_file",
    );
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.post(CHAT_POST_MESSAGE_PATH, message_forwarder, "post_text_message");
    rocketchat_router.post(format!("{}{}", ROOMS_UPLOAD_PATH, "/:channel_id"), upload_forwarder, "upload");

    let test = test
        .with_rocketchat_mock()
        .with_matrix_routes(matrix_router)
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    helpers::send_image_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!spec_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "non_existing_image.png".to_string(),
        "mxc://localhost/spec_id".to_string(),
    );

    assert!(receiver.recv_timeout(default_timeout()).is_err());
    assert!(upload_receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn do_not_forward_messages_from_the_bot_user_to_avoid_loops() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.post(CHAT_POST_MESSAGE_PATH, message_forwarder, "post_text_message");

    let test = test
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!spec_channel_id:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
        "spec message".to_string(),
    );

    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn do_not_forward_messages_from_virtual_user_to_avoid_loops() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.post(CHAT_POST_MESSAGE_PATH, message_forwarder, "post_text_message");

    let test = test
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    // create the virtual user by simulating a message from Rocket.Chat
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

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!spec_channel_id:localhost").unwrap(),
        UserId::try_from("@rocketchat_virtual_spec_user_id_1:localhost").unwrap(),
        "spec message".to_string(),
    );

    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn ignore_messages_from_unbridged_rooms() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.post(CHAT_POST_MESSAGE_PATH, message_forwarder, "post_text_message");

    let test = test
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .run();

    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    let spec_user_id = UserId::try_from("@spec_user:localhost").unwrap();
    matrix_api.create_room(Some("not_bridged_room".to_string()), None, &spec_user_id).unwrap();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!not_bridged_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "spec message".to_string(),
    );

    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn ignore_messages_from_rooms_with_empty_room_canonical_alias() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let test = test.with_matrix_routes(matrix_router).with_rocketchat_mock().run();

    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    matrix_api.create_room(Some("room".to_string()), None, &UserId::try_from("@rocketchat:localhost").unwrap()).unwrap();
    matrix_api.put_canonical_room_alias(RoomId::try_from("!room_id:localhost").unwrap(), None).unwrap();

    helpers::invite(
        &test.config,
        RoomId::try_from("!room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
    );

    helpers::join(
        &test.config,
        RoomId::try_from("!room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "spec message".to_string(),
    );

    // timeout because the message was not forwarded
    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn ignore_messages_with_a_message_type_that_is_not_supported() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.post(CHAT_POST_MESSAGE_PATH, message_forwarder, "post_text_message");

    let test = test
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    // create the virtual user by simulating a message from Rocket.Chat
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

    helpers::send_emote_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!spec_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "emote message".to_string(),
    );

    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn the_user_gets_a_message_when_forwarding_a_message_failes() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut rocketchat_router = test.default_rocketchat_routes();
    rocketchat_router.post(
        CHAT_POST_MESSAGE_PATH,
        handlers::RocketchatErrorResponder {
            message: "Rocketh.Chat chat.postMessage error".to_string(),
            status: status::InternalServerError,
        },
        "post_text_message",
    );

    let test = test
        .with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("spec_channel", vec!["spec_user"]))
        .run();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!spec_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "spec message".to_string(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard bridge message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn the_user_gets_a_message_when_when_getting_the_canonical_room_alias_failes() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let error_responder = handlers::MatrixErrorResponder {
        status: status::InternalServerError,
        message: "Could not get canonical room alias".to_string(),
    };
    matrix_router.get(
        "/_matrix/client/r0/rooms/!spec_channel_id:localhost/state/m.room.canonical_alias",
        error_responder,
        "get_room_canonical_room_alias",
    );

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
        "spec message".to_string(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard room bridged message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn the_user_gets_a_message_when_when_getting_the_canonical_room_alias_response_cannot_be_deserialized() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.get(
        "/_matrix/client/r0/rooms/!spec_channel_id:localhost/state/m.room.canonical_alias",
        handlers::InvalidJsonResponse { status: status::Ok },
        "get_room_canonical_room_alias",
    );

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
        "spec message".to_string(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard room bridged message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}
