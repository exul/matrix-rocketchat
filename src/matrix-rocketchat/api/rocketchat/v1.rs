use serde_json::Map;
use slog::Logger;
use serde_json;

use api::RestApi;
use config::Config;
use errors::*;

#[derive(Clone)]
pub struct RocketchatApi {
    /// URL to call the API
    pub base_url: String,
    /// Access token for authentication
    pub access_token: Option<String>,
    /// Logger passed to the Rocketchat API
    logger: Logger,
}

impl RocketchatApi {
    pub fn new(base_url: String, access_token: Option<String>, logger: Logger) -> RocketchatApi {
        RocketchatApi {
            base_url: base_url,
            access_token: access_token,
            logger: logger,
        }
    }
}

impl super::RocketchatApi for RocketchatApi {}
