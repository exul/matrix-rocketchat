#![allow(missing_docs)]

use iron::IronError;
use iron::status::Status;

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
    }
}

impl From<Error> for IronError {
    fn from(err: Error) -> IronError {
        match *err.kind() {
            ErrorKind::InvalidAccessToken(_) => IronError::new(err, Status::Forbidden),
            ErrorKind::MissingAccessToken => IronError::new(err, Status::Unauthorized),
            _ => IronError::new(err, Status::InternalServerError),
        }
    }
}
