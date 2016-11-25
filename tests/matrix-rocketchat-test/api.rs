use std::io::Read;
use std::str;

use reqwest::{Client, StatusCode};

pub fn call_url(url: &str) -> (String, StatusCode) {
    let client = Client::new().expect("Error when creating HTTP Client");
    let mut resp = client.get(url).send().expect("Error when calling URL");
    let mut body = String::new();

    resp.read_to_string(&mut body).expect("Error when reading response");

    return (body, *resp.status());
}
