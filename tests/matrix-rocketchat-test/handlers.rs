use rand::{thread_rng, Rng};
use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Mutex;
use std::sync::{Arc, MutexGuard};

use super::{
    extract_payload, helpers, Message, MessageForwarder, PendingInvites, RoomAliasMap, RoomsStatesMap, TestError, UserList,
    UsersInRooms, AS_TOKEN, DEFAULT_LOGGER,
};
use iron::prelude::*;
use iron::url::percent_encoding::percent_decode;
use iron::url::Url;
use iron::{status, BeforeMiddleware, Chain, Handler};
use matrix_rocketchat::api::rocketchat::v1::{LoginPayload, Message as RocketchatMessage};
use matrix_rocketchat::api::rocketchat::{Channel, User};
use matrix_rocketchat::errors::{MatrixErrorResponse, RocketchatErrorResponse};
use persistent::Write;
use router::Router;
use ruma_client_api::r0::account::register;
use ruma_client_api::r0::alias::get_alias;
use ruma_client_api::r0::media::create_content;
use ruma_client_api::r0::membership::invite_user;
use ruma_client_api::r0::profile::{get_display_name, set_display_name};
use ruma_client_api::r0::room::create_room;
use ruma_client_api::r0::sync::get_member_events;
use ruma_events::collections::only::StateEvent;
use ruma_events::room::aliases::{AliasesEvent, AliasesEventContent};
use ruma_events::room::member::{MemberEvent, MemberEventContent, MembershipState};
use ruma_events::EventType;
use ruma_identifiers::{EventId, RoomAliasId, RoomId, UserId};
use serde_json::{self, Map, Value};

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
    pub users: Arc<Mutex<HashMap<String, (String, String)>>>,
}

impl Handler for RocketchatLogin {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Rocket.Chat mock server got login request");
        let request_payload = extract_payload(request);
        let login_payload: LoginPayload = serde_json::from_str(&request_payload).unwrap();

        let users_map = self.users.lock().unwrap();
        if let Some((user_id, password)) = users_map.get(login_payload.username) {
            if password == login_payload.password {
                let payload = r#"{
                    "status": "success",
                    "data": {
                        "authToken": "spec_auth_token",
                        "userId": "USER_ID"
                    }
                 }"#
                .replace("USER_ID", &user_id);
                return Ok(Response::with((status::Ok, payload)));
            }
        }

        let payload = r#"{
                    "status": "error",
                    "message": "Unauthorized"
                }"#
        .to_string();

        Ok(Response::with((status::Unauthorized, payload)))
    }
}

//TODO: Fix this handler to use the header value and lookup the user in the users list instead of hard-code it in the struct
pub struct RocketchatMe {
    pub username: Arc<Mutex<String>>,
}

impl Handler for RocketchatMe {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Rocket.Chat mock server got me request");

        let payload = r#"{
            "_id": "USERNAME_id",
            "username": "USERNAME"
        }"#
        .replace("USERNAME", &self.username.lock().unwrap());

        Ok(Response::with((status::Ok, payload)))
    }
}

pub struct RocketchatChannelsList {
    pub channels: Arc<Mutex<HashMap<&'static str, Vec<&'static str>>>>,
    pub status: status::Status,
}

impl Handler for RocketchatChannelsList {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Rocket.Chat mock server got channel list request");

        let mut channels: Vec<String> = Vec::new();

        for channel_name in self.channels.lock().unwrap().keys().map(|k| *k) {
            let channel = r#"{
                "_id": "CHANNEL_NAME_id",
                "name": "CHANNEL_NAME",
                "t": "c",
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
            .replace("CHANNEL_NAME", channel_name);
            channels.push(channel);
        }

        let payload = "{ \"channels\": [".to_string() + &channels.join(",") + "], \"success\": true }";

        Ok(Response::with((self.status, payload)))
    }
}

pub struct RocketchatRoomMembers {
    pub channels: Arc<Mutex<HashMap<&'static str, Vec<&'static str>>>>,
    pub status: status::Status,
}

impl Handler for RocketchatRoomMembers {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Rocket.Chat mock server got room members request");

        let url: Url = request.url.clone().into();
        let mut query_pairs = url.query_pairs();
        let (_, room_id_param) = query_pairs.find(|&(ref key, _)| key == "roomId").unwrap().to_owned();
        // convert id to room name, because the list consists of room names and in the tests the room id
        // is constructed by appending _id
        let room_name = room_id_param.replace("_id", "");
        let room_name_ref: &str = room_name.as_ref();

        debug!(DEFAULT_LOGGER, "Looking up room {}", room_name_ref);
        let payload = match self.channels.lock().unwrap().get(room_name_ref) {
            Some(user_names) => {
                let mut users = Vec::new();
                for user_name in user_names {
                    let user = User { id: format!("{}_id", user_name), username: user_name.to_string() };
                    users.push(user);
                }

                let members = serde_json::to_string(&users).unwrap();
                format!(
                    "{{\"members\": {}, \"count\": {}, \"offset\": 0,\"total\": {}, \"success\": true}}",
                    members,
                    members.len(),
                    members.len()
                )
            }
            None => "foo".to_string(), //TODO: Wut, we merged that? Fix me
        };

        Ok(Response::with((self.status, payload)))
    }
}

pub struct RocketchatGroupsList {
    pub groups: Arc<Mutex<HashMap<&'static str, Vec<&'static str>>>>,
    pub status: status::Status,
}

impl Handler for RocketchatGroupsList {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Rocket.Chat mock server got groups list request");

        let mut groups: Vec<String> = Vec::new();

        for group_name in self.groups.lock().unwrap().keys().map(|k| *k) {
            let channel = r#"{
                "_id": "GROUP_NAME_id",
                "name": "GROUP_NAME",
                "t": "c",
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
            .replace("GROUP_NAME", group_name);
            groups.push(channel);
        }

        let payload = "{ \"groups\": [".to_string() + &groups.join(",") + "], \"success\": true }";

        Ok(Response::with((self.status, payload)))
    }
}

pub struct RocketchatJoinedRooms {
    pub users_in_rooms: HashMap<&'static str, Vec<&'static str>>,
}

impl Handler for RocketchatJoinedRooms {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Rocket.Chat mock server got joined rooms request");

        let user_id_raw = request.headers.get_raw("X-User-Id").unwrap();
        let user_id_header = (*user_id_raw).first().unwrap();
        let user_id = String::from_utf8(user_id_header.to_vec()).unwrap();
        let user_id_ref: &str = user_id.as_ref();

        debug!(DEFAULT_LOGGER, "Looking up joined rooms for user {}", user_id);
        let payload = match self.users_in_rooms.get(user_id_ref) {
            Some(room_names) => {
                let mut rooms = Vec::new();
                for room_name in room_names {
                    let room = Channel { id: format!("{}_id", room_name), name: Some(room_name.to_string()) };
                    rooms.push(room);
                }

                let room_list = serde_json::to_string(&rooms).unwrap();
                format!(
                    "{{\"channels\": {}, \"count\": {}, \"offset\": 0,\"total\": {}, \"success\": true}}",
                    room_list,
                    room_list.len(),
                    room_list.len()
                )
            }
            None => r#"{"channels": [], "count": 0, "offset": 0,"total": 0, "success": true}"#.to_string(),
        };

        Ok(Response::with((status::Status::Ok, payload)))
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

pub struct RocketchatMessageResponder {
    pub message: Arc<Mutex<Option<RocketchatMessage>>>,
}

impl Handler for RocketchatMessageResponder {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Rocket.Chat mock server got get message request");

        let url: Url = request.url.clone().into();
        let mut query_pairs = url.query_pairs();
        let (_, msg_id_param) = query_pairs.find(|&(ref key, _)| key == "msgId").unwrap_or_default();

        let msg = self.message.lock().unwrap();

        match *msg {
            Some(ref msg) if msg.id == msg_id_param => {
                let message = serde_json::to_string(msg).unwrap();
                let payload = "{ \"message\": ".to_string() + &message + "}";

                Ok(Response::with((status::Ok, payload)))
            }
            _ => {
                debug!(DEFAULT_LOGGER, "No message attached to responder");
                Ok(Response::with((status::NotFound, "".to_string())))
            }
        }
    }
}

pub struct RocketchatUsersInfo {}

impl Handler for RocketchatUsersInfo {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Rocket.Chat mock server got user info request");

        let url: Url = request.url.clone().into();
        let mut query_pairs = url.query_pairs();

        let (status, payload) = match query_pairs.find(|&(ref key, _)| key == "username") {
            Some((_, ref username)) => (
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
            ),
            None => (
                status::BadRequest,
                r#"{
                    "success": false,
                    "error": "The required \"userId\" or \"username\" param was not provided [error-user-param-not-provided]",
                    "errorType": "error-user-param-not-provided"
                    }"#
                .to_string(),
            ),
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

        let error_response =
            RocketchatErrorResponse { status: Some("error".to_string()), message: Some(self.message.clone()), error: None };
        let payload = serde_json::to_string(&error_response).unwrap();
        Ok(Response::with((self.status, payload)))
    }
}

pub struct RocketchatFileResponder {
    pub files: HashMap<String, Vec<u8>>,
}

impl Handler for RocketchatFileResponder {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Rocket.Chat mock server got file handle request");

        let params = request.extensions.get::<Router>().unwrap().clone();
        let url_filename = params.find("filename").unwrap();
        let filename = percent_decode(url_filename.as_bytes()).decode_utf8().unwrap();

        match self.files.get(&*filename) {
            Some(file) => Ok(Response::with((status::Ok, file.to_owned()))),
            None => Ok(Response::with((status::NotFound, "".to_string()))),
        }
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

pub struct MatrixSync {}

impl Handler for MatrixSync {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got sync request");

        let user_id = user_id_from_request(request);

        let mutex = request.get::<Write<UserList>>().unwrap();
        let user_list = mutex.lock().unwrap();

        if !user_list.contains_key(&user_id) {
            let payload = r#"{
                    "errcode":"M_GUEST_ACCESS_FORBIDDEN",
                    "error":"User is not in room"
                }"#;
            return Ok(Response::with((status::Forbidden, payload.to_string())));
        }

        let mut joined_rooms = Map::new();
        let mut left_rooms = Map::new();
        let mut invited_rooms = Map::new();

        let mutex = request.get::<Write<UsersInRooms>>().unwrap();
        let users_in_rooms = mutex.lock().unwrap();

        let empty_object = Value::Object(Map::new());
        for (room_id, users_with_room_states) in users_in_rooms.iter() {
            match users_with_room_states.get(&user_id) {
                Some(&(membership_state, _)) if membership_state == MembershipState::Join => {
                    joined_rooms.insert(room_id.to_string(), empty_object.clone())
                }
                Some(&(membership_state, _)) if membership_state == MembershipState::Leave => {
                    left_rooms.insert(room_id.to_string(), empty_object.clone())
                }
                _ => continue,
            };
        }

        let mutex = request.get::<Write<PendingInvites>>().unwrap();
        let pending_invites_for_rooms = mutex.lock().unwrap();

        for (room_id, users) in pending_invites_for_rooms.iter() {
            match users.get(&user_id) {
                Some(_) => invited_rooms.insert(room_id.to_string(), empty_object.clone()),
                None => continue,
            };
        }

        let mut rooms = Map::new();
        rooms.insert("join".to_string(), Value::Object(joined_rooms));
        rooms.insert("leave".to_string(), Value::Object(left_rooms));
        rooms.insert("invite".to_string(), Value::Object(invited_rooms));

        let mut sync_response = Map::new();
        sync_response.insert("rooms".to_string(), Value::Object(rooms));

        let payload = serde_json::to_string(&sync_response).unwrap();
        Ok(Response::with((status::Ok, payload)))
    }
}

pub struct MatrixRegister {}

impl MatrixRegister {
    pub fn with_forwarder() -> (Chain, Receiver<String>) {
        let (message_forwarder, receiver) = MessageForwarder::new();
        let mut chain = Chain::new(MatrixRegister {});
        chain.link_before(message_forwarder);
        (chain, receiver)
    }
}

impl Handler for MatrixRegister {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got register request");
        let request_payload = extract_payload(request);
        let register_payload: register::BodyParams = serde_json::from_str(&request_payload).unwrap();

        let mutex = request.get::<Write<UserList>>().unwrap();
        let mut user_list = mutex.lock().unwrap();

        let user_id = UserId::try_from(format!("@{}:localhost", register_payload.username.unwrap()).as_ref()).unwrap();
        if user_list.contains_key(&user_id) {
            let error_response = MatrixErrorResponse {
                errcode: "M_USER_IN_USE".to_string(),
                error: "The desired user ID is already taken.".to_string(),
            };
            let response_payload = serde_json::to_string(&error_response).unwrap();
            Ok(Response::with((status::BadRequest, response_payload)))
        } else {
            debug!(DEFAULT_LOGGER, "Matrix mock server adds user with ID {}", &user_id);
            user_list.insert(user_id, None);
            Ok(Response::with((status::Ok, "{}".to_string())))
        }
    }
}

pub struct MatrixSetDisplayName {}

impl MatrixSetDisplayName {
    pub fn with_forwarder() -> (Chain, Receiver<String>) {
        let (message_forwarder, receiver) = MessageForwarder::new();
        let mut chain = Chain::new(MatrixSetDisplayName {});
        chain.link_before(message_forwarder);
        (chain, receiver)
    }
}

impl Handler for MatrixSetDisplayName {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got set display name request");
        let request_payload = extract_payload(request);
        let set_display_name_payload: set_display_name::BodyParams = serde_json::from_str(&request_payload).unwrap();

        let params = request.extensions.get::<Router>().unwrap().clone();
        let url_user_id = params.find("user_id").unwrap();
        let decoded_user_id = percent_decode(url_user_id.as_bytes()).decode_utf8().unwrap();
        let user_id = UserId::try_from(decoded_user_id.as_ref()).unwrap();

        let mutex = request.get::<Write<UserList>>().unwrap();
        let mut user_list = mutex.lock().unwrap();

        if !user_list.contains_key(&user_id) {
            debug!(DEFAULT_LOGGER, "Cannot set display name, user {} does not exist", user_id);
            let payload = r#"{
                    "errcode":"M_UNKNOWN",
                    "error":"Cannot set display name, user does not exist"
                }"#;
            return Ok(Response::with((status::NotFound, payload.to_string())));
        }

        user_list.insert(user_id, Some(set_display_name_payload.displayname.unwrap_or_default()));
        Ok(Response::with((status::Ok, "{}".to_string())))
    }
}

pub struct MatrixGetDisplayName {}

impl Handler for MatrixGetDisplayName {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got get display name request");
        let params = request.extensions.get::<Router>().unwrap().clone();
        let url_user_id = params.find("user_id").unwrap();
        let decoded_user_id = percent_decode(url_user_id.as_bytes()).decode_utf8().unwrap();
        let user_id = UserId::try_from(decoded_user_id.as_ref()).unwrap();

        let mutex = request.get::<Write<UserList>>().unwrap();
        let user_list = mutex.lock().unwrap();

        if !user_list.contains_key(&user_id) {
            debug!(DEFAULT_LOGGER, "Cannot get display name, user {} does not exist", user_id);
            let payload = r#"{
                    "errcode":"M_UNKNOWN",
                    "error":"Cannot get display name, user does not exist"
                }"#;
            return Ok(Response::with((status::NotFound, payload.to_string())));
        }

        let displayname = user_list.get(&user_id).unwrap();
        let get_display_name_response = get_display_name::Response { displayname: displayname.to_owned() };

        let payload = serde_json::to_string(&get_display_name_response).unwrap();
        Ok(Response::with((status::Ok, payload.to_string())))
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

        let mut room_id = RoomId::try_from(test_room_id.as_ref()).unwrap();
        let user_id = user_id_from_request(request);

        // scope to release the mutex
        {
            // check if the room id already exists, if it does, append `_next` to it
            let users_in_rooms_mutex = request.get::<Write<UsersInRooms>>().unwrap();
            let users_in_rooms = users_in_rooms_mutex.lock().unwrap();
            if users_in_rooms.get(&room_id).is_some() {
                let next_room_id = format!("!{}_next_id:localhost", &room_id_local_part);
                room_id = RoomId::try_from(next_room_id.as_ref()).unwrap();
            }
        }

        if let Err(err) = add_membership_event_to_room(request, user_id.clone(), room_id.clone(), MembershipState::Join) {
            debug!(DEFAULT_LOGGER, "{}", err);
            let payload = r#"{
                    "errcode":"M_UNKNOWN",
                    "error":"ERR_MSG"
                }"#
            .replace("ERR_MSG", err);
            return Ok(Response::with((status::Conflict, payload.to_string())));
        }

        if let Err(err) = add_state_to_room(request, &user_id, room_id.clone(), "creator".to_string(), user_id.to_string()) {
            debug!(DEFAULT_LOGGER, "{}", err);
            let payload = r#"{
                    "errcode":"M_FORBIDDEN",
                    "error":"ERR_MSG"
                }"#
            .replace("ERR_MSG", err);
            return Ok(Response::with((status::Forbidden, payload.to_string())));
        }

        if let Some(room_alias_name) = create_room_payload.room_alias_name {
            let room_alias_id = RoomAliasId::try_from(format!("#{}:localhost", room_alias_name).as_ref()).unwrap();

            if let Err(err) = add_alias_to_room(request, room_id.clone(), room_alias_id.clone()) {
                debug!(DEFAULT_LOGGER, "{}", err);
                let payload = r#"{
                    "errcode":"M_UNKNOWN",
                    "error":"Room alias already exists."
                }"#;
                return Ok(Response::with((status::Conflict, payload.to_string())));
            }

            if let Err(err) =
                add_state_to_room(request, &user_id, room_id.clone(), "alias".to_string(), room_alias_id.to_string())
            {
                debug!(DEFAULT_LOGGER, "{}", err);
                let payload = r#"{
                    "errcode":"M_FORBIDDEN",
                    "error":"ERR_MSG"
                }"#
                .replace("ERR_MSG", err);
                return Ok(Response::with((status::Forbidden, payload.to_string())));
            }
        }

        helpers::send_join_event_from_matrix(&self.as_url, room_id.clone(), user_id, None);

        let response = create_room::Response { room_id: room_id };
        let payload = serde_json::to_string(&response).unwrap();

        Ok(Response::with((status::Ok, payload)))
    }
}

pub struct MatrixState {}

impl Handler for MatrixState {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got get state request");

        let room_id = room_id_from_request(request);
        let user_id = user_id_from_request(request);
        let mut room_states: Vec<StateEvent> = Vec::new();

        let mutex = request.get::<Write<RoomAliasMap>>().unwrap();
        let room_alias_map = mutex.lock().unwrap();
        if let Some(room_aliases) = room_alias_map.get(&room_id) {
            let aliases_event = AliasesEvent {
                content: AliasesEventContent { aliases: room_aliases.to_owned() },
                event_id: EventId::new("localhost").unwrap(),
                event_type: EventType::RoomAliases,
                prev_content: None,
                room_id: Some(room_id.clone()),
                state_key: "localhost".to_string(),
                unsigned: None,
                sender: user_id.clone(),
                origin_server_ts: 0,
            };

            room_states.push(StateEvent::RoomAliases(aliases_event));
        }

        let payload = serde_json::to_string(&room_states).unwrap();

        Ok(Response::with((status::Ok, payload)))
    }
}

pub struct SendRoomState {}

impl SendRoomState {
    pub fn with_forwarder() -> (Chain, Receiver<String>) {
        let (message_forwarder, receiver) = MessageForwarder::new();
        let mut chain = Chain::new(SendRoomState {});
        chain.link_before(message_forwarder);
        (chain, receiver)
    }
}

impl Handler for SendRoomState {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got send room state request");
        let params = request.extensions.get::<Router>().unwrap().clone();
        let url_room_id = params.find("room_id").unwrap();
        let decoded_room_id = percent_decode(url_room_id.as_bytes()).decode_utf8().unwrap();
        let room_id = RoomId::try_from(decoded_room_id.as_ref()).unwrap();
        let event_type = match params.find("event_type") {
            Some(url_event_type) => {
                let decoded_event_type = percent_decode(url_event_type.as_bytes()).decode_utf8().unwrap();
                let event_type = EventType::from(decoded_event_type.as_ref());
                Some(event_type)
            }
            None => None,
        };
        let user_id = user_id_from_request(request);
        let request_payload = extract_payload(request);
        let room_states_payload: serde_json::Value = serde_json::from_str(&request_payload).unwrap();

        match room_states_payload {
            serde_json::Value::Object(room_states) => {
                for (k, v) in room_states {
                    let value = v.to_string().trim_matches('"').to_string();

                    if let Some(EventType::RoomAliases) = event_type {
                        let room_alias_id = RoomAliasId::try_from(value.as_ref()).unwrap();

                        if let Err(err) = add_alias_to_room(request, room_id.clone(), room_alias_id) {
                            debug!(DEFAULT_LOGGER, "{}", err);
                            let payload = r#"{
                            "errcode":"M_UNKNOWN",
                            "error":"Room alias already exists."
                        }"#;
                            return Ok(Response::with((status::Conflict, payload.to_string())));
                        }
                    } else {
                        if let Err(err) = add_state_to_room(request, &user_id, room_id.clone(), k, value) {
                            debug!(DEFAULT_LOGGER, "{}", err);
                            let payload = r#"{
                          "errcode":"M_FORBIDDEN",
                          "error":"ERR_MSG"
                        }"#
                            .replace("ERR_MSG", err);
                            return Ok(Response::with((status::Forbidden, payload.to_string())));
                        }
                    }
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
        let room_id = RoomId::try_from(decoded_room_id.as_ref()).unwrap();

        let url: Url = request.url.clone().into();
        let mut query_pairs = url.query_pairs();
        let (_, user_id_param) = query_pairs
            .find(|&(ref key, _)| key == "user_id")
            .unwrap_or((Cow::from("user_id"), Cow::from("@rocketchat:localhost")));
        let user_id = UserId::try_from(user_id_param.as_ref()).unwrap();

        let mutex = request.get::<Write<UsersInRooms>>().unwrap();
        let mut users_in_rooms = mutex.lock().unwrap();
        let mut empty_users_in_room = HashMap::new();

        let users_in_room_for_users = users_in_rooms.get_mut(&room_id).unwrap_or(&mut empty_users_in_room);
        let users_in_room_for_user = match users_in_room_for_users.get(&user_id) {
            Some(&(_, ref users_in_room_for_user)) => users_in_room_for_user,
            None => {
                let payload = r#"{
                    "errcode":"M_GUEST_ACCESS_FORBIDDEN",
                    "error":"User is not in room"
                }"#;

                return Ok(Response::with((status::Forbidden, payload.to_string())));
            }
        };

        let member_events = build_member_events_from_user_ids(&users_in_room_for_user, room_id);

        let response = get_member_events::Response { chunk: member_events };
        let payload = serde_json::to_string(&response).unwrap();
        Ok(Response::with((status::Ok, payload)))
    }
}

pub struct StaticRoomMembers {
    pub user_ids: Vec<(UserId, MembershipState)>,
}

impl Handler for StaticRoomMembers {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got get static room members request");
        let params = request.extensions.get::<Router>().unwrap().clone();
        let url_room_id = params.find("room_id").unwrap();
        let decoded_room_id = percent_decode(url_room_id.as_bytes()).decode_utf8().unwrap();
        let room_id = RoomId::try_from(decoded_room_id.as_ref()).unwrap();

        let member_events = build_member_events_from_user_ids(&self.user_ids, room_id);

        let response = get_member_events::Response { chunk: member_events };
        let payload = serde_json::to_string(&response).unwrap();
        Ok(Response::with((status::Ok, payload)))
    }
}

pub struct MatrixCreateContentHandler {
    pub uploaded_files: Arc<Mutex<Vec<String>>>,
}

impl MatrixCreateContentHandler {
    pub fn with_forwarder(uploaded_files: Arc<Mutex<Vec<String>>>) -> (Chain, Receiver<String>) {
        let (message_forwarder, receiver) = MessageForwarder::new();
        let mut chain = Chain::new(MatrixCreateContentHandler { uploaded_files: uploaded_files });
        chain.link_before(message_forwarder);
        (chain, receiver)
    }
}

impl Handler for MatrixCreateContentHandler {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got upload request");

        let file_id: String = thread_rng().gen_ascii_chars().take(24).collect();
        let response = create_content::Response { content_uri: format!("mxc://localhost/{}", &file_id) };

        let mut uploaded_files = self.uploaded_files.lock().unwrap();
        uploaded_files.push(file_id);

        let payload = serde_json::to_string(&response).unwrap();
        Ok(Response::with((status::Ok, payload)))
    }
}

pub struct MatrixGetContentHandler {
    pub files: HashMap<String, Vec<u8>>,
}

impl Handler for MatrixGetContentHandler {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got get content request");

        let params = request.extensions.get::<Router>().unwrap().clone();
        let url_filename = params.find("media_id").unwrap();
        let filename = percent_decode(url_filename.as_bytes()).decode_utf8().unwrap();

        match self.files.get(&*filename) {
            Some(file) => Ok(Response::with((status::Ok, file.to_owned()))),
            None => Ok(Response::with((status::NotFound, "".to_string()))),
        }
    }
}

fn build_member_events_from_user_ids(users: &Vec<(UserId, MembershipState)>, room_id: RoomId) -> Vec<MemberEvent> {
    let mut member_events = Vec::new();
    for &(ref user, membership_state) in users.iter() {
        let member_event = MemberEvent {
            content: MemberEventContent {
                is_direct: None,
                avatar_url: None,
                displayname: None,
                membership: membership_state,
                third_party_invite: None,
            },
            event_id: EventId::new("localhost").unwrap(),
            event_type: EventType::RoomMember,
            invite_room_state: None,
            prev_content: None,
            room_id: Some(room_id.clone()),
            state_key: user.to_string(),
            unsigned: None,
            sender: user.clone(),
            origin_server_ts: 0,
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
        let room_alias = RoomAliasId::try_from(decoded_room_alias.as_ref()).unwrap();

        match get_room_id_for_alias(request, &room_alias) {
            Some(room_id) => {
                debug!(DEFAULT_LOGGER, "Matrix mock server found room ID {} for alias {}", room_id, room_alias);
                let get_alias_response = get_alias::Response { room_id: room_id, servers: vec!["localhsot".to_string()] };
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
        let room_alias = RoomAliasId::try_from(decoded_room_alias.as_ref()).unwrap();

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
        let room_id = RoomId::try_from(decoded_room_id.as_ref()).unwrap();

        let url_event_type = params.find("event_type").unwrap();
        let event_type = percent_decode(url_event_type.as_bytes()).decode_utf8().unwrap();
        let event_type_value: serde_json::Value = event_type.clone().into();
        let user_id = user_id_from_request(request);

        let state_result = match serde_json::from_value::<EventType>(event_type_value).unwrap() {
            EventType::RoomCreate => get_state_from_room(request, room_id, user_id.clone(), "creator".to_string()),
            EventType::RoomCanonicalAlias => get_state_from_room(request, room_id, user_id.clone(), "alias".to_string()),
            EventType::RoomTopic => get_state_from_room(request, room_id, user_id.clone(), "topic".to_string()),
            _ => panic!("Event type {} not covered", event_type),
        };

        let state_option = match state_result {
            Ok(state_option) => state_option,
            Err(err) => {
                let payload = r#"{
                    "errcode":"M_GUEST_ACCESS_FORBIDDEN",
                    "error":"ERR_MSG"
                }"#
                .replace("ERR_MSG", err);
                return Ok(Response::with((status::Forbidden, payload.to_string())));
            }
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
    pub send_inviter: bool,
}

impl MatrixJoinRoom {
    pub fn with_forwarder(as_url: String, send_inviter: bool) -> (Chain, Receiver<String>) {
        let (message_forwarder, receiver) = MessageForwarder::new();
        let mut chain = Chain::new(MatrixJoinRoom { as_url: as_url, send_inviter: send_inviter });
        chain.link_before(message_forwarder);
        (chain, receiver)
    }
}

impl Handler for MatrixJoinRoom {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got join room request");
        let params = request.extensions.get::<Router>().unwrap().clone();
        let url_room_id = params.find("room_id").unwrap();
        let decoded_room_id = percent_decode(url_room_id.as_bytes()).decode_utf8().unwrap();
        let room_id = RoomId::try_from(decoded_room_id.as_ref()).unwrap();

        let url: Url = request.url.clone().into();
        let mut query_pairs = url.query_pairs();
        let (_, user_id_param) = query_pairs
            .find(|&(ref key, _)| key == "user_id")
            .unwrap_or((Cow::from("user_id"), Cow::from("@rocketchat:localhost")));
        let user_id = UserId::try_from(user_id_param.as_ref()).unwrap();

        let inviter_id;
        // scope to release the mutex
        {
            let mutex = request.get::<Write<PendingInvites>>().unwrap();
            let mut pending_invites_for_rooms = mutex.lock().unwrap();
            let mut empty_invites = HashMap::new();
            let pending_invites_for_room = pending_invites_for_rooms.get_mut(&room_id).unwrap_or(&mut empty_invites);
            inviter_id = match pending_invites_for_room.get(&user_id) {
                Some(inviter_id) => inviter_id.clone(),
                None => {
                    debug!(
                        DEFAULT_LOGGER,
                        "Matrix mock server: Join failed, because user {} is not invited to room {}", user_id, room_id
                    );

                    let payload = r#"{
                    "errcode":"M_UNKNOWN",
                    "error":"User not invited"
                }"#;
                    return Ok(Response::with((status::Conflict, payload.to_string())));
                }
            };
        }

        if let Err(err) = add_membership_event_to_room(request, user_id.clone(), room_id.clone(), MembershipState::Join) {
            debug!(DEFAULT_LOGGER, "{}", err);
            let payload = r#"{
                    "errcode":"M_UNKNOWN",
                    "error":"ERR_MSG"
                }"#
            .replace("ERR_MSG", err);
            return Ok(Response::with((status::Conflict, payload.to_string())));
        }

        let join_inviter = if self.send_inviter { Some(inviter_id) } else { None };
        helpers::send_join_event_from_matrix(&self.as_url, room_id, user_id, join_inviter);

        Ok(Response::with((status::Ok, "{}")))
    }
}

pub struct MatrixInviteUser {
    pub as_url: String,
}

impl MatrixInviteUser {
    pub fn with_forwarder(as_url: String) -> (Chain, Receiver<String>) {
        let (message_forwarder, receiver) = MessageForwarder::new();
        let mut chain = Chain::new(MatrixInviteUser { as_url: as_url });
        chain.link_before(message_forwarder);
        (chain, receiver)
    }
}

impl Handler for MatrixInviteUser {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got invite user to room request");
        let params = request.extensions.get::<Router>().unwrap().clone();
        let url_room_id = params.find("room_id").unwrap();
        let decoded_room_id = percent_decode(url_room_id.as_bytes()).decode_utf8().unwrap();
        let room_id = RoomId::try_from(decoded_room_id.as_ref()).unwrap();

        let url: Url = request.url.clone().into();
        let mut query_pairs = url.query_pairs();
        let (_, user_id_param) = query_pairs
            .find(|&(ref key, _)| key == "user_id")
            .unwrap_or((Cow::from("user_id"), Cow::from("@rocketchat:localhost")));
        let inviter_id = UserId::try_from(user_id_param.as_ref()).unwrap();

        let request_payload = extract_payload(request);
        let invite_payload: invite_user::BodyParams = serde_json::from_str(&request_payload).unwrap();
        let invitee_id = invite_payload.user_id.clone();

        // scope to release the mutex, because when sending the invite event the AS will send a
        // join request immediately
        {
            let mutex = request.get::<Write<PendingInvites>>().unwrap();
            let mut pending_invites_for_rooms = mutex.lock().unwrap();
            add_pending_invite(&mut pending_invites_for_rooms, room_id.clone(), inviter_id.clone(), invitee_id.clone());
        }

        helpers::send_invite_event_from_matrix(&self.as_url, room_id, invitee_id, inviter_id);

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
        chain.link_before(message_forwarder);
        (chain, receiver)
    }
}

impl Handler for MatrixLeaveRoom {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got leave room request");
        let params = request.extensions.get::<Router>().unwrap().clone();
        let url_room_id = params.find("room_id").unwrap();
        let decoded_room_id = percent_decode(url_room_id.as_bytes()).decode_utf8().unwrap();
        let room_id = RoomId::try_from(decoded_room_id.as_ref()).unwrap();

        let url: Url = request.url.clone().into();
        let mut query_pairs = url.query_pairs();
        let (_, user_id_param) = query_pairs
            .find(|&(ref key, _)| key == "user_id")
            .unwrap_or((Cow::from("user_id"), Cow::from("@rocketchat:localhost")));
        let user_id = UserId::try_from(user_id_param.as_ref()).unwrap();

        if let Err(err) = add_membership_event_to_room(request, user_id.clone(), room_id.clone(), MembershipState::Leave) {
            debug!(DEFAULT_LOGGER, "{}", err);
            let payload = r#"{
                    "errcode":"M_UNKNOWN",
                    "error":"ERR_MSG"
                }"#
            .replace("ERR_MSG", err);
            return Ok(Response::with((status::Conflict, payload.to_string())));
        }

        helpers::send_leave_event_from_matrix(&self.as_url, room_id, user_id);

        Ok(Response::with((status::Ok, "{}")))
    }
}

pub struct EmptyJson {}

impl Handler for EmptyJson {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Server got empty json request for URL {}", request.url);
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

        let error_response = MatrixErrorResponse { errcode: "1234".to_string(), error: self.message.clone() };
        let payload = serde_json::to_string(&error_response).unwrap();
        Ok(Response::with((self.status, payload)))
    }
}

pub struct MatrixActivatableErrorResponder {
    pub status: status::Status,
    pub message: String,
    pub active: Arc<AtomicBool>,
}

impl BeforeMiddleware for MatrixActivatableErrorResponder {
    fn before(&self, request: &mut Request) -> IronResult<()> {
        let request_payload = extract_payload(request);

        if self.active.load(Ordering::Relaxed) {
            let error_response = MatrixErrorResponse { errcode: "1234".to_string(), error: self.message.clone() };
            let payload = serde_json::to_string(&error_response).unwrap();
            let err = IronError::new(TestError("Conditional Error".to_string()), (self.status, payload));
            return Err(err.into());
        }

        let message = Message { payload: request_payload };
        request.extensions.insert::<Message>(message);

        Ok(())
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
        chain.link_after(message_forwarder);
        (chain, receiver)
    }
}

impl BeforeMiddleware for MatrixConditionalErrorResponder {
    fn before(&self, request: &mut Request) -> IronResult<()> {
        let request_payload = extract_payload(request);

        if request_payload.contains(self.conditional_content) {
            let error_response = MatrixErrorResponse { errcode: "1234".to_string(), error: self.message.clone() };
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
            let error_response = MatrixErrorResponse { errcode: "1234".to_string(), error: self.message.clone() };
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
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        debug!(DEFAULT_LOGGER, "Matrix mock server got invalid JSON responder request for URL {}", request.url);
        Ok(Response::with((self.status, "invalid json")))
    }
}

pub struct PermissionCheck {}

impl BeforeMiddleware for PermissionCheck {
    fn before(&self, request: &mut Request) -> IronResult<()> {
        let user_id = user_id_from_request(request);
        let access_token = access_token_from_request(request);

        if access_token == AS_TOKEN && !user_id.to_string().starts_with("@rocketchat") {
            info!(DEFAULT_LOGGER, "Received request for {} with user that the AS can't masquerade as {}", request.url, user_id);
            let response = MatrixErrorResponse {
                errcode: "M_FORBIDDEN".to_string(),
                error: "Application service cannot masquerade as this user.".to_string(),
            };
            let payload = serde_json::to_string(&response).unwrap();

            let err = IronError::new(TestError("Cannot masquerade Error".to_string()), (status::Forbidden, payload));
            return Err(err.into());
        }

        Ok(())
    }
}

fn add_state_to_room(
    request: &mut Request,
    user_id: &UserId,
    room_id: RoomId,
    state_key: String,
    state_value: String,
) -> Result<(), &'static str> {
    debug!(DEFAULT_LOGGER, "Matrix mock server adds room state {} with value {}", state_key, state_value);

    let users_in_rooms_mutex = request.get::<Write<UsersInRooms>>().unwrap();
    let users_in_rooms = users_in_rooms_mutex.lock().unwrap();
    let empty_users_in_room = HashMap::new();
    let users_in_room = users_in_rooms.get(&room_id).unwrap_or(&empty_users_in_room);
    if !users_in_room.contains_key(user_id) {
        debug!(DEFAULT_LOGGER, "Matrix mock server: User {} not in room {}", user_id, room_id);
        return Err("User not in room");
    }

    let rooms_states_mutex = request.get::<Write<RoomsStatesMap>>().unwrap();
    let mut rooms_states_for_users = rooms_states_mutex.lock().unwrap();

    if !rooms_states_for_users.contains_key(&room_id) {
        rooms_states_for_users.insert(room_id.clone(), HashMap::new());
    }

    let room_states_for_users = rooms_states_for_users.get_mut(&room_id).unwrap();
    for (_, membership_with_room_states) in room_states_for_users {
        let &(membership_state, _) = users_in_room.get(user_id).unwrap();
        let room_states = membership_with_room_states;
        if membership_state == MembershipState::Join {
            room_states.insert(state_key.clone(), state_value.clone());
        }
    }

    Ok(())
}

fn get_state_from_room(
    request: &mut Request,
    room_id: RoomId,
    user_id: UserId,
    state_key: String,
) -> Result<Option<(String, String)>, &'static str> {
    debug!(DEFAULT_LOGGER, "Matrix mock server gets room state {}", state_key);

    let mutex = request.get::<Write<RoomsStatesMap>>().unwrap();
    let mut rooms_states_for_users = mutex.lock().unwrap();

    let users_with_room_states = match rooms_states_for_users.get_mut(&room_id) {
        Some(users_with_room_states) => users_with_room_states,
        None => {
            return Ok(None);
        }
    };

    let room_states: &mut HashMap<String, String> = match users_with_room_states.get_mut(&user_id) {
        Some(room_states) => room_states,
        None => {
            debug!(DEFAULT_LOGGER, "Matrix mock server: User {} not in room {}", user_id, room_id);
            return Err("User not in room");
        }
    };

    let room_state = match room_states.get(&state_key) {
        Some(room_state) => room_state,
        None => {
            return Ok(None);
        }
    };

    debug!(DEFAULT_LOGGER, "Matrix mock server found state {} for key {}", room_state, state_key);
    Ok(Some((state_key.clone(), room_state.to_string())))
}

fn add_membership_event_to_room(
    request: &mut Request,
    user_id: UserId,
    room_id: RoomId,
    membership_state: MembershipState,
) -> Result<(), &'static str> {
    let mutex = request.get::<Write<UsersInRooms>>().unwrap();
    let mut users_in_rooms = mutex.lock().unwrap();
    let empty_users_in_room = HashMap::new();

    if !users_in_rooms.contains_key(&room_id) {
        users_in_rooms.insert(room_id.clone(), empty_users_in_room);
    }

    let users_in_room_for_users = users_in_rooms.get_mut(&room_id).unwrap();

    for (id, membership_with_room_states) in users_in_room_for_users.iter() {
        let &(membership, _) = membership_with_room_states;
        if id == &user_id && membership == membership_state {
            match membership_state {
                MembershipState::Join => return Err("User is already in room"),
                MembershipState::Leave => return Err("User not in room"),
                _ => return Err("Unknown membership state"),
            }
        }
    }

    // Membership events have to be tracked differently for users that are currently in the room and
    // for users that left the room. Only users that are in the room get state updates. So the
    // state events are manages separately for each user.
    // In case new user joined the membership events of another user that is already in the room are
    // copied.
    let mut existing_users = Vec::new();
    let mut existing_user_in_room = None;
    for (id, membership_with_users) in users_in_room_for_users.iter() {
        let &(membership, ref users) = membership_with_users;
        if membership == MembershipState::Join {
            existing_users = users.clone();
            existing_user_in_room = Some(id.clone());
            break;
        }
    }

    let mut existing_states = HashMap::new();
    if membership_state == MembershipState::Join {
        let rooms_states_mutex = request.get::<Write<RoomsStatesMap>>().unwrap();
        let mut rooms_states_for_users = rooms_states_mutex.lock().unwrap();
        if !rooms_states_for_users.contains_key(&room_id) {
            rooms_states_for_users.insert(room_id.clone(), HashMap::new());
        }
        let room_states_for_users = rooms_states_for_users.get_mut(&room_id).unwrap();

        if let Some(existing_user_in_room) = existing_user_in_room {
            existing_states = room_states_for_users.get(&existing_user_in_room).unwrap_or(&HashMap::new()).clone();
        }

        room_states_for_users.insert(user_id.clone(), existing_states);
    }

    if !users_in_room_for_users.contains_key(&user_id) {
        users_in_room_for_users.insert(user_id.clone(), (membership_state.clone(), existing_users));
    }

    // update the user's own membership state
    let users = users_in_room_for_users.get(&user_id).unwrap().1.clone();
    users_in_room_for_users.insert(user_id.clone(), (membership_state, users));

    // update the memberships state for all users that are currently in the room
    for (current_user_id, membership_with_users) in users_in_room_for_users {
        let &mut (ref mut membership, ref mut users) = membership_with_users;
        // the room state of the current user has to be updated as well, otherwise
        // the leave event for that user will be missing.
        if membership == &MembershipState::Join || current_user_id == &user_id {
            users.retain(|&(ref id, _)| id != &user_id);
            users.push((user_id.clone(), membership_state.clone()));
        }
    }

    Ok(())
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

    let aliases = room_alias_map.get_mut(&room_id).unwrap();

    debug!(DEFAULT_LOGGER, "Matrix mock server adds alias {} to room {}", room_alias, room_id);
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

fn add_pending_invite(
    pending_invites_for_rooms: &mut MutexGuard<HashMap<RoomId, HashMap<UserId, UserId>>>,
    room_id: RoomId,
    inviter_id: UserId,
    invitee_id: UserId,
) {
    let empty_pending_invites_for_room = HashMap::new();

    if !pending_invites_for_rooms.contains_key(&room_id) {
        pending_invites_for_rooms.insert(room_id.clone(), empty_pending_invites_for_room);
    }

    let pending_invites_for_room = pending_invites_for_rooms.get_mut(&room_id).unwrap();

    pending_invites_for_room.insert(invitee_id, inviter_id);
}

fn user_id_from_request(request: &mut Request) -> UserId {
    let url: Url = request.url.clone().into();
    let mut query_pairs = url.query_pairs();
    let (_, user_id_param) = query_pairs
        .find(|&(ref key, _)| key == "user_id")
        .unwrap_or((Cow::from("user_id"), Cow::from("@rocketchat:localhost")));
    UserId::try_from(user_id_param.as_ref()).unwrap()
}

fn access_token_from_request(request: &mut Request) -> String {
    let url: Url = request.url.clone().into();
    let mut query_pairs = url.query_pairs();
    let (_, access_token) = query_pairs.find(|&(ref key, _)| key == "access_token").unwrap_or_default();
    access_token.to_string()
}

fn room_id_from_request(request: &mut Request) -> RoomId {
    let params = request.extensions.get::<Router>().unwrap().clone();
    let url_room_id = params.find("room_id").unwrap();
    let decoded_room_id = percent_decode(url_room_id.as_bytes()).decode_utf8().unwrap();
    RoomId::try_from(decoded_room_id.as_ref()).unwrap()
}
