#![feature(try_from)]

extern crate diesel;
extern crate iron;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_events;
extern crate ruma_identifiers;
extern crate serde_json;
extern crate tempdir;

use std::convert::TryFrom;

use iron::status;
use matrix_rocketchat::api::MatrixApi;
use matrix_rocketchat::models::{Events, Room};
use matrix_rocketchat_test::{build_test_config, default_timeout, handlers, helpers, MessageForwarder, Test, DEFAULT_LOGGER,
                             TEMP_DIR_NAME};
use ruma_client_api::Endpoint;
use ruma_client_api::r0::membership::forget_room::Endpoint as ForgetRoomEndpoint;
use ruma_client_api::r0::membership::join_room_by_id::Endpoint as JoinEndpoint;
use ruma_client_api::r0::membership::leave_room::Endpoint as LeaveRoomEndpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_client_api::r0::send::send_state_event_for_empty_key::Endpoint as SendStateEventForEmptyKeyEndpoint;
use ruma_client_api::r0::sync::get_member_events::Endpoint as GetMemberEventsEndpoint;
use ruma_client_api::r0::sync::get_state_events_for_empty_key::{self, Endpoint as GetStateEventsForEmptyKey};
use ruma_events::EventType;
use ruma_events::collections::all::Event;
use ruma_events::room::member::{MemberEvent, MemberEventContent, MembershipState};
use ruma_identifiers::{EventId, RoomId, UserId};
use serde_json::to_string;
use tempdir::TempDir;

#[test]
fn successfully_create_an_admin_room() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test = test.with_matrix_routes(matrix_router).with_admin_room().run();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Hi, I'm the Rocket.Chat application service"));
    assert!(message_received_by_matrix.contains(
        "You have to connect this room to a Rocket.Chat server. To do so you can \
         either use an already connected server (if there is one) or connect to a \
         new server.",
    ));
    assert!(message_received_by_matrix.contains("No Rocket.Chat server is connected yet."));

    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    let room_id = RoomId::try_from("!admin_room_id:localhost").unwrap();
    let room = Room::new(&test.config, &DEFAULT_LOGGER, &(*matrix_api), room_id);
    let members = room.user_ids(None).unwrap();
    assert_eq!(members.len(), 2);
    assert!(members.iter().any(|id| id == &UserId::try_from("@rocketchat:localhost").unwrap()));
    assert!(members.iter().any(|id| id == &UserId::try_from("@spec_user:localhost").unwrap()));
}

#[test]
fn attempt_to_create_an_admin_room_with_other_users_in_it() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let test = test.with_matrix_routes(matrix_router).run();

    helpers::create_room(
        &test.config,
        "admin_room",
        UserId::try_from("@spec_user:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
    );

    helpers::join(
        &test.config,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
    );

    helpers::invite(
        &test.config,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains(
        "Admin rooms must only contain the user that invites the bot. \
         Too many members in the room, leaving.",
    ));
}

#[test]
fn bot_leaves_and_forgets_the_admin_room_when_the_user_leaves_it() {
    let test = Test::new();
    let (forget_message_forwarder, forget_receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    let (leave_room, leave_receiver) = handlers::MatrixLeaveRoom::with_forwarder(test.config.as_url.clone());
    matrix_router.post(LeaveRoomEndpoint::router_path(), leave_room, "leave_room");
    matrix_router.post(ForgetRoomEndpoint::router_path(), forget_message_forwarder, "forget_room");

    let test = test.with_matrix_routes(matrix_router).with_admin_room().run();

    helpers::leave_room(
        &test.config,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    leave_receiver.recv_timeout(default_timeout()).unwrap();
    forget_receiver.recv_timeout(default_timeout()).unwrap();
}

#[test]
fn bot_ignoeres_when_a_user_leaves_a_room_that_is_not_bridged() {
    let test = Test::new();
    let (leave_message_forwarder, leave_receiver) = MessageForwarder::new();
    let (forget_message_forwarder, forget_receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.post(LeaveRoomEndpoint::router_path(), leave_message_forwarder, "leave_room");
    matrix_router.post(ForgetRoomEndpoint::router_path(), forget_message_forwarder, "forget_room");
    let test = test.with_matrix_routes(matrix_router).with_admin_room().run();

    helpers::send_leave_event_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!unknown:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    // no messages is received on the leave and forget endpoints, because the leave event is
    // ignored
    assert!(leave_receiver.recv_timeout(default_timeout()).is_err());
    assert!(forget_receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn the_user_does_not_get_a_message_when_joining_the_room_failes_for_the_bot_user() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    let error_responder = handlers::MatrixErrorResponder {
        status: status::InternalServerError,
        message: "Could not join room".to_string(),
    };
    matrix_router.post(JoinEndpoint::router_path(), error_responder, "join");
    let admin_room_creator_handler = handlers::RoomStateCreate {
        creator: UserId::try_from("@spec_user:localhost").unwrap(),
    };
    matrix_router.get(GetStateEventsForEmptyKey::router_path(), admin_room_creator_handler, "get_room_creator_admin_room");
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test = test.with_matrix_routes(matrix_router).run();

    helpers::create_room(
        &test.config,
        "admin_room",
        UserId::try_from("@spec_user:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
    );

    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn the_bot_user_leaves_the_admin_room_when_getting_the_room_members_failes() {
    let test = Test::new();
    let mut matrix_router = test.default_matrix_routes();
    let error_responder = handlers::MatrixErrorResponder {
        status: status::InternalServerError,
        message: "Could not get room members".to_string(),
    };
    matrix_router.get(GetMemberEventsEndpoint::router_path(), error_responder, "get_member_events");
    let admin_room_creator_handler = handlers::RoomStateCreate {
        creator: UserId::try_from("@spec_user:localhost").unwrap(),
    };
    let (leave_room, leave_room_receiver) = handlers::MatrixLeaveRoom::with_forwarder(test.config.as_url.clone());
    matrix_router.post(LeaveRoomEndpoint::router_path(), leave_room, "leave_room");
    let (forget_forwarder, forget_receiver) = MessageForwarder::new();
    matrix_router.post(ForgetRoomEndpoint::router_path(), forget_forwarder, "forget");
    matrix_router.get(GetStateEventsForEmptyKey::router_path(), admin_room_creator_handler, "get_room_creator_admin_room");
    let test = test.with_matrix_routes(matrix_router).run();

    helpers::create_room(
        &test.config,
        "admin_room",
        UserId::try_from("@spec_user:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
    );

    helpers::invite(
        &test.config,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    let leave_room_message = leave_room_receiver.recv_timeout(default_timeout()).unwrap();
    assert_eq!(leave_room_message, "{}");
    assert!(forget_receiver.recv_timeout(default_timeout()).is_ok());
}

#[test]
fn the_bot_user_leaves_the_admin_room_when_the_room_members_cannot_be_deserialized() {
    let test = Test::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.get(
        GetMemberEventsEndpoint::router_path(),
        handlers::InvalidJsonResponse {
            status: status::Ok,
        },
        "get_member",
    );
    let admin_room_creator_handler = handlers::RoomStateCreate {
        creator: UserId::try_from("@spec_user:localhost").unwrap(),
    };
    matrix_router.get(GetStateEventsForEmptyKey::router_path(), admin_room_creator_handler, "get_room_creator_admin_room");
    let (leave_room, leave_room_receiver) = handlers::MatrixLeaveRoom::with_forwarder(test.config.as_url.clone());
    matrix_router.post(LeaveRoomEndpoint::router_path(), leave_room, "leave_room");
    let (forget_forwarder, forget_receiver) = MessageForwarder::new();
    matrix_router.post(ForgetRoomEndpoint::router_path(), forget_forwarder, "forget");

    let test = test.with_matrix_routes(matrix_router).run();

    helpers::create_room(
        &test.config,
        "admin_room",
        UserId::try_from("@spec_user:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
    );

    helpers::invite(
        &test.config,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    let leave_room_message = leave_room_receiver.recv_timeout(default_timeout()).unwrap();
    assert_eq!(leave_room_message, "{}");
    assert!(forget_receiver.recv_timeout(default_timeout()).is_ok());
}

#[test]
fn the_bot_user_does_not_leave_the_admin_room_just_because_setting_the_room_display_name_fails() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    let error_responder = handlers::MatrixErrorResponder {
        status: status::InternalServerError,
        message: "Could not set display name for room".to_string(),
    };
    matrix_router.put(SendStateEventForEmptyKeyEndpoint::router_path(), error_responder, "send_state_event_for_empty_key");
    let admin_room_creator_handler = handlers::RoomStateCreate {
        creator: UserId::try_from("@spec_user:localhost").unwrap(),
    };
    matrix_router.get(GetStateEventsForEmptyKey::router_path(), admin_room_creator_handler, "get_room_creator_admin_room");
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test = test.with_matrix_routes(matrix_router).run();

    helpers::create_room(
        &test.config,
        "admin_room",
        UserId::try_from("@spec_user:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    // the bot doesn't leave the room
    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    let room_id = RoomId::try_from("!admin_room_id:localhost").unwrap();
    let room = Room::new(&test.config, &DEFAULT_LOGGER, &(*matrix_api), room_id);
    let members = room.user_ids(None).unwrap();
    assert_eq!(members.len(), 2);
    assert!(members.iter().any(|id| id == &UserId::try_from("@spec_user:localhost").unwrap()));
    assert!(members.iter().any(|id| id == &UserId::try_from("@rocketchat:localhost").unwrap()));
}

#[test]
fn the_bot_user_does_not_leave_the_admin_room_just_because_getting_the_topic_failes() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let room_creator_params = get_state_events_for_empty_key::PathParams {
        room_id: RoomId::try_from("!admin_room_id:localhost").unwrap(),
        event_type: EventType::RoomTopic.to_string(),
    };
    matrix_router.get(
        GetStateEventsForEmptyKey::request_path(room_creator_params),
        handlers::MatrixErrorResponder {
            status: status::InternalServerError,
            message: "Could not get room topic.".to_string(),
        },
        "get_room_topic",
    );
    let test = test.with_matrix_routes(matrix_router).with_admin_room().run();

    // the user doesn't receive a welcome message, because without a topic it's not possible to
    // determine which message has to be sent
    assert!(receiver.recv_timeout(default_timeout()).is_err());

    // the bot doesn't leave the room
    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    let room_id = RoomId::try_from("!admin_room_id:localhost").unwrap();
    let room = Room::new(&test.config, &DEFAULT_LOGGER, &(*matrix_api), room_id);
    let members = room.user_ids(None).unwrap();
    assert_eq!(members.len(), 2);
    assert!(members.iter().any(|id| id == &UserId::try_from("@spec_user:localhost").unwrap()));
    assert!(members.iter().any(|id| id == &UserId::try_from("@rocketchat:localhost").unwrap()));
}

#[test]
fn the_bot_user_does_not_leave_the_admin_room_just_because_the_get_topic_response_cannot_be_deserialized() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let room_topic_params = get_state_events_for_empty_key::PathParams {
        room_id: RoomId::try_from("!admin_room_id:localhost").unwrap(),
        event_type: EventType::RoomTopic.to_string(),
    };
    matrix_router.get(
        GetStateEventsForEmptyKey::request_path(room_topic_params),
        handlers::InvalidJsonResponse {
            status: status::Ok,
        },
        "get_room_topic",
    );
    let test = test.with_matrix_routes(matrix_router).with_admin_room().run();

    // he user doesn't receive a welcome message, because without a topic it's not possible to
    // determine which message has to be sent
    assert!(receiver.recv_timeout(default_timeout()).is_err());

    // the bot doesn't leave the room
    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    let room_id = RoomId::try_from("!admin_room_id:localhost").unwrap();
    let room = Room::new(&test.config, &DEFAULT_LOGGER, &(*matrix_api), room_id);
    let members = room.user_ids(None).unwrap();
    assert_eq!(members.len(), 2);
    assert!(members.iter().any(|id| id == &UserId::try_from("@spec_user:localhost").unwrap()));
    assert!(members.iter().any(|id| id == &UserId::try_from("@rocketchat:localhost").unwrap()));
}

#[test]
fn the_user_does_not_get_a_message_when_an_leaving_the_room_failes_for_the_bot_user() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let error_responder = handlers::MatrixErrorResponder {
        status: status::InternalServerError,
        message: "Could not leave room".to_string(),
    };
    matrix_router.post(LeaveRoomEndpoint::router_path(), error_responder, "leave_room");
    let test = test.with_matrix_routes(matrix_router).run();

    helpers::create_room(
        &test.config,
        "admin_room",
        UserId::try_from("@spec_user:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
    );

    helpers::join(
        &test.config,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
    );

    helpers::invite(
        &test.config,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains(
        "Admin rooms must only contain the user that invites the bot. \
         Too many members in the room, leaving.",
    ));
    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn the_user_does_not_get_a_message_when_forgetting_the_room_failes_for_the_bot_user() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let error_responder = handlers::MatrixErrorResponder {
        status: status::InternalServerError,
        message: "Could not forget room".to_string(),
    };
    matrix_router.post(ForgetRoomEndpoint::router_path(), error_responder, "forget_room");

    let test = test.with_matrix_routes(matrix_router).run();

    helpers::create_room(
        &test.config,
        "admin_room",
        UserId::try_from("@spec_user:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
    );

    helpers::join(
        &test.config,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
    );

    helpers::invite(
        &test.config,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    // discard user readable error message that triggers the bot leave
    receiver.recv_timeout(default_timeout()).unwrap();

    // no error message is sent when the leave fails
    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn bot_leaves_when_a_third_user_joins_the_admin_room() {
    let test = Test::new();
    let (message_forwarder, message_receiver) = MessageForwarder::new();
    let (forget_forwarder, forget_receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.post(ForgetRoomEndpoint::router_path(), forget_forwarder, "forget");
    let (leave_room, leave_receiver) = handlers::MatrixLeaveRoom::with_forwarder(test.config.as_url.clone());
    matrix_router.post(LeaveRoomEndpoint::router_path(), leave_room, "leave_room");

    let test = test.with_matrix_routes(matrix_router).with_admin_room().run();

    let matrix_api = MatrixApi::new(&test.config, DEFAULT_LOGGER.clone()).unwrap();
    let room_id = RoomId::try_from("!admin_room_id:localhost").unwrap();
    let room = Room::new(&test.config, &DEFAULT_LOGGER, &(*matrix_api), room_id);
    let user_ids = room.user_ids(None).unwrap();
    assert_eq!(user_ids.len(), 2);

    helpers::invite(
        &test.config,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    helpers::join(
        &test.config,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
    );

    // leave was called
    leave_receiver.recv_timeout(default_timeout()).unwrap();

    // forget was called
    forget_receiver.recv_timeout(default_timeout()).unwrap();

    // discard welcome message
    message_receiver.recv_timeout(default_timeout()).unwrap();

    let message_received_by_matrix = message_receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Another user join the admin room, leaving, please create a new admin room."));
}

#[test]
fn unkown_membership_states_are_skipped() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test = test.with_matrix_routes(matrix_router).run();

    let unknown_event = MemberEvent {
        content: MemberEventContent {
            avatar_url: None,
            displayname: None,
            membership: MembershipState::Ban,
            third_party_invite: None,
        },
        event_id: EventId::new("localhost").unwrap(),
        event_type: EventType::RoomMember,
        invite_room_state: None,
        prev_content: None,
        room_id: RoomId::new("localhost").unwrap(),
        state_key: "@spec_user:localhost".to_string(),
        unsigned: None,
        user_id: UserId::new("localhost").unwrap(),
    };

    let events = Events {
        events: vec![Box::new(Event::RoomMember(unknown_event))],
    };

    let payload = to_string(&events).unwrap();

    helpers::simulate_message_from_matrix(&test.config.as_url, &payload);

    // the user does not get a message, because the event is ignored
    // so the receiver never gets a message and times out
    receiver.recv_timeout(default_timeout()).is_err();
}

#[test]
fn ignore_messages_from_the_bot_user() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");

    let test = test.with_admin_room().with_matrix_routes(matrix_router).run();

    helpers::send_room_message_from_matrix(
        &test.config.as_url,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
        "help".to_string(),
    );

    // discard welcome message
    receiver.recv_timeout(default_timeout()).unwrap();

    // no command is executed, so we get a timeout
    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn accept_invites_from_local_rooms_if_accept_remote_invites_is_set_to_false() {
    let temp_dir = TempDir::new(TEMP_DIR_NAME).unwrap();
    let mut config = build_test_config(&temp_dir);
    config.accept_remote_invites = false;
    let test = Test::new().with_custom_config(config);
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.get(
        GetStateEventsForEmptyKey::router_path(),
        handlers::RoomStateCreate {
            creator: UserId::try_from("@spec_user:localhost").unwrap(),
        },
        "get_state_events_for_empty_key",
    );
    let _test = test.with_admin_room().with_matrix_routes(matrix_router).run();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Hi, I'm the Rocket.Chat application service"));
}

#[test]
fn ignore_invites_from_rooms_on_other_homeservers_if_accept_remote_invites_is_set_to_false() {
    let temp_dir = TempDir::new(TEMP_DIR_NAME).unwrap();
    let mut config = build_test_config(&temp_dir);
    config.accept_remote_invites = false;
    let test = Test::new().with_custom_config(config);
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.get(
        GetStateEventsForEmptyKey::router_path(),
        handlers::RoomStateCreate {
            creator: UserId::try_from("@spec_user:localhost").unwrap(),
        },
        "get_state_events_for_empty_key",
    );
    let user_ids = vec![(UserId::try_from("@spec_user:other-homeserver.com").unwrap(), MembershipState::Join)];
    matrix_router.get(
        GetMemberEventsEndpoint::router_path(),
        handlers::StaticRoomMembers {
            user_ids: user_ids,
        },
        "room_members",
    );
    let test = test.with_matrix_routes(matrix_router).run();

    helpers::invite(
        &test.config,
        RoomId::try_from("!other_server_room_id:other-homeserver.com").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
        UserId::try_from("@spec_user:other-homeserver.com").unwrap(),
    );

    // the room doesn't get a message, because the bot user ignores the invite
    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn accept_invites_from_local_rooms_if_accept_remote_invites_is_set_to_true() {
    let temp_dir = TempDir::new(TEMP_DIR_NAME).unwrap();
    let mut config = build_test_config(&temp_dir);
    config.accept_remote_invites = true;
    let test = Test::new().with_custom_config(config);
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.get(
        GetStateEventsForEmptyKey::router_path(),
        handlers::RoomStateCreate {
            creator: UserId::try_from("@spec_user:localhost").unwrap(),
        },
        "get_state_events_for_empty_key",
    );
    let _test = test.with_admin_room().with_matrix_routes(matrix_router).run();

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Hi, I'm the Rocket.Chat application service"));
}

#[test]
fn accept_invites_from_rooms_on_other_homeservers_if_accept_remote_invites_is_set_to_true() {
    let temp_dir = TempDir::new(TEMP_DIR_NAME).unwrap();
    let mut config = build_test_config(&temp_dir);
    config.accept_remote_invites = true;
    let test = Test::new().with_custom_config(config);
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    matrix_router.get(
        GetStateEventsForEmptyKey::router_path(),
        handlers::RoomStateCreate {
            creator: UserId::try_from("@spec_user:other-homeserver.com").unwrap(),
        },
        "get_state_events_for_empty_key",
    );
    let user_ids = vec![
        (UserId::try_from("@spec_user:other-homeserver.com").unwrap(), MembershipState::Join),
        (UserId::try_from("@rocketchat:localhost").unwrap(), MembershipState::Join),
    ];
    matrix_router.get(
        GetMemberEventsEndpoint::router_path(),
        handlers::StaticRoomMembers {
            user_ids: user_ids,
        },
        "room_members",
    );

    let test = test.with_matrix_routes(matrix_router).run();

    helpers::invite(
        &test.config,
        RoomId::try_from("!other_server_room_id:other-homeserver.com").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
        UserId::try_from("@spec_user:other-homeserver.com").unwrap(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Hi, I'm the Rocket.Chat application service"));
}

#[test]
fn reject_invites_when_the_inviting_user_is_not_the_room_creator() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test = test.with_matrix_routes(matrix_router).run();

    helpers::create_room(
        &test.config,
        "admin_room",
        UserId::try_from("@other_user:loalhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    helpers::join(
        &test.config,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    helpers::leave_room(
        &test.config,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
    );

    helpers::invite(
        &test.config,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains(
        "Only the room creator can invite the Rocket.Chat bot user, \
         please create a new room and invite the Rocket.Chat user to \
         create an admin room.",
    ));
}

#[test]
fn the_user_gets_a_message_when_getting_the_room_creator_fails() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let room_creator_params = get_state_events_for_empty_key::PathParams {
        room_id: RoomId::try_from("!admin_room_id:localhost").unwrap(),
        event_type: EventType::RoomCreate.to_string(),
    };
    matrix_router.get(
        GetStateEventsForEmptyKey::request_path(room_creator_params),
        handlers::MatrixErrorResponder {
            status: status::InternalServerError,
            message: "Could not get room creator.".to_string(),
        },
        "get_room_creator",
    );
    let (leave_room, leave_receiver) = handlers::MatrixLeaveRoom::with_forwarder(test.config.as_url.clone());
    matrix_router.post(LeaveRoomEndpoint::router_path(), leave_room, "leave_room");
    let test = test.with_matrix_routes(matrix_router).run();

    helpers::create_room(
        &test.config,
        "admin_room",
        UserId::try_from("@spec_user:loalhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
    );

    let error_message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(error_message_received_by_matrix.contains("An internal error occurred"));
    assert!(leave_receiver.recv_timeout(default_timeout()).is_ok());
}

#[test]
fn the_user_does_get_a_message_when_getting_the_room_creator_cannot_be_deserialized() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let room_creator_params = get_state_events_for_empty_key::PathParams {
        room_id: RoomId::try_from("!admin_room_id:localhost").unwrap(),
        event_type: EventType::RoomCreate.to_string(),
    };
    matrix_router.get(
        GetStateEventsForEmptyKey::request_path(room_creator_params),
        handlers::InvalidJsonResponse {
            status: status::Ok,
        },
        "get_state_events_for_empty_key_with_invalid_json",
    );
    let (leave_room, leave_receiver) = handlers::MatrixLeaveRoom::with_forwarder(test.config.as_url.clone());
    matrix_router.post(LeaveRoomEndpoint::router_path(), leave_room, "leave_room");

    let test = test.with_matrix_routes(matrix_router).run();

    helpers::create_room(
        &test.config,
        "admin_room",
        UserId::try_from("@spec_user:loalhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
    );

    helpers::invite(
        &test.config,
        RoomId::try_from("!admin_room_id:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("An internal error occurred"));
    assert!(leave_receiver.recv_timeout(default_timeout()).is_ok());
}

#[test]
fn join_events_for_rooms_that_are_not_accessible_by_the_bot_user_are_ignored() {
    let test = Test::new();
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = test.default_matrix_routes();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let room_creator_params = get_state_events_for_empty_key::PathParams {
        room_id: RoomId::try_from("!some_room_id:localhost").unwrap(),
        event_type: EventType::RoomCreate.to_string(),
    };
    matrix_router.get(
        GetStateEventsForEmptyKey::request_path(room_creator_params),
        handlers::MatrixErrorResponder {
            status: status::Forbidden,
            message: "Guest access not allowed".to_string(),
        },
        "get_state_events_for_empty_key_forbidden",
    );

    let test = test.with_matrix_routes(matrix_router).run();

    helpers::create_room(
        &test.config,
        "some_room",
        UserId::try_from("@spec_user:loalhost").unwrap(),
        UserId::try_from("@other_user:localhost").unwrap(),
    );

    helpers::invite(
        &test.config,
        RoomId::try_from("!some_room_id:localhost").unwrap(),
        UserId::try_from("@third_user:localhost").unwrap(),
        UserId::try_from("@spec_user:localhost").unwrap(),
    );

    helpers::join(
        &test.config,
        RoomId::try_from("!some_room_id:localhost").unwrap(),
        UserId::try_from("@third_user:localhost").unwrap(),
    );

    assert!(receiver.recv_timeout(default_timeout()).is_err());
}

#[test]
fn the_bot_user_leaves_the_admin_room_the_inviter_is_unknown() {
    let test = Test::new();
    let mut matrix_router = test.default_matrix_routes();
    let (message_forwarder, receiver) = MessageForwarder::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let (leave_room, leave_room_receiver) = handlers::MatrixLeaveRoom::with_forwarder(test.config.as_url.clone());
    matrix_router.post(LeaveRoomEndpoint::router_path(), leave_room, "leave_room");
    let (forget_forwarder, forget_receiver) = MessageForwarder::new();
    matrix_router.post(ForgetRoomEndpoint::router_path(), forget_forwarder, "forget");
    matrix_router.post(JoinEndpoint::router_path(), handlers::EmptyJson {}, "join_room");
    let join_room_handler = handlers::MatrixJoinRoom {
        as_url: test.config.as_url.clone(),
        send_inviter: false,
    };
    matrix_router.post(JoinEndpoint::router_path(), join_room_handler, "join_room");

    let test = test.with_matrix_routes(matrix_router).run();

    helpers::create_room(
        &test.config,
        "admin_room",
        UserId::try_from("@spec_user:localhost").unwrap(),
        UserId::try_from("@rocketchat:localhost").unwrap(),
    );

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains(
        "Could not determine if the admin room is valid, because the inviter is unknown. \
         Possibly because the bot user was invited into an existing room. \
         Please start a direct chat with the bot user @rocketchat:localhost",
    ));

    assert!(leave_room_receiver.recv_timeout(default_timeout()).is_ok());
    assert!(forget_receiver.recv_timeout(default_timeout()).is_ok());
}
