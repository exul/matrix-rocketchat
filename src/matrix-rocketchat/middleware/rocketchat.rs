use std::io::Read;

use iron::{BeforeMiddleware, IronResult, Request};
use serde_json;

use api::rocketchat::Message;
use db::{ConnectionPool, RocketchatServer};
use errors::*;
use log::*;

/// Compares the supplied access token to the one that is in the config
pub struct RocketchatToken {}

impl BeforeMiddleware for RocketchatToken {
    fn before(&self, request: &mut Request) -> IronResult<()> {
        let logger = IronLogger::from_request(request)?;
        let mut payload = String::new();
        request.body.read_to_string(&mut payload).chain_err(|| ErrorKind::InternalServerError).map_err(Error::from)?;
        let message = match serde_json::from_str::<Message>(&payload) {
            Ok(message) => message,
            Err(err) => {
                let msg = format!("Could not deserialize message that was sent to the rocketchat endpoint: `{}`", payload);
                let json_err = simple_error!(ErrorKind::InvalidJSON(msg));
                error!(logger, "{}", err);
                return Err(json_err.into());
            }
        };

        let token = match message.token.clone() {
            Some(token) => token,
            None => {
                let err = simple_error!(ErrorKind::MissingRocketchatToken);
                info!(logger, "{}", err);
                return Err(err.into());
            }
        };

        let connection = ConnectionPool::from_request(request)?;
        let rocketchat_server = match RocketchatServer::find_by_token(&connection, &token)? {
            Some(rocketchat_server) => rocketchat_server,
            None => {
                let err = simple_error!(ErrorKind::InvalidRocketchatToken(token));
                info!(logger, "{}", err);
                return Err(err.into());
            }
        };

        request.extensions.insert::<Message>(message);
        request.extensions.insert::<RocketchatServer>(rocketchat_server);

        Ok(())
    }
}
