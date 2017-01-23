use std::collections::HashMap;

use reqwest::Method;
use serde_json;
use slog::Logger;

use api::RestApi;
use config::Config;
use errors::*;

mod v1;

/// Rocket.Chat REST API
pub trait RocketchatApi {}

/// Response format when querying the Rocket.Chat info endpoint
#[derive(Deserialize, Serialize)]
pub struct GetInfoResponse {
    info: InfoContent,
}

/// The content of the Rocket.Chat info response
#[derive(Deserialize, Serialize)]
pub struct InfoContent {
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
                bail!(ErrorKind::RocketchatServerUnreachable(url));
            }
        };

        if !status_code.is_success() {
            bail!(ErrorKind::NoRocketchatServer(url));
        }

        let rocketchat_info: GetInfoResponse =
            serde_json::from_str(&body).chain_err(|| ErrorKind::NoRocketchatServer(url))?;

        debug!(logger, format!("Rocket.Chat version {:?}", rocketchat_info.info.version));

        RocketchatApi::get_max_supported_version_api(rocketchat_info.info.version, base_url, access_token, logger)
    }

    fn get_max_supported_version_api(version: String,
                                     base_url: String,
                                     access_token: Option<String>,
                                     logger: Logger)
                                     -> Result<Box<RocketchatApi>> {
        let major = 0;
        let minor = 0;

        let version_string = version.clone();
        let mut versions = version_string.split(".").into_iter();
        let major: i32 = versions.next().unwrap_or("0").parse().unwrap_or(0);
        let minor: i32 = versions.next().unwrap_or("0").parse().unwrap_or(0);

        if major == 0 && minor >= 49 {
            let rocketchat_api = v1::RocketchatApi::new(base_url, access_token, logger);
            return Ok(Box::new(rocketchat_api));
        }

        Err(Error::from(ErrorKind::UnsupportedRocketchatApiVersion(version)))
    }
}
