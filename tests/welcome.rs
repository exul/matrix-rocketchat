extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate reqwest;

use std::collections::HashMap;

use matrix_rocketchat_test::{Test, call_url};
use reqwest::StatusCode;

#[test]
fn root_url_returns_a_welcome_message() {
    let test = Test::new().run();
    let url = test.config.as_url.clone();
    let params = HashMap::new();

    let (body, status) = call_url("GET", &url, "", &params);
    assert_eq!(body, "Your Rocket.Chat <-> Matrix application service is running");
    assert_eq!(status, StatusCode::Ok);
}
