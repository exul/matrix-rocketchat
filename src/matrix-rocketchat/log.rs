use iron::{Plugin, Request};
use iron::typemap::Key;
use persistent::State;
use slog::Logger;

use errors::*;

/// Struct to attach a logger to an iron request.
pub struct IronLogger;

impl IronLogger {
    /// Gets the logger from the request.
    pub fn from_request(request: &mut Request) -> Result<Logger> {
        let lock = request.get::<State<IronLogger>>().chain_err(|| "Could not get iron logger lock from request")?;
        let logger = lock.read().unwrap();
        Ok(logger.clone())
    }
}

impl Key for IronLogger {
    type Value = Logger;
}
