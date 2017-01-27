use std::io::Read;

use iron::{Handler, status};
use iron::prelude::*;
use iron::request::Body;
use serde_json;
use slog::Logger;

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

        let events_batch = match deserialize_events(&mut request.body) {
            Ok(events_batch) => events_batch,
            Err(err) => {
                log_error(logger, &err);
                return Err(err.into());
            }
        };

        let connection = ConnectionPool::get_from_request(request)?;

        if let Err(err) = EventDispatcher::new(&self.config, &connection, logger.clone(), self.matrix_api.clone())
            .process(events_batch.events) {
            log_error(logger, &err);
        }

        Ok(Response::with((status::Ok, "{}".to_string())))
    }
}

fn deserialize_events(body: &mut Body) -> Result<Events> {
    let mut payload = String::new();
    body.read_to_string(&mut payload).chain_err(|| ErrorKind::InternalServerError)?;
    serde_json::from_str(&payload).chain_err(|| {
        ErrorKind::InvalidJSON(format!("Could not deserialize events that were sent to the transactions \
                                        endpoint: `{}`",
                                       payload))
    })
}

fn log_error(logger: Logger, err: &Error) {
    let mut msg = format!("{}", err);
    for err in err.iter().skip(1) {
        msg = msg + " caused by: " + &format!("{}", err);
    }
    error!(logger, msg);
}
