extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate reqwest;

use std::collections::HashMap;

use matrix_rocketchat::api::RestApi;
use matrix_rocketchat_test::{HS_TOKEN, Test};
use reqwest::{Method, StatusCode};

#[test]
fn homeserver_sends_mal_formatted_json() {
    let test = Test::new().run();
    let payload = "bad_json";

    let url = format!("{}/transactions/{}", &test.config.as_url, "specid");
    let mut params = HashMap::new();
    params.insert("access_token", HS_TOKEN);
    let (_, status_code) = RestApi::call(Method::Put, &url, payload, &mut params, None).unwrap();
    assert_eq!(status_code, StatusCode::UnprocessableEntity)
}
