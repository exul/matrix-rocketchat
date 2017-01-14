extern crate matrix_rocketchat;
#[macro_use]
extern crate matrix_rocketchat_test;
extern crate reqwest;

use std::collections::HashMap;

use matrix_rocketchat::api::RestApi;
use matrix_rocketchat::errors::*;
use reqwest::Method;

#[test]
fn rest_api_returns_an_error_when_called_with_an_unkown_http_method() {
    let mut params = HashMap::new();
    let api_result = RestApi::call(Method::Head, "http://localhost", "", &mut params, None);
    let err = api_result.unwrap_err();
    let _method = Method::Head.to_string();
    assert_error_kind!(err, ErrorKind::UnsupportedHttpMethod(ref _method));
}
