//! HHRF HTTP Server implementation.

use crate::function::{FezzFunction, FunctionRegistry};
use crate::http::{FezzRequest, FezzResponse, Method, StatusCode};
use crate::runtime::HhrfConfig;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{debug, error, info, warn};

/// HHRF (Host HTTP Runtime) Server.
/// 
/// This server handles incoming HTTP requests and routes them to
/// registered Fezz functions using the load-run-unload execution model.
pub struct HhrfServer {
    /// Server configuration.
    config: HhrfConfig,
    /// Function registry.
    registry: Arc<FunctionRegistry>,
}

impl HhrfServer {
    /// Create a new HHRF server.
    pub fn new(config: HhrfConfig) -> Self {
        let registry = Arc::new(FunctionRegistry::with_env(config.env.clone()));
        Self { config, registry }
    }

    /// Create a new HHRF server with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(HhrfConfig::default())
    }

    /// Get the function registry.
    pub fn registry(&self) -> Arc<FunctionRegistry> {
        self.registry.clone()
    }

    /// Register a function with the server.
    pub async fn register_function(
        &self,
        name: impl Into<String>,
        function: Box<dyn FezzFunction>,
    ) -> Result<(), crate::function::handler::FezzError> {
        self.registry.register(name, function).await
    }

    /// Start the HTTP server.
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr: SocketAddr = self.config.bind_addr().parse()?;
        let listener = TcpListener::bind(addr).await?;

        info!("HHRF Server listening on {}", addr);

        let registry = self.registry.clone();
        let config = self.config.clone();

        loop {
            let (stream, remote_addr) = listener.accept().await?;
            let io = TokioIo::new(stream);
            
            let registry = registry.clone();
            let config = config.clone();

            tokio::task::spawn(async move {
                let service = service_fn(move |req| {
                    let registry = registry.clone();
                    let config = config.clone();
                    async move { handle_request(req, registry, config, remote_addr).await }
                });

                if let Err(err) = http1::Builder::new()
                    .serve_connection(io, service)
                    .await
                {
                    error!("Error serving connection: {:?}", err);
                }
            });
        }
    }
}

/// Handle an incoming HTTP request.
async fn handle_request(
    req: Request<Incoming>,
    registry: Arc<FunctionRegistry>,
    config: HhrfConfig,
    remote_addr: SocketAddr,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let path = req.uri().path().to_string();
    let method = req.method().clone();
    let request_id = generate_request_id();

    debug!(
        "Handling request: {} {} from {} [{}]",
        method,
        path,
        remote_addr,
        request_id
    );

    // Handle system endpoints
    if config.enable_health && path == "/_health" {
        return Ok(build_response(FezzResponse::text("OK")));
    }

    if config.enable_metrics && path == "/_metrics" {
        let functions = registry.list().await;
        let metrics = serde_json::json!({
            "functions": functions.iter().map(|(name, state)| {
                serde_json::json!({
                    "name": name,
                    "state": format!("{:?}", state)
                })
            }).collect::<Vec<_>>()
        });
        return Ok(build_response(
            FezzResponse::json(&metrics).unwrap_or_else(|_| FezzResponse::text("{}")),
        ));
    }

    // Route to function based on path
    // Expected format: /{function_name}/...
    let parts: Vec<&str> = path.trim_start_matches('/').splitn(2, '/').collect();
    
    if parts.is_empty() || parts[0].is_empty() {
        return Ok(build_response(FezzResponse::error(
            StatusCode::NOT_FOUND,
            "No function specified",
        )));
    }

    let function_name = parts[0].to_string();
    let sub_path = if parts.len() > 1 {
        format!("/{}", parts[1])
    } else {
        "/".to_string()
    };

    // Convert hyper request to FezzRequest
    let fezz_request = match convert_request(req, &sub_path, &config).await {
        Ok(req) => req,
        Err(e) => {
            warn!("Failed to convert request: {}", e);
            return Ok(build_response(FezzResponse::error(
                StatusCode::BAD_REQUEST,
                e.to_string(),
            )));
        }
    };

    // Execute function
    match registry.execute(&function_name, fezz_request, &request_id).await {
        Ok(response) => Ok(build_response(response)),
        Err(e) => {
            error!(
                "Function '{}' error: {} [{}]",
                function_name, e, request_id
            );
            Ok(build_response(e.into()))
        }
    }
}

/// Convert a hyper Request to FezzRequest.
async fn convert_request(
    req: Request<Incoming>,
    path: &str,
    config: &HhrfConfig,
) -> Result<FezzRequest, Box<dyn std::error::Error + Send + Sync>> {
    let method = Method::from(req.method());
    let url = path.to_string();
    
    let mut headers = HashMap::new();
    for (name, value) in req.headers() {
        if let Ok(v) = value.to_str() {
            headers.insert(name.as_str().to_string(), v.to_string());
        }
    }

    let body_bytes = req.collect().await?.to_bytes();
    let body = if body_bytes.len() > config.max_body_size {
        return Err("Request body too large".into());
    } else if body_bytes.is_empty() {
        None
    } else {
        Some(body_bytes)
    };

    Ok(FezzRequest {
        method,
        url,
        headers,
        body,
    })
}

/// Build a hyper Response from FezzResponse.
fn build_response(fezz_response: FezzResponse) -> Response<Full<Bytes>> {
    let status = hyper::StatusCode::from_u16(fezz_response.status.0).unwrap_or_else(|_| {
        warn!(
            "Invalid status code {}, falling back to 500 Internal Server Error",
            fezz_response.status.0
        );
        hyper::StatusCode::INTERNAL_SERVER_ERROR
    });

    let mut builder = Response::builder().status(status);

    for (name, value) in fezz_response.headers {
        builder = builder.header(name, value);
    }

    let body = fezz_response.body.unwrap_or_default();
    builder.body(Full::new(body)).unwrap()
}

/// Generate a unique request ID.
fn generate_request_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:x}", timestamp)
}
