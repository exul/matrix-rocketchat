use iron::{BeforeMiddleware, Handler, status};
use iron::prelude::*;
use iron::typemap::Key;
use std::io::Read;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};

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
        let mut payload = String::new();
        request.body.read_to_string(&mut payload).unwrap();
        self.tx.lock().unwrap().send(payload).unwrap();

        Ok(Response::with((status::Ok, "{}".to_string())))
    }
}

impl BeforeMiddleware for MessageForwarder {
    fn before(&self, request: &mut Request) -> IronResult<()> {
        let mut payload = String::new();
        request.body.read_to_string(&mut payload).unwrap();
        let message = Message { payload: payload.clone() };
        request.extensions.insert::<Message>(message);
        self.tx.lock().unwrap().send(payload).unwrap();

        Ok(())
    }
}

impl Key for Message {
    type Value = Message;
}
