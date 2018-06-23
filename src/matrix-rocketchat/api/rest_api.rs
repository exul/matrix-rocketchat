use std::collections::HashMap;
use std::io::Read;

use http::{HeaderMap, Method, StatusCode};
use reqwest::multipart::Form;
use reqwest::{Body, Client, Response, Url};
use ruma_client_api::Method as RumaHttpMethod;
use url;

use api::rocketchat::Endpoint;
use errors::*;

/// Request data types.
pub enum RequestData<T: Into<Body>> {
    /// Any type that can be converted into a body.
    Body(T),
    /// A multipart form
    MultipartForm(Form),
}

/// REST API
pub struct RestApi {}

impl RestApi {
    /// Call a matrix REST API endpoint
    pub fn call_matrix<'a, T: Into<Body>>(
        method: &RumaHttpMethod,
        url: &str,
        payload: T,
        params: &HashMap<&str, &'a str>,
    ) -> Result<(String, StatusCode)> {
        let method = match *method {
            RumaHttpMethod::Delete => Method::DELETE,
            RumaHttpMethod::Get => Method::GET,
            RumaHttpMethod::Post => Method::POST,
            RumaHttpMethod::Put => Method::PUT,
        };

        let data = RequestData::Body(payload.into());
        RestApi::call(&method, url, data, params, None)
    }

    /// Get a file that was uploaded to a Matrix homeserver
    pub fn get_matrix_file<'a, T: Into<Body>>(
        method: &RumaHttpMethod,
        url: &str,
        payload: T,
        params: &HashMap<&str, &'a str>,
    ) -> Result<Response> {
        let method = match *method {
            RumaHttpMethod::Delete => Method::DELETE,
            RumaHttpMethod::Get => Method::GET,
            RumaHttpMethod::Post => Method::POST,
            RumaHttpMethod::Put => Method::PUT,
        };

        let data = RequestData::Body(payload.into());
        RestApi::call_raw(&method, url, data, params, None)
    }

    /// Call a Rocket.Chat API endpoint
    pub fn call_rocketchat<T: Into<Body>>(endpoint: &dyn Endpoint<T>) -> Result<(String, StatusCode)> {
        RestApi::call(&endpoint.method(), &endpoint.url(), endpoint.payload()?, &endpoint.query_params(), endpoint.headers()?)
    }

    /// Get a file that was uploaded to Rocket.Chat
    pub fn get_rocketchat_file<T: Into<Body>>(endpoint: &dyn Endpoint<T>) -> Result<Response> {
        RestApi::call_raw(
            &endpoint.method(),
            &endpoint.url(),
            endpoint.payload()?,
            &endpoint.query_params(),
            endpoint.headers()?,
        )
    }

    /// Call a REST API endpoint
    pub fn call<'a, T: Into<Body>>(
        method: &Method,
        url: &str,
        payload: RequestData<T>,
        params: &HashMap<&str, &'a str>,
        headers: Option<HeaderMap>,
    ) -> Result<(String, StatusCode)> {
        let mut resp = RestApi::call_raw(method, url, payload, params, headers)?;
        let mut body = String::new();
        resp.read_to_string(&mut body).chain_err(|| ErrorKind::ApiCallFailed(url.to_owned()))?;

        Ok((body, resp.status()))
    }

    fn call_raw<'a, T: Into<Body>>(
        method: &Method,
        url: &str,
        data: RequestData<T>,
        params: &HashMap<&str, &'a str>,
        headers: Option<HeaderMap>,
    ) -> Result<Response> {
        let client = Client::new();
        let encoded_url = RestApi::encode_url(url.to_string(), params)?;

        let mut req = match *method {
            Method::GET => client.get(&encoded_url),
            Method::PUT => client.put(&encoded_url),
            Method::POST => client.post(&encoded_url),
            Method::DELETE => client.delete(&encoded_url),
            _ => {
                return Err(Error::from(ErrorKind::UnsupportedHttpMethod(method.to_string())));
            }
        };

        if let Some(headers) = headers {
            req = req.headers(headers);
        }

        req = match data {
            RequestData::Body(body) => req.body(body),
            RequestData::MultipartForm(form) => req.multipart(form),
        };

        let resp = req.send().chain_err(|| ErrorKind::ApiCallFailed(url.to_owned()))?;

        Ok(resp)
    }

    fn encode_url(base: String, parameters: &HashMap<&str, &str>) -> Result<String> {
        let query_string = parameters.iter().fold("?".to_string(), |init, (k, v)| {
            [
                init,
                [
                    url::form_urlencoded::byte_serialize(k.as_bytes()).collect::<String>(),
                    url::form_urlencoded::byte_serialize(v.as_bytes()).collect::<String>(),
                ]
                    .join("="),
            ]
                .join("&")
        });
        let url_string = [base, query_string].join("");
        let encoded_url = Url::parse(&url_string).chain_err(|| ErrorKind::ApiCallFailed(url_string))?;
        Ok(encoded_url.to_string())
    }
}
