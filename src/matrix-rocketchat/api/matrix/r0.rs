use std::collections::HashMap;
use std::convert::TryFrom;

use pulldown_cmark::{html, Options, Parser};
use reqwest::StatusCode;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::alias::get_alias::{self, Endpoint as GetAliasEndpoint};
use ruma_client_api::r0::alias::delete_alias::Endpoint as DeleteAliasEndpoint;
use ruma_client_api::r0::account::register::{self, Endpoint as RegisterEndpoint};
use ruma_client_api::r0::membership::forget_room::{self, Endpoint as ForgetRoomEndpoint};
use ruma_client_api::r0::membership::invite_user::{self, Endpoint as InviteUserEndpoint};
use ruma_client_api::r0::membership::join_room_by_id::{self, Endpoint as JoinRoomByIdEndpoint};
use ruma_client_api::r0::membership::leave_room::{self, Endpoint as LeaveRoomEndpoint};
use ruma_client_api::r0::profile::get_display_name::{self, Endpoint as GetDisplayNameEndpoint};
use ruma_client_api::r0::profile::set_display_name::{self, Endpoint as SetDisplayNameEndpoint};
use ruma_client_api::r0::room::create_room::{self, Endpoint as CreateRoomEndpoint, RoomPreset};
use ruma_client_api::r0::send::send_message_event::{self, Endpoint as SendMessageEventEndpoint};
use ruma_client_api::r0::send::send_state_event_for_empty_key::{self, Endpoint as SendStateEventForEmptyKeyEndpoint};
use ruma_client_api::r0::sync::sync_events::Endpoint as SyncEventsEndpoint;
use ruma_client_api::r0::sync::get_member_events::{self, Endpoint as GetMemberEventsEndpoint};
use ruma_client_api::r0::sync::get_state_events::{self, Endpoint as GetStateEvents};
use ruma_client_api::r0::sync::get_state_events_for_empty_key::{self, Endpoint as GetStateEventsForEmptyKeyEndpoint};
use ruma_events::EventType;
use ruma_events::collections::all::Event;
use ruma_events::room::member::MemberEvent;
use ruma_events::room::message::MessageType;
use ruma_identifiers::{EventId, RoomAliasId, RoomId, UserId};
use serde_json::{self, Map, Value};
use slog::Logger;
use url;

use api::RestApi;
use config::Config;
use errors::*;

#[derive(Clone)]
/// Rocket.Chat REST API v0
pub struct MatrixApi {
    /// URL to call the API
    pub base_url: String,
    /// Access token for authentication
    pub access_token: String,
    /// Logger passed to the Matrix API
    logger: Logger,
}

impl MatrixApi {
    /// Create a new MatrixApi.
    pub fn new(config: &Config, logger: Logger) -> MatrixApi {
        MatrixApi {
            base_url: config.hs_url.to_string(),
            access_token: config.as_token.to_string(),
            logger: logger,
        }
    }

    fn params_hash(&self) -> HashMap<&str, &str> {
        let mut params: HashMap<&str, &str> = HashMap::new();
        params.insert("access_token", &self.access_token);
        params
    }
}

impl super::MatrixApi for MatrixApi {
    fn create_room(
        &self,
        room_name: Option<String>,
        room_alias_name: Option<String>,
        room_creator_id: &UserId,
    ) -> Result<RoomId> {
        let endpoint = self.base_url.clone() + &CreateRoomEndpoint::request_path(());
        let body_params = create_room::BodyParams {
            creation_content: None,
            invite: vec![],
            name: room_name,
            preset: Some(RoomPreset::PrivateChat),
            room_alias_name: room_alias_name,
            topic: None,
            visibility: Some("private".to_string()),
        };
        let payload = serde_json::to_string(&body_params).chain_err(|| body_params_error!("create_room"))?;
        let user_id = room_creator_id.to_string();
        let mut params = self.params_hash();
        params.insert("user_id", &user_id);

        let (body, status_code) = RestApi::call_matrix(CreateRoomEndpoint::method(), &endpoint, &payload, &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }

        let create_room_response: create_room::Response = serde_json::from_str(&body).chain_err(|| {
            ErrorKind::InvalidJSON(format!("Could not deserialize response from Matrix create_room API endpoint: `{}`", body))
        })?;

        debug!(self.logger, "Successfully created room with ID {}", create_room_response.room_id);
        Ok(create_room_response.room_id)
    }

    fn delete_room_alias(&self, matrix_room_alias_id: RoomAliasId) -> Result<()> {
        // the ruma client api path params cannot be used here, because they are not url encoded
        let encoded_room_alias =
            url::form_urlencoded::byte_serialize(matrix_room_alias_id.to_string().as_bytes()).collect::<String>();
        let endpoint = self.base_url.clone() + &format!("/_matrix/client/r0/directory/room/{}", &encoded_room_alias);
        let params = self.params_hash();

        let (body, status_code) = RestApi::call_matrix(DeleteAliasEndpoint::method(), &endpoint, "{}", &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }

        Ok(())
    }

    fn forget_room(&self, room_id: RoomId) -> Result<()> {
        let path_params = forget_room::PathParams { room_id: room_id };
        let endpoint = self.base_url.clone() + &ForgetRoomEndpoint::request_path(path_params);
        let params = self.params_hash();

        let (body, status_code) = RestApi::call_matrix(ForgetRoomEndpoint::method(), &endpoint, "{}", &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }
        Ok(())
    }

    fn get_joined_rooms(&self, user_id: UserId) -> Result<Vec<RoomId>> {
        let endpoint = self.base_url.clone() + &SyncEventsEndpoint::request_path(());
        let user_id = user_id.to_string();
        let mut params = self.params_hash();
        params.insert("user_id", &user_id);

        let (body, status_code) = RestApi::call_matrix(SyncEventsEndpoint::method(), &endpoint, "", &params)?;

        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }

        let sync_response: Value = serde_json::from_str(&body).chain_err(|| {
            ErrorKind::InvalidJSON(format!("Could not deserialize response from Matrix sync_events API endpoint: `{}`", body))
        })?;

        let empty_rooms = Value::Object(Map::new());
        let raw_rooms = sync_response.get("rooms").unwrap_or(&empty_rooms).get("join").unwrap_or(&empty_rooms);
        let rooms: HashMap<RoomId, Value> = serde_json::from_value(raw_rooms.clone()).unwrap_or_default();
        Ok(rooms.keys().map(|k| k.to_owned()).collect())
    }

    fn get_display_name(&self, user_id: UserId) -> Result<Option<String>> {
        let path_params = get_display_name::PathParams { user_id: user_id };
        let endpoint = self.base_url.clone() + &GetDisplayNameEndpoint::request_path(path_params);
        let params = self.params_hash();

        let (body, status_code) = RestApi::call_matrix(GetDisplayNameEndpoint::method(), &endpoint, "", &params)?;
        if status_code == StatusCode::NotFound {
            return Ok(None);
        }

        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }

        let get_display_name_response: get_display_name::Response = serde_json::from_str(&body).chain_err(|| {
            ErrorKind::InvalidJSON(
                format!("Could not deserialize response from Matrix get_display_name API endpoint: `{}`", body),
            )
        })?;

        Ok(Some(get_display_name_response.displayname.unwrap_or_default()))
    }

    fn get_room_alias(&self, matrix_room_alias_id: RoomAliasId) -> Result<Option<RoomId>> {
        // the ruma client api path params cannot be used here, because they are not url encoded
        let encoded_room_alias =
            url::form_urlencoded::byte_serialize(matrix_room_alias_id.to_string().as_bytes()).collect::<String>();
        let endpoint = self.base_url.clone() + &format!("/_matrix/client/r0/directory/room/{}", &encoded_room_alias);
        let params = self.params_hash();

        let (body, status_code) = RestApi::call_matrix(GetAliasEndpoint::method(), &endpoint, "{}", &params)?;
        if status_code == StatusCode::NotFound {
            return Ok(None);
        }

        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }

        let get_alias_response: get_alias::Response = serde_json::from_str(&body).chain_err(
            || ErrorKind::InvalidJSON(format!("Could not deserialize response from Matrix get_alias API endpoint: `{}`", body)),
        )?;

        Ok(Some(get_alias_response.room_id.clone()))
    }

    fn get_room_aliases(&self, room_id: RoomId, user_id: UserId) -> Result<Vec<RoomAliasId>> {
        let path_params = get_state_events::PathParams { room_id: room_id };
        let endpoint = self.base_url.clone() + &GetStateEvents::request_path(path_params);
        let user_id = user_id.to_string();
        let mut params = self.params_hash();
        params.insert("user_id", &user_id);

        let (body, status_code) = RestApi::call_matrix(GetStateEvents::method(), &endpoint, "", &params)?;

        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }

        let state_events: Vec<Event> = serde_json::from_str(&body).chain_err(|| {
            ErrorKind::InvalidJSON(
                format!("Could not deserialize response from Matrix get_state_events API endpoint: `{}`", body),
            )
        })?;

        let mut aliases = Vec::new();
        for event in state_events {
            match event {
                Event::RoomAliases(mut aliases_event) => aliases.append(&mut aliases_event.content.aliases),
                _ => {
                    debug!(self.logger, "Noop");
                }
            }
        }

        Ok(aliases)
    }

    fn get_room_canonical_alias(&self, room_id: RoomId) -> Result<Option<RoomAliasId>> {
        let path_params = get_state_events_for_empty_key::PathParams {
            room_id: room_id,
            event_type: EventType::RoomCanonicalAlias.to_string(),
        };
        let endpoint = self.base_url.clone() + &GetStateEventsForEmptyKeyEndpoint::request_path(path_params);
        let params = self.params_hash();

        let (body, status_code) = RestApi::call_matrix(GetStateEventsForEmptyKeyEndpoint::method(), &endpoint, "{}", &params)?;
        if status_code == StatusCode::NotFound {
            return Ok(None);
        }

        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }

        let room_canonical_alias_response: Value = serde_json::from_str(&body).chain_err(|| {
            ErrorKind::InvalidJSON(
                format!("Could not deserialize response from Matrix get_state_events_for_empty_key API endpoint: `{}`", body),
            )
        })?;

        let alias = room_canonical_alias_response["alias"].to_string().replace("\"", "");
        if alias.is_empty() {
            return Ok(None);
        }

        let room_canonical_alias = RoomAliasId::try_from(&alias).chain_err(|| ErrorKind::InvalidRoomAliasId(alias))?;
        Ok(Some(room_canonical_alias))
    }

    fn get_room_creator(&self, room_id: RoomId) -> Result<UserId> {
        let path_params = get_state_events_for_empty_key::PathParams {
            room_id: room_id,
            event_type: EventType::RoomCreate.to_string(),
        };
        let endpoint = self.base_url.clone() + &GetStateEventsForEmptyKeyEndpoint::request_path(path_params);
        let params = self.params_hash();

        let (body, status_code) = RestApi::call_matrix(GetStateEventsForEmptyKeyEndpoint::method(), &endpoint, "{}", &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }

        let room_create: Value = serde_json::from_str(&body).chain_err(|| {
            ErrorKind::InvalidJSON(
                format!("Could not deserialize response from Matrix get_state_events_for_empty_key API endpoint: `{}`", body),
            )
        })?;

        let room_creator = room_create["creator"].to_string().replace("\"", "");
        let user_id = UserId::try_from(&room_creator).chain_err(|| ErrorKind::InvalidUserId(room_creator))?;
        Ok(user_id)
    }


    fn get_room_members(&self, room_id: RoomId, sender_id: Option<UserId>) -> Result<Vec<MemberEvent>> {
        let path_params = get_member_events::PathParams {
            room_id: room_id.clone(),
        };
        let endpoint = self.base_url.clone() + &GetMemberEventsEndpoint::request_path(path_params);
        let user_id;
        let mut params = self.params_hash();
        if let Some(id) = sender_id {
            user_id = id.to_string();
            params.insert("user_id", &user_id);
        }

        let (body, status_code) = RestApi::call_matrix(GetMemberEventsEndpoint::method(), &endpoint, "{}", &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }

        debug!(self.logger, "List of room members for room {} successfully received", room_id);

        let room_member_events: get_member_events::Response = serde_json::from_str(&body).chain_err(
            || ErrorKind::InvalidJSON(format!("Could not deserialize response from Matrix members API endpoint: `{}`", body)),
        )?;
        Ok(room_member_events.chunk)
    }

    fn get_room_topic(&self, room_id: RoomId) -> Result<Option<String>> {
        let path_params = get_state_events_for_empty_key::PathParams {
            room_id: room_id,
            event_type: EventType::RoomTopic.to_string(),
        };
        let endpoint = self.base_url.clone() + &GetStateEventsForEmptyKeyEndpoint::request_path(path_params);
        let params = self.params_hash();

        let (body, status_code) = RestApi::call_matrix(GetStateEventsForEmptyKeyEndpoint::method(), &endpoint, "{}", &params)?;
        if status_code == StatusCode::NotFound {
            return Ok(None);
        }

        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }

        let room_topic_response: Value = serde_json::from_str(&body).chain_err(|| {
            ErrorKind::InvalidJSON(
                format!("Could not deserialize response from Matrix get_state_events_for_empty_key API endpoint: `{}`", body),
            )
        })?;

        Ok(Some(room_topic_response["topic"].to_string().replace("\"", "")))
    }

    fn invite(&self, room_id: RoomId, receiver_user_id: UserId, sender_user_id: UserId) -> Result<()> {
        let path_params = invite_user::PathParams {
            room_id: room_id.clone(),
        };
        let endpoint = self.base_url.clone() + &InviteUserEndpoint::request_path(path_params);
        let user_id = sender_user_id.to_string();
        let mut params = self.params_hash();
        params.insert("user_id", &user_id);
        let body_params = invite_user::BodyParams {
            user_id: receiver_user_id.clone(),
        };
        let payload = serde_json::to_string(&body_params).chain_err(|| body_params_error!("invite"))?;

        let (body, status_code) = RestApi::call_matrix(InviteUserEndpoint::method(), &endpoint, &payload, &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }

        debug!(self.logger, "User {} successfully invited into room {} by {}", receiver_user_id, room_id, sender_user_id);
        Ok(())
    }

    fn is_room_accessible_by_bot(&self, room_id: RoomId) -> Result<bool> {
        let path_params = get_state_events_for_empty_key::PathParams {
            room_id: room_id,
            event_type: EventType::RoomCreate.to_string(),
        };
        let endpoint = self.base_url.clone() + &GetStateEventsForEmptyKeyEndpoint::request_path(path_params);
        let params = self.params_hash();

        let (_, status_code) = RestApi::call_matrix(GetStateEventsForEmptyKeyEndpoint::method(), &endpoint, "{}", &params)?;

        Ok(status_code != StatusCode::Forbidden)
    }

    fn join(&self, room_id: RoomId, user_id: UserId) -> Result<()> {
        let path_params = join_room_by_id::PathParams {
            room_id: room_id.clone(),
        };
        let endpoint = self.base_url.clone() + &JoinRoomByIdEndpoint::request_path(path_params);
        let user_id = user_id.to_string();
        let mut params = self.params_hash();
        params.insert("user_id", &user_id);

        let (body, status_code) = RestApi::call_matrix(JoinRoomByIdEndpoint::method(), &endpoint, "{}", &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }

        debug!(self.logger, "User {} successfully joined room {}", user_id, room_id);
        Ok(())
    }

    fn leave_room(&self, room_id: RoomId, user_id: UserId) -> Result<()> {
        let path_params = leave_room::PathParams { room_id: room_id };
        let endpoint = self.base_url.clone() + &LeaveRoomEndpoint::request_path(path_params);
        let user_id = user_id.to_string();
        let mut params = self.params_hash();
        params.insert("user_id", &user_id);


        let (body, status_code) = RestApi::call_matrix(LeaveRoomEndpoint::method(), &endpoint, "{}", &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }
        Ok(())
    }

    fn put_canonical_room_alias(&self, room_id: RoomId, matrix_room_alias_id: Option<RoomAliasId>) -> Result<()> {
        let path_params = send_state_event_for_empty_key::PathParams {
            room_id: room_id,
            event_type: EventType::RoomCanonicalAlias,
        };
        let endpoint = self.base_url.clone() + &SendStateEventForEmptyKeyEndpoint::request_path(path_params);
        let room_alias = match matrix_room_alias_id {
            Some(matrix_room_alias_id) => matrix_room_alias_id.to_string(),
            None => String::new(),
        };
        let params = self.params_hash();

        let mut body_params = serde_json::Map::new();
        body_params.insert("alias".to_string(), json!(room_alias));
        let payload = serde_json::to_string(&body_params).chain_err(|| body_params_error!("canonical room alias"))?;

        let (body, status_code) =
            RestApi::call_matrix(SendStateEventForEmptyKeyEndpoint::method(), &endpoint, &payload, &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }
        Ok(())
    }

    fn register(&self, user_id_local_part: String) -> Result<()> {
        let endpoint = self.base_url.clone() + &RegisterEndpoint::request_path(());
        let params = self.params_hash();
        let body_params = register::BodyParams {
            bind_email: None,
            password: None,
            username: Some(user_id_local_part.to_lowercase()),
            device_id: None,
            initial_device_display_name: None,
            auth: None,
        };
        let payload = serde_json::to_string(&body_params).chain_err(|| body_params_error!("account"))?;

        let (body, status_code) = RestApi::call_matrix(RegisterEndpoint::method(), &endpoint, &payload, &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }
        Ok(())
    }

    fn send_text_message_event(&self, room_id: RoomId, user_id: UserId, body: String) -> Result<()> {
        let formatted_body = render_markdown(&body);
        let mut message = Map::new();
        message.insert("body".to_string(), json!(body));
        message.insert("formatted_body".to_string(), json!(formatted_body));
        message.insert("msgtype".to_string(), json!(MessageType::Text));
        message.insert("format".to_string(), json!("org.matrix.custom.html"));
        let payload = serde_json::to_string(&message).chain_err(|| body_params_error!("send message"))?;
        let txn_id = EventId::new(&self.base_url).chain_err(|| ErrorKind::EventIdGenerationFailed)?;
        let path_params = send_message_event::PathParams {
            room_id: room_id.clone(),
            event_type: EventType::RoomMessage,
            txn_id: txn_id.to_string(),
        };
        let endpoint = self.base_url.clone() + &SendMessageEventEndpoint::request_path(path_params);
        let user_id = user_id.to_string();
        let mut params = self.params_hash();
        params.insert("user_id", &user_id);

        let (body, status_code) = RestApi::call_matrix(SendMessageEventEndpoint::method(), &endpoint, &payload, &params)?;

        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }

        debug!(self.logger, "User {} successfully sent a message to room {}", user_id, room_id);
        Ok(())
    }

    fn set_default_powerlevels(&self, room_id: RoomId, room_creator_user_id: UserId) -> Result<()> {
        let path_params = send_state_event_for_empty_key::PathParams {
            room_id: room_id,
            event_type: EventType::RoomPowerLevels,
        };
        let endpoint = self.base_url.clone() + &SendStateEventForEmptyKeyEndpoint::request_path(path_params);
        let user_id = room_creator_user_id.to_string();
        let mut params = self.params_hash();
        params.insert("user_id", &user_id);
        let mut body_params = serde_json::Map::new();
        let mut users = serde_json::Map::new();
        users.insert(room_creator_user_id.to_string(), json!(100));
        body_params.insert("invite".to_string(), json!(50));
        body_params.insert("kick".to_string(), json!(50));
        body_params.insert("ban".to_string(), json!(50));
        body_params.insert("redact".to_string(), json!(50));
        body_params.insert("users".to_string(), json!(users));
        body_params.insert("events".to_string(), json!(serde_json::Map::new()));
        let payload = serde_json::to_string(&body_params).chain_err(|| body_params_error!("power levels"))?;

        let (body, status_code) =
            RestApi::call_matrix(SendStateEventForEmptyKeyEndpoint::method(), &endpoint, &payload, &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }
        Ok(())
    }

    fn set_display_name(&self, user_id: UserId, name: String) -> Result<()> {
        let path_params = set_display_name::PathParams {
            user_id: user_id.clone(),
        };
        let endpoint = self.base_url.clone() + &SetDisplayNameEndpoint::request_path(path_params);
        let user_id = user_id.to_string();
        let mut params = self.params_hash();
        params.insert("user_id", &user_id);
        let body_params = set_display_name::BodyParams {
            displayname: Some(name),
        };
        let payload = serde_json::to_string(&body_params).chain_err(|| body_params_error!("set display name"))?;

        let (body, status_code) = RestApi::call_matrix(SetDisplayNameEndpoint::method(), &endpoint, &payload, &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }
        Ok(())
    }

    fn set_room_name(&self, room_id: RoomId, name: String) -> Result<()> {
        let path_params = send_state_event_for_empty_key::PathParams {
            room_id: room_id,
            event_type: EventType::RoomName,
        };
        let endpoint = self.base_url.clone() + &SendStateEventForEmptyKeyEndpoint::request_path(path_params);
        let params = self.params_hash();
        let mut body_params = serde_json::Map::new();
        body_params.insert("name".to_string(), Value::String(name));
        let payload = serde_json::to_string(&body_params).chain_err(|| body_params_error!("room name"))?;

        let (body, status_code) =
            RestApi::call_matrix(SendStateEventForEmptyKeyEndpoint::method(), &endpoint, &payload, &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }
        Ok(())
    }

    fn set_room_topic(&self, room_id: RoomId, topic: String) -> Result<()> {
        let path_params = send_state_event_for_empty_key::PathParams {
            room_id: room_id,
            event_type: EventType::RoomTopic,
        };
        let endpoint = self.base_url.clone() + &SendStateEventForEmptyKeyEndpoint::request_path(path_params);
        let params = self.params_hash();
        let mut body_params = serde_json::Map::new();
        body_params.insert("topic".to_string(), Value::String(topic));
        let payload = serde_json::to_string(&body_params).chain_err(|| body_params_error!("room topic"))?;

        let (body, status_code) =
            RestApi::call_matrix(SendStateEventForEmptyKeyEndpoint::method(), &endpoint, &payload, &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }
        Ok(())
    }
}

fn build_error(endpoint: &str, body: &str, status_code: &StatusCode) -> Error {
    let json_error_msg = format!(
        "Could not deserialize error from Matrix API endpoint {} with status code {}: `{}`",
        endpoint,
        status_code,
        body
    );
    let json_error = ErrorKind::InvalidJSON(json_error_msg);
    let matrix_error_resp: MatrixErrorResponse = match serde_json::from_str(body).chain_err(|| json_error).map_err(Error::from)
    {
        Ok(matrix_error_resp) => matrix_error_resp,
        Err(err) => {
            return err;
        }
    };
    Error::from(ErrorKind::MatrixError(matrix_error_resp.error))
}

fn render_markdown(input: &str) -> String {
    // The html will not have the same length as the msg, but it's a good starting point
    let mut output = String::with_capacity(input.len());
    let opts = Options::empty();
    let parser = Parser::new_ext(input, opts);
    html::push_html(&mut output, parser);
    output
}
