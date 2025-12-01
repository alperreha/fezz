//! Fezz function handler trait and context.

use crate::http::{FezzRequest, FezzResponse};
use async_trait::async_trait;
use std::collections::HashMap;

/// Execution context for Fezz functions.
#[derive(Debug, Clone, Default)]
pub struct FunctionContext {
    /// Environment variables available to the function.
    pub env: HashMap<String, String>,
    /// Function name.
    pub function_name: String,
    /// Request ID for tracing.
    pub request_id: String,
}

impl FunctionContext {
    /// Create a new function context.
    pub fn new(function_name: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self {
            env: HashMap::new(),
            function_name: function_name.into(),
            request_id: request_id.into(),
        }
    }

    /// Add an environment variable.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Get an environment variable.
    pub fn get_env(&self, key: &str) -> Option<&String> {
        self.env.get(key)
    }
}

/// Fezz function trait for implementing serverless functions.
/// 
/// This trait defines the interface for Fezz functions that follow
/// the load-run-unload execution model similar to Cloudflare Workers.
#[async_trait]
pub trait FezzFunction: Send + Sync {
    /// Called when the function is loaded into the runtime.
    /// Use this for initialization logic.
    async fn on_load(&mut self, ctx: &FunctionContext) -> Result<(), FezzError> {
        let _ = ctx;
        Ok(())
    }

    /// Handle an incoming HTTP request (fetch event).
    /// This is the main entry point for the function.
    async fn fetch(
        &self,
        request: FezzRequest,
        ctx: &FunctionContext,
    ) -> Result<FezzResponse, FezzError>;

    /// Called when the function is about to be unloaded.
    /// Use this for cleanup logic.
    async fn on_unload(&mut self, ctx: &FunctionContext) -> Result<(), FezzError> {
        let _ = ctx;
        Ok(())
    }

    /// Get the function name.
    fn name(&self) -> &str;
}

/// Fezz function error type.
#[derive(Debug, Clone)]
pub struct FezzError {
    /// Error message.
    pub message: String,
    /// Error code.
    pub code: u16,
}

impl FezzError {
    /// Create a new FezzError.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: 500,
        }
    }

    /// Create a FezzError with a specific code.
    pub fn with_code(code: u16, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code,
        }
    }

    /// Create a not found error.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::with_code(404, message)
    }

    /// Create a bad request error.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::with_code(400, message)
    }
}

impl std::fmt::Display for FezzError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for FezzError {}

impl From<FezzError> for FezzResponse {
    fn from(err: FezzError) -> Self {
        FezzResponse::error(err.code, err.message)
    }
}

impl From<std::io::Error> for FezzError {
    fn from(err: std::io::Error) -> Self {
        FezzError::new(err.to_string())
    }
}

impl From<serde_json::Error> for FezzError {
    fn from(err: serde_json::Error) -> Self {
        FezzError::bad_request(err.to_string())
    }
}
