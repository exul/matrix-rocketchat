use std::io::Read;

use iron::{Handler, status};
use iron::prelude::*;
use iron::request::Body;
use serde_json;

use i18n::*;
use api::MatrixApi;
use config::Config;
use db::{ConnectionPool, RocketchatServer};
use errors::*;
use handlers::ErrorNotifier;
use handlers::rocketchat::{Credentials, Login};
use log::IronLogger;

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
        debug!(logger, "Received login command via REST API");

        let connection = ConnectionPool::from_request(request)?;
        let credentials = deserialize_credentials(&mut request.body)?;
        let login = Login {
            config: &self.config,
            connection: &connection,
            logger: &logger,
            matrix_api: self.matrix_api.as_ref(),
        };
        let rocketchat_server = match RocketchatServer::find_by_url(&connection, credentials.rocketchat_url.clone())? {
            Some(rocketchat_server) => rocketchat_server,
            None => {
                return Err(user_error!(
                    ErrorKind::AdminRoomForRocketchatServerNotFound(credentials.rocketchat_url.clone()),
                    t!(["errors", "admin_room_for_rocketchat_server_not_found"]).with_vars(vec![
                        (
                            "rocketchat_url",
                            credentials.rocketchat_url.clone()
                        ),
                    ])
                ))?;
            }
        };

        if let Err(err) = login.call(&credentials, &rocketchat_server) {
            if let Some(admin_room) = rocketchat_server.admin_room_for_user(&connection, &credentials.matrix_user_id)? {
                let error_notifier = ErrorNotifier {
                    config: &self.config,
                    connection: &connection,
                    logger: &logger,
                    matrix_api: self.matrix_api.as_ref(),
                };
                error_notifier.send_message_to_user(&err, admin_room.matrix_room_id.clone(), &credentials.matrix_user_id)?;
            }
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
