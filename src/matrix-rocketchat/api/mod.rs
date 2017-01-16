//! REST API types.

/// Matrix REST API
pub mod matrix;
/// REST API
pub mod rest_api;

pub use self::matrix::MatrixApi;
pub use self::rest_api::RestApi;
