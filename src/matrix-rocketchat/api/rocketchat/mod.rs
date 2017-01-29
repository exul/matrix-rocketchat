use std::collections::HashMap;

use reqwest::Method;
use serde_json;
use slog::Logger;

use api::RestApi;
use errors::*;
use i18n::*;

mod v1;

/// Rocket.Chat REST API
pub trait RocketchatApi {}

/// Response format when querying the Rocket.Chat info endpoint
#[derive(Deserialize, Serialize)]
pub struct GetInfoResponse {
    version: String,
}

impl RocketchatApi {
    /// Creates a new Rocket.Chat API depending on the version of the API.
    /// It returns a `RocketchatApi` trait, because for each version a different API is created.
    pub fn new(base_url: String, access_token: Option<String>, logger: Logger) -> Result<Box<RocketchatApi>> {
        let url = base_url.clone() + "/api/info";
        let params = HashMap::new();

        debug!(logger, format!("Querying Rocket.Chat server {} for API versions", url));

        let (body, status_code) = match RestApi::call(Method::Get, &url, "", &params, None) {
            Ok((body, status_code)) => (body, status_code),
            Err(err) => {
                debug!(logger, err);
                bail!(ErrorKind::RocketchatServerUnreachable(url));
            }
        };

        if !status_code.is_success() {
            let mut keys = HashMap::new();
            keys.insert("rocketchat_url", url.clone());
            return Err(Error {
                error_chain: ErrorKind::NoRocketchatServer(url).into(),
                user_message: Some((t!(["errors", "no_rocketchat_server"]), keys)),
            });
        }

        let rocketchat_info: GetInfoResponse = match serde_json::from_str(&body)
            .chain_err(|| ErrorKind::NoRocketchatServer(url.clone())) {
            Ok(rocketchat_info) => rocketchat_info,
            Err(err) => {
                let mut keys = HashMap::new();
                keys.insert("rocketchat_url", url);
                return Err(Error {
                    error_chain: err,
                    user_message: Some((t!(["errors", "no_rocketchat_server"]), keys)),
                });
            }
        };

        debug!(logger, format!("Rocket.Chat version {:?}", rocketchat_info.version));

        RocketchatApi::get_max_supported_version_api(rocketchat_info.version, base_url, access_token, logger)
    }

    fn get_max_supported_version_api(version: String,
                                     base_url: String,
                                     access_token: Option<String>,
                                     logger: Logger)
                                     -> Result<Box<RocketchatApi>> {
        let version_string = version.clone();
        let mut versions = version_string.split('.').into_iter();
        let major: i32 = versions.next().unwrap_or("0").parse().unwrap_or(0);
        let minor: i32 = versions.next().unwrap_or("0").parse().unwrap_or(0);

        if major == 0 && minor >= 49 {
            let rocketchat_api = v1::RocketchatApi::new(base_url, access_token, logger);
            return Ok(Box::new(rocketchat_api));
        }

        Err(simple_error!(ErrorKind::UnsupportedRocketchatApiVersion("0.49".to_string(), version)))
    }
}
