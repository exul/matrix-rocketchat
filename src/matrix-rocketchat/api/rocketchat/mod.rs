use std::collections::HashMap;

use iron::typemap::Key;
use reqwest::header::Headers;
use reqwest::Method;
use serde_json;
use slog::Logger;

use api::RestApi;
use errors::*;
use i18n::*;

/// Rocket.Chat REST API v1
pub mod v1;

/// A Rocket.Chat REST API endpoint.
pub trait Endpoint {
    /// HTTP Method
    fn method(&self) -> Method;
    /// The URL of the endpoint
    fn url(&self) -> String;
    /// Payload that is sent to the server
    fn payload(&self) -> Result<String>;
    /// Headers that are sent to the server
    fn headers(&self) -> Option<Headers>;
}

//TODO: Move this into v1, because those structs are depending on the api version as well
/// A Rocket.Chat channel
#[derive(Deserialize, Debug)]
pub struct Channel {
    /// ID of the Rocket.Chat room
    #[serde(rename = "_id")]
    pub id: String,
    /// Name of the Rocket.Chat room
    pub name: String,
    /// List of users in the room
    pub usernames: Vec<String>,
}

/// A Rocket.Chat message
#[derive(Deserialize, Debug, Serialize)]
pub struct Message {
    /// ID of the message
    pub message_id: String,
    /// Rocket.Chat token
    pub token: Option<String>,
    /// ID of the channel from which the message was sent
    pub channel_id: String,
    /// Name of the channel from which the message was sent
    pub channel_name: String,
    /// ID of the user who sent the message
    pub user_id: String,
    /// Name of the user who sent the message
    pub user_name: String,
    /// Message content
    pub text: String,
}

/// Rocket.Chat REST API
pub trait RocketchatApi {
    /// Login a user on the Rocket.Chat server
    fn login(&self, username: &str, password: &str) -> Result<(String, String)>;
    /// Get the logged in users username
    fn username(&self, user_id: String, auth_token: String) -> Result<String>;
    /// List of channels on the Rocket.Chat server
    fn channels_list(&self, user_id: String, auth_token: String) -> Result<Vec<Channel>>;
    /// Post a chat message
    fn post_chat_message(&self, user_id: String, auth_token: String, text: &str, room_id: &str) -> Result<()>;
}

/// Response format when querying the Rocket.Chat info endpoint
#[derive(Deserialize, Serialize)]
pub struct GetInfoResponse {
    version: String,
}

impl RocketchatApi {
    /// Creates a new Rocket.Chat API depending on the version of the API.
    /// It returns a `RocketchatApi` trait, because for each version a different API is created.
    pub fn new(base_url: String, access_token: Option<String>, logger: Logger) -> Result<Box<RocketchatApi>> {
        let url = base_url.clone() + "/api/info";
        let params = HashMap::new();

        debug!(logger, format!("Querying Rocket.Chat server {} for API versions", url));

        let (body, status_code) = match RestApi::call(Method::Get, &url, "", &params, None) {
            Ok((body, status_code)) => (body, status_code),
            Err(err) => {
                debug!(logger, err);
                bail_error!(ErrorKind::RocketchatServerUnreachable(url.clone()),
                            t!(["errors", "rocketchat_server_unreachable"]).with_vars(vec![("rocketchat_url", url)]));
            }
        };

        if !status_code.is_success() {
            bail_error!(ErrorKind::NoRocketchatServer(url.clone()),
                        t!(["errors", "no_rocketchat_server"]).with_vars(vec![("rocketchat_url", url.clone())]));
        }

        let rocketchat_info: GetInfoResponse =
            match serde_json::from_str(&body).chain_err(|| ErrorKind::NoRocketchatServer(url.clone())) {
                Ok(rocketchat_info) => rocketchat_info,
                Err(err) => {
                    bail_error!(err,
                                t!(["errors", "no_rocketchat_server"]).with_vars(vec![("rocketchat_url", url)]));
                }
            };

        debug!(logger, format!("Rocket.Chat version {:?}", rocketchat_info.version));

        RocketchatApi::get_max_supported_version_api(rocketchat_info.version, base_url, access_token, logger)
    }

    fn get_max_supported_version_api(version: String,
                                     base_url: String,
                                     access_token: Option<String>,
                                     logger: Logger)
                                     -> Result<Box<RocketchatApi>> {
        let version_string = version.clone();
        let mut versions = version_string.split('.').into_iter();
        let major: i32 = versions.next()
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
        let minor: i32 = versions.next()
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);

        if major == 0 && minor >= 49 {
            let rocketchat_api = v1::RocketchatApi::new(base_url, access_token, logger);
            return Ok(Box::new(rocketchat_api));
        }

        let min_version = "0.49".to_string();
        Err(Error {
                error_chain: ErrorKind::UnsupportedRocketchatApiVersion(min_version.clone(), version.clone()).into(),
                user_message: Some(t!(["errors", "unsupported_rocketchat_api_version"]).with_vars(vec![("min_version",
                                                                                                        min_version),
                                                                                                       ("version",
                                                                                                        version)])),
            })
    }
}

impl Key for Message {
    type Value = Message;
}
