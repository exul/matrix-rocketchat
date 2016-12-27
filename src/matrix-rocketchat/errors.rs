#![allow(missing_docs)]

use iron::{IronError, Response};
use iron::modifier::Modifier;
use iron::status::Status;
use serde_json;

/// `ErrorResponse` defines the format that is used to send an error response as JSON.
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    causes: Vec<String>,
}

/// Response from the Matrix homeserver when an error occurred
#[derive(Deserialize)]
pub struct MatrixErrorResponse {
    /// Error code returned by the Matrix API
    pub errcode: String,
    /// Error message returned by the Matrix API
    pub error: String,
}

error_chain!{
    errors {
        InvalidAccessToken(token: String) {
            description("The provided access token is not valid")
            display("Could not process request, the access token {} is not valid", token)
        }

        MissingAccessToken {
            description("Access token missing")
            display("Could not process request, no access token was provided")
        }

        InvalidJSON(msg: String) {
            description("The provided JSON is not valid: {}")
            display("Could not process request, the submitted data is not valid json")
        }

        InvalidUserId(user_id: String) {
            description("The provided user ID is not valid")
            display("The provided user ID {} is not valid", user_id)
        }

        EventIdGenerationFailed{
            description("Could not generate a new event ID")
            display("Could not generate a new event ID")
        }

        UnsupportedHttpMethod(method: String) {
            description("Could not call REST API")
            display("Unsupported HTTP method {}", method)
        }

        ApiCallFailed {
            description("Call to REST API failed")
            display("Call to REST API failed")
        }

        MatrixError(error_msg: String) {
            description("An error occurred when calling the Matrix API")
            display("Matrix error: {}", error_msg)
        }

        UnsupportedMatrixApiVersion(versions: String) {
            description("None of the Matrix homeserver's versions are supported")
            display("No supported API version found for the Matrix homeserver, found versions: {}", versions)
        }

        InternalServerError {
            description("An internal error occurred")
            display("An internal error occurred")
        }
    }
}

impl ErrorKind {
    pub fn status_code(&self) -> Status {
        match *self {
            ErrorKind::InvalidAccessToken(_) => Status::Forbidden,
            ErrorKind::MissingAccessToken => Status::Unauthorized,
            ErrorKind::InvalidJSON(_) => Status::UnprocessableEntity,
            _ => Status::InternalServerError,
        }
    }
}

impl From<Error> for IronError {
    fn from(error: Error) -> IronError {
        let response = Response::with(&error);
        IronError {
            error: Box::new(error),
            response: response,
        }
    }
}

impl<'a> Modifier<Response> for &'a Error {
    fn modify(self, response: &mut Response) {
        let mut causes = Vec::with_capacity(self.iter().count() - 1);
        for err in self.iter().skip(1) {
            causes.push(format!("{}", err));
        }

        let resp = ErrorResponse {
            error: format!("{}", self),
            causes: causes,
        };

        let err_msg = serde_json::to_string(&resp).expect("ErrorResponse is always serializable");
        response.status = Some(self.status_code());
        response.body = Some(Box::new(err_msg));
    }
}
