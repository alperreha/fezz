//! Fezz Runtime - Example HHRF Server
//!
//! This example demonstrates running the HHRF server with sample Fezz functions.

use fezz::prelude::*;
use tracing_subscriber::EnvFilter;

/// Example "Hello World" function.
struct HelloFunction;

#[async_trait]
impl FezzFunction for HelloFunction {
    async fn on_load(&mut self, ctx: &FunctionContext) -> Result<(), FezzError> {
        tracing::info!("Loading function: {}", ctx.function_name);
        Ok(())
    }

    async fn fetch(
        &self,
        request: FezzRequest,
        ctx: &FunctionContext,
    ) -> Result<FezzResponse, FezzError> {
        let name = request
            .get_header("X-Name")
            .cloned()
            .unwrap_or_else(|| "World".to_string());

        let response_body = serde_json::json!({
            "message": format!("Hello, {}!", name),
            "method": request.method.to_string(),
            "path": request.url,
            "request_id": ctx.request_id,
        });

        FezzResponse::json(&response_body).map_err(|e| FezzError::new(e.to_string()))
    }

    async fn on_unload(&mut self, ctx: &FunctionContext) -> Result<(), FezzError> {
        tracing::info!("Unloading function: {}", ctx.function_name);
        Ok(())
    }

    fn name(&self) -> &str {
        "hello"
    }
}

/// Echo function - echoes back the request body.
struct EchoFunction;

#[async_trait]
impl FezzFunction for EchoFunction {
    async fn fetch(
        &self,
        request: FezzRequest,
        _ctx: &FunctionContext,
    ) -> Result<FezzResponse, FezzError> {
        let body = request.text().unwrap_or_default();
        Ok(FezzResponse::text(body))
    }

    fn name(&self) -> &str {
        "echo"
    }
}

/// Counter function with state (demonstrates on_load/on_unload).
struct CounterFunction {
    count: std::sync::atomic::AtomicU64,
}

impl CounterFunction {
    fn new() -> Self {
        Self {
            count: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl FezzFunction for CounterFunction {
    async fn on_load(&mut self, ctx: &FunctionContext) -> Result<(), FezzError> {
        // Reset counter on load
        self.count
            .store(0, std::sync::atomic::Ordering::SeqCst);
        tracing::info!("Counter function loaded: {}", ctx.function_name);
        Ok(())
    }

    async fn fetch(
        &self,
        _request: FezzRequest,
        _ctx: &FunctionContext,
    ) -> Result<FezzResponse, FezzError> {
        let count = self
            .count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        
        let response = serde_json::json!({
            "count": count
        });

        FezzResponse::json(&response).map_err(|e| FezzError::new(e.to_string()))
    }

    async fn on_unload(&mut self, ctx: &FunctionContext) -> Result<(), FezzError> {
        let final_count = self.count.load(std::sync::atomic::Ordering::SeqCst);
        tracing::info!(
            "Counter function unloaded: {} (final count: {})",
            ctx.function_name,
            final_count
        );
        Ok(())
    }

    fn name(&self) -> &str {
        "counter"
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!("Starting Fezz HHRF Server...");

    // Create HHRF server configuration
    let config = HhrfConfig::new()
        .host("0.0.0.0")
        .port(8080)
        .env("ENVIRONMENT", "development");

    // Create the server
    let server = HhrfServer::new(config);

    // Register functions
    server
        .register_function("hello", Box::new(HelloFunction))
        .await?;
    server
        .register_function("echo", Box::new(EchoFunction))
        .await?;
    server
        .register_function("counter", Box::new(CounterFunction::new()))
        .await?;

    tracing::info!("Registered functions: hello, echo, counter");
    tracing::info!("Try: curl http://localhost:8080/hello");
    tracing::info!("Try: curl -X POST -d 'test' http://localhost:8080/echo");
    tracing::info!("Try: curl http://localhost:8080/counter");
    tracing::info!("Health check: curl http://localhost:8080/_health");
    tracing::info!("Metrics: curl http://localhost:8080/_metrics");

    // Run the server
    server.run().await
}

