#![feature(try_from)]

extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_identifiers;

use std::convert::TryFrom;

use matrix_rocketchat::db::Room;
use matrix_rocketchat_test::{MessageForwarder, Test, default_timeout};
use matrix_rocketchat_test::handlers;
use matrix_rocketchat_test::helpers;
use router::Router;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::membership::join_room_by_id::Endpoint as JoinEndpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_client_api::r0::sync::get_member_events::Endpoint as GetMemberEventsEndpoint;
use ruma_identifiers::{RoomId, UserId};

#[test]
fn successfully_create_an_admin_room() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.post(JoinEndpoint::router_path(), handlers::EmptyJson {});
    let two_room_members = handlers::TwoRoomMembers {
        room_id: RoomId::try_from("!admin:localhost").expect("Could not create room ID"),
        members: [UserId::try_from("@spec_user:localhost").expect("Could not create user ID"),
                  UserId::try_from("@spec_user:localhost").expect("Could not create user ID")],
    };
    matrix_router.get(GetMemberEventsEndpoint::router_path(), two_room_members);
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder);
    let test = Test::new().with_matrix_homeserver_mock().with_custom_matrix_routes(matrix_router).run();

    helpers::create_admin_room(test.config.as_url.to_string(),
                               RoomId::try_from("!admin:localhost").expect("Could not create room ID"),
                               UserId::try_from("@spec_user:localhost").expect("Could not create user ID"),
                               UserId::try_from("@rocketchat:localhost").expect("Could not create user ID"));

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.starts_with("Hi, I'm the Rocket.Chat application service"));

    let connection = test.connection_pool.get().unwrap();
    let room = Room::find(&connection, &RoomId::try_from("!admin:localhost").unwrap()).unwrap();
    assert!(room.is_admin_room);

    let members = room.users(&connection).unwrap();
    assert!(members.iter().any(|m| m.matrix_user_id == UserId::try_from("@rocketchat:localhost").unwrap()));
    assert!(members.iter().any(|m| m.matrix_user_id == UserId::try_from("@spec_user:localhost").unwrap()));
    assert_eq!(members.len(), 2);
}

#[test]
fn attempt_to_create_an_admin_room_with_other_users_in_it() {}

#[test]
fn bot_leaves_and_forgetts_the_room_when_the_user_leaves_it() {}

#[test]
fn the_user_gets_a_message_when_joining_the_room_failes_for_the_bot_user() {}

#[test]
fn the_user_gets_a_message_when_getting_the_room_members_failes() {}

#[test]
fn the_user_gets_a_message_when_setting_the_display_name_failes() {}

#[test]
fn the_user_gets_a_message_when_an_leaving_the_room_failes_for_the_bot_user() {}

#[test]
fn the_user_gets_a_message_when_forgetting_the_room_failes_for_the_bot_user() {}

#[test]
fn bot_leaves_when_a_third_user_joins_the_admin_room() {}
