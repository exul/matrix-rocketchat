use iron::{Handler, status};
use iron::prelude::*;

use api::MatrixApi;
use api::rocketchat::Message;
use config::Config;
use db::{ConnectionPool, RocketchatServer};
use handlers::rocketchat::Forwarder;
use log::{self, IronLogger};
use middleware::RocketchatToken;

/// Rocket.Chat is an endpoint of the application service API which is called by the Rocket.Chat
/// server to push new messages.
pub struct Rocketchat {
    /// Application service configuration
    pub config: Config,
    /// Matrix REST API
    pub matrix_api: Box<MatrixApi>,
}

impl Rocketchat {
    /// Rocket.Chat endpoint with middleware
    pub fn chain(config: Config, matrix_api: Box<MatrixApi>) -> Chain {
        let rocketchat = Rocketchat {
            config: config.clone(),
            matrix_api: matrix_api,
        };
        let mut chain = Chain::new(rocketchat);
        chain.link_before(RocketchatToken {});

        chain
    }
}

impl Handler for Rocketchat {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        let logger = IronLogger::from_request(request)?;
        let connection = ConnectionPool::from_request(request)?;

        let message =
            request.extensions.get::<Message>().expect("Middleware ensures the presence of the Rocket.Chat message");
        let rocketchat_server = request.extensions
            .get::<RocketchatServer>()
            .expect("Middleware ensures the presence of the Rocket.Chat server");

        let forwarder = Forwarder {
            config: &self.config,
            connection: &connection,
            matrix_api: &self.matrix_api,
            logger: &logger,
        };

        if let Err(err) = forwarder.send(rocketchat_server, message) {
            log::log_error(&logger, &err);
        }

        Ok(Response::with((status::Ok, "{}".to_string())))
    }
}
