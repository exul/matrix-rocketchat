use std::collections::HashMap;

use iron::typemap::Key;
use reqwest::header::{ContentType, Headers};
use reqwest::mime::Mime;
use reqwest::{Body, Method};
use serde_json;
use slog::Logger;

use api::{RequestData, RestApi};
use errors::*;
use i18n::*;

/// Rocket.Chat REST API v1
pub mod v1;

const MAX_REQUESTS_PER_ENDPOINT_CALL: i32 = 1000;
const MIN_MAJOR_VERSION: i32 = 0;
const MIN_MINOR_VERSION: i32 = 60;

/// A Rocket.Chat REST API endpoint.
pub trait Endpoint<T: Into<Body>> {
    /// HTTP Method
    fn method(&self) -> Method;
    /// The URL of the endpoint
    fn url(&self) -> String;
    /// Payload that is sent to the server
    fn payload(&self) -> Result<RequestData<T>>;
    /// Headers that are sent to the server
    fn headers(&self) -> Option<Headers>;
    /// The query parameters that are used when sending the request
    fn query_params(&self) -> HashMap<&'static str, &str> {
        HashMap::new()
    }
}

/// A file that was uploaded to Rocket.Chat
pub struct Attachment {
    /// The content type according to RFC7231
    pub content_type: ContentType,
    /// The file
    pub data: Vec<u8>,
    /// A title that describes the file
    pub title: String,
}

/// A Rocket.Chat channel
#[derive(Deserialize, Debug, Serialize, Clone)]
pub struct Channel {
    /// ID of the Rocket.Chat room
    #[serde(rename = "_id")]
    pub id: String,
    /// Name of the Rocket.Chat room
    pub name: Option<String>,
}

/// A Rocket.Chat message
#[derive(Clone, Debug)]
pub struct Message {
    /// The unique message identifier
    pub id: String,
    /// The text content of the message
    pub msg: String,
    /// Optional file, only present when a file is attached to the message
    pub file: Option<File>,
    /// A list of attachments that are associated with the message
    pub attachments: Option<Vec<MessageAttachment>>,
}

/// A file attached to a message
#[derive(Clone, Deserialize, Debug)]
pub struct File {
    /// The file's MIME type
    pub mimetype: String,
}

/// Metadata for a file that is uploaded to Rocket.Chat
#[derive(Clone, Debug)]
pub struct MessageAttachment {
    /// The content type according to RFC7231
    pub content_type: ContentType,
    /// URL to download the image, it's only present when the attachment is an image
    pub image_url: Option<String>,
    /// An optional title for the file
    pub title: String,
    /// Link to file
    pub title_link: String,
}

/// A Rocket.Chat user
#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct User {
    /// ID of the Rocket.Chat user
    #[serde(rename = "_id")]
    pub id: String,
    /// Name that is displayed in Rocket.Chat
    pub username: String,
}

/// A Rocket.Chat message
#[derive(Deserialize, Debug, Serialize)]
pub struct WebhookMessage {
    /// ID of the message
    pub message_id: String,
    /// Rocket.Chat token
    pub token: Option<String>,
    /// ID of the channel from which the message was sent
    pub channel_id: String,
    /// Name of the channel from which the message was sent
    pub channel_name: Option<String>,
    /// ID of the user who sent the message
    pub user_id: String,
    /// Name of the user who sent the message
    pub user_name: String,
    /// Message content
    pub text: String,
}

/// Rocket.Chat REST API
pub trait RocketchatApi {
    /// Get the url of an image that is attached to a message.
    fn attachments(&self, message_id: &str) -> Result<Vec<Attachment>>;
    /// Get all members of a channel
    fn channels_members(&self, room_id: &str) -> Result<Vec<User>>;
    /// List of channels on the Rocket.Chat server
    fn channels_list(&self) -> Result<Vec<Channel>>;
    /// Get all the channels that the user of the request has joiend.
    fn channels_list_joined(&self) -> Result<Vec<Channel>>;
    /// Get a chat message
    fn chat_get_message(&self, message_id: &str) -> Result<Message>;
    /// Post a chat message
    fn chat_post_message(&self, text: &str, room_id: &str) -> Result<()>;
    /// List of direct messages the user is part of
    fn dm_list(&self) -> Result<Vec<Channel>>;
    /// List of al private groups the authenticated user has joined on the Rocket.Chat server
    fn groups_list(&self) -> Result<Vec<Channel>>;
    /// Get all members of a group
    fn groups_members(&self, room_id: &str) -> Result<Vec<User>>;
    /// Login a user on the Rocket.Chat server
    fn login(&self, username: &str, password: &str) -> Result<(String, String)>;
    /// Get current user information
    fn me(&self) -> Result<User>;
    /// Post a message with an attachment
    fn rooms_upload(&self, file: Vec<u8>, filename: &str, mimetype: Mime, room_id: &str) -> Result<()>;
    /// Get information like user_id, status, etc. about a user
    fn users_info(&self, username: &str) -> Result<User>;
    /// Set credentials that are used for all API calls that need authentication
    fn with_credentials(self: Box<Self>, user_id: String, auth_token: String) -> Box<RocketchatApi>;
}

/// Response format when querying the Rocket.Chat info endpoint
#[derive(Deserialize, Serialize)]
pub struct GetInfoResponse {
    version: String,
}

impl RocketchatApi {
    /// Creates a new Rocket.Chat API depending on the version of the API.
    /// It returns a `RocketchatApi` trait, because for each version a different API is created.
    pub fn new(base_url: String, logger: Logger) -> Result<Box<RocketchatApi>> {
        let url = base_url.clone() + "/api/info";
        let params = HashMap::new();

        let (body, status_code) = match RestApi::call(&Method::Get, &url, RequestData::Body(""), &params, None) {
            Ok((body, status_code)) => (body, status_code),
            Err(err) => {
                debug!(logger, "{}", err);
                bail_error!(
                    ErrorKind::RocketchatServerUnreachable(url.clone()),
                    t!(["errors", "rocketchat_server_unreachable"]).with_vars(vec![("rocketchat_url", url)])
                );
            }
        };

        if !status_code.is_success() {
            bail_error!(
                ErrorKind::NoRocketchatServer(url.clone()),
                t!(["errors", "no_rocketchat_server"]).with_vars(vec![("rocketchat_url", url.clone())])
            );
        }

        let rocketchat_info: GetInfoResponse =
            match serde_json::from_str(&body).chain_err(|| ErrorKind::NoRocketchatServer(url.clone())) {
                Ok(rocketchat_info) => rocketchat_info,
                Err(err) => {
                    bail_error!(err, t!(["errors", "no_rocketchat_server"]).with_vars(vec![("rocketchat_url", url)]));
                }
            };

        RocketchatApi::get_max_supported_version_api(rocketchat_info.version, base_url, logger)
    }

    fn get_max_supported_version_api(version: String, base_url: String, logger: Logger) -> Result<Box<RocketchatApi>> {
        let version_string = version.clone();
        let mut versions = version_string.split('.').into_iter();
        let major: i32 = versions.next().unwrap_or("0").parse().unwrap_or(0);
        let minor: i32 = versions.next().unwrap_or("0").parse().unwrap_or(0);

        if major == MIN_MAJOR_VERSION && minor >= MIN_MINOR_VERSION {
            let rocketchat_api = v1::RocketchatApi::new(base_url, logger);
            return Ok(Box::new(rocketchat_api));
        }

        let min_version = format!("{}.{}", MIN_MAJOR_VERSION, MIN_MINOR_VERSION);
        Err(Error {
            error_chain: ErrorKind::UnsupportedRocketchatApiVersion(min_version.clone(), version.clone()).into(),
            user_message: Some(
                t!(["errors", "unsupported_rocketchat_api_version"])
                    .with_vars(vec![("min_version", min_version), ("version", version)]),
            ),
        })
    }
}

impl Key for WebhookMessage {
    type Value = WebhookMessage;
}
