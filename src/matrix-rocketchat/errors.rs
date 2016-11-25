use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fmt::Error as FmtError;
use std::io::Error as IoError;

use iron::error::HttpError;
use serde_yaml::Error as YamlError;

/// Application Service Error
#[derive(Debug)]
pub struct ASError {
    /// Application service error code
    error_code: ASErrorCode,
    /// The detailed error message
    error_message: String,
    /// Error message that is supposed to be shown to the user
    user_message: String,
}

/// Application Service specific error codes.
#[derive(Clone, Debug, Serialize)]
pub enum ASErrorCode {
    /// Errors not fitting into another category.
    InternalServerError,
}

impl ASError {
    /// Build an ASError of type InternalServerError
    pub fn internal_server_error(message: &str) -> ASError {
        ASError {
            error_code: ASErrorCode::InternalServerError,
            error_message: format!("An internal error occured: {}", message),
            user_message: "An internal error occurred".to_string(),
        }
    }
}

impl Error for ASError {
    fn description(&self) -> &str {
        &self.error_message
    }
}

impl Display for ASError {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        write!(f, "{}", self.user_message)
    }
}

impl From<IoError> for ASError {
    fn from(error: IoError) -> ASError {
        let message = format!("IO Error: {}", error);
        ASError::internal_server_error(&message)
    }
}

impl From<YamlError> for ASError {
    fn from(error: YamlError) -> ASError {
        let message = format!("YAML error: {}", error);
        ASError::internal_server_error(&message)
    }
}

impl From<HttpError> for ASError {
    fn from(error: HttpError) -> ASError {
        let message = format!("HTTP error: {}", error);
        ASError::internal_server_error(&message)
    }
}
