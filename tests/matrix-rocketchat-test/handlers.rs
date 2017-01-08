use iron::prelude::*;
use iron::{Handler, status};
use matrix_rocketchat::errors::MatrixErrorResponse;
use ruma_client_api::r0::sync::get_member_events;
use ruma_events::EventType;
use ruma_events::room::member::{MemberEvent, MemberEventContent, MembershipState};
use ruma_identifiers::{EventId, RoomId, UserId};
use serde_json;

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

        let response = get_member_events::Response { chunks: member_events };
        let payload = serde_json::to_string(&response).expect("Could not serialize members response");
        Ok(Response::with((status::Ok, payload)))
    }
}

pub struct EmptyJson {}

impl Handler for EmptyJson {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        Ok(Response::with((status::Ok, "{}")))
    }
}

pub struct ErrorResponse {
    pub status: status::Status,
}

impl Handler for ErrorResponse {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        let error_response = MatrixErrorResponse {
            errcode: "1234".to_string(),
            error: "An error occurred".to_string(),
        };
        let payload = serde_json::to_string(&error_response).expect("Could not serialize error response");
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
