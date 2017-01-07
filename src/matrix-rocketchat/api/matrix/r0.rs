use std::collections::HashMap;

use ruma_client_api::Endpoint;
use ruma_client_api::r0::account::register::{self, Endpoint as RegisterEndpoint};
use ruma_client_api::r0::membership::forget_room::{self, Endpoint as ForgetRoomEndpoint};
use ruma_client_api::r0::membership::join_room_by_id::{self, Endpoint as JoinRoomByIdEndpoint};
use ruma_client_api::r0::membership::leave_room::{self, Endpoint as LeaveRoomEndpoint};
use ruma_client_api::r0::send::send_message_event::{self, Endpoint as SendMessageEventEndpoint};
use ruma_client_api::r0::sync::get_member_events::{self, Endpoint as GetMemberEventsEndpoint};
use ruma_events::EventType;
use ruma_events::room::member::MemberEvent;
use ruma_events::room::message::{MessageType, TextMessageEventContent};
use ruma_identifiers::{EventId, RoomId, UserId};
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
    fn forget_room(&self, matrix_room_id: RoomId) -> Result<()> {
        let path_params = forget_room::PathParams { room_id: matrix_room_id };
        let endpoint = self.base_url.clone() + &ForgetRoomEndpoint::request_path(path_params);
        let parameters = self.parameter_hash();

        let (body, status_code) = RestApi::call_matrix(ForgetRoomEndpoint::method(), &endpoint, "{}")?;
        if !status_code.is_success() {
            let matrix_error_resp: MatrixErrorResponse = serde_json::from_str(&body).chain_err(|| {
                    ErrorKind::InvalidJSON(format!("Could not deserialize error response from Matrix members API \
                                                    endpoint: `{}`",
                                                   body))
                })?;
            bail!(ErrorKind::MatrixError(matrix_error_resp.error));
        }
        Ok(())
    }

    fn get_room_members(&self, matrix_room_id: RoomId) -> Result<Vec<MemberEvent>> {
        let path_params = get_member_events::PathParams { room_id: matrix_room_id.clone() };
        let endpoint = self.base_url.clone() + &GetMemberEventsEndpoint::request_path(path_params);
        let parameters = self.parameter_hash();

        let (body, status_code) = RestApi::call_matrix(GetMemberEventsEndpoint::method(), &endpoint, "{}")?;
        if !status_code.is_success() {
            let matrix_error_resp: MatrixErrorResponse = serde_json::from_str(&body).chain_err(|| {
                    ErrorKind::InvalidJSON(format!("Could not deserialize error response from Matrix members API \
                                                    endpoint: `{}`",
                                                   body))
                })?;
            bail!(ErrorKind::MatrixError(matrix_error_resp.error));
        }

        debug!(self.logger,
               format!("List of room members for room {} successfully received", matrix_room_id));

        let room_member_events: get_member_events::Response = serde_json::from_str(&body).chain_err(|| {
                ErrorKind::InvalidJSON(format!("Could not deserialize reseponse from Matrix members API endpoint: `{}`",
                                               body))
            })?;
        Ok(room_member_events.chunks)
    }

    fn join(&self, matrix_room_id: RoomId, matrix_user_id: UserId) -> Result<()> {
        let path_params = join_room_by_id::PathParams { room_id: matrix_room_id.clone() };
        let endpoint = self.base_url.clone() + &JoinRoomByIdEndpoint::request_path(path_params);
        let user_id = matrix_user_id.to_string();
        let mut parameters = self.parameter_hash();
        parameters.insert("user_id", &user_id);

        let (body, status_code) = RestApi::call_matrix(JoinRoomByIdEndpoint::method(), &endpoint, "{}")?;
        if !status_code.is_success() {
            let matrix_error_resp: MatrixErrorResponse = serde_json::from_str(&body).chain_err(|| {
                    ErrorKind::InvalidJSON(format!("Could not deserialize error response from Matrix join API endpoint: \
                                                    `{}`",
                                                   body))
                })?;
            bail!(ErrorKind::MatrixError(matrix_error_resp.error));
        }

        debug!(self.logger,
               "User {} successfully joined room {}",
               matrix_room_id,
               matrix_user_id);
        Ok(())
    }

    fn leave_room(&self, matrix_room_id: RoomId) -> Result<()> {
        let path_params = leave_room::PathParams { room_id: matrix_room_id };
        let endpoint = self.base_url.clone() + &LeaveRoomEndpoint::request_path(path_params);
        let parameters = self.parameter_hash();

        let (body, status_code) = RestApi::call_matrix(LeaveRoomEndpoint::method(), &endpoint, "{}")?;
        if !status_code.is_success() {
            let matrix_error_resp: MatrixErrorResponse = serde_json::from_str(&body).chain_err(|| {
                    ErrorKind::InvalidJSON(format!("Could not deserialize error response from Matrix members API \
                                                    endpoint: `{}`",
                                                   body))
                })?;
            bail!(ErrorKind::MatrixError(matrix_error_resp.error));
        }
        Ok(())
    }

    fn register(&self, user_id_local_part: String) -> Result<()> {
        let endpoint = self.base_url.clone() + &RegisterEndpoint::request_path(());
        let parameters = self.parameter_hash();
        let body = register::BodyParams {
            bind_email: None,
            password: None,
            username: Some(user_id_local_part),
            device_id: None,
            initial_device_display_name: None,
            auth: None,
        };
        let payload = serde_json::to_string(&body).chain_err(|| ErrorKind::InvalidJSON("Could not serialize account body params".to_string()))?;

        let (body, status_code) = RestApi::call_matrix(RegisterEndpoint::method(), &endpoint, &payload)?;
        if !status_code.is_success() {
            let matrix_error_resp: MatrixErrorResponse = serde_json::from_str(&body).chain_err(|| {
                    ErrorKind::InvalidJSON(format!("Could not deserialize error response from Matrix registration API \
                                                    endpoint: `{}`",
                                                   body))
                })?;
            bail!(ErrorKind::MatrixError(matrix_error_resp.error));
        }
        Ok(())
    }

    fn send_text_message_event(&self, matrix_room_id: RoomId, matrix_user_id: UserId, body: String) -> Result<()> {
        let message = TextMessageEventContent {
            body: body,
            msgtype: MessageType::Text,
        };
        let payload =
            serde_json::to_string(&message).chain_err(|| ErrorKind::InvalidJSON("Could not serialize message".to_string()))?;
        let txn_id = EventId::new(&self.base_url).chain_err(|| ErrorKind::EventIdGenerationFailed)?;
        let path_params = send_message_event::PathParams {
            room_id: matrix_room_id.clone(),
            event_type: EventType::RoomMessage,
            txn_id: txn_id.to_string(),
        };
        let endpoint = self.base_url.clone() + &SendMessageEventEndpoint::request_path(path_params);
        let user_id = matrix_user_id.to_string();
        let mut parameters = self.parameter_hash();
        parameters.insert("user_id", &user_id);

        let (body, status_code) = RestApi::call_matrix(SendMessageEventEndpoint::method(), &endpoint, &payload)?;

        if !status_code.is_success() {
            let matrix_error_resp: MatrixErrorResponse = serde_json::from_str(&body).chain_err(|| {
                    ErrorKind::InvalidJSON(format!("Could not deserialize error response from Matrix join API endpoint: \
                                                    `{}`",
                                                   body))
                })?;
            bail!(ErrorKind::MatrixError(matrix_error_resp.error));
        }

        debug!(self.logger,
               "User {} successfully sent a message to room {}",
               matrix_user_id,
               matrix_room_id);
        Ok(())
    }
}
