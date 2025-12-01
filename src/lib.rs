//! # Fezz - HHRF-based Serverless Runtime
//! 
//! Fezz is a Rust-based Host HTTP Runtime (HHRF) that runs lightweight
//! "fetch-like" function modules using a load-run-unload execution model,
//! providing a serverless-style platform similar to Cloudflare Workers.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                          Envoy Proxy                                │
//! │                    (Load Balancing, TLS, etc.)                      │
//! └─────────────────────────────────────────────────────────────────────┘
//!                                   │
//!                                   ▼
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                      HHRF (Host HTTP Runtime)                       │
//! │  ┌─────────────────────────────────────────────────────────────┐   │
//! │  │                    Function Registry                         │   │
//! │  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐    │   │
//! │  │  │ Function │  │ Function │  │ Function │  │ Function │    │   │
//! │  │  │   (A)    │  │   (B)    │  │   (C)    │  │   ...    │    │   │
//! │  │  └──────────┘  └──────────┘  └──────────┘  └──────────┘    │   │
//! │  └─────────────────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use fezz::prelude::*;
//!
//! // Define a custom function
//! struct HelloFunction;
//!
//! #[async_trait::async_trait]
//! impl FezzFunction for HelloFunction {
//!     async fn fetch(
//!         &self,
//!         request: FezzRequest,
//!         ctx: &FunctionContext,
//!     ) -> Result<FezzResponse, FezzError> {
//!         Ok(FezzResponse::text("Hello from Fezz!"))
//!     }
//!     
//!     fn name(&self) -> &str {
//!         "hello"
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!     // Create the HHRF server
//!     let server = HhrfServer::with_defaults();
//!     
//!     // Register functions
//!     server.register_function("hello", Box::new(HelloFunction)).await?;
//!     
//!     // Run the server
//!     server.run().await
//! }
//! ```
//!
//! ## Function Lifecycle
//!
//! Fezz functions follow a load-run-unload execution model:
//!
//! 1. **Load** (`on_load`): Called when a function is first initialized
//! 2. **Run** (`fetch`): Handles incoming HTTP requests
//! 3. **Unload** (`on_unload`): Called when a function is being cleaned up
//!
//! ## Envoy Integration
//!
//! The HHRF server is designed to sit behind Envoy proxy for:
//! - Load balancing
//! - TLS termination  
//! - Rate limiting
//! - Authentication
//!
//! Configure Envoy to forward requests to the HHRF server's address (default: `0.0.0.0:8080`).

pub mod function;
pub mod http;
pub mod runtime;

/// Re-export commonly used types.
pub mod prelude {
    pub use crate::function::{FezzFunction, FunctionContext, FunctionRegistry};
    pub use crate::function::handler::FezzError;
    pub use crate::http::{FezzRequest, FezzResponse, Method, StatusCode};
    pub use crate::runtime::{HhrfConfig, HhrfServer};
    pub use async_trait::async_trait;
}

// Re-export for convenience
pub use function::{FezzFunction, FunctionContext, FunctionRegistry};
pub use function::handler::FezzError;
pub use http::{FezzRequest, FezzResponse};
pub use runtime::{HhrfConfig, HhrfServer};
