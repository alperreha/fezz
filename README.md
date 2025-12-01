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
- **Macro DSL**: Use `#[fezz_function]` attribute macro for ergonomic function definitions
- **Function Manifests**: Compile-time metadata for routing and configuration
- **Control-Plane Integration**: Support for etcd/Consul-style configuration stores
- **Route Table**: Dynamic routing based on path patterns and HTTP methods
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

### Creating Functions with the Macro DSL

The easiest way to create Fezz functions is using the `#[fezz_function]` attribute macro:

```rust
use fezz::prelude::*;

#[fezz_function(
    id = "hello-world",
    version = "v1",
    method = "GET",
    path = "/api/hello"
)]
async fn hello(req: FezzRequest, ctx: &FunctionContext) -> Result<FezzResponse, FezzError> {
    let name = req.get_header("X-Name")
        .cloned()
        .unwrap_or_else(|| "World".to_string());
    
    Ok(FezzResponse::text(format!("Hello, {}!", name)))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = HhrfServer::with_defaults();
    
    // Register the macro-generated function
    server.register_function("hello-world", Box::new(HelloFunction::new())).await?;
    
    server.run().await
}
```

The macro generates:
- A `HelloFunction` struct implementing `FezzFunction`
- A static `HELLO_MANIFEST` containing function metadata
- A `HelloFunction::manifest()` method for accessing metadata

### Macro Attributes

| Attribute | Required | Default | Description |
|-----------|----------|---------|-------------|
| `id` | Yes | - | Unique function identifier |
| `version` | No | "v1" | Function version |
| `method` | No | "GET" | HTTP method |
| `path` | No | "/" | URL path pattern |
| `timeout` | No | 30 | Request timeout in seconds |
| `description` | No | "" | Function description |

### Creating a Custom Function (Manual Implementation)

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

### FunctionManifest

```rust
// Static manifest (generated by macro)
pub struct FunctionManifest {
    pub id: &'static str,
    pub version: &'static str,
    pub method: &'static str,
    pub path: &'static str,
    pub timeout: u64,
    pub description: &'static str,
}

// Owned manifest (for runtime use)
pub struct OwnedFunctionManifest {
    pub id: String,
    pub version: String,
    pub method: String,
    pub path: String,
    pub timeout: u64,
    pub description: String,
}
```

## Control-Plane Integration

Fezz includes a control-plane module for managing function metadata:

### Function Store

```rust
use fezz::control_plane::{FunctionEntry, FunctionStore, MemoryStore};
use fezz::function::OwnedFunctionManifest;

// Create an in-memory store
let store = MemoryStore::new();

// Register a function
let manifest = OwnedFunctionManifest::new("my-fn", "v1", "GET", "/api/test");
let entry = FunctionEntry::new(manifest).on_node("node-1");
store.register(entry).await?;

// Get a function
let entry = store.get("my-fn").await?;

// Watch for changes
let mut rx = store.watch().await?;
while let Some(event) = rx.recv().await {
    match event {
        StoreEvent::Added(entry) => println!("Added: {}", entry.manifest.id),
        StoreEvent::Updated(entry) => println!("Updated: {}", entry.manifest.id),
        StoreEvent::Removed(id) => println!("Removed: {}", id),
    }
}
```

### Route Table

```rust
use fezz::control_plane::{Route, RouteTable, RouteMethod};

let table = RouteTable::new();

// Add routes
table.add(Route::new(RouteMethod::Get, "/api/users", "users-list")).await;
table.add(Route::new(RouteMethod::Get, "/api/users/:id", "user-get")).await;
table.add(Route::new(RouteMethod::Get, "/api/*", "api-catch-all").priority(0)).await;

// Find matching route
let route = table.find("/api/users/123", "GET").await;
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

## Project Structure

```
fezz/
├── Cargo.toml           # Main package manifest
├── fezz-macro/          # Procedural macro crate
│   ├── Cargo.toml
│   └── src/lib.rs       # #[fezz_function] macro
├── src/
│   ├── lib.rs           # Library entry point
│   ├── main.rs          # Example server
│   ├── control_plane/   # Control-plane module
│   │   ├── mod.rs
│   │   ├── registry.rs  # Function store
│   │   └── routing.rs   # Route table
│   ├── function/        # Function module
│   │   ├── mod.rs
│   │   ├── handler.rs   # FezzFunction trait
│   │   ├── manifest.rs  # Function manifest
│   │   └── registry.rs  # Function registry
│   ├── http/            # HTTP types
│   │   ├── mod.rs
│   │   ├── request.rs   # FezzRequest
│   │   └── response.rs  # FezzResponse
│   └── runtime/         # HHRF runtime
│       ├── mod.rs
│       ├── config.rs    # HhrfConfig
│       └── server.rs    # HhrfServer
└── tests/
    └── integration_tests.rs
```

## License

MIT
