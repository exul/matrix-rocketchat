use std::collections::HashMap;

use reqwest::StatusCode;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::account::register::{self, Endpoint as RegisterEndpoint};
use ruma_client_api::r0::membership::forget_room::{self, Endpoint as ForgetRoomEndpoint};
use ruma_client_api::r0::membership::join_room_by_id::{self, Endpoint as JoinRoomByIdEndpoint};
use ruma_client_api::r0::membership::leave_room::{self, Endpoint as LeaveRoomEndpoint};
use ruma_client_api::r0::send::send_message_event::{self, Endpoint as SendMessageEventEndpoint};
use ruma_client_api::r0::send::send_state_event_for_empty_key::{self, Endpoint as SendStateEventForEmptyKeyEndpoint};
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

    fn params_hash(&self) -> HashMap<&str, &str> {
        let mut params: HashMap<&str, &str> = HashMap::new();
        params.insert("access_token", &self.access_token);
        params
    }
}

impl super::MatrixApi for MatrixApi {
    fn forget_room(&self, matrix_room_id: RoomId) -> Result<()> {
        let path_params = forget_room::PathParams { room_id: matrix_room_id };
        let endpoint = self.base_url.clone() + &ForgetRoomEndpoint::request_path(path_params);
        let params = self.params_hash();

        let (body, status_code) = RestApi::call_matrix(ForgetRoomEndpoint::method(), &endpoint, "{}", &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }
        Ok(())
    }

    fn get_room_members(&self, matrix_room_id: RoomId) -> Result<Vec<MemberEvent>> {
        let path_params = get_member_events::PathParams { room_id: matrix_room_id.clone() };
        let endpoint = self.base_url.clone() + &GetMemberEventsEndpoint::request_path(path_params);
        let params = self.params_hash();

        let (body, status_code) = RestApi::call_matrix(GetMemberEventsEndpoint::method(), &endpoint, "{}", &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
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
        let mut params = self.params_hash();
        params.insert("user_id", &user_id);

        let (body, status_code) = RestApi::call_matrix(JoinRoomByIdEndpoint::method(), &endpoint, "{}", &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
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
        let params = self.params_hash();

        let (body, status_code) = RestApi::call_matrix(LeaveRoomEndpoint::method(), &endpoint, "{}", &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }
        Ok(())
    }

    fn register(&self, user_id_local_part: String) -> Result<()> {
        let endpoint = self.base_url.clone() + &RegisterEndpoint::request_path(());
        let params = self.params_hash();
        let body_params = register::BodyParams {
            bind_email: None,
            password: None,
            username: Some(user_id_local_part),
            device_id: None,
            initial_device_display_name: None,
            auth: None,
        };
        let payload = serde_json::to_string(&body_params).chain_err(|| ErrorKind::InvalidJSON("Could not serialize account body params".to_string()))?;

        let (body, status_code) = RestApi::call_matrix(RegisterEndpoint::method(), &endpoint, &payload, &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
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
        let mut params = self.params_hash();
        params.insert("user_id", &user_id);

        let (body, status_code) = RestApi::call_matrix(SendMessageEventEndpoint::method(), &endpoint, &payload, &params)?;

        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }

        debug!(self.logger,
               "User {} successfully sent a message to room {}",
               matrix_user_id,
               matrix_room_id);
        Ok(())
    }

    fn set_room_name(&self, matrix_room_id: RoomId, name: String) -> Result<()> {
        let path_params = send_state_event_for_empty_key::PathParams {
            room_id: matrix_room_id,
            event_type: EventType::RoomName,
        };
        let endpoint = self.base_url.clone() + &SendStateEventForEmptyKeyEndpoint::request_path(path_params);
        let params = self.params_hash();
        let mut body_params = serde_json::Map::new();
        body_params.insert("name", name);

        let payload = serde_json::to_string(&body_params).chain_err(|| ErrorKind::InvalidJSON("Could not serialize account body params".to_string()))?;

        let (body, status_code) =
            RestApi::call_matrix(SendStateEventForEmptyKeyEndpoint::method(), &endpoint, &payload, &params)?;
        if !status_code.is_success() {
            return Err(build_error(&endpoint, &body, &status_code));
        }
        Ok(())
    }
}

fn build_error(endpoint: &str, body: &str, status_code: &StatusCode) -> Error {
    let json_error_msg = format!("Could not deserialize error from Matrix API endpoint {} with status code {}: `{}`",
                                 endpoint,
                                 status_code,
                                 body);
    let json_error = ErrorKind::InvalidJSON(json_error_msg);
    let matrix_error_resp: MatrixErrorResponse = match serde_json::from_str(body).chain_err(|| json_error) {
        Ok(matrix_error_resp) => matrix_error_resp,
        Err(err) => {
            return err;
        }
    };
    Error::from(ErrorKind::MatrixError(matrix_error_resp.error))
}
