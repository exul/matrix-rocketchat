use reqwest::header::{ContentType, Headers};
use reqwest::{Method, StatusCode};
use serde_json;
use slog::Logger;

use api::RestApi;
use errors::*;
use i18n::*;
use super::{Channel, Endpoint};

/// Login endpoint path
pub const LOGIN_PATH: &'static str = "/api/v1/login";
/// Me endpoint path
pub const ME_PATH: &'static str = "/api/v1/me";
/// Channels list endpoint path
pub const CHANNELS_LIST_PATH: &'static str = "/api/v1/channels.list";
/// Post chat message endpoint path
pub const POST_CHAT_MESSAGE_PATH: &'static str = "/api/v1/chat.postMessage";

/// V1 login endpoint
pub struct LoginEndpoint<'a> {
    base_url: String,
    payload: LoginPayload<'a>,
}

/// Payload of the login endpoint
#[derive(Serialize)]
pub struct LoginPayload<'a> {
    username: &'a str,
    password: &'a str,
}

impl<'a> Endpoint for LoginEndpoint<'a> {
    fn method(&self) -> Method {
        Method::Post
    }

    fn url(&self) -> String {
        self.base_url.clone() + LOGIN_PATH
    }

    fn payload(&self) -> Result<String> {
        let payload = serde_json::to_string(&self.payload).chain_err(|| ErrorKind::InvalidJSON("Could not serialize login payload".to_string()))?;
        Ok(payload)
    }

    fn headers(&self) -> Option<Headers> {
        None
    }
}

/// V1 me endpoint
pub struct MeEndpoint {
    base_url: String,
    user_id: String,
    auth_token: String,
}

impl Endpoint for MeEndpoint {
    fn method(&self) -> Method {
        Method::Get
    }

    fn url(&self) -> String {
        self.base_url.clone() + ME_PATH
    }

    fn payload(&self) -> Result<String> {
        Ok("".to_string())
    }

    fn headers(&self) -> Option<Headers> {
        let mut headers = Headers::new();
        headers.set(ContentType::json());
        headers.set_raw("X-User-Id", vec![self.user_id.clone().into_bytes()]);
        headers.set_raw("X-Auth-Token", vec![self.auth_token.clone().into_bytes()]);
        Some(headers)
    }
}

/// V1 channels list endpoint
pub struct ChannelsListEndpoint {
    base_url: String,
    user_id: String,
    auth_token: String,
}

impl Endpoint for ChannelsListEndpoint {
    fn method(&self) -> Method {
        Method::Get
    }

    fn url(&self) -> String {
        self.base_url.clone() + CHANNELS_LIST_PATH
    }

    fn payload(&self) -> Result<String> {
        Ok("".to_string())
    }

    fn headers(&self) -> Option<Headers> {
        let mut headers = Headers::new();
        headers.set(ContentType::json());
        headers.set_raw("X-User-Id", vec![self.user_id.clone().into_bytes()]);
        headers.set_raw("X-Auth-Token", vec![self.auth_token.clone().into_bytes()]);
        Some(headers)
    }
}

/// V1 post chat message endpoint
pub struct PostChatMessageEndpoint<'a> {
    base_url: String,
    user_id: String,
    auth_token: String,
    payload: PostChatMessagePayload<'a>,
}

/// Payload of the post chat message endpoint
#[derive(Serialize)]
pub struct PostChatMessagePayload<'a> {
    #[serde(rename = "roomId")]
    room_id: &'a str,
    text: Option<&'a str>,
}

impl<'a> Endpoint for PostChatMessageEndpoint<'a> {
    fn method(&self) -> Method {
        Method::Post
    }

    fn url(&self) -> String {
        self.base_url.clone() + POST_CHAT_MESSAGE_PATH
    }

    fn payload(&self) -> Result<String> {
        let payload = serde_json::to_string(&self.payload).chain_err(|| ErrorKind::InvalidJSON("Could not serialize post chat message payload".to_string()))?;
        Ok(payload)
    }

    fn headers(&self) -> Option<Headers> {
        let mut headers = Headers::new();
        headers.set(ContentType::json());
        headers.set_raw("X-User-Id", vec![self.user_id.clone().into_bytes()]);
        headers.set_raw("X-Auth-Token", vec![self.auth_token.clone().into_bytes()]);
        Some(headers)
    }
}

#[derive(Deserialize)]
/// Response payload from the Rocket.Chat login endpoint.
pub struct LoginResponse {
    /// Status of the response (success, error)
    pub status: String,
    /// Data of the response
    pub data: Credentials,
}

/// User credentials.
#[derive(Deserialize)]
pub struct Credentials {
    /// The authentication token for Rocket.Chat
    #[serde(rename = "authToken")]
    pub auth_token: String,
    /// The users unique id on the rocketchat server.
    #[serde(rename = "userId")]
    pub user_id: String,
}

/// Response payload from the Rocket.Chat me endpoint.
#[derive(Deserialize)]
pub struct MeResponse {
    /// The users username on the Rocket.Chat server
    pub username: String,
}

/// Response payload from the Rocket.Chat channels.list endpoint.
#[derive(Deserialize)]
pub struct ChannelsListResponse {
    /// A list of channels on the Rocket.Chat server
    pub channels: Vec<Channel>,
}

#[derive(Clone)]
/// Rocket.Chat REST API v1
pub struct RocketchatApi {
    /// URL to call the API
    pub base_url: String,
    /// Access token for authentication
    pub access_token: Option<String>,
    /// Logger passed to the Rocketchat API
    logger: Logger,
}

impl RocketchatApi {
    /// Create a new `RocketchatApi`.
    pub fn new(base_url: String, access_token: Option<String>, logger: Logger) -> RocketchatApi {
        RocketchatApi {
            base_url: base_url,
            access_token: access_token,
            logger: logger,
        }
    }
}

impl super::RocketchatApi for RocketchatApi {
    fn login(&self, username: &str, password: &str) -> Result<(String, String)> {
        debug!(self.logger, format!("Logging in user with username {} on Rocket.Chat server {}", username, &self.base_url));

        let login_endpoint = LoginEndpoint {
            base_url: self.base_url.clone(),
            payload: LoginPayload {
                username: username,
                password: password,
            },
        };

        let (body, status_code) = RestApi::call_rocketchat(&login_endpoint)?;
        if !status_code.is_success() {
            return Err(build_error(login_endpoint.url(), &body, &status_code));
        }

        let login_response: LoginResponse = serde_json::from_str(&body).chain_err(|| {
                ErrorKind::InvalidJSON(format!("Could not deserialize response from Rocket.Chat login API endpoint: `{}`",
                                               body))
            })?;
        Ok((login_response.data.user_id, login_response.data.auth_token))
    }

    fn username(&self, user_id: String, auth_token: String) -> Result<String> {
        debug!(self.logger, format!("Querying username for user_id {} on Rocket.Chat server {}", user_id, &self.base_url));

        let me_endpoint = MeEndpoint {
            base_url: self.base_url.clone(),
            user_id: user_id,
            auth_token: auth_token,
        };

        let (body, status_code) = RestApi::call_rocketchat(&me_endpoint)?;
        if !status_code.is_success() {
            return Err(build_error(me_endpoint.url(), &body, &status_code));
        }

        let me_response: MeResponse = serde_json::from_str(&body).chain_err(|| {
                ErrorKind::InvalidJSON(format!("Could not deserialize response from Rocket.Chat me API endpoint: `{}`",
                                               body))
            })?;

        Ok(me_response.username)
    }

    fn channels_list(&self, user_id: String, auth_token: String) -> Result<Vec<Channel>> {
        debug!(self.logger, format!("Getting channel list from Rocket.Chat server {}", &self.base_url));

        let channels_list_endpoint = ChannelsListEndpoint {
            base_url: self.base_url.clone(),
            user_id: user_id,
            auth_token: auth_token,
        };

        let (body, status_code) = RestApi::call_rocketchat(&channels_list_endpoint)?;
        if !status_code.is_success() {
            return Err(build_error(channels_list_endpoint.url(), &body, &status_code));
        }

        let channels_list_response: ChannelsListResponse = serde_json::from_str(&body).chain_err(|| {
                ErrorKind::InvalidJSON(format!("Could not deserialize response from Rocket.Chat channels.list API \
                                                endpoint: `{}`",
                                               body))
            })?;

        Ok(channels_list_response.channels)
    }

    fn post_chat_message(&self, user_id: String, auth_token: String, text: &str, room_id: &str) -> Result<()> {
        debug!(self.logger, format!("Forwarding message to to Rocket.Chat room {}", room_id));

        let post_chat_message_endpoint = PostChatMessageEndpoint {
            base_url: self.base_url.clone(),
            user_id: user_id,
            auth_token: auth_token,
            payload: PostChatMessagePayload {
                text: Some(text),
                room_id: room_id,
            },
        };

        let (body, status_code) = RestApi::call_rocketchat(&post_chat_message_endpoint)?;
        if !status_code.is_success() {
            return Err(build_error(post_chat_message_endpoint.url(), &body, &status_code));
        }

        Ok(())
    }
}

fn build_error(endpoint: String, body: &str, status_code: &StatusCode) -> Error {
    let json_error_msg = format!("Could not deserialize error from Rocket.Chat API endpoint {} with status code {}: `{}`",
                                 endpoint,
                                 status_code,
                                 body);
    let json_error = ErrorKind::InvalidJSON(json_error_msg);
    let rocketchat_error_resp: RocketchatErrorResponse =
        match serde_json::from_str(body).chain_err(|| json_error).map_err(Error::from) {
            Ok(rocketchat_error_resp) => rocketchat_error_resp,
            Err(err) => {
                return err;
            }
        };

    if *status_code == StatusCode::Unauthorized {
        return Error {
                   error_chain: ErrorKind::AuthenticationFailed(rocketchat_error_resp.message.unwrap_or_default()).into(),
                   user_message: Some(t!(["errors", "authentication_failed"])),
               };
    }

    let msg = rocketchat_error_resp.message.unwrap_or(rocketchat_error_resp.error.unwrap_or(body.to_string()));
    Error::from(ErrorKind::RocketchatError(msg))
}
