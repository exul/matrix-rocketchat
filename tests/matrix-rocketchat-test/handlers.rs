use rand::{Rng, thread_rng};
use std::borrow::{Borrow, Cow};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::mpsc::Receiver;
use std::sync::MutexGuard;

use iron::prelude::*;
use iron::url::Url;
use iron::url::percent_encoding::percent_decode;
use iron::{BeforeMiddleware, Chain, Handler, status};
use matrix_rocketchat::errors::{MatrixErrorResponse, RocketchatErrorResponse};
use persistent::Write;
use router::Router;
use ruma_client_api::r0::alias::get_alias;
use ruma_client_api::r0::account::register;
use ruma_client_api::r0::room::create_room;
use ruma_client_api::r0::sync::get_member_events;
use ruma_events::EventType;
use ruma_events::room::member::{MemberEvent, MemberEventContent, MembershipState};
use ruma_identifiers::{EventId, RoomAliasId, RoomId, UserId};
use serde_json;
use super::{DEFAULT_LOGGER, Message, MessageForwarder, RoomAliasMap, RoomsStatesMap, TestError, UsernameList, UsersInRoomMap,
            extract_payload, helpers};

#[derive(Serialize)]
pub struct RocketchatInfo {
    pub version: &'static str,
}

impl Handler for RocketchatInfo {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Rocket.Chat mock server got info request");

        let payload = r#"{
            "version": "VERSION"
        }"#
            .replace("VERSION", self.version);

        Ok(Response::with((status::Ok, payload)))
    }
}

pub struct RocketchatLogin {
    pub successful: bool,
    pub rocketchat_user_id: Option<String>,
}

impl Handler for RocketchatLogin {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Rocket.Chat mock server got login request");

        let (status, payload) = match self.successful {
            true => {
                let user_id: String =
                    self.rocketchat_user_id.clone().unwrap_or(thread_rng().gen_ascii_chars().take(10).collect());
                (
                    status::Ok,
                    r#"{
                    "status": "success",
                    "data": {
                        "authToken": "spec_auth_token",
                        "userId": "USER_ID"
                    }
                 }"#
                        .replace("USER_ID", &user_id),
                )
            }
            false => {
                (
                    status::Unauthorized,
                    r#"{
                    "status": "error",
                    "message": "Unauthorized"
                }"#
                        .to_string(),
                )
            }
        };

        Ok(Response::with((status, payload)))
    }
}

pub struct RocketchatMe {
    pub username: String,
}

impl Handler for RocketchatMe {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Rocket.Chat mock server got me request");

        let payload = r#"{
            "username": "USERNAME"
        }"#
            .replace("USERNAME", &self.username);

        Ok(Response::with((status::Ok, payload)))
    }
}

pub struct RocketchatChannelsList {
    pub channels: HashMap<&'static str, Vec<&'static str>>,
    pub status: status::Status,
}

impl Handler for RocketchatChannelsList {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Rocket.Chat mock server got channel list request");

        let mut channels: Vec<String> = Vec::new();

        for (channel_name, user_names) in self.channels.iter() {
            let channel = r#"{
                "_id": "CHANNEL_NAME_id",
                "name": "CHANNEL_NAME",
                "t": "c",
                "usernames": [
                    "CHANNEL_USERNAMES"
                ],
                "msgs": 0,
                "u": {
                    "_id": "spec_user_id",
                    "username": "spec_username"
                },
                "ts": "2017-02-12T13:20:22.092Z",
                "ro": false,
                "sysMes": true,
                "_updatedAt": "2017-02-12T13:20:22.092Z"
            }"#
                .replace("CHANNEL_NAME", channel_name)
                .replace("CHANNEL_USERNAMES", &user_names.join("\",\""));
            channels.push(channel);
        }

        let payload = "{ \"channels\": [".to_string() + &channels.join(",") + "], \"success\": true }";

        Ok(Response::with((self.status, payload)))
    }
}

pub struct RocketchatDirectMessagesList {
    pub direct_messages: HashMap<&'static str, Vec<&'static str>>,
    pub status: status::Status,
}

impl Handler for RocketchatDirectMessagesList {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Rocket.Chat mock server got direct message list request");

        let mut dms = Vec::new();
        for (id, user_names) in self.direct_messages.iter() {
            let dm = r#"{
                "_id": "DIRECT_MESSAGE_ID",
                "_updatedAt": "2017-05-25T21:51:04.429Z",
                "t": "d",
                "msgs": 5,
                "ts": "2017-05-12T14:49:01.806Z",
                "lm": "2017-05-25T21:51:04.414Z",
                "username": "admin",
                "usernames": [
                    "USER_NAMES"
                ]}"#
                .replace("DIRECT_MESSAGE_ID", id)
                .replace("USER_NAMES", &user_names.join("\",\""));
            dms.push(dm);
        }

        let payload = "{ \"ims\": [".to_string() + &dms.join(",") + "]}";

        Ok(Response::with((status::Ok, payload)))
    }
}



pub struct RocketchatUsersInfo {}

impl Handler for RocketchatUsersInfo {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Rocket.Chat mock server got user info request");

        let url: Url = request.url.clone().into();
        let mut query_pairs = url.query_pairs();

        let (status, payload) = match query_pairs.find(|&(ref key, _)| key == "username") {
            Some((_, ref username)) => {
                (
                    status::Ok,
                    r#"{
                    "user": {
                        "name": "Name USERNAME",
                        "username": "USERNAME",
                        "status": "online",
                        "utcOffset": 1,
                        "type": "user",
                        "active": true,
                        "_id": "USERNAME_id"
                    },
                    "success": true
                }"#
                        .replace("USERNAME", username),
                )
            }
            None => {
                (
                    status::BadRequest,
                    r#"{
                    "success": false,
                    "error": "The required \"userId\" or \"username\" param was not provided [error-user-param-not-provided]",
                    "errorType": "error-user-param-not-provided"
                    }"#
                        .to_string(),
                )
            }
        };

        Ok(Response::with((status, payload)))
    }
}

pub struct RocketchatErrorResponder {
    pub message: String,
    pub status: status::Status,
}

impl Handler for RocketchatErrorResponder {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Rocket.Chat mock server got handle error request");

        let error_response = RocketchatErrorResponse {
            status: Some("error".to_string()),
            message: Some(self.message.clone()),
            error: None,
        };
        let payload = serde_json::to_string(&error_response).unwrap();
        Ok(Response::with((self.status, payload)))
    }
}

#[derive(Serialize)]
pub struct MatrixVersion {
    pub versions: Vec<&'static str>,
}

impl Handler for MatrixVersion {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got version request");

        let payload = serde_json::to_string(self).unwrap();
        Ok(Response::with((status::Ok, payload)))
    }
}

pub struct MatrixRegister {}

impl MatrixRegister {
    pub fn with_forwarder() -> (Chain, Receiver<String>) {
        let (message_forwarder, receiver) = MessageForwarder::new();
        let mut chain = Chain::new(MatrixRegister {});
        chain.link_before(message_forwarder);;
        (chain, receiver)
    }
}

impl Handler for MatrixRegister {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got register request");
        let request_payload = extract_payload(request);
        let register_payload: register::BodyParams = serde_json::from_str(&request_payload).unwrap();

        let mutex = request.get::<Write<UsernameList>>().unwrap();
        let mut username_list = mutex.lock().unwrap();

        if username_list.iter().any(|u| u == register_payload.username.as_ref().unwrap()) {
            let error_response = MatrixErrorResponse {
                errcode: "M_USER_IN_USE".to_string(),
                error: "The desired user ID is already taken.".to_string(),
            };
            let response_payload = serde_json::to_string(&error_response).unwrap();
            Ok(Response::with((status::BadRequest, response_payload)))
        } else {
            username_list.push(register_payload.username.unwrap());
            Ok(Response::with((status::Ok, "{}".to_string())))
        }
    }
}

pub struct MatrixCreateRoom {
    pub as_url: String,
}

impl MatrixCreateRoom {
    /// Create a `MatrixCreateRoom` handler with a message forwarder middleware.
    pub fn with_forwarder(as_url: String) -> (Chain, Receiver<String>) {
        let (message_forwarder, receiver) = MessageForwarder::new();
        let mut chain = Chain::new(MatrixCreateRoom { as_url: as_url });
        chain.link_before(message_forwarder);
        (chain, receiver)
    }
}

impl Handler for MatrixCreateRoom {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got create room request");
        let request_payload = extract_payload(request);
        let create_room_payload: create_room::BodyParams = serde_json::from_str(&request_payload).unwrap();
        let room_id_local_part: String = create_room_payload
            .name
            .unwrap_or("1234".to_string())
            .chars()
            .into_iter()
            .filter(|c| c.is_alphanumeric() || c == &'_')
            .collect();
        let test_room_id = format!("!{}_id:localhost", &room_id_local_part);
        let room_id = RoomId::try_from(&test_room_id).unwrap();

        let url: Url = request.url.clone().into();
        let mut query_pairs = url.query_pairs();
        let (_, user_id_param) = query_pairs.find(|&(ref key, _)| key == "user_id").unwrap_or((
            Cow::from("user_id"),
            Cow::from("@rocketchat:localhost"),
        ));
        let user_id = UserId::try_from(user_id_param.borrow()).unwrap();

        add_user_to_users_in_room(request, user_id.clone(), room_id.clone());
        add_state_to_room(request, room_id.clone(), "creator".to_string(), user_id.to_string());

        if let Some(room_alias_name) = create_room_payload.room_alias_name {
            let room_alias_id = RoomAliasId::try_from(&format!("#{}:localhost", room_alias_name)).unwrap();

            if let Err(err) = add_alias_to_room(request, room_id.clone(), room_alias_id.clone()) {
                debug!(DEFAULT_LOGGER, format!("{}", err));
                let payload = r#"{
                    "errcode":"M_UNKNOWN",
                    "error":"Room alias already exists."
                }"#;
                return Ok(Response::with((status::Conflict, payload.to_string())));
            }

            add_state_to_room(request, room_id.clone(), "alias".to_string(), room_alias_id.to_string());
        }

        helpers::send_join_event_from_matrix(&self.as_url, room_id.clone(), user_id);

        let response = create_room::Response { room_id: room_id };
        let payload = serde_json::to_string(&response).unwrap();

        Ok(Response::with((status::Ok, payload)))
    }
}

pub struct SendRoomState {}

impl SendRoomState {
    pub fn with_forwarder() -> (Chain, Receiver<String>) {
        let (message_forwarder, receiver) = MessageForwarder::new();
        let mut chain = Chain::new(SendRoomState {});
        chain.link_before(message_forwarder);;
        (chain, receiver)
    }
}

impl Handler for SendRoomState {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got send room state request");
        let params = request.extensions.get::<Router>().unwrap().clone();
        let url_room_id = params.find("room_id").unwrap();
        let decoded_room_id = percent_decode(url_room_id.as_bytes()).decode_utf8().unwrap();
        let room_id = RoomId::try_from(&decoded_room_id).unwrap();

        let request_payload = extract_payload(request);
        let room_states_payload: serde_json::Value = serde_json::from_str(&request_payload).unwrap();

        match room_states_payload {
            serde_json::Value::Object(room_states) => {
                for (k, v) in room_states {
                    add_state_to_room(request, room_id.clone(), k, v.to_string().trim_matches('"').to_string());
                }
            }
            _ => panic!("JSON type not covered"),
        }

        let mut values = serde_json::Map::new();
        let event_id = EventId::new("localhost").unwrap();
        values.insert("event_id".to_string(), serde_json::Value::String(event_id.to_string()));
        let payload = serde_json::to_string(&values).unwrap();

        Ok(Response::with((status::Ok, payload)))
    }
}


pub struct RoomMembers {}

impl Handler for RoomMembers {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got get room members request");
        let params = request.extensions.get::<Router>().unwrap().clone();
        let url_room_id = params.find("room_id").unwrap();
        let decoded_room_id = percent_decode(url_room_id.as_bytes()).decode_utf8().unwrap();
        let room_id = RoomId::try_from(&decoded_room_id).unwrap();

        let mutex = request.get::<Write<UsersInRoomMap>>().unwrap();
        let user_in_room_map = mutex.lock().unwrap();
        let empty_users = Vec::new();
        let user_ids = &user_in_room_map.get(&room_id).unwrap_or(&empty_users);

        let member_events = build_member_events_from_user_ids(user_ids, room_id);

        let response = get_member_events::Response { chunk: member_events };
        let payload = serde_json::to_string(&response).unwrap();
        Ok(Response::with((status::Ok, payload)))
    }
}

pub struct StaticRoomMembers {
    pub user_ids: Vec<UserId>,
}

impl Handler for StaticRoomMembers {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got get static room members request");
        let params = request.extensions.get::<Router>().unwrap().clone();
        let url_room_id = params.find("room_id").unwrap();
        let decoded_room_id = percent_decode(url_room_id.as_bytes()).decode_utf8().unwrap();
        let room_id = RoomId::try_from(&decoded_room_id).unwrap();

        let member_events = build_member_events_from_user_ids(&self.user_ids, room_id);

        let response = get_member_events::Response { chunk: member_events };
        let payload = serde_json::to_string(&response).unwrap();
        Ok(Response::with((status::Ok, payload)))
    }
}

fn build_member_events_from_user_ids(users: &Vec<UserId>, room_id: RoomId) -> Vec<MemberEvent> {
    let mut member_events = Vec::new();
    for user in users.iter() {
        let member_event = MemberEvent {
            content: MemberEventContent {
                avatar_url: None,
                displayname: None,
                membership: MembershipState::Join,
                third_party_invite: None,
            },
            event_id: EventId::new("localhost").unwrap(),
            event_type: EventType::RoomMember,
            invite_room_state: None,
            prev_content: None,
            room_id: room_id.clone(),
            state_key: user.to_string(),
            unsigned: None,
            user_id: user.clone(),
        };
        member_events.push(member_event);
    }

    member_events
}

pub struct GetRoomAlias {}

impl Handler for GetRoomAlias {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got get room alias request");

        let params = request.extensions.get::<Router>().unwrap().clone();
        let url_room_alias = params.find("room_alias").unwrap();
        let decoded_room_alias = percent_decode(url_room_alias.as_bytes()).decode_utf8().unwrap();
        let room_alias = RoomAliasId::try_from(&decoded_room_alias).unwrap();

        match get_room_id_for_alias(request, &room_alias) {
            Some(room_id) => {
                debug!(DEFAULT_LOGGER, "Matrix mock server found room ID {} for alias {}", room_id, room_alias);
                let get_alias_response = get_alias::Response {
                    room_id: room_id,
                    servers: vec!["localhsot".to_string()],
                };
                let payload = serde_json::to_string(&get_alias_response).unwrap();
                Ok(Response::with((status::Ok, payload.to_string())))
            }
            None => {
                debug!(DEFAULT_LOGGER, "Matrix mock server did not find any room ID for alias {}", room_alias);
                let payload = r#"{
                    "errcode":"M_NOT_FOUND",
                    "error":"Event not found."
                }"#;
                Ok(Response::with((status::NotFound, payload.to_string())))
            }
        }
    }
}

pub struct DeleteRoomAlias {}

impl Handler for DeleteRoomAlias {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got delete room alias request");

        let params = request.extensions.get::<Router>().unwrap().clone();
        let url_room_alias = params.find("room_alias").unwrap();
        let decoded_room_alias = percent_decode(url_room_alias.as_bytes()).decode_utf8().unwrap();
        let room_alias = RoomAliasId::try_from(&decoded_room_alias).unwrap();

        match remove_alias_from_room(request, &room_alias) {
            Some(room_id) => {
                debug!(DEFAULT_LOGGER, "Matrix mock server deleted alias {} for room {}", room_alias, room_id);
                Ok(Response::with((status::Ok, "{}".to_string())))
            }
            None => {
                debug!(DEFAULT_LOGGER, "Matrix mock server could not delete alias {}", room_alias);
                let payload = r#"{
                    "errcode":"M_NOT_FOUND",
                    "error":"Event not found."
                }"#;
                Ok(Response::with((status::NotFound, payload.to_string())))
            }
        }
    }
}

pub struct GetRoomState {}

impl Handler for GetRoomState {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got get room state request");
        let params = request.extensions.get::<Router>().unwrap().clone();
        let url_room_id = params.find("room_id").unwrap();
        let decoded_room_id = percent_decode(url_room_id.as_bytes()).decode_utf8().unwrap();
        let room_id = RoomId::try_from(&decoded_room_id).unwrap();

        let url_event_type = params.find("event_type").unwrap();
        let event_type = percent_decode(url_event_type.as_bytes()).decode_utf8().unwrap();
        let event_type_value: serde_json::Value = event_type.clone().into();

        let state_option = match serde_json::from_value::<EventType>(event_type_value).unwrap() {
            EventType::RoomCreate => get_state_from_room(request, room_id, "creator".to_string()),
            EventType::RoomCanonicalAlias => get_state_from_room(request, room_id, "alias".to_string()),
            EventType::RoomTopic => get_state_from_room(request, room_id, "topic".to_string()),
            _ => panic!("Event type {} not covered", event_type),
        };

        let (k, v) = match state_option {
            Some((k, v)) => (k, v),
            None => {
                let payload = r#"{
                    "errcode":"M_NOT_FOUND",
                    "error":"Event not found."
                }"#;
                return Ok(Response::with((status::NotFound, payload.to_string())));
            }
        };

        let mut values: HashMap<String, String> = HashMap::new();
        values.insert(k, v);
        let payload = serde_json::to_string(&values).unwrap();

        Ok(Response::with((status::Ok, payload)))
    }
}

pub struct RoomStateCreate {
    pub creator: UserId,
}

impl Handler for RoomStateCreate {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got room state create request");
        let mut values = serde_json::Map::new();
        values.insert("creator".to_string(), serde_json::Value::String(self.creator.to_string()));
        let payload = serde_json::to_string(&values).unwrap();

        Ok(Response::with((status::Ok, payload)))
    }
}

pub struct MatrixJoinRoom {
    pub as_url: String,
}

impl MatrixJoinRoom {
    pub fn with_forwarder(as_url: String) -> (Chain, Receiver<String>) {
        let (message_forwarder, receiver) = MessageForwarder::new();
        let mut chain = Chain::new(MatrixJoinRoom { as_url: as_url });
        chain.link_before(message_forwarder);;
        (chain, receiver)
    }
}

impl Handler for MatrixJoinRoom {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got join room request");
        let params = request.extensions.get::<Router>().unwrap().clone();
        let url_room_id = params.find("room_id").unwrap();
        let decoded_room_id = percent_decode(url_room_id.as_bytes()).decode_utf8().unwrap();
        let room_id = RoomId::try_from(&decoded_room_id).unwrap();

        let url: Url = request.url.clone().into();
        let mut query_pairs = url.query_pairs();
        let (_, user_id_param) = query_pairs.find(|&(ref key, _)| key == "user_id").unwrap_or((
            Cow::from("user_id"),
            Cow::from("@rocketchat:localhost"),
        ));
        let user_id = UserId::try_from(user_id_param.borrow()).unwrap();

        add_user_to_users_in_room(request, user_id.clone(), room_id.clone());

        helpers::send_join_event_from_matrix(&self.as_url, room_id, user_id);

        Ok(Response::with((status::Ok, "{}")))
    }
}

pub struct MatrixLeaveRoom {
    pub as_url: String,
}

impl MatrixLeaveRoom {
    pub fn with_forwarder(as_url: String) -> (Chain, Receiver<String>) {
        let (message_forwarder, receiver) = MessageForwarder::new();
        let mut chain = Chain::new(MatrixLeaveRoom { as_url: as_url });
        chain.link_before(message_forwarder);;
        (chain, receiver)
    }
}

impl Handler for MatrixLeaveRoom {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got leave room request");
        let params = request.extensions.get::<Router>().unwrap().clone();
        let url_room_id = params.find("room_id").unwrap();
        let decoded_room_id = percent_decode(url_room_id.as_bytes()).decode_utf8().unwrap();
        let room_id = RoomId::try_from(&decoded_room_id).unwrap();

        let url: Url = request.url.clone().into();
        let mut query_pairs = url.query_pairs();
        let (_, user_id_param) = query_pairs.find(|&(ref key, _)| key == "user_id").unwrap_or((
            Cow::from("user_id"),
            Cow::from("@rocketchat:localhost"),
        ));
        let user_id = UserId::try_from(user_id_param.borrow()).unwrap();

        remove_user_from_users_in_room(request, user_id.clone(), room_id.clone());

        helpers::send_leave_event_from_matrix(&self.as_url, room_id, user_id);

        Ok(Response::with((status::Ok, "{}")))
    }
}

pub struct EmptyJson {}

impl Handler for EmptyJson {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got empty json request");
        Ok(Response::with((status::Ok, "{}")))
    }
}

pub struct MatrixErrorResponder {
    pub status: status::Status,
    pub message: String,
}

impl Handler for MatrixErrorResponder {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got error responder request");

        let error_response = MatrixErrorResponse {
            errcode: "1234".to_string(),
            error: self.message.clone(),
        };
        let payload = serde_json::to_string(&error_response).unwrap();
        Ok(Response::with((self.status, payload)))
    }
}

pub struct MatrixConditionalErrorResponder {
    pub status: status::Status,
    pub message: String,
    pub conditional_content: &'static str,
}

impl MatrixConditionalErrorResponder {
    pub fn with_forwarder(error_message: String, conditional_content: &'static str) -> (Chain, Receiver<String>) {
        let (message_forwarder, receiver) = MessageForwarder::new();

        let conditional_error_responder = MatrixConditionalErrorResponder {
            status: status::InternalServerError,
            message: error_message,
            conditional_content: conditional_content,
        };

        let mut chain = Chain::new(conditional_error_responder);
        chain.link_after(message_forwarder);;
        (chain, receiver)
    }
}

impl BeforeMiddleware for MatrixConditionalErrorResponder {
    fn before(&self, request: &mut Request) -> IronResult<()> {
        let request_payload = extract_payload(request);

        if request_payload.contains(self.conditional_content) {
            let error_response = MatrixErrorResponse {
                errcode: "1234".to_string(),
                error: self.message.clone(),
            };
            let payload = serde_json::to_string(&error_response).unwrap();
            let err = IronError::new(TestError("Conditional Error".to_string()), (self.status, payload));
            return Err(err.into());
        }

        let message = Message { payload: request_payload };
        request.extensions.insert::<Message>(message);

        Ok(())
    }
}

impl Handler for MatrixConditionalErrorResponder {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got conditional error responder request");
        let request_payload = extract_payload(request);

        if request_payload.contains(self.conditional_content) {
            let error_response = MatrixErrorResponse {
                errcode: "1234".to_string(),
                error: self.message.clone(),
            };
            let payload = serde_json::to_string(&error_response).unwrap();
            Ok(Response::with((self.status, payload)))
        } else {
            Ok(Response::with((status::Ok, "{}".to_string())))
        }
    }
}

pub struct ConditionalInvalidJsonResponse {
    pub status: status::Status,
    pub conditional_content: &'static str,
}

impl BeforeMiddleware for ConditionalInvalidJsonResponse {
    fn before(&self, request: &mut Request) -> IronResult<()> {
        let request_payload = extract_payload(request);

        if request_payload.contains(self.conditional_content) {
            let err =
                IronError::new(TestError("Conditional invalid JSON".to_string()), (self.status, "invalid json".to_string()));
            return Err(err.into());
        }

        let message = Message { payload: request_payload };
        request.extensions.insert::<Message>(message);

        Ok(())
    }
}

pub struct InvalidJsonResponse {
    pub status: status::Status,
}

impl Handler for InvalidJsonResponse {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got invali JSON responder request");
        Ok(Response::with((self.status, "invalid json")))
    }
}

fn add_state_to_room(request: &mut Request, room_id: RoomId, state_key: String, state_value: String) {
    debug!(DEFAULT_LOGGER, "Matrix mock server adds room state {} with value {}", state_key, state_value);

    let mutex = request.get::<Write<RoomsStatesMap>>().unwrap();
    let mut rooms_states = mutex.lock().unwrap();

    if !rooms_states.contains_key(&room_id) {
        rooms_states.insert(room_id.clone(), HashMap::new());
    }

    let mut room_states = rooms_states.get_mut(&room_id).unwrap();
    room_states.insert(state_key, state_value);
}

fn get_state_from_room(request: &mut Request, room_id: RoomId, state_key: String) -> Option<(String, String)> {
    debug!(DEFAULT_LOGGER, "Matrix mock server gets room state {}", state_key);

    let mutex = request.get::<Write<RoomsStatesMap>>().unwrap();
    let mut rooms_states = mutex.lock().unwrap();
    let room_states = match rooms_states.get_mut(&room_id) {
        Some(room_states) => room_states,
        None => {
            return None;
        }
    };

    let room_state = match room_states.get(&state_key) {
        Some(room_state) => room_state,
        None => {
            return None;
        }
    };

    Some((state_key.clone(), room_state.to_string()))
}

fn add_user_to_users_in_room(request: &mut Request, user_id: UserId, room_id: RoomId) {
    let mutex = request.get::<Write<UsersInRoomMap>>().unwrap();
    let mut user_in_room_map = mutex.lock().unwrap();
    if !user_in_room_map.contains_key(&room_id) {
        user_in_room_map.insert(room_id.clone(), Vec::new());
    }

    let mut users = user_in_room_map.get_mut(&room_id).unwrap();

    if users.iter().any(|id| id == &user_id) {
        return;
    }

    users.push(user_id);
}

fn add_alias_to_room(request: &mut Request, room_id: RoomId, room_alias: RoomAliasId) -> Result<(), &'static str> {
    let mutex = request.get::<Write<RoomAliasMap>>().unwrap();
    let mut room_alias_map = mutex.lock().unwrap();

    for (_, aliases) in room_alias_map.iter() {
        if aliases.iter().any(|id| id == &room_alias) {
            return Err("Alias already taken");
        }
    }


    if !room_alias_map.contains_key(&room_id) {
        room_alias_map.insert(room_id.clone(), Vec::new());
    }

    let mut aliases = room_alias_map.get_mut(&room_id).unwrap();

    debug!(DEFAULT_LOGGER, "Matrix mock server adds alias {} to room {}", room_alias, room_id);;
    aliases.push(room_alias);
    Ok(())
}

fn get_room_id_for_alias(request: &mut Request, room_alias: &RoomAliasId) -> Option<RoomId> {
    let mutex = request.get::<Write<RoomAliasMap>>().unwrap();
    let room_alias_map = mutex.lock().unwrap();
    room_id_from_alias_map(&room_alias_map, room_alias)
}

fn remove_alias_from_room(request: &mut Request, room_alias: &RoomAliasId) -> Option<RoomId> {
    let mutex = request.get::<Write<RoomAliasMap>>().unwrap();
    let mut room_alias_map = mutex.lock().unwrap();
    let room_id = match room_id_from_alias_map(&room_alias_map, room_alias) {
        Some(room_id) => room_id,
        None => {
            return None;
        }
    };
    let aliases = room_alias_map.get_mut(&room_id).unwrap();
    let index = match aliases.iter().position(|alias| alias == room_alias) {
        Some(index) => index,
        None => {
            return None;
        }
    };

    aliases.remove(index);
    Some(room_id.clone())
}

fn remove_user_from_users_in_room(request: &mut Request, user_id: UserId, room_id: RoomId) {
    let mutex = request.get::<Write<UsersInRoomMap>>().unwrap();
    let mut user_in_room_map = mutex.lock().unwrap();
    let mut empty_users = Vec::new();
    let mut users = user_in_room_map.get_mut(&room_id).unwrap_or(&mut empty_users);
    users.retain(|ref u| *u != &user_id);
}


fn room_id_from_alias_map(
    room_alias_map: &MutexGuard<HashMap<RoomId, Vec<RoomAliasId>>>,
    room_alias: &RoomAliasId,
) -> Option<RoomId> {
    for (room_id, aliases) in room_alias_map.iter() {
        if aliases.iter().any(|alias| alias == room_alias) {
            return Some(room_id.clone());
        }
    }

    None
}
