use std::convert::TryFrom;
use std::borrow::{Borrow, Cow};
use std::io::Read;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};

use iron::{BeforeMiddleware, AfterMiddleware, Handler, status};
use iron::prelude::*;
use iron::typemap::Key;
use iron::url::Url;
use iron::url::percent_encoding::percent_decode;
use matrix_rocketchat::errors::MatrixErrorResponse;
use persistent::Write;
use router::Router;
use ruma_identifiers::{RoomId, UserId};
use serde_json;

use super::{TestError, UsersInRoomMap, extract_payload};

/// Forwards a message from an iron handler to a channel so that it can be received outside of the
/// iron handler.
pub struct MessageForwarder {
    tx: Mutex<Sender<String>>,
}

/// An wrapper type that is used to store
pub struct Message {
    pub payload: String,
}

impl MessageForwarder {
    /// Creates a new MessageForwarder and a receiver. The MessageForwarder can be passed to the
    /// iron router while the receiver is used to read the message that gets forwarded.
    pub fn new() -> (MessageForwarder, Receiver<String>) {
        let (tx, rx) = channel::<String>();
        let message_forwarder = MessageForwarder { tx: Mutex::new(tx) };
        (message_forwarder, rx)
    }
}

impl Handler for MessageForwarder {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        let url: Url = request.url.clone().into();

        // endpoints that are changing the room state are only accessible if the user is in
        // the room, except for the forget endpoint.
        let is_restricted_room_endpoint = url.path().contains("rooms") && !url.path().contains("forget");
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
    fn after(&self, request: &mut Request, response: Response) -> IronResult<Response>{
        let payload = extract_payload(request);
        self.tx.lock().unwrap().send(payload).unwrap();

        Ok(response)
    }
}

impl Key for Message {
    type Value = Message;
}

fn validate_message_forwarding_for_user(request: &mut Request, url: Url) -> IronResult<()>{
    let params = request.extensions.get::<Router>().unwrap().clone();
    let url_room_id = params.find("room_id").unwrap();
    let decoded_room_id = percent_decode(url_room_id.as_bytes()).decode_utf8().unwrap();
    let room_id = RoomId::try_from(&decoded_room_id).unwrap();

    let mut query_pairs = url.query_pairs();
    let (_, user_id_param) = query_pairs.find(|&(ref key, _)| key == "user_id").unwrap_or((
        Cow::from("user_id"),
        Cow::from("@rocketchat:localhost"),
    ));
    let user_id = UserId::try_from(user_id_param.borrow()).unwrap();
    let mutex = request.get::<Write<UsersInRoomMap>>().unwrap();
    let user_in_room_map = mutex.lock().unwrap();
    let empty_users = Vec::new();
    let user_ids = &user_in_room_map.get(&room_id).unwrap_or(&empty_users);

    if !user_ids.iter().any(|id| id == &user_id) {
        let matrix_err = MatrixErrorResponse{
            errcode: "M_FORBIDDEN".to_string(),
            error: format!("{} not in room {}", user_id, room_id),
        };

        let err_payload = serde_json::to_string(&matrix_err).unwrap();
        let err = IronError::new(TestError("Send message error".to_string()), (status::Forbidden, err_payload));
        return Err(err);
    }

    return Ok(())
}