#![feature(try_from)]

extern crate diesel;
extern crate iron;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate router;
extern crate ruma_client_api;
extern crate ruma_identifiers;

use std::convert::TryFrom;

use diesel::result::Error as DieselError;
use iron::status;
use matrix_rocketchat::db::Room;
use matrix_rocketchat::db::UserInRoom;
use matrix_rocketchat_test::{MessageForwarder, Test, default_timeout};
use matrix_rocketchat_test::handlers;
use matrix_rocketchat_test::helpers;
use router::Router;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::membership::forget_room::Endpoint as ForgetRoomEndpoint;
use ruma_client_api::r0::membership::join_room_by_id::Endpoint as JoinEndpoint;
use ruma_client_api::r0::membership::leave_room::Endpoint as LeaveRoomEndpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_client_api::r0::sync::get_member_events::Endpoint as GetMemberEventsEndpoint;
use ruma_identifiers::{RoomId, UserId};

#[test]
fn successfully_create_an_admin_room() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.post(JoinEndpoint::router_path(), handlers::EmptyJson {});
    let room_members = handlers::RoomMembers {
        room_id: RoomId::try_from("!admin:localhost").expect("Could not create room ID"),
        members: vec![UserId::try_from("@spec_user:localhost").expect("Could not create user ID"),
                      UserId::try_from("@rocketchat:localhost").expect("Could not create user ID")],
    };
    matrix_router.get(GetMemberEventsEndpoint::router_path(), room_members);
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder);
    let test = Test::new().with_custom_matrix_routes(matrix_router).run();

    helpers::create_admin_room(test.config.as_url.to_string(),
                               RoomId::try_from("!admin:localhost").expect("Could not create room ID"),
                               UserId::try_from("@spec_user:localhost").expect("Could not create user ID"),
                               UserId::try_from("@rocketchat:localhost").expect("Could not create user ID"));

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Hi, I'm the Rocket.Chat application service"));

    let connection = test.connection_pool.get().unwrap();
    let room = Room::find(&connection, &RoomId::try_from("!admin:localhost").unwrap()).unwrap();
    assert!(room.is_admin_room);

    let members = room.users(&connection).unwrap();
    assert_eq!(members.len(), 2);
    assert!(members.iter().any(|m| m.matrix_user_id == UserId::try_from("@rocketchat:localhost").unwrap()));
    assert!(members.iter().any(|m| m.matrix_user_id == UserId::try_from("@spec_user:localhost").unwrap()));
}

#[test]
fn attempt_to_create_an_admin_room_with_other_users_in_it() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.post(JoinEndpoint::router_path(), handlers::EmptyJson {});
    let room_members = handlers::RoomMembers {
        room_id: RoomId::try_from("!admin:localhost").expect("Could not create room ID"),
        members: vec![UserId::try_from("@spec_user:localhost").expect("Could not create user ID"),
                      UserId::try_from("@other_user:localhost").expect("Could not create user ID"),
                      UserId::try_from("@rocketchat:localhost").expect("Could not create user ID")],
    };
    matrix_router.get(GetMemberEventsEndpoint::router_path(), room_members);
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder);
    let test = Test::new().with_custom_matrix_routes(matrix_router).run();

    helpers::create_admin_room(test.config.as_url.to_string(),
                               RoomId::try_from("!admin:localhost").expect("Could not create room ID"),
                               UserId::try_from("@spec_user:localhost").expect("Could not create user ID"),
                               UserId::try_from("@rocketchat:localhost").expect("Could not create user ID"));

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.contains("Admin rooms must only contain the user that invites the bot. Too many members in the room, leaving."));

    let connection = test.connection_pool.get().unwrap();
    let room_error = Room::find(&connection, &RoomId::try_from("!admin:localhost").unwrap()).err().unwrap();
    let room_diesel_error = room_error.iter().nth(1).unwrap();
    assert_eq!(format!("{}", room_diesel_error), format!("{}", DieselError::NotFound));

    let spec_user_in_room_error = UserInRoom::find(&connection,
                                                   &UserId::try_from("@spec_user:localhost").unwrap(),
                                                   &RoomId::try_from("!admin:localhost").unwrap())
        .err()
        .unwrap();
    let spec_user_in_room_diesel_error = spec_user_in_room_error.iter().nth(1).unwrap();
    assert_eq!(format!("{}", spec_user_in_room_diesel_error),
               format!("{}", DieselError::NotFound));

    let bot_user_in_room_error = UserInRoom::find(&connection,
                                                  &UserId::try_from("@rocketchat:localhost").unwrap(),
                                                  &RoomId::try_from("!admin:localhost").unwrap())
        .err()
        .unwrap();
    let bot_user_in_room_diesel_error = bot_user_in_room_error.iter().nth(1).unwrap();
    assert_eq!(format!("{}", bot_user_in_room_diesel_error),
               format!("{}", DieselError::NotFound));
}

#[test]
fn bot_leaves_and_forgets_the_room_when_the_user_leaves_it() {
    let (leave_message_forwarder, leave_receiver) = MessageForwarder::new();
    let (forget_message_forwarder, forget_receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.post(LeaveRoomEndpoint::router_path(), leave_message_forwarder);
    matrix_router.post(ForgetRoomEndpoint::router_path(), forget_message_forwarder);
    let test = Test::new().with_custom_matrix_routes(matrix_router).with_admin_room().run();

    let connection = test.connection_pool.get().unwrap();
    let room = Room::find(&connection, &RoomId::try_from("!admin:localhost").unwrap()).unwrap();
    assert!(room.is_admin_room);

    helpers::leave_room(test.config.as_url.to_string(),
                        RoomId::try_from("!admin:localhost").expect("Could not create room ID"),
                        UserId::try_from("@spec_user:localhost").expect("Could not create user ID"));

    leave_receiver.recv_timeout(default_timeout()).unwrap();
    forget_receiver.recv_timeout(default_timeout()).unwrap();

    let room_error = Room::find(&connection, &RoomId::try_from("!admin:localhost").unwrap()).err().unwrap();
    let room_diesel_error = room_error.iter().nth(1).unwrap();
    assert_eq!(format!("{}", room_diesel_error), format!("{}", DieselError::NotFound));

    let spec_user_in_room_error = UserInRoom::find(&connection,
                                                   &UserId::try_from("@spec_user:localhost").unwrap(),
                                                   &RoomId::try_from("!admin:localhost").unwrap())
        .err()
        .unwrap();
    let spec_user_in_room_diesel_error = spec_user_in_room_error.iter().nth(1).unwrap();
    assert_eq!(format!("{}", spec_user_in_room_diesel_error),
               format!("{}", DieselError::NotFound));

    let bot_user_in_room_error = UserInRoom::find(&connection,
                                                  &UserId::try_from("@rocketchat:localhost").unwrap(),
                                                  &RoomId::try_from("!admin:localhost").unwrap())
        .err()
        .unwrap();
    let bot_user_in_room_diesel_error = bot_user_in_room_error.iter().nth(1).unwrap();
    assert_eq!(format!("{}", bot_user_in_room_diesel_error),
               format!("{}", DieselError::NotFound));
}

#[test]
fn the_user_gets_a_message_when_joining_the_room_failes_for_the_bot_user() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.post(JoinEndpoint::router_path(),
                       handlers::ErrorResponse { status: status::InternalServerError });
    let room_members = handlers::RoomMembers {
        room_id: RoomId::try_from("!admin:localhost").expect("Could not create room ID"),
        members: vec![UserId::try_from("@spec_user:localhost").expect("Could not create user ID"),
                      UserId::try_from("@rocketchat:localhost").expect("Could not create user ID")],
    };
    matrix_router.get(GetMemberEventsEndpoint::router_path(), room_members);
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder);
    let test = Test::new().with_custom_matrix_routes(matrix_router).run();

    helpers::invite(test.config.as_url.to_string(),
                    RoomId::try_from("!admin:localhost").expect("Could not create room ID"),
                    UserId::try_from("@spec_user:localhost").expect("Could not create user ID"),
                    UserId::try_from("@rocketchat:localhost").expect("Could not create user ID"));

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    println!("MESSAGE: {}", message_received_by_matrix);
    assert!(message_received_by_matrix.contains("An internal error occurred (Matrix error: An error occurred)"));

    let connection = test.connection_pool.get().unwrap();
    let room = Room::find(&connection, &RoomId::try_from("!admin:localhost").unwrap()).unwrap();
    assert!(room.is_admin_room);

    let members = room.users(&connection).unwrap();
    assert_eq!(members.len(), 1);
    assert!(members.iter().any(|m| m.matrix_user_id == UserId::try_from("@spec_user:localhost").unwrap()));
}

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
