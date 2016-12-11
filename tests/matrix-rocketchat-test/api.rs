use std::io::Read;
use std::collections::HashMap;
use std::str;

use reqwest::{Client, Method, StatusCode, Url};
use super::HS_TOKEN;

pub fn simulate_message_from_matrix(method: &str, url: &str, payload: &str) -> (String, StatusCode) {
    let mut params = HashMap::new();
    params.insert("access_token", HS_TOKEN);
    call_url(method, url, payload, &params)
}

pub fn call_url(method: &str, url: &str, payload: &str, params: &HashMap<&str, &str>) -> (String, StatusCode) {
    let client = Client::new().expect("Error when creating HTTP Client");
    let encoded_url = encode_url(url.to_string(), params);

    let req = match method {
        "GET" => client.get(&encoded_url),
        "PUT" => client.request(Method::Put, &encoded_url).body(payload),
        _ => {
            return ("".to_string(), StatusCode::ImATeapot);
        }
    };

    let mut resp = req.send().expect("Error when calling URL");
    let mut body = String::new();

    resp.read_to_string(&mut body).expect("Error when reading response");

    return (body, *resp.status());
}

fn encode_url(base: String, parameters: &HashMap<&str, &str>) -> String {
    let query_string = parameters.iter()
        .fold("?".to_string(),
              |init, (k, v)| [init, [k.to_string(), v.to_string()].join("=")].join("&"));
    let url_string = [base, query_string].join("");
    format!("{}", Url::parse(&url_string).unwrap())
}
