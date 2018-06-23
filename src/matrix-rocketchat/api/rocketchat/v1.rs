use std::collections::HashMap;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use http::header::{HeaderValue, CONTENT_TYPE};
use http::{HeaderMap, Method, StatusCode};
use reqwest::multipart::{Form, Part};
use serde_json;
use slog::Logger;

use api::rocketchat::{
    Attachment as RocketchatAttachment, Channel, Endpoint, File as RocketchatFile, Message as RocketchatMessage,
    MessageAttachment, User,
};
use api::{RequestData, RestApi};
use errors::*;
use i18n::*;

/// Room members endpoint path
pub const CHANNELS_MEMBERS_PATH: &str = "/api/v1/channels.members";
/// Channels list endpoint path
pub const CHANNELS_LIST_PATH: &str = "/api/v1/channels.list";
/// Joined rooms endpoint path
pub const CHANNELS_LIST_JOINED_PATH: &str = "/api/v1/channels.list.joined";
/// Get a chat message endpoint path
pub const CHAT_GET_MESSAGE_PATH: &str = "/api/v1/chat.getMessage";
/// Post chat message endpoint path
pub const CHAT_POST_MESSAGE_PATH: &str = "/api/v1/chat.postMessage";
/// Direct messages list endpoint path
pub const DM_LIST_PATH: &str = "/api/v1/dm.list";
/// Group list endpoint path
pub const GROUPS_LIST_PATH: &str = "/api/v1/groups.list";
/// Group members endpoint path
pub const GROUPS_MEMBERS_PATH: &str = "/api/v1/groups.members";
/// Login endpoint path
pub const LOGIN_PATH: &str = "/api/v1/login";
/// Me endpoint path
pub const ME_PATH: &str = "/api/v1/me";
/// Users list endpoint path
pub const USERS_INFO_PATH: &str = "/api/v1/users.info";
/// Upload a file endpoint path
pub const ROOMS_UPLOAD_PATH: &str = "/api/v1/rooms.upload";

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
    /// Optional file, only present when a file is attached to the message
    pub file: Option<File>,
    /// The timestamp when the message was updated the last time
    #[serde(rename = "_updatedAt")]
    pub updated_at: String,
}

/// A file attached to a message
#[derive(Deserialize, Debug, Serialize, Clone, Default)]
pub struct File {
    /// The file's MIME type
    #[serde(rename = "type")]
    pub mimetype: String,
}

//TODO: remove
// impl Message {
//     /// The content the of the attachment
//     pub fn content_type(&self) -> Result<ContentType> {
//         let mimetype = self.file.clone().unwrap_or_default().mimetype;
//         let mime: Mime = mimetype
//             .parse()
//             .chain_err(|| ErrorKind::UnknownMimeType(mimetype))?;
//         Ok(ContentType(mime))
//     }
// }

/// Metadata for a file that is uploaded to Rocket.Chat
#[derive(Deserialize, Debug, Serialize)]
pub struct Attachment {
    /// An optinal description of the file
    pub description: String,
    /// URL to download the image, it's only present when the attachment is an image
    pub image_url: Option<String>,
    /// The type of the uploaded file, it's only present when the attachment is an image
    pub image_type: Option<String>,
    /// The size of the uploaded image in bytes, it's only present when the attachment is an image
    pub image_size: Option<i64>,
    /// The files MIME type
    #[serde(rename = "type")]
    pub mimetype: String,
    /// An optional title for the file
    pub title: String,
    /// Link to file
    pub title_link: String,
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

/// V1 get endpoints that require authentication
pub struct GetWithAuthEndpoint<'a> {
    base_url: String,
    path: &'a str,
    user_id: String,
    auth_token: String,
    query_params: HashMap<&'static str, &'a str>,
}

impl<'a> Endpoint<String> for GetWithAuthEndpoint<'a> {
    fn method(&self) -> Method {
        Method::GET
    }

    fn url(&self) -> String {
        self.base_url.clone() + self.path
    }

    fn payload(&self) -> Result<RequestData<String>> {
        Ok(RequestData::Body("".to_string()))
    }

    fn headers(&self) -> Result<Option<HeaderMap>> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_str("application/json")?);
        headers.insert("X-User-Id", HeaderValue::from_str(&self.user_id)?);
        headers.insert("X-Auth-Token", HeaderValue::from_str(&self.auth_token)?);
        Ok(Some(headers))
    }

    fn query_params(&self) -> HashMap<&'static str, &str> {
        self.query_params.clone()
    }
}

/// Get a file that was uploaded to Rocket.Chat and needs authentication to be downloaded
pub struct GetFileEndpoint<'a> {
    base_url: String,
    path: &'a str,
    user_id: String,
    auth_token: String,
    query_params: HashMap<&'static str, &'a str>,
}

impl<'a> Endpoint<String> for GetFileEndpoint<'a> {
    fn method(&self) -> Method {
        Method::GET
    }

    fn url(&self) -> String {
        self.base_url.clone() + self.path
    }

    fn payload(&self) -> Result<RequestData<String>> {
        Ok(RequestData::Body("".to_string()))
    }

    fn headers(&self) -> Result<Option<HeaderMap>> {
        let mut headers = HeaderMap::new();
        let cookie = "rc_uid=".to_string() + &self.user_id + "; rc_token=" + &self.auth_token;
        headers.insert("Cookie", HeaderValue::from_str(&cookie)?);
        Ok(Some(headers))
    }

    fn query_params(&self) -> HashMap<&'static str, &str> {
        self.query_params.clone()
    }
}

/// V1 get group members endpoint
pub struct GroupsMembersEndpoint<'a> {
    base_url: String,
    user_id: String,
    auth_token: String,
    query_params: HashMap<&'static str, &'a str>,
}

impl<'a> Endpoint<String> for GroupsMembersEndpoint<'a> {
    fn method(&self) -> Method {
        Method::GET
    }

    fn url(&self) -> String {
        self.base_url.clone() + GROUPS_MEMBERS_PATH
    }

    fn payload(&self) -> Result<RequestData<String>> {
        Ok(RequestData::Body("".to_string()))
    }

    fn headers(&self) -> Result<Option<HeaderMap>> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_str("application/json")?);
        headers.insert("X-User-Id", HeaderValue::from_str(&self.user_id)?);
        headers.insert("X-Auth-Token", HeaderValue::from_str(&self.auth_token)?);
        Ok(Some(headers))
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
#[derive(Serialize, Deserialize)]
pub struct LoginPayload<'a> {
    /// Rocket.Chat username
    pub username: &'a str,
    /// Rocket.Chat password
    pub password: &'a str,
}

impl<'a> Endpoint<String> for LoginEndpoint<'a> {
    fn method(&self) -> Method {
        Method::POST
    }

    fn url(&self) -> String {
        self.base_url.clone() + LOGIN_PATH
    }

    fn payload(&self) -> Result<RequestData<String>> {
        let payload = serde_json::to_string(&self.payload)
            .chain_err(|| ErrorKind::InvalidJSON("Could not serialize login payload".to_string()))?;
        Ok(RequestData::Body(payload))
    }

    fn headers(&self) -> Result<Option<HeaderMap>> {
        Ok(None)
    }
}

/// V1 post chat message endpoint
pub struct ChatPostMessageEndpoint<'a> {
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

impl<'a> Endpoint<String> for ChatPostMessageEndpoint<'a> {
    fn method(&self) -> Method {
        Method::POST
    }

    fn url(&self) -> String {
        self.base_url.clone() + CHAT_POST_MESSAGE_PATH
    }

    fn payload(&self) -> Result<RequestData<String>> {
        let payload = serde_json::to_string(&self.payload)
            .chain_err(|| ErrorKind::InvalidJSON("Could not serialize post chat message payload".to_string()))?;
        Ok(RequestData::Body(payload))
    }

    fn headers(&self) -> Result<Option<HeaderMap>> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_str("application/json")?);
        headers.insert("X-User-Id", HeaderValue::from_str(&self.user_id)?);
        headers.insert("X-Auth-Token", HeaderValue::from_str(&self.auth_token)?);
        Ok(Some(headers))
    }
}

/// V1 endpoint to post a messag with an attachement
pub struct RoomsUploadEndpoint<'a> {
    base_url: String,
    user_id: String,
    auth_token: String,
    payload: PostFileMessagePayload<'a>,
    room_id: &'a str,
}

/// Payload of the post chat message endpoint
pub struct PostFileMessagePayload<'a> {
    file: Vec<u8>,
    filename: &'a str,
    mimetype: HeaderValue,
}

impl<'a> Endpoint<String> for RoomsUploadEndpoint<'a> {
    fn method(&self) -> Method {
        Method::POST
    }

    fn url(&self) -> String {
        self.base_url.clone() + ROOMS_UPLOAD_PATH + "/" + self.room_id
    }

    fn payload(&self) -> Result<RequestData<String>> {
        let mut c = Cursor::new(Vec::new());
        c.write_all(&self.payload.file)?;
        c.seek(SeekFrom::Start(0))?;

        let part = Part::reader(c).file_name(self.payload.filename.to_owned()).mime_str(self.payload.mimetype.to_str()?)?;
        let form = Form::new().part("file", part);
        Ok(RequestData::MultipartForm(form))
    }

    fn headers(&self) -> Result<Option<HeaderMap>> {
        let mut headers = HeaderMap::new();
        // TODO: Fix unwrap
        headers.insert(CONTENT_TYPE, HeaderValue::from_str("application/x-www-form-urlencoded")?);
        headers.insert("X-User-Id", HeaderValue::from_str(&self.user_id)?);
        headers.insert("X-Auth-Token", HeaderValue::from_str(&self.auth_token)?);
        Ok(Some(headers))
    }
}

/// V1 get room members endpoint
pub struct ChannelsMembersEndpoint<'a> {
    base_url: String,
    user_id: String,
    auth_token: String,
    query_params: HashMap<&'static str, &'a str>,
}

/// Response payload from the Rocket.Chat members (channel or group) endpoint.
#[derive(Deserialize)]
///
pub struct MembersResponse {
    members: Vec<User>,
    count: i32,
    offset: i32,
    total: i32,
}

impl<'a> Endpoint<String> for ChannelsMembersEndpoint<'a> {
    fn method(&self) -> Method {
        Method::GET
    }

    fn url(&self) -> String {
        self.base_url.clone() + CHANNELS_MEMBERS_PATH
    }

    fn payload(&self) -> Result<RequestData<String>> {
        Ok(RequestData::Body("".to_string()))
    }

    fn headers(&self) -> Result<Option<HeaderMap>> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_str("application/json")?);
        headers.insert("X-User-Id", HeaderValue::from_str(&self.user_id)?);
        headers.insert("X-Auth-Token", HeaderValue::from_str(&self.auth_token)?);
        Ok(Some(headers))
    }

    fn query_params(&self) -> HashMap<&'static str, &str> {
        self.query_params.clone()
    }
}

/// V1 get joined rooms endpoint
pub struct ChannelsListJoinedEndpoint<'a> {
    base_url: String,
    user_id: String,
    auth_token: String,
    query_params: HashMap<&'static str, &'a str>,
}

/// Response payload from the Rocket.Chat joind channels endpoint.
#[derive(Deserialize)]
pub struct GetJoinedChannelsResponse {
    channels: Vec<Channel>,
    count: i32,
    offset: i32,
    total: i32,
}

impl<'a> Endpoint<String> for ChannelsListJoinedEndpoint<'a> {
    fn method(&self) -> Method {
        Method::GET
    }

    fn url(&self) -> String {
        self.base_url.clone() + CHANNELS_LIST_JOINED_PATH
    }

    fn payload(&self) -> Result<RequestData<String>> {
        Ok(RequestData::Body("".to_string()))
    }

    fn headers(&self) -> Result<Option<HeaderMap>> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_str("application/json")?);
        headers.insert("X-User-Id", HeaderValue::from_str(&self.user_id)?);
        headers.insert("X-Auth-Token", HeaderValue::from_str(&self.auth_token)?);
        Ok(Some(headers))
    }

    fn query_params(&self) -> HashMap<&'static str, &str> {
        self.query_params.clone()
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
pub struct DMListResponse {
    /// A list of direct messages that the user is part of.
    pub ims: Vec<Channel>,
}

/// Response payload from the Rocket.Chat groups.list endpoint.
#[derive(Deserialize)]
pub struct GroupsListResponse {
    /// A list of groups on the Rocket.Chat server
    pub groups: Vec<Channel>,
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

impl Message {
    /// The content the of the attachment
    pub fn content_type(&self) -> Result<HeaderValue> {
        let mimetype = self.file.clone().unwrap_or_default().mimetype;
        let mime: HeaderValue = mimetype.parse().chain_err(|| ErrorKind::UnknownMimeType(mimetype))?;
        Ok(mime)
    }
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
        RocketchatApi { base_url, logger, user_id: "".to_string(), auth_token: "".to_string() }
    }
}

impl super::RocketchatApi for RocketchatApi {
    fn attachments(&self, message_id: &str) -> Result<Vec<RocketchatAttachment>> {
        debug!(self.logger, "Retreiving attachment {}", message_id);

        let message = self.chat_get_message(message_id)?;

        let mut files = Vec::new();

        if let Some(attachments) = message.attachments {
            for attachment in attachments {
                debug!(self.logger, "Getting file {}", attachment.title_link);

                let get_file_endpoint = GetFileEndpoint {
                    base_url: self.base_url.clone(),
                    user_id: self.user_id.clone(),
                    auth_token: self.auth_token.clone(),
                    path: &attachment.title_link,
                    query_params: HashMap::new(),
                };

                let mut resp = RestApi::get_rocketchat_file(&get_file_endpoint)?;

                if !resp.status().is_success() {
                    let mut body = String::new();
                    resp.read_to_string(&mut body).chain_err(|| ErrorKind::ApiCallFailed(attachment.title_link.clone()))?;
                    return Err(build_error(&get_file_endpoint.url(), &body, &resp.status()));
                }

                let mut buffer = Vec::new();
                resp.read_to_end(&mut buffer).chain_err(|| ErrorKind::InternalServerError)?;
                let rocketchat_attachment =
                    RocketchatAttachment { content_type: attachment.content_type, data: buffer, title: attachment.title };
                files.push(rocketchat_attachment);
            }
        } else {
            debug!(self.logger, "No attachments found for message ID {}", message_id);
        }

        Ok(files)
    }

    fn channels_members(&self, room_id: &str) -> Result<Vec<User>> {
        debug!(self.logger, "Getting rooms members for room {} from Rocket.Chat server", room_id);

        let mut users = Vec::new();
        let mut offset = 0;
        for i in 0..super::MAX_REQUESTS_PER_ENDPOINT_CALL {
            if i == super::MAX_REQUESTS_PER_ENDPOINT_CALL {
                bail_error!(ErrorKind::TooManyRequests(CHANNELS_MEMBERS_PATH.to_string()))
            }

            let mut members_response = get_channel_members(&self, room_id, offset)?;
            users.append(&mut members_response.members);
            let subtotal = members_response.count + members_response.offset;
            if subtotal == members_response.total {
                break;
            }

            offset = subtotal;
        }

        Ok(users)
    }

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

    fn channels_list_joined(&self) -> Result<Vec<Channel>> {
        debug!(self.logger, "Getting joined channels for user {} from Rocket.Chat server", self.user_id);

        let mut channels = Vec::new();
        let mut offset = 0;
        for i in 0..super::MAX_REQUESTS_PER_ENDPOINT_CALL {
            if i == super::MAX_REQUESTS_PER_ENDPOINT_CALL {
                bail_error!(ErrorKind::TooManyRequests(CHANNELS_LIST_JOINED_PATH.to_string()))
            }

            let mut channels_response = channels_list_joined(&self, offset)?;
            channels.append(&mut channels_response.channels);
            let subtotal = channels_response.count + channels_response.offset;
            if subtotal == channels_response.total {
                break;
            }

            offset = subtotal;
        }

        Ok(channels)
    }

    fn chat_get_message(&self, message_id: &str) -> Result<RocketchatMessage> {
        debug!(self.logger, "Retreiving message {}", message_id);

        let mut query_params = HashMap::new();
        query_params.insert("msgId", message_id);

        let message_endpoint = GetWithAuthEndpoint {
            base_url: self.base_url.clone(),
            user_id: self.user_id.clone(),
            auth_token: self.auth_token.clone(),
            path: CHAT_GET_MESSAGE_PATH,
            query_params,
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

        let mut message_attachments_option: Option<Vec<MessageAttachment>> = None;
        // Rocket.Chat stores the proper content type (for example 'text/plain') only in the message,
        // the attachment contains a type as well, but it's set to 'file' most of the time.
        let content_type = message_response.message.content_type()?;
        if let Some(attachments) = message_response.message.attachments {
            let mut message_attachments = Vec::new();
            for attachment in attachments {
                message_attachments.push(MessageAttachment {
                    content_type: content_type.clone(),
                    image_url: attachment.image_url,
                    title: attachment.title,
                    title_link: attachment.title_link,
                })
            }

            message_attachments_option = Some(message_attachments)
        };

        let file = message_response.message.file.as_ref().map(|f| RocketchatFile { mimetype: f.mimetype.clone() });
        let message = RocketchatMessage {
            attachments: message_attachments_option,
            id: message_response.message.id,
            msg: message_response.message.msg,
            file,
        };

        Ok(message)
    }

    fn chat_post_message(&self, text: &str, room_id: &str) -> Result<()> {
        debug!(self.logger, "Forwarding message to to Rocket.Chat room {}", room_id);

        let chat_post_message_endpoint = ChatPostMessageEndpoint {
            base_url: self.base_url.clone(),
            user_id: self.user_id.clone(),
            auth_token: self.auth_token.clone(),
            payload: PostChatMessagePayload { text: Some(text), room_id },
        };

        let (body, status_code) = RestApi::call_rocketchat(&chat_post_message_endpoint)?;
        if !status_code.is_success() {
            return Err(build_error(&chat_post_message_endpoint.url(), &body, &status_code));
        }

        Ok(())
    }

    fn dm_list(&self) -> Result<Vec<Channel>> {
        debug!(self.logger, "Getting direct messages list from Rocket.Chat server {}", &self.base_url);

        let direct_messages_list_endpoint = GetWithAuthEndpoint {
            base_url: self.base_url.clone(),
            user_id: self.user_id.clone(),
            auth_token: self.auth_token.clone(),
            path: DM_LIST_PATH,
            query_params: HashMap::new(),
        };

        let (body, status_code) = RestApi::call_rocketchat(&direct_messages_list_endpoint)?;
        if !status_code.is_success() {
            return Err(build_error(&direct_messages_list_endpoint.url(), &body, &status_code));
        }

        let direct_messages_list_response: DMListResponse = serde_json::from_str(&body).chain_err(|| {
            ErrorKind::InvalidJSON(format!(
                "Could not deserialize response from Rocket.Chat dm.list API \
                 endpoint: `{}`",
                body
            ))
        })?;

        Ok(direct_messages_list_response.ims)
    }

    fn groups_list(&self) -> Result<Vec<Channel>> {
        debug!(self.logger, "Getting group list from Rocket.Chat server {}", &self.base_url);

        let channels_list_endpoint = GetWithAuthEndpoint {
            base_url: self.base_url.clone(),
            user_id: self.user_id.clone(),
            auth_token: self.auth_token.clone(),
            path: GROUPS_LIST_PATH,
            query_params: HashMap::new(),
        };

        let (body, status_code) = RestApi::call_rocketchat(&channels_list_endpoint)?;
        if !status_code.is_success() {
            return Err(build_error(&channels_list_endpoint.url(), &body, &status_code));
        }

        let channels_list_response: GroupsListResponse = serde_json::from_str(&body).chain_err(|| {
            ErrorKind::InvalidJSON(format!(
                "Could not deserialize response from Rocket.Chat groups.list API \
                 endpoint: `{}`",
                body
            ))
        })?;

        Ok(channels_list_response.groups)
    }

    fn groups_members(&self, room_id: &str) -> Result<Vec<User>> {
        debug!(self.logger, "Getting group members for group {} from Rocket.Chat server", room_id);

        let mut users = Vec::new();
        let mut offset = 0;
        for i in 0..super::MAX_REQUESTS_PER_ENDPOINT_CALL {
            if i == super::MAX_REQUESTS_PER_ENDPOINT_CALL {
                bail_error!(ErrorKind::TooManyRequests(GROUPS_MEMBERS_PATH.to_string()))
            }

            let mut members_response = get_group_members(&self, room_id, offset)?;
            users.append(&mut members_response.members);
            let subtotal = members_response.count + members_response.offset;
            if subtotal == members_response.total {
                break;
            }

            offset = subtotal;
        }

        Ok(users)
    }

    fn login(&self, username: &str, password: &str) -> Result<(String, String)> {
        debug!(self.logger, "Logging in user with username {} on Rocket.Chat server {}", username, &self.base_url);

        let login_endpoint = LoginEndpoint { base_url: self.base_url.clone(), payload: LoginPayload { username, password } };

        let (body, status_code) = RestApi::call_rocketchat(&login_endpoint)?;
        if !status_code.is_success() {
            return Err(build_error(&login_endpoint.url(), &body, &status_code));
        }

        let login_response: LoginResponse = serde_json::from_str(&body).chain_err(|| {
            ErrorKind::InvalidJSON(format!("Could not deserialize response from Rocket.Chat login API endpoint: `{}`", body))
        })?;
        Ok((login_response.data.user_id, login_response.data.auth_token))
    }

    fn me(&self) -> Result<User> {
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

        let user: User = serde_json::from_str(&body).chain_err(|| {
            ErrorKind::InvalidJSON(format!("Could not deserialize response from Rocket.Chat me API endpoint: `{}`", body))
        })?;

        Ok(user)
    }

    fn rooms_upload(&self, file: Vec<u8>, filename: &str, mimetype: HeaderValue, room_id: &str) -> Result<()> {
        debug!(self.logger, "Uploading file to room {}", room_id);

        let post_file_message_endpoint = RoomsUploadEndpoint {
            base_url: self.base_url.clone(),
            user_id: self.user_id.clone(),
            auth_token: self.auth_token.clone(),
            payload: PostFileMessagePayload { file, filename, mimetype },
            room_id,
        };

        let (body, status_code) = RestApi::call_rocketchat(&post_file_message_endpoint)?;
        if !status_code.is_success() {
            return Err(build_error(&post_file_message_endpoint.url(), &body, &status_code));
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
            query_params,
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

    fn with_credentials(mut self: Box<Self>, user_id: String, auth_token: String) -> Box<dyn super::RocketchatApi> {
        self.user_id = user_id;
        self.auth_token = auth_token;
        self
    }
}

fn get_channel_members(rocketchat_api: &RocketchatApi, room_id: &str, offset: i32) -> Result<MembersResponse> {
    let offset_param = offset.to_string();
    let mut query_params = HashMap::new();
    query_params.insert("roomId", room_id);
    query_params.insert("offset", &offset_param);
    let channel_members_endpoint = ChannelsMembersEndpoint {
        base_url: rocketchat_api.base_url.clone(),
        user_id: rocketchat_api.user_id.clone(),
        auth_token: rocketchat_api.auth_token.clone(),
        query_params,
    };

    let (body, status_code) = RestApi::call_rocketchat(&channel_members_endpoint)?;
    if !status_code.is_success() {
        return Err(build_error(&channel_members_endpoint.url(), &body, &status_code));
    }

    let channel_members_response: MembersResponse = serde_json::from_str(&body).chain_err(|| {
        ErrorKind::InvalidJSON(format!(
            "Could not deserialize response from Rocket.Chat channel members API endpoint: `{}`",
            body
        ))
    })?;
    Ok(channel_members_response)
}

fn get_group_members(rocketchat_api: &RocketchatApi, room_id: &str, offset: i32) -> Result<MembersResponse> {
    let offset_param = offset.to_string();
    let mut query_params = HashMap::new();
    query_params.insert("roomId", room_id);
    query_params.insert("offset", &offset_param);
    let group_members_endpoint = GroupsMembersEndpoint {
        base_url: rocketchat_api.base_url.clone(),
        user_id: rocketchat_api.user_id.clone(),
        auth_token: rocketchat_api.auth_token.clone(),
        query_params,
    };

    let (body, status_code) = RestApi::call_rocketchat(&group_members_endpoint)?;
    if !status_code.is_success() {
        return Err(build_error(&group_members_endpoint.url(), &body, &status_code));
    }

    let group_members_response: MembersResponse = serde_json::from_str(&body).chain_err(|| {
        ErrorKind::InvalidJSON(format!(
            "Could not deserialize response from Rocket.Chat group members API endpoint: `{}`",
            body
        ))
    })?;
    Ok(group_members_response)
}

fn channels_list_joined(rocketchat_api: &RocketchatApi, offset: i32) -> Result<GetJoinedChannelsResponse> {
    let offset_param = offset.to_string();
    let mut query_params = HashMap::new();
    query_params.insert("offset", offset_param.as_ref());
    let channels_list_joined_endpoint = ChannelsListJoinedEndpoint {
        base_url: rocketchat_api.base_url.clone(),
        user_id: rocketchat_api.user_id.clone(),
        auth_token: rocketchat_api.auth_token.clone(),
        query_params,
    };

    let (body, status_code) = RestApi::call_rocketchat(&channels_list_joined_endpoint)?;
    if !status_code.is_success() {
        return Err(build_error(&channels_list_joined_endpoint.url(), &body, &status_code));
    }

    let channels_list_joined_response: GetJoinedChannelsResponse = serde_json::from_str(&body).chain_err(|| {
        ErrorKind::InvalidJSON(format!(
            "Could not deserialize response from Rocket.Chat joined channels API endpoint: `{}`",
            body
        ))
    })?;
    Ok(channels_list_joined_response)
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

    if *status_code == StatusCode::UNAUTHORIZED {
        return Error {
            error_chain: ErrorKind::RocketchatAuthenticationFailed(rocketchat_error_resp.message.unwrap_or_default()).into(),
            user_message: Some(t!(["errors", "authentication_failed"])),
        };
    }

    let message = rocketchat_error_resp.message.clone();
    let error_msg = message.unwrap_or_else(|| rocketchat_error_resp.error.clone().unwrap_or_else(|| body.to_string()));
    Error::from(ErrorKind::RocketchatError(error_msg))
}
