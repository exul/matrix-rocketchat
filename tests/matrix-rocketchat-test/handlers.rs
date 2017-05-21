use rand::{Rng, thread_rng};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::io::Read;
use std::sync::mpsc::Receiver;

use iron::prelude::*;
use iron::url::Url;
use iron::url::percent_encoding::percent_decode;
use iron::{Chain, Handler, status};
use matrix_rocketchat::errors::{MatrixErrorResponse, RocketchatErrorResponse};
use persistent::Write;
use router::Router;
use ruma_client_api::r0::account::register;
use ruma_client_api::r0::room::create_room;
use ruma_client_api::r0::sync::get_member_events;
use ruma_events::EventType;
use ruma_events::room::member::{MemberEvent, MemberEventContent, MembershipState};
use ruma_identifiers::{EventId, RoomId, UserId};
use serde_json;
use super::{Message, MessageForwarder, UsernameList, helpers};

#[derive(Serialize)]
pub struct RocketchatInfo {
    pub version: &'static str,
}

impl Handler for RocketchatInfo {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
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

        let (status, payload) = match self.successful {
            true => {
                let user_id: String =
                    self.rocketchat_user_id.clone().unwrap_or(thread_rng().gen_ascii_chars().take(10).collect());
                (status::Ok,
                 r#"{
                    "status": "success",
                    "data": {
                        "authToken": "spec_auth_token",
                        "userId": "USER_ID"
                    }
                 }"#
                         .replace("USER_ID", &user_id))
            }
            false => {
                (status::Unauthorized,
                 r#"{
                    "status": "error",
                    "message": "Unauthorized"
                }"#
                         .to_string())
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

pub struct RocketchatUsersInfo {}

impl Handler for RocketchatUsersInfo {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        let url: Url = request.url.clone().into();
        let mut query_pairs = url.query_pairs();

        let (status, payload) = match query_pairs.find(|&(ref key, _)| key == "username") {
            Some((_, ref username)) => {
                (status::Ok,
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
                         .replace("USERNAME", username))
            }
            None => {
                (status::BadRequest,
                 r#"{
                    "success": false,
                    "error": "The required \"userId\" or \"username\" param was not provided [error-user-param-not-provided]",
                    "errorType": "error-user-param-not-provided"
                    }"#
                         .to_string())
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
        let payload = serde_json::to_string(self).unwrap();
        Ok(Response::with((status::Ok, payload)))
    }
}

pub struct MatrixRegister {}

impl Handler for MatrixRegister {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
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

pub struct MatrixCreateRoom {}

impl MatrixCreateRoom {
    /// Create a `MatrixCreateRoom` handler with a message forwarder middleware.
    pub fn with_forwarder() -> (Chain, Receiver<String>) {
        let (message_forwarder, receiver) = MessageForwarder::new();
        let mut chain = Chain::new(MatrixCreateRoom {});
        chain.link_before(message_forwarder);;
        (chain, receiver)
    }
}

impl Handler for MatrixCreateRoom {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        let request_payload = extract_payload(request);
        let create_room_payload: create_room::BodyParams = serde_json::from_str(&request_payload).unwrap();

        let room_id = RoomId::try_from(&format!("!{}_id:localhost", create_room_payload.name.unwrap())).unwrap();
        let response = create_room::Response { room_id: room_id };
        let payload = serde_json::to_string(&response).unwrap();
        Ok(Response::with((status::Ok, payload)))
    }
}


pub struct RoomMembers {
    pub members: Vec<UserId>,
    pub room_id: RoomId,
}

impl Handler for RoomMembers {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        let mut member_events = Vec::with_capacity(2);
        for member in self.members.iter() {
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
                room_id: self.room_id.clone(),
                state_key: member.to_string(),
                unsigned: None,
                user_id: member.clone(),
            };
            member_events.push(member_event);
        }

        let response = get_member_events::Response { chunk: member_events };
        let payload = serde_json::to_string(&response).unwrap();
        Ok(Response::with((status::Ok, payload)))
    }
}

pub struct RoomStateCreate {
    pub creator: UserId,
}

impl Handler for RoomStateCreate {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        let mut values = serde_json::Map::new();
        values.insert("creator".to_string(), serde_json::Value::String(self.creator.to_string()));
        let payload = serde_json::to_string(&values).unwrap();
        Ok(Response::with((status::Ok, payload)))
    }
}

pub struct MatrixLeaveRoom {
    pub as_url: String,
    pub user_id: UserId,
}

impl MatrixLeaveRoom {
    pub fn with_forwarder(as_url: String, user_id: UserId) -> (Chain, Receiver<String>) {
        let (message_forwarder, receiver) = MessageForwarder::new();
        let mut chain = Chain::new(MatrixLeaveRoom {
                                       as_url: as_url,
                                       user_id: user_id,
                                   });
        chain.link_before(message_forwarder);;
        (chain, receiver)
    }
}

impl Handler for MatrixLeaveRoom {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        let params = request.extensions.get::<Router>().unwrap().clone();
        let url_room_id = params.find("room_id").unwrap();
        let decoded_room_id = percent_decode(url_room_id.as_bytes()).decode_utf8().unwrap();
        let room_id = RoomId::try_from(&decoded_room_id).unwrap();

        helpers::leave_room(&self.as_url, room_id, self.user_id.clone());

        Ok(Response::with((status::Ok, "{}")))
    }
}

pub struct EmptyJson {}

impl Handler for EmptyJson {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        Ok(Response::with((status::Ok, "{}")))
    }
}

pub struct MatrixErrorResponder {
    pub status: status::Status,
    pub message: String,
}

impl Handler for MatrixErrorResponder {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
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

impl Handler for MatrixConditionalErrorResponder {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
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

pub struct InvalidJsonResponse {
    pub status: status::Status,
}

impl Handler for InvalidJsonResponse {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        Ok(Response::with((self.status, "invalid json")))
    }
}

fn extract_payload(request: &mut Request) -> String {
    let mut payload = String::new();
    request.body.read_to_string(&mut payload).unwrap();

    // if the request payload is empty, try to get it from the middleware
    if payload.is_empty() {
        if let Some(message) = request.extensions.get::<Message>() {
            payload = message.payload.clone()
        }
    }

    payload
}
