extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate router;

use matrix_rocketchat::db::Room;
use matrix_rocketchat_test::{MessageForwarder, Test, create_admin_room, default_timeout};
use router::Router;

#[test]
fn successfully_create_an_admin_room() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put("/_matrix/client/r0/rooms/!admin:localhost/send/m.room.message/:txid",
                      message_forwarder);
    let test = Test::new().with_matrix_homeserver_mock().with_custom_matrix_routes(matrix_router).run();

    create_admin_room(test.config.as_url.to_string(),
                      "!admin:localhost",
                      "@spec_user:localhost",
                      "@rocketchat:localhost");

    let message_received_by_matrix = receiver.recv_timeout(default_timeout()).unwrap();
    assert!(message_received_by_matrix.starts_with("Hi, I'm the Rocket.Chat application service"));

    let room = Room::find("!admin:localhost").unwrap();
    assert!(room.is_admin_room);

    let members = room.users().unwrap();
    assert!(members.iter().any(|m| m.matrix_user_id == "@rocketchat:localhost"));
    assert!(members.iter().any(|m| m.matrix_user_id == "@spec_user:localhost"));
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
