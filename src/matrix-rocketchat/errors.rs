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

        InvalidJSON {
            description("The provided JSON is not valid")
            display("Could not process request, the submitted data is not valid json")
        }
    }
}

impl ErrorKind {
    pub fn status_code(&self) -> Status {
        match *self {
            ErrorKind::InvalidAccessToken(_) => Status::Forbidden,
            ErrorKind::MissingAccessToken => Status::Unauthorized,
            ErrorKind::InvalidJSON => Status::UnprocessableEntity,
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
