//! REST API types.

/// Matrix REST API
pub mod matrix;
/// Generic REST API
mod rest_api;
/// Rocket.Chat REST API
pub mod rocketchat;

pub use self::matrix::MatrixApi;
pub use self::rest_api::{RequestData, RestApi};
pub use self::rocketchat::RocketchatApi;
