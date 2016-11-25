use iron::{Handler, status};
use iron::prelude::*;

/// The welcome page
pub struct Welcome {}

impl Handler for Welcome {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        Ok(Response::with((status::Ok, "Your Rocket.Chat <-> Matrix application service is running".to_string())))
    }
}
