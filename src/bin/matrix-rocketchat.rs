//! Application service to bridge Matrix <-> Rocket.Chat.

#![deny(missing_docs)]

extern crate matrix_rocketchat;

use matrix_rocketchat::{Config, Server};

fn main() {
    let config = Config {};
    Server::new(&config).run();
}
