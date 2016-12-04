use std::io::Read;
use std::str;

use reqwest::{Client, Method, StatusCode};

pub fn call_url(method: &str, url: &str, payload: &str) -> (String, StatusCode) {
    let client = Client::new().expect("Error when creating HTTP Client");

    let req = match method {
        "GET" => client.get(url),
        "PUT" => client.request(Method::Put, url).body(payload),
        _ => {
            return ("".to_string(), StatusCode::ImATeapot);
        }
    };

    let mut resp = req.send().expect("Error when calling URL");
    let mut body = String::new();

    resp.read_to_string(&mut body).expect("Error when reading response");

    return (body, *resp.status());
}
