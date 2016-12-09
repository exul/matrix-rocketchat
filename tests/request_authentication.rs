extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate reqwest;

use std::collections::HashMap;

use matrix_rocketchat_test::{HS_TOKEN, Test, call_url};
use reqwest::StatusCode;

#[test]
fn returns_unauthorized_when_token_is_missing() {
    let test = Test::new().run();
    let url = test.config.as_url.clone() + "/transactions/txn_id";
    let params = HashMap::new();

    let (_, status) = call_url("PUT", &url, "{}", &params);
    assert_eq!(status, StatusCode::Unauthorized);
}

#[test]
fn returns_forbidden_when_token_is_wrong() {
    let test = Test::new().run();
    let url = test.config.as_url.clone() + "/transactions/txn_id";
    let mut params = HashMap::new();
    params.insert("access_token", "wrong_token");

    let (_, status) = call_url("PUT", &url, "{}", &params);
    assert_eq!(status, StatusCode::Forbidden);
}

#[test]
fn returns_ok_when_token_is_correct() {
    let test = Test::new().run();
    let url = test.config.as_url.clone() + "/transactions/txn_id";
    let mut params = HashMap::new();
    params.insert("access_token", HS_TOKEN);

    let (_, status) = call_url("PUT", &url, "{}", &params);

    assert_eq!(status, StatusCode::Ok);
}
