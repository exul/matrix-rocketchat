use std::collections::HashMap;

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

/// Matrix REST API v0
pub mod r0;

/// Matrix REST API
pub trait MatrixApi: Send + Sync + MatrixApiClone {
    /// Create a room.
    fn create_room(&self, room_name: Option<String>, room_alias_name: Option<String>, creator_id: &UserId) -> Result<RoomId>;
    /// Forget a room.
    fn forget_room(&self, matrix_room_id: RoomId) -> Result<()>;
    /// Get the `user_id` of the user that created the room.
    fn get_room_creator(&self, matrix_room_id: RoomId) -> Result<UserId>;
    /// Get the list of members for this room.
    fn get_room_members(&self, matrix_room_id: RoomId) -> Result<Vec<MemberEvent>>;
    /// Invite a user to a room.
    fn invite(&self, matrix_room_id: RoomId, matrix_user_id: UserId) -> Result<()>;
    /// Join a room with a user.
    fn join(&self, matrix_room_id: RoomId, matrix_user_id: UserId) -> Result<()>;
    /// Leave a room.
    fn leave_room(&self, matrix_room_id: RoomId) -> Result<()>;
    /// Register a user.
    fn register(&self, user_id_local_part: String) -> Result<()>;
    /// Send a text message to a room.
    fn send_text_message_event(&self, matrix_room_id: RoomId, matrix_user_id: UserId, body: String) -> Result<()>;
    /// Set the default power levels for a room. Only the bot will be able to control the room.
    /// The power levels for invite, kick, ban, and redact are all set to 50.
    fn set_default_powerlevels(&self, matrix_room_id: RoomId, bot_user_id: UserId) -> Result<()>;
    /// Set the display name for a user
    fn set_display_name(&self, matrix_user_id: UserId, name: String) -> Result<()>;
    /// Set the name for a room
    fn set_room_name(&self, matrix_room_id: RoomId, name: String) -> Result<()>;
}

/// Helper trait because Clone cannot be part of the `MatrixApi` trait since that would cause the
/// `MatrixApi` trait to not be object safe.
pub trait MatrixApiClone {
    /// Clone the object inside the trait and return it in box.
    fn clone_box(&self) -> Box<MatrixApi>;
}

impl<T> MatrixApiClone for T
where
    T: 'static + MatrixApi + Clone,
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
        let params = HashMap::new();

        debug!(logger, format!("Querying homeserver {} for API versions", url));
        let (body, status_code) = RestApi::call_matrix(GetSupportedVersionsEndpoint::method(), &url, "", &params)?;
        if !status_code.is_success() {
            let matrix_error_resp: MatrixErrorResponse = serde_json::from_str(&body).chain_err(|| {
                ErrorKind::InvalidJSON(format!(
                    "Could not deserialize error response from Matrix supported versions \
                                                    API endpoint: `{}` ",
                    body
                ))
            })?;
            return Err(Error::from(ErrorKind::MatrixError(matrix_error_resp.error)));
        }

        let supported_versions: GetSupportedVersionsResponse = serde_json::from_str(&body).chain_err(|| {
            ErrorKind::InvalidJSON(format!(
                "Could not deserialize response from Matrix supported versions API \
                                                endpoint: `{}`",
                body
            ))
        })?;
        debug!(logger, format!("Homeserver supports versions {:?}", supported_versions.versions));
        MatrixApi::get_max_supported_version_api(supported_versions.versions, config, logger)
    }

    fn get_max_supported_version_api(versions: Vec<String>, config: &Config, logger: Logger) -> Result<Box<MatrixApi>> {
        for version in versions.iter().rev() {
            if version.starts_with("r0") {
                let matrix_api = r0::MatrixApi::new(config, logger);
                return Ok(Box::new(matrix_api));
            }
        }

        Err(Error::from(ErrorKind::UnsupportedMatrixApiVersion(versions.join(", "))))
    }
}
