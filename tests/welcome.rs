extern crate http;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate reqwest;

use std::collections::HashMap;

use http::{Method, StatusCode};
use matrix_rocketchat::api::{RequestData, RestApi};
use matrix_rocketchat_test::Test;

#[test]
fn root_url_returns_a_welcome_message() {
    let test = Test::new().run();
    let url = test.config.as_url.clone();
    let params = HashMap::new();

    let (body, status) = RestApi::call(&Method::GET, &url, RequestData::Body("".to_string()), &params, None).unwrap();
    assert_eq!(body, "Your Rocket.Chat <-> Matrix application service is running");
    assert_eq!(status, StatusCode::OK);
}
