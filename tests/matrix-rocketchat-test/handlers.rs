use iron::prelude::*;
use iron::{Handler, status};
use ruma_client_api::r0::get::members;
use ruma_events::EventType;
use ruma_events::room::member::{MemberEvent, MemberEventContent, MembershipState};
use ruma_identifiers::{EventId, RoomId, UserId};
use serde_json;

pub struct MatrixVersion {}

impl Handler for MatrixVersion {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        let payload = r#"{"versions":["r0.0.1","r0.1.0","r0.2.0"]}"#;
        Ok(Response::with((status::Ok, payload)))
    }
}

pub struct TwoRoomMembers {
    pub members: [UserId; 2],
    pub room_id: RoomId,
}

impl Handler for TwoRoomMembers {
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

        let response = members::Response { chunk: member_events };
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
