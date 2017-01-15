extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate reqwest;
extern crate ruma_client_api;
extern crate ruma_events;
extern crate ruma_identifiers;
extern crate router;
extern crate serde_json;

use std::collections::HashMap;

use matrix_rocketchat::api::RestApi;
use matrix_rocketchat::models::Events;
use matrix_rocketchat_test::{HS_TOKEN, MessageForwarder, Test, default_timeout, helpers};
use reqwest::{Method, StatusCode};
use router::Router;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::send::send_message_event::Endpoint as SendMessageEventEndpoint;
use ruma_events::EventType;
use ruma_events::call::hangup::{HangupEvent, HangupEventContent};
use ruma_events::collections::all::Event;
use ruma_identifiers::{EventId, RoomId, UserId};
use serde_json::to_string;

#[test]
fn homeserver_sends_mal_formatted_json() {
    let test = Test::new().run();
    let payload = "bad_json";

    let url = format!("{}/transactions/{}", &test.config.as_url, "specid");
    let mut params = HashMap::new();
    params.insert("access_token", HS_TOKEN);
    let (_, status_code) = RestApi::call(Method::Put, &url, payload, &mut params, None).unwrap();
    assert_eq!(status_code, StatusCode::UnprocessableEntity)
}

#[test]
fn unknown_event_types_are_skipped() {
    let (message_forwarder, receiver) = MessageForwarder::new();
    let mut matrix_router = Router::new();
    matrix_router.put(SendMessageEventEndpoint::router_path(), message_forwarder, "send_message_event");
    let test = Test::new().with_custom_matrix_routes(matrix_router).run();

    let unknown_event = HangupEvent {
        content: HangupEventContent {
            call_id: "1234".to_string(),
            version: 1,
        },
        event_id: EventId::new("localhost").unwrap(),
        event_type: EventType::CallHangup,
        room_id: RoomId::new("localhost").unwrap(),
        user_id: UserId::new("localhost").unwrap(),
        unsigned: None,
    };

    let events = Events { events: vec![Box::new(Event::CallHangup(unknown_event))] };

    let payload = to_string(&events).unwrap();

    helpers::simulate_message_from_matrix(&test.config.as_url, &payload);

    // the user does not get a message, because the event is ignored
    // so the receiver never gets a message and times out
    receiver.recv_timeout(default_timeout()).is_err();
}
