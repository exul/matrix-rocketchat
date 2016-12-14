use std::collections::HashMap;

use reqwest::StatusCode;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::membership::join_by_room_id::{self, Endpoint as JoinEndpoint};
use ruma_identifiers::{RoomId, UserId};
use slog::Logger;
use serde_json;

use api::RestApi;
use config::Config;
use errors::*;

#[derive(Clone)]
pub struct MatrixApi {
    /// URL to call the API
    pub base_url: String,
    /// Access token for authentication
    pub access_token: String,
    /// Logger passed to the Matrix API
    logger: Logger,
}

impl MatrixApi {
    pub fn new(config: &Config, logger: Logger) -> MatrixApi {
        MatrixApi {
            base_url: config.hs_url.to_string(),
            access_token: config.hs_token.to_string(),
            logger: logger,
        }
    }

    fn parameter_hash(&self) -> HashMap<&str, &str> {
        let mut parameters: HashMap<&str, &str> = HashMap::new();
        parameters.insert("access_token", &self.access_token);
        parameters
    }
}

impl super::MatrixApi for MatrixApi {
    fn join(&self, matrix_room_id: RoomId, matrix_user_id: UserId) -> Result<()> {
        let path_params = join_by_room_id::PathParams{room_id: matrix_room_id.clone()};
        let endpoint = self.base_url.clone() + &JoinEndpoint::request_path(path_params);
        let user_id = matrix_user_id.to_string();
        let mut parameters = self.parameter_hash();
        parameters.insert("user_id", &user_id);

        let (body, status_code) = RestApi::call_matrix(JoinEndpoint::method(), &endpoint, "{}")?;
        if !status_code.is_success() {
            let matrix_error_resp: MatrixErrorResponse = serde_json::from_str(&body).
                chain_err(|| ErrorKind::InvalidJSON(format!("Could not deserialize error response from Matrix join API endpoint: `{}`", body)))?;
            bail!(ErrorKind::MatrixError(matrix_error_resp.error));
        }

        debug!(self.logger, "User {} successfully joined room {}", matrix_room_id, matrix_user_id);
        Ok(())
    }
}
