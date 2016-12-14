use std::io::Read;

use iron::{Handler, status};
use iron::prelude::*;
use serde_json;

use api::MatrixApi;
use config::Config;
use db::ConnectionPool;
use errors::*;
use handlers::events::EventDispatcher;
use log::IronLogger;
use middleware::AccessToken;
use models::Events;

/// Transactions is an endpoint of the application service API which is called by the homeserver
/// to push new events.
pub struct Transactions {
    config: Config,
    matrix_api: Box<MatrixApi>,
}

impl Transactions {
    /// Transactions endpoint with middleware
    pub fn chain(config: Config, matrix_api: Box<MatrixApi>) -> Chain {
        let transactions = Transactions {
            config: config.clone(),
            matrix_api: matrix_api,
        };
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

        let events_batch: Events = match serde_json::from_str(&payload).
            chain_err(|| ErrorKind::InvalidJSON(format!("Could not deserialize events that the homeserver sent to the transactions endpoint: `{}`", payload))) {
            Ok(events_batch) => events_batch,
            Err(err) => {
                error!(logger, format!("{:?}", err));
                return Err(err.into());
            }
        };

        let connection = ConnectionPool::get_from_request(request)?;

        if let Err(err) = EventDispatcher::new(&self.config, &connection, logger.clone(), self.matrix_api.clone())
            .process(events_batch.events) {
            error!(logger, format!("{:?}", err));
            return Err(err.into());
        }

        Ok(Response::with((status::Ok, "{}".to_string())))
    }
}
