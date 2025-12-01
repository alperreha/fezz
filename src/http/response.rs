//! Fezz HTTP Response type providing a fetch-like API.

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// HTTP status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusCode(pub u16);

impl StatusCode {
    pub const OK: StatusCode = StatusCode(200);
    pub const CREATED: StatusCode = StatusCode(201);
    pub const NO_CONTENT: StatusCode = StatusCode(204);
    pub const BAD_REQUEST: StatusCode = StatusCode(400);
    pub const UNAUTHORIZED: StatusCode = StatusCode(401);
    pub const FORBIDDEN: StatusCode = StatusCode(403);
    pub const NOT_FOUND: StatusCode = StatusCode(404);
    pub const INTERNAL_SERVER_ERROR: StatusCode = StatusCode(500);
    pub const BAD_GATEWAY: StatusCode = StatusCode(502);
    pub const SERVICE_UNAVAILABLE: StatusCode = StatusCode(503);

    /// Check if the status code indicates success (2xx).
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.0)
    }

    /// Check if the status code indicates a client error (4xx).
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.0)
    }

    /// Check if the status code indicates a server error (5xx).
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.0)
    }
}

impl Default for StatusCode {
    fn default() -> Self {
        StatusCode::OK
    }
}

impl From<u16> for StatusCode {
    fn from(code: u16) -> Self {
        StatusCode(code)
    }
}

impl From<StatusCode> for u16 {
    fn from(code: StatusCode) -> Self {
        code.0
    }
}

/// Fetch-like HTTP response for Fezz functions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FezzResponse {
    /// HTTP status code.
    pub status: StatusCode,
    /// HTTP headers.
    pub headers: HashMap<String, String>,
    /// Response body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<Bytes>,
}

impl FezzResponse {
    /// Create a new FezzResponse with the given status code.
    pub fn new(status: impl Into<StatusCode>) -> Self {
        Self {
            status: status.into(),
            headers: HashMap::new(),
            body: None,
        }
    }

    /// Create an OK response.
    pub fn ok() -> Self {
        Self::new(StatusCode::OK)
    }

    /// Create a response with JSON body.
    pub fn json<T: Serialize>(data: &T) -> Result<Self, serde_json::Error> {
        let body = serde_json::to_vec(data)?;
        Ok(Self::new(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(body))
    }

    /// Create a text response.
    pub fn text(content: impl Into<String>) -> Self {
        Self::new(StatusCode::OK)
            .header("Content-Type", "text/plain")
            .body(content.into())
    }

    /// Create an error response.
    pub fn error(status: impl Into<StatusCode>, message: impl Into<String>) -> Self {
        Self::new(status)
            .header("Content-Type", "text/plain")
            .body(message.into())
    }

    /// Add a header to the response.
    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Set the response body.
    pub fn body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// Get the body as text if present.
    pub fn text_body(&self) -> Option<String> {
        self.body
            .as_ref()
            .map(|b| String::from_utf8_lossy(b).to_string())
    }

    /// Parse the body as JSON if present.
    pub fn json_body<T: serde::de::DeserializeOwned>(
        &self,
    ) -> Option<Result<T, serde_json::Error>> {
        self.body.as_ref().map(|b| serde_json::from_slice(b))
    }
}

impl Default for FezzResponse {
    fn default() -> Self {
        Self::ok()
    }
}
