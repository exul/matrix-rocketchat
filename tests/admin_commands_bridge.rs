#![feature(try_from)]

extern crate iron;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_events;
extern crate ruma_identifiers;

use std::collections::HashMap;
use std::convert::TryFrom;

use iron::{Chain, status};
use matrix_rocketchat::api::MatrixApi;
use matrix_rocketchat::api::rocketchat::v1::{LOGIN_PATH, ME_PATH, USERS_INFO_PATH};
use matrix_rocketchat::db::Room;
use matrix_rocketchat_test::{DEFAULT_LOGGER, MessageForwarder, Test, default_timeout, handlers, helpers};
use router::Router;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::alias::get_alias::Endpoint as GetAliasEndpoint;
use ruma_client_api::r0::membership::invite_user::Endpoint as InviteEndpoint;
use ruma_client_api::r0::room::create_room::Endpoint as CreateRoomEndpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_client_api::r0::send::send_state_event_for_empty_key::{self, Endpoint as SendStateEventForEmptyKeyEndpoint};
use ruma_events::EventType;
use ruma_identifiers::{RoomId, UserId};

#[test]
fn successfully_bridge_a_rocketchat_room() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let (invite_forwarder, invite_receiver) = MessageForwarder::new();
    let (state_forwarder, state_receiver) = handlers::SendRoomState::with_forwarder();
    let (create_room_forwarder, create_room_receiver) = handlers::MatrixCreateRoom::with_forwarder(test.config.as_url.clone());
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.put(SendStateEventForEmptyKeyEndpoint::router_path(), state_forwarder, "send_state_event_for_key");
    matrix_router.post(InviteEndpoint::router_path(), invite_forwarder, "invite_user");
    matrix_router.post(CreateRoomEndpoint::router_path(), create_room_forwarder, "create_room");

    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user", "user_1", "user_2", "user_3"]);

    let test = test.with_matrix_routes(matrix_router)
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

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "bridge joined_channel".to_string(),
    );

    // discard admin room creation message
    create_room_receiver.recv_timeout(default_timeout()).unwrap();

    let create_room_message = create_room_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(create_room_message.contains("\"name\":\"joined_channel\""));
    assert!(create_room_message.contains("\"room_alias_name\":\"rocketchat#rc_id#joined_channel_id\""));

    let invite_spec_user = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(invite_spec_user.contains("@spec_user:localhost"));
    let invite_virtual_spec_user = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(invite_virtual_spec_user.contains("rocketchat_spec_user_id_rc_id:localhost"));
    let invite_user_1 = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(invite_user_1.contains("@rocketchat_user_1_id_rc_id:localhost"));
    let invite_user_2 = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(invite_user_2.contains("@rocketchat_user_2_id_rc_id:localhost"));
    let invite_user_3 = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(invite_user_3.contains("@rocketchat_user_3_id_rc_id:localhost"));

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("joined_channel is now bridged."));

    let set_room_name_received_by_matrix = state_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(set_room_name_received_by_matrix.contains("Admin Room (Rocket.Chat)"));

    let topic_received_by_matrix = state_receiver.recv_timeout(default_timeout()).unwrap();
    let topic_message = format!("\"topic\":\"{}\"", test.rocketchat_mock_url.clone().unwrap());
    assert!(topic_received_by_matrix.contains(&topic_message));

    // only moderators and admins can invite other users
    let power_levels_received_by_matrix = state_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(power_levels_received_by_matrix.contains("\"invite\":50"));
    assert!(power_levels_received_by_matrix.contains("\"kick\":50"));
    assert!(power_levels_received_by_matrix.contains("\"ban\":50"));
    assert!(power_levels_received_by_matrix.contains("\"redact\":50"));
    assert!(power_levels_received_by_matrix.contains("@rocketchat:localhost"));

    helpers::join(
        &test.config,
        RoomId::try_from("!joined_channel_id:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
    );

    helpers::join(
        &test.config,
        RoomId::try_from("!joined_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    let user_ids = Room::user_ids(&(*matrix_api), RoomId::try_from("!joined_channel_id:localhost").unwrap()).unwrap();
    assert!(user_ids.iter().any(|id| id == &UserId::try_from("@rocketchat:localhost").unwrap()));
    assert!(user_ids.iter().any(|id| id == &UserId::try_from("@spec_user:localhost").unwrap()));
    assert!(user_ids.iter().any(|id| id == &UserId::try_from("@rocketchat_spec_user_id_rc_id:localhost").unwrap()));
    assert!(user_ids.iter().any(|id| id == &UserId::try_from("@rocketchat_user_1_id_rc_id:localhost").unwrap()));
    assert!(user_ids.iter().any(|id| id == &UserId::try_from("@rocketchat_user_2_id_rc_id:localhost").unwrap()));
    assert!(user_ids.iter().any(|id| id == &UserId::try_from("@rocketchat_user_3_id_rc_id:localhost").unwrap()));
}

#[test]
fn successfully_bridge_a_rocketchat_room_that_an_other_user_already_bridged() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let (invite_forwarder, invite_receiver) = MessageForwarder::new();

    let spec_user_id = UserId::try_from("@spec_user:localhost").unwrap();
    let other_user_id = UserId::try_from("@other_user:localhost").unwrap();
    let bot_user_id = UserId::try_from("@rocketchat:localhost").unwrap();
    let admin_room_id = RoomId::try_from("!admin_room_id:localhost").unwrap();

    // common routes/mocked endpoints
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(InviteEndpoint::router_path(), invite_forwarder, "invite_user");

    let mut rocketchat_router = Router::new();
    rocketchat_router.post(
        LOGIN_PATH,
        handlers::RocketchatLogin {
            successful: true,
            rocketchat_user_id: None,
        },
        "login",
    );
    rocketchat_router.get(ME_PATH, handlers::RocketchatMe { username: "spec_user".to_string() }, "me");
    rocketchat_router.get(USERS_INFO_PATH, handlers::RocketchatUsersInfo {}, "users_info");

    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user", "other_user"]);

    let test = test.with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_custom_channel_list(channels)
        .run();

    // login spec user
    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        admin_room_id.clone(),
        spec_user_id.clone(),
        "login spec_user secret".to_string(),
    );

    // create other admin room
    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    matrix_api.create_room(Some("other_admin_room".to_string()), None, &other_user_id).unwrap();
    helpers::invite(
        &test.config.as_url,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        other_user_id.clone(),
        bot_user_id.clone(),
    );

    // connect other admin room
    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        other_user_id.clone(),
        format!("connect {}", test.rocketchat_mock_url.clone().unwrap()),
    );


    // login other user
    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        other_user_id.clone(),
        "login other_user secret".to_string(),
    );

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        admin_room_id,
        spec_user_id.clone(),
        "bridge joined_channel".to_string(),
    );

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!other_admin_room_id:localhost").unwrap(),
        other_user_id.clone(),
        "bridge joined_channel".to_string(),
    );

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

    let other_message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(other_message_received_by_matrix.contains("joined_channel is now bridged."));

    helpers::join(&test.config, RoomId::try_from("!joined_channel_id:localhost").unwrap(), bot_user_id.clone());

    helpers::join(&test.config, RoomId::try_from("!joined_channel_id:localhost").unwrap(), spec_user_id.clone());

    helpers::join(&test.config, RoomId::try_from("!joined_channel_id:localhost").unwrap(), other_user_id.clone());

    let spec_user_invite_received_by_matrix = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(spec_user_invite_received_by_matrix.contains("@spec_user:localhost"));

    let virtual_spec_user_invite_received_by_matrix = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(virtual_spec_user_invite_received_by_matrix.contains("@rocketchat_spec_user_id_rc_id:localhost"));

    let other_user_invite_received_by_matrix = invite_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(other_user_invite_received_by_matrix.contains("@rocketchat_other_user_id_rc_id:localhost"));

    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    let user_ids = Room::user_ids(&(*matrix_api), RoomId::try_from("!joined_channel_id:localhost").unwrap()).unwrap();
    assert!(user_ids.iter().any(|id| id == &bot_user_id));
    assert!(user_ids.iter().any(|id| id == &spec_user_id));
    assert!(user_ids.iter().any(|id| id == &other_user_id));
}

#[test]
fn susccessfully_bridge_a_rocketchat_room_that_was_unbridged_before() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let (invite_forwarder, invite_receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(InviteEndpoint::router_path(), invite_forwarder, "invite_user");

    let test = test.with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_bridged_room(("joined_channel", "spec_user"))
        .run();

    helpers::leave_room(
        &test.config,
        RoomId::try_from("!joined_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "unbridge joined_channel".to_string(),
    );

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "bridge joined_channel".to_string(),
    );

    helpers::join(
        &test.config,
        RoomId::try_from("!joined_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

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

    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    let user_ids = Room::user_ids(&(*matrix_api), RoomId::try_from("!joined_channel_id:localhost").unwrap()).unwrap();
    assert!(user_ids.iter().any(|id| id == &UserId::try_from("@rocketchat:localhost").unwrap()));
    assert!(user_ids.iter().any(|id| id == &UserId::try_from("@spec_user:localhost").unwrap()));
}

#[test]
fn successfully_bridge_two_different_rocketchat_rooms() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut channels = HashMap::new();
    channels.insert("first_channel", vec!["spec_user", "other_user"]);
    channels.insert("second_channel", vec!["spec_user", "other_user"]);

    let test = test.with_matrix_routes(matrix_router)
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

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "bridge first_channel".to_string(),
    );

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "bridge second_channel".to_string(),
    );

    let first_message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(first_message_received_by_matrix.contains("first_channel is now bridged."));

    let second_message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(second_message_received_by_matrix.contains("second_channel is now bridged."));

    helpers::join(
        &test.config,
        RoomId::try_from("!first_channel_id:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
    );

    helpers::join(
        &test.config,
        RoomId::try_from("!first_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    helpers::join(
        &test.config,
        RoomId::try_from("!second_channel_id:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
    );

    helpers::join(
        &test.config,
        RoomId::try_from("!second_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    let first_user_ids = Room::user_ids(&(*matrix_api), RoomId::try_from("!first_channel_id:localhost").unwrap()).unwrap();
    let rocketchat_user_id = UserId::try_from("@rocketchat:localhost").unwrap();
    let spec_user_id = UserId::try_from("@spec_user:localhost").unwrap();
    let virtual_spec_user_id = UserId::try_from("@rocketchat_spec_user_id_rc_id:localhost").unwrap();
    let virtual_other_user_id = UserId::try_from("@rocketchat_other_user_id_rc_id:localhost").unwrap();

    assert!(first_user_ids.iter().any(|id| id == &rocketchat_user_id));
    assert!(first_user_ids.iter().any(|id| id == &spec_user_id));
    assert!(first_user_ids.iter().any(|id| id == &virtual_spec_user_id));
    assert!(first_user_ids.iter().any(|id| id == &virtual_other_user_id));

    let sec_users = Room::user_ids(&(*matrix_api), RoomId::try_from("!second_channel_id:localhost").unwrap()).unwrap();
    assert!(sec_users.iter().any(|id| id == &rocketchat_user_id));
    assert!(sec_users.iter().any(|id| id == &spec_user_id));
    assert!(sec_users.iter().any(|id| id == &virtual_spec_user_id));
    assert!(sec_users.iter().any(|id| id == &virtual_other_user_id));
}

#[test]
fn do_not_allow_to_bridge_channels_that_the_user_has_not_joined_on_the_rocketchat_server() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut channels = HashMap::new();
    channels.insert("normal_channel", Vec::new());

    let test = test.with_matrix_routes(matrix_router)
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

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "bridge normal_channel".to_string(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains(
        "You have to join the channel normal_channel on the Rocket.Chat server \
                                                before you can bridge it.",
    ));
}

#[test]
fn attempting_to_bridge_a_non_existing_channel_returns_an_error() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut channels = HashMap::new();
    channels.insert("normal_channel", Vec::new());
    let test = test.with_matrix_routes(matrix_router)
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

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "bridge nonexisting_channel".to_string(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("No channel with the name nonexisting_channel found."));
}

#[test]
fn attempting_to_bridge_an_already_bridged_channel_returns_an_error() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user"]);
    let test = test.with_matrix_routes(matrix_router)
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

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "bridge joined_channel".to_string(),
    );

    // spec_user accepts invite from bot user
    helpers::join(
        &test.config,
        RoomId::try_from("!joined_channel_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    // discard successful bridge message
    receiver.recv_timeout(default_timeout()).unwrap();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "bridge joined_channel".to_string(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("The channel joined_channel is already bridged."));
}

#[test]
fn the_room_is_not_bridged_when_setting_the_canonical_room_alias_failes() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let error_responder = handlers::MatrixErrorResponder {
        status: status::InternalServerError,
        message: "Could not set canonical room alias".to_string(),
    };
    matrix_router.put(
        "/_matrix/client/r0/rooms/!joined_channel_id:localhost/state/m.room.canonical_alias",
        error_responder,
        "put_room_canonical_room_alias",
    );

    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user"]);
    let test = test.with_matrix_routes(matrix_router)
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

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "bridge joined_channel".to_string(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn bridge_a_room_after_setting_the_canonical_room_failed() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let (put_room_canonical_room_alias_forwarder, put_room_canonical_room_alias_receiver) =
        handlers::SendRoomState::with_forwarder();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.put(
        "/_matrix/client/r0/rooms/:room_id/state/m.room.canonical_alias",
        put_room_canonical_room_alias_forwarder,
        "put_room_canonical_room_alias",
    );
    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user"]);
    let test = test.with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_connected_admin_room()
        .with_logged_in_user()
        .with_custom_channel_list(channels)
        .run();

    // create a room with an alias that is used by the bridge, this can happen when attempting to
    // bridge a channel, but the canonical room alias could not be set. When tried again, the room
    // with the alias has to be used to bridge the channel.
    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    matrix_api
        .create_room(
            Some("joined_channel".to_string()),
            Some("rocketchat#rc_id#joined_channel_id".to_string()),
            &UserId::try_from("@rocketchat:localhost").unwrap(),
        )
        .unwrap();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "bridge joined_channel".to_string(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();

    let put_canonical_room_alias_message = put_room_canonical_room_alias_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(put_canonical_room_alias_message.contains("rocketchat#rc_id#joined_channel_id"));

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("joined_channel is now bridged."))
}

#[test]
fn the_user_gets_a_message_when_creating_the_room_failes() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let create_room = handlers::MatrixCreateRoom { as_url: test.config.as_url.clone() };
    let conditional_error = handlers::MatrixConditionalErrorResponder {
        status: status::InternalServerError,
        message: "Could not set power levels".to_string(),
        conditional_content: "joined_channel",
    };
    let mut create_room_with_error = Chain::new(create_room);
    create_room_with_error.link_before(conditional_error);

    matrix_router.post(CreateRoomEndpoint::router_path(), create_room_with_error, "create_room");
    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user"]);
    let test = test.with_matrix_routes(matrix_router)
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

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "bridge joined_channel".to_string(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn the_user_gets_a_message_when_setting_the_powerlevels_failes() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let powerlevel_params = send_state_event_for_empty_key::PathParams {
        room_id: RoomId::try_from("!joined_channel_id:localhost").unwrap(),
        event_type: EventType::RoomPowerLevels,
    };
    matrix_router.put(
        SendStateEventForEmptyKeyEndpoint::request_path(powerlevel_params),
        handlers::MatrixConditionalErrorResponder {
            status: status::InternalServerError,
            message: "Could not set power levels".to_string(),
            conditional_content: "invite",
        },
        "set_power_levels",
    );
    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user"]);
    let test = test.with_matrix_routes(matrix_router)
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

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "bridge joined_channel".to_string(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn the_user_gets_a_message_when_inviting_the_user_failes() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(
        InviteEndpoint::router_path(),
        handlers::MatrixErrorResponder {
            status: status::InternalServerError,
            message: "Could not invite user".to_string(),
        },
        "invite",
    );
    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user"]);
    let test = test.with_matrix_routes(matrix_router)
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

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "bridge joined_channel".to_string(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn the_user_gets_a_message_when_getting_the_users_info_failes() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user"]);

    let mut rocketchat_router = test.default_matrix_routes();
    rocketchat_router.post(
        LOGIN_PATH,
        handlers::RocketchatLogin {
            successful: true,
            rocketchat_user_id: None,
        },
        "login",
    );
    rocketchat_router.get(ME_PATH, handlers::RocketchatMe { username: "spec_user".to_string() }, "me");
    rocketchat_router.get(
        USERS_INFO_PATH,
        handlers::RocketchatErrorResponder {
            message: "Rocketh.Chat users.info error".to_string(),
            status: status::InternalServerError,
        },
        "users_info",
    );

    let test = test.with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_custom_channel_list(channels)
        .run();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "login spec_user secret".to_string(),
    );

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "bridge joined_channel".to_string(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn the_user_gets_a_message_when_getting_the_room_alias_failes() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.get(
        GetAliasEndpoint::router_path(),
        handlers::MatrixErrorResponder {
            status: status::InternalServerError,
            message: "Could not get room alias".to_string(),
        },
        "get_room_alias",
    );
    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user"]);
    let test = test.with_matrix_routes(matrix_router)
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

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "bridge joined_channel".to_string(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn the_user_gets_a_message_when_the_create_room_response_cannot_be_deserialized() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let create_room = handlers::MatrixCreateRoom { as_url: test.config.as_url.clone() };
    let conditional_invalid_json_responder = handlers::ConditionalInvalidJsonResponse {
        status: status::Ok,
        conditional_content: "joined_channel",
    };
    let mut create_room_with_invalid_error_responder = Chain::new(create_room);
    create_room_with_invalid_error_responder.link_before(conditional_invalid_json_responder);

    matrix_router.post(CreateRoomEndpoint::router_path(), create_room_with_invalid_error_responder, "create_room");
    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user"]);
    let test = test.with_matrix_routes(matrix_router)
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

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "bridge joined_channel".to_string(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn the_user_gets_a_message_when_the_users_info_response_cannot_be_deserialized() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user"]);

    let mut rocketchat_router = Router::new();
    rocketchat_router.post(
        LOGIN_PATH,
        handlers::RocketchatLogin {
            successful: true,
            rocketchat_user_id: None,
        },
        "login",
    );
    rocketchat_router.get(ME_PATH, handlers::RocketchatMe { username: "spec_user".to_string() }, "me");
    rocketchat_router.get(USERS_INFO_PATH, handlers::InvalidJsonResponse { status: status::Ok }, "users_info");

    let test = test.with_matrix_routes(matrix_router)
        .with_rocketchat_mock()
        .with_custom_rocketchat_routes(rocketchat_router)
        .with_connected_admin_room()
        .with_custom_channel_list(channels)
        .run();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "login spec_user secret".to_string(),
    );

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "bridge joined_channel".to_string(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard connect message
    receiver.recv_timeout(default_timeout()).unwrap();
    // discard login message
    receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}

#[test]
fn the_user_gets_a_message_when_getting_the_room_alias_response_cannot_be_deserialized() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.get(GetAliasEndpoint::router_path(), handlers::InvalidJsonResponse { status: status::Ok }, "get_room_alias");
    let mut channels = HashMap::new();
    channels.insert("joined_channel", vec!["spec_user"]);
    let test = test.with_matrix_routes(matrix_router)
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

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
        "bridge joined_channel".to_string(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
}