#![feature(try_from)]

extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate ruma_identifiers;

use std::convert::TryFrom;
use std::error::Error;

use matrix_rocketchat::db::UserInRoom;
use matrix_rocketchat_test::Test;
use ruma_identifiers::{RoomId, UserId};

#[test]
fn error_descriptions_from_the_error_chain_are_passed_to_the_outer_error() {
    let test = Test::new().run();

    let connection = test.connection_pool.get().unwrap();
    let not_found_error = UserInRoom::find(&connection,
                                           &UserId::try_from("@nonexisting:localhost").unwrap(),
                                           &RoomId::try_from("!some_room:localhost").unwrap())
        .unwrap_err();

    assert_eq!(not_found_error.description(), "Select record from the database failed");
}
