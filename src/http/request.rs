//! Fezz HTTP Request type providing a fetch-like API.

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// HTTP method enumeration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    #[default]
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Method::Get => write!(f, "GET"),
            Method::Post => write!(f, "POST"),
            Method::Put => write!(f, "PUT"),
            Method::Delete => write!(f, "DELETE"),
            Method::Patch => write!(f, "PATCH"),
            Method::Head => write!(f, "HEAD"),
            Method::Options => write!(f, "OPTIONS"),
        }
    }
}

impl From<&hyper::Method> for Method {
    fn from(method: &hyper::Method) -> Self {
        match *method {
            hyper::Method::GET => Method::Get,
            hyper::Method::POST => Method::Post,
            hyper::Method::PUT => Method::Put,
            hyper::Method::DELETE => Method::Delete,
            hyper::Method::PATCH => Method::Patch,
            hyper::Method::HEAD => Method::Head,
            hyper::Method::OPTIONS => Method::Options,
            _ => Method::Get,
        }
    }
}

/// Fetch-like HTTP request for Fezz functions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FezzRequest {
    /// HTTP method.
    pub method: Method,
    /// Request URL.
    pub url: String,
    /// HTTP headers.
    pub headers: HashMap<String, String>,
    /// Request body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<Bytes>,
}

impl FezzRequest {
    /// Create a new FezzRequest.
    pub fn new(method: Method, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: HashMap::new(),
            body: None,
        }
    }

    /// Add a header to the request.
    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Set the request body.
    pub fn body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// Get a header value.
    pub fn get_header(&self, key: &str) -> Option<&String> {
        self.headers.get(key)
    }

    /// Get the body as text if present.
    pub fn text(&self) -> Option<String> {
        self.body
            .as_ref()
            .map(|b| String::from_utf8_lossy(b).to_string())
    }

    /// Parse the body as JSON if present.
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Option<Result<T, serde_json::Error>> {
        self.body
            .as_ref()
            .map(|b| serde_json::from_slice(b))
    }
}

impl Default for FezzRequest {
    fn default() -> Self {
        Self::new(Method::Get, "/")
    }
}
