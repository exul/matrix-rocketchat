use ruma_client_api::Endpoint;
use ruma_client_api::unversioned::get_supported_versions::{Endpoint as GetSupportedVersionsEndpoint,
                                                           Response as GetSupportedVersionsResponse};
use ruma_events::room::member::MemberEvent;
use ruma_identifiers::{RoomId, UserId};
use serde_json;
use slog::Logger;

use api::RestApi;
use config::Config;
use errors::*;

mod r0;

/// Matrix REST API
pub trait MatrixApi: Send + Sync + MatrixApiClone {
    /// Looks up the creator of a room.
    fn get_room_creator(&self, matrix_room_id: RoomId) -> Result<UserId>;
    /// Get the list of members for this room.
    fn get_room_members(&self, matrix_room_id: RoomId) -> Result<Vec<MemberEvent>>;
    /// Join a room with a user.
    fn join(&self, matrix_room_id: RoomId, matrix_user_id: UserId) -> Result<()>;
    /// Send a text message to a room.
    fn send_text_message_event(&self, matrix_room_id: RoomId, matrix_user_id: UserId, body: String) -> Result<()>;
}

/// Helper trait because Clone cannot be part of the `MatrixApi` trait since that would cause the
/// `MatrixApi` trait to not be object safe.
pub trait MatrixApiClone {
    /// Clone the object inside the trait and return it in box.
    fn clone_box(&self) -> Box<MatrixApi>;
}

impl<T> MatrixApiClone for T
    where T: 'static + MatrixApi + Clone
{
    fn clone_box(&self) -> Box<MatrixApi> {
        Box::new(self.clone())
    }
}

impl Clone for Box<MatrixApi> {
    fn clone(&self) -> Box<MatrixApi> {
        self.clone_box()
    }
}

impl MatrixApi {
    /// Creates a new Matrix API depending on the version of the homeserver.
    /// It returns a `MatrixApi` trait, because for each version a different API is created.
    pub fn new(config: &Config, logger: Logger) -> Result<Box<MatrixApi>> {
        let url = config.hs_url.clone() + &GetSupportedVersionsEndpoint::request_path(());

        let (body, status_code) = RestApi::call_matrix(GetSupportedVersionsEndpoint::method(), &url, "")?;
        if !status_code.is_success() {
            let matrix_error_resp: MatrixErrorResponse = serde_json::from_str(&body).chain_err(|| {
                    ErrorKind::InvalidJSON(format!("Could not deserialize error response from Matrix supported versions \
                                                    API endpoint: `{}` ",
                                                   body))
                })?;
            return Err(Error::from(ErrorKind::MatrixError(matrix_error_resp.error)));
        }

        let supported_versions: GetSupportedVersionsResponse = serde_json::from_str(&body).chain_err(|| {
                ErrorKind::InvalidJSON(format!("Could not deserialize response from Matrix supported versions API \
                                                endpoint: `{}`",
                                               body))
            })?;
        MatrixApi::get_max_supported_version_api(supported_versions.versions, config, logger)
    }

    fn get_max_supported_version_api(versions: Vec<String>, config: &Config, logger: Logger) -> Result<Box<MatrixApi>> {
        for version in versions.iter().rev() {
            if version.starts_with("r0") {
                let matrix_api = r0::MatrixApi::new(config, logger);
                return Ok(Box::new(matrix_api));
            }
        }

        let homeserver_api_versions = versions.into_iter()
            .fold("".to_string(), |acc, version| format!("{}, {}", &acc, version));
        Err(Error::from(ErrorKind::UnsupportedMatrixApiVersion(homeserver_api_versions.to_string())))
    }
}