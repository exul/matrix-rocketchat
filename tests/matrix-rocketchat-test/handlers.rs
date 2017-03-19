use std::collections::HashMap;
use rand::{Rng, thread_rng};

use iron::prelude::*;
use iron::{Handler, status};
use matrix_rocketchat::errors::{MatrixErrorResponse, RocketchatErrorResponse};
use ruma_client_api::r0::room::create_room;
use ruma_client_api::r0::sync::get_member_events;
use ruma_events::EventType;
use ruma_events::room::member::{MemberEvent, MemberEventContent, MembershipState};
use ruma_identifiers::{EventId, RoomId, UserId};
use serde_json;

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

pub struct MatrixCreateRoom {
    pub room_id: RoomId,
}

impl Handler for MatrixCreateRoom {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        let response = create_room::Response { room_id: self.room_id.clone() };
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

pub struct InvalidJsonResponse {
    pub status: status::Status,
}

impl Handler for InvalidJsonResponse {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        Ok(Response::with((self.status, "invalid json")))
    }
}
