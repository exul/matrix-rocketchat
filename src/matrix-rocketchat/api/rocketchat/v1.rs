use std::collections::HashMap;
use std::io::Read;

use reqwest::header::{ContentType, Headers};
use reqwest::{Method, StatusCode};
use serde_json;
use slog::Logger;

use api::RestApi;
use api::rocketchat::{Attachment as RocketchatAttachment, Channel, Endpoint, User};
use errors::*;
use i18n::*;

/// Login endpoint path
pub const LOGIN_PATH: &str = "/api/v1/login";
/// Me endpoint path
pub const ME_PATH: &str = "/api/v1/me";
/// Users list endpoint path
pub const USERS_INFO_PATH: &str = "/api/v1/users.info";
/// Channels list endpoint path
pub const CHANNELS_LIST_PATH: &str = "/api/v1/channels.list";
/// Direct messages list endpoint path
pub const DIRECT_MESSAGES_LIST_PATH: &str = "/api/v1/dm.list";
/// Get a chat message endpoint path
pub const GET_CHAT_MESSAGE_PATH: &str = "/api/v1/chat.getMessage";
/// Post chat message endpoint path
pub const POST_CHAT_MESSAGE_PATH: &str = "/api/v1/chat.postMessage";

/// A single Message on the Rocket.Chat server.
#[derive(Deserialize, Debug, Serialize)]
pub struct Message {
    /// The unique message identifier
    #[serde(rename = "_id")]
    pub id: String,
    /// The ID of the room the message was sent in.
    pub rid: String,
    /// The text content of the message
    pub msg: String,
    /// The timestamp when the message was sent
    pub ts: String,
    /// A list of attachments that are associated with the message
    pub attachments: Option<Vec<Attachment>>,
    /// Information about the user who sent the message
    pub u: UserInfo,
    /// A list of mentions
    pub mentions: Vec<serde_json::Value>,
    /// A list of channels
    pub channels: Vec<serde_json::Value>,
    /// The timestamp when the message was updated the last time
    #[serde(rename = "_updatedAt")]
    pub updated_at: String,
}

/// Metadata for a file that is uploaded to Rocket.Chat
#[derive(Deserialize, Debug, Serialize)]
pub struct Attachment {
    /// An optional title for the file
    pub title: String,
    /// An optinal description of the file
    pub description: String,
    /// URL to download the image, it's only present when the attachment is an image
    pub image_url: Option<String>,
    /// The type of the uploaded file, it's only present when the attachment is an image
    pub image_type: Option<String>,
    /// The size of the uploaded image in bytes, it's only present when the attachment is an image
    pub image_size: Option<i64>,
}

#[derive(Deserialize, Debug, Serialize)]
/// Further information about a user.
pub struct UserInfo {
    /// The users unique identifier
    #[serde(rename = "_id")]
    pub id: String,
    /// The users username
    pub username: String,
    /// The users name
    pub name: String,
}

impl Attachment {
    /// The content the of the attachment
    pub fn content_type(&self) -> Result<ContentType> {
        match self.image_type {
            Some(ref content_type) if content_type == "image/jpeg" => Ok(ContentType::jpeg()),
            Some(ref content_type) if content_type == "image/png" => Ok(ContentType::png()),
            _ => bail_error!(ErrorKind::UnknownContentType(self.image_type.clone().unwrap_or_default())),
        }
    }
}

/// V1 get endpoints that require authentication
pub struct GetWithAuthEndpoint<'a> {
    base_url: String,
    path: &'a str,
    user_id: String,
    auth_token: String,
    query_params: HashMap<&'static str, &'a str>,
}

impl<'a> Endpoint for GetWithAuthEndpoint<'a> {
    fn method(&self) -> Method {
        Method::Get
    }

    fn url(&self) -> String {
        self.base_url.clone() + self.path
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

    fn query_params(&self) -> HashMap<&'static str, &str> {
        self.query_params.clone()
    }
}

/// Get a file that was uploaded to Rocket.Chat and needs authentication to be downloaded
pub struct GetFileWithAuthEndpoint<'a> {
    base_url: String,
    path: &'a str,
    user_id: String,
    auth_token: String,
    query_params: HashMap<&'static str, &'a str>,
}

impl<'a> Endpoint for GetFileWithAuthEndpoint<'a> {
    fn method(&self) -> Method {
        Method::Get
    }

    fn url(&self) -> String {
        self.base_url.clone() + self.path
    }

    fn payload(&self) -> Result<String> {
        Ok("".to_string())
    }

    fn headers(&self) -> Option<Headers> {
        let mut headers = Headers::new();
        let cookie = "rc_uid=".to_string() + &self.user_id + "; rc_token=" + &self.auth_token;
        headers.set_raw("Cookie", vec![cookie.into_bytes()]);
        Some(headers)
    }

    fn query_params(&self) -> HashMap<&'static str, &str> {
        self.query_params.clone()
    }
}

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
        let payload = serde_json::to_string(&self.payload)
            .chain_err(|| ErrorKind::InvalidJSON("Could not serialize login payload".to_string()))?;
        Ok(payload)
    }

    fn headers(&self) -> Option<Headers> {
        None
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
    #[serde(rename = "roomId")] room_id: &'a str,
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
        let payload = serde_json::to_string(&self.payload)
            .chain_err(|| ErrorKind::InvalidJSON("Could not serialize post chat message payload".to_string()))?;
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

/// Response payload from the Rocket.Chat channels.list endpoint.
#[derive(Deserialize)]
pub struct ChannelsListResponse {
    /// A list of channels on the Rocket.Chat server
    pub channels: Vec<Channel>,
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

/// Response payload from the Rocket.Chat im.list endpoint.
#[derive(Deserialize)]
pub struct DirectMessagesListResponse {
    /// A list of direct messages that the user is part of.
    pub ims: Vec<Channel>,
}

#[derive(Deserialize)]
/// Response payload from the Rocket.Chat login endpoint.
pub struct LoginResponse {
    /// Status of the response (success, error)
    pub status: String,
    /// Data of the response
    pub data: Credentials,
}

/// Response payload from the Rocket.Chat me endpoint.
#[derive(Deserialize)]
pub struct MeResponse {
    /// The users username on the Rocket.Chat server
    pub username: String,
}

/// Response payload from the Rocket.Chat users.info endpoint.
#[derive(Deserialize)]
pub struct UsersInfoResponse {
    /// A user on the Rocket.Chat server
    pub user: User,
}

/// Response payload from the Rocket.Chat chat.message endpoint.
#[derive(Deserialize)]
pub struct MessageResponse {
    /// The chat messsage for the requested ID
    pub message: Message,
}

#[derive(Clone)]
/// Rocket.Chat REST API v1
pub struct RocketchatApi {
    /// URL to call the API
    pub base_url: String,
    /// Logger passed to the Rocketchat API
    logger: Logger,
    /// The user id that is passed to the auth header
    user_id: String,
    /// The auth token that is passed to the auth header
    auth_token: String,
}

impl RocketchatApi {
    /// Create a new `RocketchatApi`.
    pub fn new(base_url: String, logger: Logger) -> RocketchatApi {
        RocketchatApi {
            base_url: base_url,
            logger: logger,
            user_id: "".to_string(),
            auth_token: "".to_string(),
        }
    }
}

impl super::RocketchatApi for RocketchatApi {
    fn channels_list(&self) -> Result<Vec<Channel>> {
        debug!(self.logger, "Getting channel list from Rocket.Chat server {}", &self.base_url);

        let channels_list_endpoint = GetWithAuthEndpoint {
            base_url: self.base_url.clone(),
            user_id: self.user_id.clone(),
            auth_token: self.auth_token.clone(),
            path: CHANNELS_LIST_PATH,
            query_params: HashMap::new(),
        };

        let (body, status_code) = RestApi::call_rocketchat(&channels_list_endpoint)?;
        if !status_code.is_success() {
            return Err(build_error(&channels_list_endpoint.url(), &body, &status_code));
        }

        let channels_list_response: ChannelsListResponse = serde_json::from_str(&body).chain_err(|| {
            ErrorKind::InvalidJSON(format!(
                "Could not deserialize response from Rocket.Chat channels.list API \
                 endpoint: `{}`",
                body
            ))
        })?;

        Ok(channels_list_response.channels)
    }

    fn current_username(&self) -> Result<String> {
        debug!(self.logger, "Querying username for user_id {} on Rocket.Chat server {}", self.user_id, &self.base_url);

        let me_endpoint = GetWithAuthEndpoint {
            base_url: self.base_url.clone(),
            user_id: self.user_id.clone(),
            auth_token: self.auth_token.clone(),
            path: ME_PATH,
            query_params: HashMap::new(),
        };

        let (body, status_code) = RestApi::call_rocketchat(&me_endpoint)?;
        if !status_code.is_success() {
            return Err(build_error(&me_endpoint.url(), &body, &status_code));
        }

        let me_response: MeResponse = serde_json::from_str(&body).chain_err(|| {
            ErrorKind::InvalidJSON(format!("Could not deserialize response from Rocket.Chat me API endpoint: `{}`", body))
        })?;

        Ok(me_response.username)
    }

    fn direct_messages_list(&self) -> Result<Vec<Channel>> {
        debug!(self.logger, "Getting direct messages list from Rocket.Chat server {}", &self.base_url);

        let direct_messages_list_endpoint = GetWithAuthEndpoint {
            base_url: self.base_url.clone(),
            user_id: self.user_id.clone(),
            auth_token: self.auth_token.clone(),
            path: DIRECT_MESSAGES_LIST_PATH,
            query_params: HashMap::new(),
        };

        let (body, status_code) = RestApi::call_rocketchat(&direct_messages_list_endpoint)?;
        if !status_code.is_success() {
            return Err(build_error(&direct_messages_list_endpoint.url(), &body, &status_code));
        }

        let direct_messages_list_response: DirectMessagesListResponse = serde_json::from_str(&body).chain_err(|| {
            ErrorKind::InvalidJSON(format!(
                "Could not deserialize response from Rocket.Chat dm.list API \
                 endpoint: `{}`",
                body
            ))
        })?;

        Ok(direct_messages_list_response.ims)
    }

    fn get_attachments(&self, message_id: &str) -> Result<Vec<RocketchatAttachment>> {
        debug!(self.logger, "Retreiving image URL for message {}", message_id);

        let mut query_params = HashMap::new();
        query_params.insert("msgId", message_id);

        let message_endpoint = GetWithAuthEndpoint {
            base_url: self.base_url.clone(),
            user_id: self.user_id.clone(),
            auth_token: self.auth_token.clone(),
            path: GET_CHAT_MESSAGE_PATH,
            query_params: query_params,
        };

        let (body, status_code) = RestApi::call_rocketchat(&message_endpoint)?;
        if !status_code.is_success() {
            return Err(build_error(&message_endpoint.url(), &body, &status_code));
        }

        let message_response: MessageResponse = serde_json::from_str(&body).chain_err(|| {
            ErrorKind::InvalidJSON(format!(
                "Could not deserialize response from Rocket.Chat chat message API endpoint: `{}`",
                body
            ))
        })?;

        let mut files = Vec::new();

        if let Some(attachments) = message_response.message.attachments {
            for attachment in attachments {
                if let Some(ref image_url) = attachment.image_url {
                    debug!(self.logger, "Getting file {}", image_url);

                    let mut get_file_endpoint = GetFileWithAuthEndpoint {
                        base_url: self.base_url.clone(),
                        user_id: self.user_id.clone(),
                        auth_token: self.auth_token.clone(),
                        path: image_url,
                        query_params: HashMap::new(),
                    };

                    let mut resp = RestApi::get_rocketchat_file(&get_file_endpoint)?;

                    if !resp.status().is_success() {
                        let mut body = String::new();
                        resp.read_to_string(&mut body).chain_err(|| ErrorKind::ApiCallFailed(image_url.to_owned()))?;
                        return Err(build_error(&get_file_endpoint.url(), &body, &resp.status()));
                    }

                    let mut buffer = Vec::new();
                    resp.read_to_end(&mut buffer).chain_err(|| ErrorKind::InternalServerError)?;
                    let content_type = attachment.content_type()?;
                    let rocketchat_attachment = RocketchatAttachment {
                        content_type: content_type,
                        data: buffer,
                        title: attachment.title,
                    };
                    files.push(rocketchat_attachment);
                }
            }
        }else{
            info!(self.logger, "No attachments found for message ID {}", message_id);
        }

        Ok(files)
    }

    fn login(&self, username: &str, password: &str) -> Result<(String, String)> {
        debug!(self.logger, "Logging in user with username {} on Rocket.Chat server {}", username, &self.base_url);

        let login_endpoint = LoginEndpoint {
            base_url: self.base_url.clone(),
            payload: LoginPayload {
                username: username,
                password: password,
            },
        };

        let (body, status_code) = RestApi::call_rocketchat(&login_endpoint)?;
        if !status_code.is_success() {
            return Err(build_error(&login_endpoint.url(), &body, &status_code));
        }

        let login_response: LoginResponse = serde_json::from_str(&body).chain_err(|| {
            ErrorKind::InvalidJSON(format!("Could not deserialize response from Rocket.Chat login API endpoint: `{}`", body))
        })?;
        Ok((login_response.data.user_id, login_response.data.auth_token))
    }

    fn post_chat_message(&self, text: &str, room_id: &str) -> Result<()> {
        debug!(self.logger, "Forwarding message to to Rocket.Chat room {}", room_id);

        let post_chat_message_endpoint = PostChatMessageEndpoint {
            base_url: self.base_url.clone(),
            user_id: self.user_id.clone(),
            auth_token: self.auth_token.clone(),
            payload: PostChatMessagePayload {
                text: Some(text),
                room_id: room_id,
            },
        };

        let (body, status_code) = RestApi::call_rocketchat(&post_chat_message_endpoint)?;
        if !status_code.is_success() {
            return Err(build_error(&post_chat_message_endpoint.url(), &body, &status_code));
        }

        Ok(())
    }

    fn users_info(&self, username: &str) -> Result<User> {
        debug!(self.logger, "Querying user info for user {} on Rocket.Chat server {}", &username, &self.base_url);

        let mut query_params = HashMap::new();
        query_params.insert("username", username);
        let users_info_endpoint = GetWithAuthEndpoint {
            base_url: self.base_url.clone(),
            user_id: self.user_id.clone(),
            auth_token: self.auth_token.clone(),
            path: USERS_INFO_PATH,
            query_params: query_params,
        };

        let (body, status_code) = RestApi::call_rocketchat(&users_info_endpoint)?;
        if !status_code.is_success() {
            return Err(build_error(&users_info_endpoint.url(), &body, &status_code));
        }

        let users_info_response: UsersInfoResponse = serde_json::from_str(&body).chain_err(|| {
            ErrorKind::InvalidJSON(format!(
                "Could not deserialize response from Rocket.Chat users.info API endpoint: `{}`",
                body
            ))
        })?;

        Ok(users_info_response.user)
    }

    fn with_credentials(mut self: Box<Self>, user_id: String, auth_token: String) -> Box<super::RocketchatApi> {
        self.user_id = user_id;
        self.auth_token = auth_token;
        self
    }
}

fn build_error(endpoint: &str, body: &str, status_code: &StatusCode) -> Error {
    let json_error_msg = format!(
        "Could not deserialize error from Rocket.Chat API endpoint {} with status code {}: `{}`",
        endpoint, status_code, body
    );
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

    let message = rocketchat_error_resp.message.clone();
    let error_msg = message.unwrap_or_else(|| rocketchat_error_resp.error.clone().unwrap_or_else(|| body.to_string()));
    Error::from(ErrorKind::RocketchatError(error_msg))
}
