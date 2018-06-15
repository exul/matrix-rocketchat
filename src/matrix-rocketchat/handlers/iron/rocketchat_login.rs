use std::io::Read;

use iron::prelude::*;
use iron::request::Body;
use iron::{status, Handler};
use serde_json;

use api::MatrixApi;
use config::Config;
use errors::*;
use i18n::*;
use log::IronLogger;
use models::{ConnectionPool, Credentials, RocketchatServer};

/// `RocketchatLogin` is an endpoint that allows a user to login to Rocket.Chat via REST API.
pub struct RocketchatLogin {
    /// Application service configuration
    pub config: Config,
    /// Matrix REST API
    pub matrix_api: Box<MatrixApi>,
}

impl Handler for RocketchatLogin {
    fn handle(&self, request: &mut Request) -> IronResult<Response> {
        let logger = IronLogger::from_request(request)?;
        info!(logger, "Received login command via REST API");

        let connection = ConnectionPool::from_request(request)?;
        let credentials = deserialize_credentials(&mut request.body)?;
        let server = match RocketchatServer::find_by_url(&connection, &credentials.rocketchat_url)? {
            Some(server) => server,
            None => {
                return Err(user_error!(
                    ErrorKind::AdminRoomForRocketchatServerNotFound(credentials.rocketchat_url.clone()),
                    t!(["errors", "rocketchat_server_not_found"])
                        .with_vars(vec![("rocketchat_url", credentials.rocketchat_url.clone())])
                ))?;
            }
        };

        if let Err(err) = server.login(&self.config, &connection, &logger, self.matrix_api.as_ref(), &credentials, None) {
            return Err(err)?;
        }

        Ok(Response::with((status::Ok, t!(["handlers", "rocketchat_login_successful"]).l(DEFAULT_LANGUAGE))))
    }
}

fn deserialize_credentials(body: &mut Body) -> Result<Credentials> {
    let mut payload = String::new();
    body.read_to_string(&mut payload).chain_err(|| ErrorKind::InternalServerError)?;
    serde_json::from_str(&payload)
        .chain_err(|| ErrorKind::InvalidJSON(format!("Could not deserialize login request: `{}`", payload)))
        .map_err(Error::from)
}
