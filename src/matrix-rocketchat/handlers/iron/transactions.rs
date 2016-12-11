use std::io::Read;

use iron::{Handler, status};
use iron::prelude::*;
use serde_json;

use config::Config;
use db::ConnectionPool;
use errors::*;
use handlers::events::EventDispatcher;
use log::IronLogger;
use middleware::AccessToken;
use models::Events;

/// Transactions is an endpoint of the application service API which is called by the homeserver
/// to push new events.
pub struct Transactions {}

impl Transactions {
    /// Transactions endpoint with middleware
    pub fn chain(config: Config) -> Chain {
        let transactions = Transactions {};
        let mut chain = Chain::new(transactions);
        chain.link_before(AccessToken { config: config });

        chain
    }
}

impl Handler for Transactions {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        let logger = IronLogger::from_request(request)?;

        let mut payload = String::new();
        if let Err(err) = request.body.read_to_string(&mut payload).chain_err(|| ErrorKind::InternalServerError) {
            error!(logger, format!("{:?}", err));
            return Err(err.into());
        };

        let events_batch: Events = match serde_json::from_str(&payload).chain_err(|| ErrorKind::InvalidJSON) {
            Ok(events_batch) => events_batch,
            Err(err) => {
                error!(logger, format!("{:?}", err));
                return Err(err.into());
            }
        };

        let connection = ConnectionPool::get_from_request(request)?;

        EventDispatcher::new(&connection, logger).process(events_batch.events)?;

        Ok(Response::with((status::Ok, "{}".to_string())))
    }
}
