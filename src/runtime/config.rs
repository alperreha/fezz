//! HHRF configuration.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for the HHRF server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HhrfConfig {
    /// Host address to bind to.
    pub host: String,
    /// Port to listen on.
    pub port: u16,
    /// Global environment variables for all functions.
    pub env: HashMap<String, String>,
    /// Whether to enable metrics endpoint.
    pub enable_metrics: bool,
    /// Whether to enable health check endpoint.
    pub enable_health: bool,
    /// Maximum request body size in bytes.
    pub max_body_size: usize,
    /// Request timeout in seconds.
    pub request_timeout: u64,
}

impl Default for HhrfConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            env: HashMap::new(),
            enable_metrics: true,
            enable_health: true,
            max_body_size: 10 * 1024 * 1024, // 10MB
            request_timeout: 30,
        }
    }
}

impl HhrfConfig {
    /// Create a new config with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the host address.
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    /// Set the port.
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Add an environment variable.
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Get the bind address.
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
