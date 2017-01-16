//! Iron middleware

/// Middleware for requests from the Matrix homeserver
pub mod matrix;

pub use self::matrix::AccessToken;
