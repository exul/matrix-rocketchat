use std::borrow::Cow;
use std::convert::TryFrom;
use std::collections::HashMap;
use std::io::Read;
use std::sync::Mutex;
use std::sync::mpsc::{channel, Receiver, Sender};

use iron::{status, AfterMiddleware, BeforeMiddleware, Handler};
use iron::prelude::*;
use iron::typemap::Key;
use iron::url::Url;
use iron::url::percent_encoding::percent_decode;
use matrix_rocketchat::errors::MatrixErrorResponse;
use persistent::Write;
use router::Router;
use ruma_events::room::member::MembershipState;
use ruma_identifiers::{RoomId, UserId};
use serde_json;

use super::{extract_payload, TestError, UsersInRooms, DEFAULT_LOGGER};

/// Forwards a message from an iron handler to a channel so that it can be received outside of the
/// iron handler.
pub struct MessageForwarder {
    tx: Mutex<Sender<String>>,
    path_filter: Option<&'static str>,
}

/// An wrapper type that is used to store
pub struct Message {
    pub payload: String,
}

impl MessageForwarder {
    /// Creates a new MessageForwarder and a receiver. The MessageForwarder can be passed to the
    /// iron router while the receiver is used to read the message that gets forwarded.
    pub fn new() -> (MessageForwarder, Receiver<String>) {
        MessageForwarder::build(None)
    }

    pub fn with_path_filter(path_filter: &'static str) -> (MessageForwarder, Receiver<String>) {
        MessageForwarder::build(Some(path_filter))
    }

    fn build(path_filter: Option<&'static str>) -> (MessageForwarder, Receiver<String>) {
        let (tx, rx) = channel::<String>();
        let message_forwarder = MessageForwarder {
            tx: Mutex::new(tx),
            path_filter: path_filter,
        };
        (message_forwarder, rx)
    }
}

impl Handler for MessageForwarder {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        let url: Url = request.url.clone().into();

        if let Some(path_filter) = self.path_filter {
            if !url.path().contains(path_filter) {
                debug!(DEFAULT_LOGGER, "Dropping message, it was sent to {} path filter is {}", url.path(), path_filter);
                return Ok(Response::with((status::Ok, "{}".to_string())));
            }
        }

        // endpoints that are changing the room state are only accessible if the user is in
        // the room, except for the forget endpoint.
        let is_restricted_room_endpoint = url.path().contains("/rooms/") && !url.path().contains("forget");
        if is_restricted_room_endpoint {
            validate_message_forwarding_for_user(request, url)?;
        }

        let mut payload = String::new();
        request.body.read_to_string(&mut payload).unwrap();
        self.tx.lock().unwrap().send(payload).unwrap();

        Ok(Response::with((status::Ok, "{}".to_string())))
    }
}

impl BeforeMiddleware for MessageForwarder {
    fn before(&self, request: &mut Request) -> IronResult<()> {
        let payload = extract_payload(request);
        self.tx.lock().unwrap().send(payload).unwrap();

        Ok(())
    }
}

impl AfterMiddleware for MessageForwarder {
    fn after(&self, request: &mut Request, response: Response) -> IronResult<Response> {
        let payload = extract_payload(request);
        self.tx.lock().unwrap().send(payload).unwrap();

        Ok(response)
    }
}

impl Key for Message {
    type Value = Message;
}

fn validate_message_forwarding_for_user(request: &mut Request, url: Url) -> IronResult<()> {
    let params = request.extensions.get::<Router>().unwrap().clone();
    let url_room_id = params.find("room_id").unwrap();
    let decoded_room_id = percent_decode(url_room_id.as_bytes()).decode_utf8().unwrap();
    let room_id = RoomId::try_from(decoded_room_id.as_ref()).unwrap();

    let mut query_pairs = url.query_pairs();
    let (_, user_id_param) = query_pairs
        .find(|&(ref key, _)| key == "user_id")
        .unwrap_or((Cow::from("user_id"), Cow::from("@rocketchat:localhost")));
    let user_id = UserId::try_from(user_id_param.as_ref()).unwrap();
    let mutex = request.get::<Write<UsersInRooms>>().unwrap();
    let users_in_rooms = mutex.lock().unwrap();
    let empty_users = HashMap::new();
    let users_in_room = &users_in_rooms.get(&room_id).unwrap_or(&empty_users);

    if !users_in_room.iter().any(|(id, &(membership, _))| id == &user_id && membership == MembershipState::Join) {
        let matrix_err = MatrixErrorResponse {
            errcode: "M_FORBIDDEN".to_string(),
            error: format!("{} not in room {}", user_id, room_id),
        };

        let err_payload = serde_json::to_string(&matrix_err).unwrap();
        let err = IronError::new(TestError("Send message error".to_string()), (status::Forbidden, err_payload));
        return Err(err);
    }

    Ok(())
}
