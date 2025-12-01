//! HTTP types for Fezz functions providing a fetch-like API.

mod request;
mod response;

pub use request::{FezzRequest, Method};
pub use response::{FezzResponse, StatusCode};
