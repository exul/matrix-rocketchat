use std::collections::HashMap;
use std::convert::TryFrom;

use diesel::sqlite::SqliteConnection;
use matrix_rocketchat::Config;
use matrix_rocketchat::api::{MatrixApi, RestApi};
use matrix_rocketchat::models::Events;
use matrix_rocketchat::models::UserOnRocketchatServer;
use reqwest::{Method, StatusCode};
use ruma_events::EventType;
use ruma_events::collections::all::Event;
use ruma_events::room::member::{MemberEvent, MemberEventContent, MembershipState};
use ruma_events::room::message::{MessageEvent, MessageEventContent, MessageType, TextMessageEventContent};
use ruma_identifiers::{EventId, RoomId, UserId};
use serde_json::{to_string, Map, Value};
use super::{DEFAULT_LOGGER, HS_TOKEN};

pub fn invite(config: &Config, room_id: RoomId, user_id: UserId, sender_id: UserId) {
    let matrix_api = MatrixApi::new(config, DEFAULT_LOGGER.clone()).unwrap();
    matrix_api.invite(room_id, user_id, sender_id).unwrap();
}

pub fn join(config: &Config, room_id: RoomId, user_id: UserId) {
    let matrix_api = MatrixApi::new(config, DEFAULT_LOGGER.clone()).unwrap();
    matrix_api.join(room_id, user_id).unwrap();
}

pub fn create_room(config: &Config, room_name: &str, sender_id: UserId, user_id: UserId) {
    let matrix_api = MatrixApi::new(&config, DEFAULT_LOGGER.clone()).unwrap();
    matrix_api
        .create_room(Some(room_name.to_string()), None, &sender_id)
        .unwrap();

    let room_id = RoomId::try_from(&format!("!{}_id:localhost", room_name)).unwrap();
    invite(&config, room_id, user_id, sender_id);
}

pub fn send_invite_event_from_matrix(as_url: &str, room_id: RoomId, user_id: UserId, inviter_id: UserId) {
    let invite_event = MemberEvent {
        content: MemberEventContent {
            avatar_url: None,
            displayname: None,
            membership: MembershipState::Invite,
            third_party_invite: None,
        },
        event_id: EventId::new("localhost").unwrap(),
        event_type: EventType::RoomMember,
        invite_room_state: None,
        prev_content: None,
        room_id: room_id.clone(),
        state_key: format!("{}", user_id),
        unsigned: None,
        user_id: inviter_id,
    };

    let events = Events {
        events: vec![Box::new(Event::RoomMember(invite_event))],
    };

    let invite_payload = to_string(&events).unwrap();

    simulate_message_from_matrix(as_url, &invite_payload);
}

pub fn send_join_event_from_matrix(as_url: &str, room_id: RoomId, user_id: UserId, inviter_id: Option<UserId>) {
    let mut unsigned: Option<Value> = None;

    if let Some(inviter_id) = inviter_id {
        let mut unsigned_content = Map::new();
        unsigned_content.insert("prev_sender".to_string(), Value::String(inviter_id.to_string()));
        unsigned = Some(Value::Object(unsigned_content))
    }

    let join_event = MemberEvent {
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
        room_id: room_id,
        state_key: format!("{}", &user_id),
        unsigned: unsigned,
        user_id: user_id,
    };

    let events = Events {
        events: vec![Box::new(Event::RoomMember(join_event))],
    };
    let join_payload = to_string(&events).unwrap();
    simulate_message_from_matrix(&as_url, &join_payload);
}

pub fn leave_room(config: &Config, room_id: RoomId, user_id: UserId) {
    let matrix_api = MatrixApi::new(config, DEFAULT_LOGGER.clone()).unwrap();
    matrix_api.leave_room(room_id, user_id).unwrap();
}

pub fn send_leave_event_from_matrix(as_url: &str, room_id: RoomId, user_id: UserId) {
    let leave_event = MemberEvent {
        content: MemberEventContent {
            avatar_url: None,
            displayname: None,
            membership: MembershipState::Leave,
            third_party_invite: None,
        },
        event_id: EventId::new("localhost").unwrap(),
        event_type: EventType::RoomMember,
        invite_room_state: None,
        prev_content: None,
        room_id: room_id,
        state_key: format!("{}", user_id),
        unsigned: None,
        user_id: user_id,
    };

    let events = Events {
        events: vec![Box::new(Event::RoomMember(leave_event))],
    };
    let leave_payload = to_string(&events).unwrap();
    simulate_message_from_matrix(as_url, &leave_payload);
}

pub fn send_room_message_from_matrix(as_url: &str, room_id: RoomId, user_id: UserId, body: String) {
    let message_event = MessageEvent {
        content: MessageEventContent::Text(TextMessageEventContent {
            body: body,
            msgtype: MessageType::Text,
        }),
        event_id: EventId::new("localhost").unwrap(),
        event_type: EventType::RoomMessage,
        room_id: room_id,
        unsigned: None,
        user_id: user_id,
    };

    let events = Events {
        events: vec![Box::new(Event::RoomMessage(message_event))],
    };
    let payload = to_string(&events).unwrap();

    simulate_message_from_matrix(as_url, &payload);
}

pub fn send_emote_message_from_matrix(as_url: &str, room_id: RoomId, user_id: UserId, body: String) {
    let message_event = MessageEvent {
        content: MessageEventContent::Text(TextMessageEventContent {
            body: body,
            msgtype: MessageType::Emote,
        }),
        event_id: EventId::new("localhost").unwrap(),
        event_type: EventType::RoomMessage,
        room_id: room_id,
        unsigned: None,
        user_id: user_id,
    };

    let events = Events {
        events: vec![Box::new(Event::RoomMessage(message_event))],
    };
    let payload = to_string(&events).unwrap();

    simulate_message_from_matrix(as_url, &payload);
}

pub fn simulate_message_from_matrix(as_url: &str, payload: &str) -> (String, StatusCode) {
    let url = format!("{}/transactions/{}", as_url, "specid");
    let mut params = HashMap::new();
    params.insert("access_token", HS_TOKEN);
    RestApi::call(Method::Put, &url, payload, &params, None).unwrap()
}

pub fn simulate_message_from_rocketchat(as_url: &str, payload: &str) -> (String, StatusCode) {
    let url = format!("{}/rocketchat", as_url);
    let params = HashMap::new();
    RestApi::call(Method::Post, &url, payload, &params, None).unwrap()
}

pub fn logout_user_from_rocketchat_server_on_bridge(
    connection: &SqliteConnection,
    rocketchat_server_id: String,
    user_id: &UserId,
) {
    let mut user_on_rocketchat_server = UserOnRocketchatServer::find(connection, &user_id, rocketchat_server_id).unwrap();
    let rocketchat_user_id = user_on_rocketchat_server.rocketchat_user_id.clone();
    user_on_rocketchat_server
        .set_credentials(connection, rocketchat_user_id, None)
        .unwrap();
}
