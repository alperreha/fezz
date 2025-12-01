//! Integration tests for the Fezz runtime.

use fezz::prelude::*;

/// A simple test function for testing.
struct TestFunction {
    response_text: String,
}

impl TestFunction {
    fn new(response_text: impl Into<String>) -> Self {
        Self {
            response_text: response_text.into(),
        }
    }
}

#[async_trait]
impl FezzFunction for TestFunction {
    async fn fetch(
        &self,
        _request: FezzRequest,
        _ctx: &FunctionContext,
    ) -> Result<FezzResponse, FezzError> {
        Ok(FezzResponse::text(&self.response_text))
    }

    fn name(&self) -> &str {
        "test"
    }
}

// Test the macro-generated function
#[fezz_function(
    id = "macro-hello",
    version = "v1",
    method = "GET",
    path = "/api/hello"
)]
async fn macro_hello(
    _req: FezzRequest,
    _ctx: &FunctionContext,
) -> Result<FezzResponse, FezzError> {
    Ok(FezzResponse::text("Hello from macro!"))
}

// Test macro with custom timeout
#[fezz_function(
    id = "macro-timeout",
    version = "v2",
    method = "POST",
    path = "/api/submit",
    timeout = 60,
    description = "A function with custom timeout"
)]
async fn macro_timeout(
    req: FezzRequest,
    _ctx: &FunctionContext,
) -> Result<FezzResponse, FezzError> {
    let body = req.text().unwrap_or_default();
    Ok(FezzResponse::text(format!("Received: {}", body)))
}

#[tokio::test]
async fn test_macro_function_creation() {
    // Test that macro creates the struct
    let func = MacroHelloFunction::new();
    assert_eq!(func.name(), "macro-hello");
}

#[tokio::test]
async fn test_macro_function_manifest() {
    // Test that macro creates the manifest
    let manifest = MacroHelloFunction::manifest();
    assert_eq!(manifest.id, "macro-hello");
    assert_eq!(manifest.version, "v1");
    assert_eq!(manifest.method, "GET");
    assert_eq!(manifest.path, "/api/hello");
    assert_eq!(manifest.timeout, 30); // default timeout
}

#[tokio::test]
async fn test_macro_function_manifest_with_timeout() {
    let manifest = MacroTimeoutFunction::manifest();
    assert_eq!(manifest.id, "macro-timeout");
    assert_eq!(manifest.version, "v2");
    assert_eq!(manifest.method, "POST");
    assert_eq!(manifest.path, "/api/submit");
    assert_eq!(manifest.timeout, 60);
    assert_eq!(manifest.description, "A function with custom timeout");
}

#[tokio::test]
async fn test_macro_function_execution() {
    let registry = FunctionRegistry::new();
    
    registry
        .register("macro-hello", Box::new(MacroHelloFunction::new()))
        .await
        .unwrap();
    
    let request = FezzRequest::new(Method::Get, "/api/hello");
    let response = registry.execute("macro-hello", request, "req-123").await.unwrap();
    
    assert!(response.status.is_success());
    assert_eq!(response.text_body(), Some("Hello from macro!".to_string()));
}

#[tokio::test]
async fn test_function_registry_register() {
    let registry = FunctionRegistry::new();
    
    let result = registry
        .register("test", Box::new(TestFunction::new("Hello")))
        .await;
    
    assert!(result.is_ok());
    
    let functions = registry.list().await;
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0].0, "test");
}

#[tokio::test]
async fn test_function_registry_duplicate_register() {
    let registry = FunctionRegistry::new();
    
    registry
        .register("test", Box::new(TestFunction::new("Hello")))
        .await
        .unwrap();
    
    // Should fail on duplicate registration
    let result = registry
        .register("test", Box::new(TestFunction::new("Hello2")))
        .await;
    
    assert!(result.is_err());
}

#[tokio::test]
async fn test_function_registry_execute() {
    let registry = FunctionRegistry::new();
    
    registry
        .register("test", Box::new(TestFunction::new("Test Response")))
        .await
        .unwrap();
    
    let request = FezzRequest::new(Method::Get, "/");
    let response = registry.execute("test", request, "req-123").await.unwrap();
    
    assert!(response.status.is_success());
    assert_eq!(response.text_body(), Some("Test Response".to_string()));
}

#[tokio::test]
async fn test_function_registry_execute_not_found() {
    let registry = FunctionRegistry::new();
    
    let request = FezzRequest::new(Method::Get, "/");
    let result = registry.execute("nonexistent", request, "req-123").await;
    
    assert!(result.is_err());
}

#[tokio::test]
async fn test_fezz_request_builder() {
    let request = FezzRequest::new(Method::Post, "/api/test")
        .header("Content-Type", "application/json")
        .body(r#"{"key": "value"}"#);
    
    assert_eq!(request.method, Method::Post);
    assert_eq!(request.url, "/api/test");
    assert_eq!(
        request.get_header("Content-Type"),
        Some(&"application/json".to_string())
    );
    assert!(request.body.is_some());
}

#[tokio::test]
async fn test_fezz_response_json() {
    #[derive(serde::Serialize)]
    struct TestData {
        message: String,
        count: u32,
    }
    
    let data = TestData {
        message: "Hello".to_string(),
        count: 42,
    };
    
    let response = FezzResponse::json(&data).unwrap();
    
    assert!(response.status.is_success());
    assert_eq!(
        response.headers.get("Content-Type"),
        Some(&"application/json".to_string())
    );
}

#[tokio::test]
async fn test_fezz_response_error() {
    let response = FezzResponse::error(StatusCode::NOT_FOUND, "Resource not found");
    
    assert_eq!(response.status, StatusCode::NOT_FOUND);
    assert!(response.status.is_client_error());
    assert_eq!(response.text_body(), Some("Resource not found".to_string()));
}

#[tokio::test]
async fn test_function_context() {
    let ctx = FunctionContext::new("test-fn", "req-456")
        .with_env("API_KEY", "secret123")
        .with_env("ENV", "test");
    
    assert_eq!(ctx.function_name, "test-fn");
    assert_eq!(ctx.request_id, "req-456");
    assert_eq!(ctx.get_env("API_KEY"), Some(&"secret123".to_string()));
    assert_eq!(ctx.get_env("ENV"), Some(&"test".to_string()));
    assert_eq!(ctx.get_env("NONEXISTENT"), None);
}

#[tokio::test]
async fn test_fezz_error_conversion() {
    let error = FezzError::not_found("Item not found");
    let response: FezzResponse = error.into();
    
    assert_eq!(response.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_status_code_helpers() {
    assert!(StatusCode::OK.is_success());
    assert!(StatusCode::CREATED.is_success());
    assert!(!StatusCode::NOT_FOUND.is_success());
    
    assert!(StatusCode::BAD_REQUEST.is_client_error());
    assert!(StatusCode::NOT_FOUND.is_client_error());
    assert!(!StatusCode::OK.is_client_error());
    
    assert!(StatusCode::INTERNAL_SERVER_ERROR.is_server_error());
    assert!(StatusCode::BAD_GATEWAY.is_server_error());
    assert!(!StatusCode::OK.is_server_error());
}

#[tokio::test]
async fn test_method_display() {
    assert_eq!(Method::Get.to_string(), "GET");
    assert_eq!(Method::Post.to_string(), "POST");
    assert_eq!(Method::Put.to_string(), "PUT");
    assert_eq!(Method::Delete.to_string(), "DELETE");
}

/// Test the load-unload lifecycle of a function.
struct LifecycleTestFunction {
    loaded: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl LifecycleTestFunction {
    fn new(loaded: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self { loaded }
    }
}

#[async_trait]
impl FezzFunction for LifecycleTestFunction {
    async fn on_load(&mut self, _ctx: &FunctionContext) -> Result<(), FezzError> {
        self.loaded.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
    
    async fn fetch(
        &self,
        _request: FezzRequest,
        _ctx: &FunctionContext,
    ) -> Result<FezzResponse, FezzError> {
        Ok(FezzResponse::ok())
    }
    
    async fn on_unload(&mut self, _ctx: &FunctionContext) -> Result<(), FezzError> {
        self.loaded.store(false, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
    
    fn name(&self) -> &str {
        "lifecycle"
    }
}

#[tokio::test]
async fn test_function_load_unload_lifecycle() {
    use fezz::function::registry::FunctionState;
    
    let loaded = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let registry = FunctionRegistry::new();
    
    registry
        .register("lifecycle", Box::new(LifecycleTestFunction::new(loaded.clone())))
        .await
        .unwrap();
    
    // Initially unloaded
    assert_eq!(registry.get_state("lifecycle").await, Some(FunctionState::Unloaded));
    assert!(!loaded.load(std::sync::atomic::Ordering::SeqCst));
    
    // Load the function
    registry.load("lifecycle").await.unwrap();
    assert_eq!(registry.get_state("lifecycle").await, Some(FunctionState::Ready));
    assert!(loaded.load(std::sync::atomic::Ordering::SeqCst));
    
    // Unload the function
    registry.unload("lifecycle").await.unwrap();
    assert_eq!(registry.get_state("lifecycle").await, Some(FunctionState::Unloaded));
    assert!(!loaded.load(std::sync::atomic::Ordering::SeqCst));
}
