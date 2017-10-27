use iron::{status, Handler};
use iron::prelude::*;

use i18n::*;

/// The welcome page
pub struct Welcome {}

impl Handler for Welcome {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        Ok(Response::with((status::Ok, t!(["handlers", "welcome"]).l(DEFAULT_LANGUAGE))))
    }
}
