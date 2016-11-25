extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;

use matrix_rocketchat_test::{Test, call_url};

#[test]
fn root_url_returns_a_welcome_message() {
    let test = Test::new().run();
    let url = test.config.as_url.clone();

    let (body, status) = call_url(&url);
    assert_eq!(body, "Your Rocket.Chat <-> Matrix application service is running");
    assert!(status.is_success());
}
