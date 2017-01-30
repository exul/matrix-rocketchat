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
                bail_error!(ErrorKind::RocketchatServerUnreachable(url.clone()),
                            t!(["errors", "rocketchat_server_unreachable"]).with_vars(vec![("rocketchat_url", url)]))
            }
        };

        if !status_code.is_success() {
            bail_error!(ErrorKind::NoRocketchatServer(url.clone()),
                        t!(["errors", "no_rocketchat_server"]).with_vars(vec![("rocketchat_url", url.clone())]));
        }

        let rocketchat_info: GetInfoResponse = match serde_json::from_str(&body)
            .chain_err(|| ErrorKind::NoRocketchatServer(url.clone())) {
            Ok(rocketchat_info) => rocketchat_info,
            Err(err) => {
                bail_error!(err,
                            t!(["errors", "no_rocketchat_server"]).with_vars(vec![("rocketchat_url", url)]))
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

        let min_version = "0.49".to_string();
        Err(Error {
            error_chain: ErrorKind::UnsupportedRocketchatApiVersion(min_version.clone(), version.clone()).into(),
            user_message: Some(t!(["errors", "unsupported_rocketchat_api_version"])
                .with_vars(vec![("min_version", min_version), ("version", version)])),
        })
    }
}
