use std::collections::HashMap;
use std::io::Read;

use url;
use reqwest::{Client, Method, StatusCode, Url};
use reqwest::header::Headers;
use ruma_client_api::Method as RumaHttpMethod;

use errors::*;
use api::rocketchat::Endpoint;

/// REST API
pub struct RestApi {}

impl RestApi {
    /// Call a matrix REST API endpoint
    pub fn call_matrix<'a>(
        method: RumaHttpMethod,
        url: &str,
        payload: &str,
        params: &HashMap<&str, &'a str>,
    ) -> Result<(String, StatusCode)> {
        let method = match method {
            RumaHttpMethod::Delete => Method::Delete,
            RumaHttpMethod::Get => Method::Get,
            RumaHttpMethod::Post => Method::Post,
            RumaHttpMethod::Put => Method::Put,
        };

        RestApi::call(method, url, payload, params, None)
    }

    /// Call a Rocket.Chat API endpoint
    pub fn call_rocketchat(endpoint: &Endpoint) -> Result<(String, StatusCode)> {
        RestApi::call(endpoint.method(), &endpoint.url(), &endpoint.payload()?, &endpoint.query_params(), endpoint.headers())
    }

    /// Call a REST API endpoint
    pub fn call<'a>(
        method: Method,
        url: &str,
        payload: &str,
        params: &HashMap<&str, &'a str>,
        headers: Option<Headers>,
    ) -> Result<(String, StatusCode)> {
        let client = Client::new();
        let encoded_url = RestApi::encode_url(url.to_string(), params)?;

        let mut req = match method {
            Method::Get => client.get(&encoded_url),
            Method::Put => client.put(&encoded_url),
            Method::Post => client.post(&encoded_url),
            Method::Delete => client.delete(&encoded_url),
            _ => {
                return Err(Error::from(ErrorKind::UnsupportedHttpMethod(method.to_string())));
            }
        };

        if let Some(headers) = headers {
            req.headers(headers);
        }

        req.body(payload.to_owned());

        let mut resp = req.send().chain_err(|| ErrorKind::ApiCallFailed(url.to_owned()))?;
        let mut body = String::new();

        resp.read_to_string(&mut body).chain_err(|| ErrorKind::ApiCallFailed(url.to_owned()))?;

        Ok((body, resp.status()))
    }

    fn encode_url(base: String, parameters: &HashMap<&str, &str>) -> Result<String> {
        let query_string = parameters.iter().fold("?".to_string(), |init, (k, v)| {
            [
                init,
                [
                    url::form_urlencoded::byte_serialize(k.as_bytes()).collect::<String>(),
                    url::form_urlencoded::byte_serialize(v.as_bytes()).collect::<String>(),
                ].join("="),
            ].join("&")
        });
        let url_string = [base, query_string].join("");
        let encoded_url = Url::parse(&url_string).chain_err(|| ErrorKind::ApiCallFailed(url_string))?;
        Ok(encoded_url.to_string())
    }
}
