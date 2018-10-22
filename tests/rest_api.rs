extern crate http;
extern crate matrix_rocketchat;
#[macro_use]
extern crate matrix_rocketchat_test;
extern crate reqwest;

use std::collections::HashMap;

use http::Method;
use matrix_rocketchat::api::{RequestData, RestApi};
use matrix_rocketchat::errors::*;

#[test]
fn rest_api_returns_an_error_when_called_with_an_unkown_http_method() {
    let params = HashMap::new();
    let api_result = RestApi::call(&Method::HEAD, "http://localhost", RequestData::Body("".to_string()), &params, None);
    let err = api_result.unwrap_err();
    let _method = Method::HEAD.to_string();
    assert_error_kind!(err, ErrorKind::UnsupportedHttpMethod(ref _method));
}
