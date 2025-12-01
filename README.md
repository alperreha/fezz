# Fezz

A Rust-based Host HTTP Runtime (HHRF) that runs lightweight "fetch-like" Fezz function modules using a load-run-unload execution model, providing a serverless-style platform similar to Cloudflare Workers.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────┐
│                          Envoy Proxy                                │
│                    (Load Balancing, TLS, etc.)                      │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      HHRF (Host HTTP Runtime)                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                    Function Registry                         │   │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐    │   │
│  │  │ Function │  │ Function │  │ Function │  │ Function │    │   │
│  │  │   (A)    │  │   (B)    │  │   (C)    │  │   ...    │    │   │
│  │  └──────────┘  └──────────┘  └──────────┘  └──────────┘    │   │
│  └─────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

## Features

- **Load-Run-Unload Execution Model**: Functions are loaded on-demand, executed, and can be unloaded when idle
- **Fetch-like API**: Similar to Cloudflare Workers' fetch event model
- **Function Lifecycle Hooks**: `on_load`, `fetch`, and `on_unload` methods
- **Built-in Health & Metrics**: Endpoints for monitoring and observability
- **Envoy-Ready**: Designed to sit behind Envoy for load balancing, TLS, and rate limiting

## Quick Start

### Running the Example Server

```bash
cargo run
```

The server starts on `http://localhost:8080` with example functions:

```bash
# Health check
curl http://localhost:8080/_health

# Hello function
curl http://localhost:8080/hello
curl -H "X-Name: Alice" http://localhost:8080/hello

# Echo function
curl -X POST -d "Hello, World!" http://localhost:8080/echo

# Counter function (stateful)
curl http://localhost:8080/counter
curl http://localhost:8080/counter

# Metrics
curl http://localhost:8080/_metrics
```

### Creating a Custom Function

```rust
use fezz::prelude::*;

struct MyFunction;

#[async_trait]
impl FezzFunction for MyFunction {
    async fn on_load(&mut self, ctx: &FunctionContext) -> Result<(), FezzError> {
        println!("Function loaded: {}", ctx.function_name);
        Ok(())
    }

    async fn fetch(
        &self,
        request: FezzRequest,
        ctx: &FunctionContext,
    ) -> Result<FezzResponse, FezzError> {
        let response = serde_json::json!({
            "message": "Hello from MyFunction!",
            "path": request.url,
            "method": request.method.to_string(),
        });
        FezzResponse::json(&response).map_err(|e| FezzError::new(e.to_string()))
    }

    async fn on_unload(&mut self, ctx: &FunctionContext) -> Result<(), FezzError> {
        println!("Function unloaded: {}", ctx.function_name);
        Ok(())
    }

    fn name(&self) -> &str {
        "my-function"
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = HhrfConfig::new()
        .host("0.0.0.0")
        .port(8080);

    let server = HhrfServer::new(config);
    server.register_function("my-function", Box::new(MyFunction)).await?;
    server.run().await
}
```

## API Reference

### FezzRequest

```rust
pub struct FezzRequest {
    pub method: Method,     // HTTP method (GET, POST, etc.)
    pub url: String,        // Request path
    pub headers: HashMap<String, String>,
    pub body: Option<Bytes>,
}
```

### FezzResponse

```rust
// Create text response
FezzResponse::text("Hello, World!")

// Create JSON response
FezzResponse::json(&my_data)?

// Create error response
FezzResponse::error(StatusCode::NOT_FOUND, "Resource not found")

// Build custom response
FezzResponse::new(StatusCode::OK)
    .header("X-Custom", "value")
    .body("content")
```

### FezzFunction Trait

```rust
#[async_trait]
pub trait FezzFunction: Send + Sync {
    /// Called when function is loaded (optional)
    async fn on_load(&mut self, ctx: &FunctionContext) -> Result<(), FezzError>;
    
    /// Handle incoming requests (required)
    async fn fetch(&self, request: FezzRequest, ctx: &FunctionContext) -> Result<FezzResponse, FezzError>;
    
    /// Called when function is unloaded (optional)
    async fn on_unload(&mut self, ctx: &FunctionContext) -> Result<(), FezzError>;
    
    /// Function name
    fn name(&self) -> &str;
}
```

## Envoy Integration

Example Envoy configuration for use with HHRF:

```yaml
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 80
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                          route:
                            cluster: hhrf_cluster
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router

  clusters:
    - name: hhrf_cluster
      connect_timeout: 0.25s
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: hhrf_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: hhrf
                      port_value: 8080
```

## Configuration

```rust
let config = HhrfConfig::new()
    .host("0.0.0.0")           // Bind address
    .port(8080)                 // Port
    .env("API_KEY", "secret");  // Global environment variable
```

## System Endpoints

| Endpoint | Description |
|----------|-------------|
| `/_health` | Health check, returns "OK" |
| `/_metrics` | Function registry status |

## License

MIT
