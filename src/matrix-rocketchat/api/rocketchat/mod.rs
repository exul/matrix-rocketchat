use std::collections::HashMap;

use reqwest::Method;
use serde_json;
use slog::Logger;

use api::RestApi;
use config::Config;
use errors::*;

mod v1;

/// Rocket.Chat REST API
pub trait RocketchatApi: Send + Sync + RocketchatApiClone {}

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

/// Helper trait because Clone cannot be part of the `RocketchatApi` trait since that would cause the
/// `RocketchatApi` trait to not be object safe.
pub trait RocketchatApiClone {
    /// Clone the object inside the trait and return it in box.
    fn clone_box(&self) -> Box<RocketchatApi>;
}

impl<T> RocketchatApiClone for T
    where T: 'static + RocketchatApi + Clone
{
    fn clone_box(&self) -> Box<RocketchatApi> {
        Box::new(self.clone())
    }
}

impl Clone for Box<RocketchatApi> {
    fn clone(&self) -> Box<RocketchatApi> {
        self.clone_box()
    }
}

impl RocketchatApi {
    /// Creates a new Rocket.Chat API depending on the version of the API.
    /// It returns a `RocketchatApi` trait, because for each version a different API is created.
    pub fn new(base_url: String, access_token: Option<String>, logger: Logger) -> Result<Box<RocketchatApi>> {
        let url = base_url.clone() + "/api/info";
        let params = HashMap::new();

        debug!(logger, format!("Querying Rocket.Chat server {} for API versions", url));

        let (body, status_code) = RestApi::call(Method::Get, &url, "", &params, None)?;
        if !status_code.is_success() {
            let rocketchat_error_resp: RocketchatErrorResponse = serde_json::from_str(&body).chain_err(|| {
                    ErrorKind::InvalidJSON(format!("Could not deserialize error response from Rocket.Chat info API \
                                                    endpoint: `{}` ",
                                                   body))
                })?;
            bail!(ErrorKind::RocketchatError(rocketchat_error_resp.message));
        }

        let rocketchat_info: GetInfoResponse = serde_json::from_str(&body).chain_err(|| {
                ErrorKind::InvalidJSON(format!("Could not deserialize response from Rocket.Chat info API \
                                                endpoint: `{}`",
                                               body))
            })?;

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
