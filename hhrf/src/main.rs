use axum::{extract::Path, routing::get, Router};
use fezz_sdk::{FezzHttpRequest, FezzHttpResponse};
use serde::Deserialize;
use std::{
    sync::Arc,
    time::Instant,
};
use tokio::{
    net::TcpListener,
    process::Command,
};
use tokio::io::{AsyncWriteExt, AsyncReadExt};

#[derive(Deserialize)]
struct FezzManifest {
    id: String,
    version: String,
    entry: String,
    routes: Vec<FezzRoute>,
}

#[derive(Deserialize)]
struct FezzRoute {
    path: String,
    method: String,
}

#[tokio::main]
async fn main() {
    // HHRF_ROOT env'den gelsin
    let root = std::env::var("HHRF_ROOT").unwrap_or_else(|_| "./HHRF_ROOT".into());
    let shared_root = Arc::new(root);

    let app = Router::new().route(
        "/rpc/:id",
        get({
            let root = shared_root.clone();
            move |Path(id): Path<String>| handle_rpc(root.clone(), id)
        }),
    );

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handle_rpc(
    root: Arc<String>,
    id: String,
) -> axum::response::Response {
    let start_time = Instant::now();

    // 1) Read fezz.json manifest
    let manifest_path = format!("{root}/functions/{id}/fezz.json");
    println!("[HHRF] Loading manifest: {}", manifest_path);
    let manifest_str = match std::fs::read_to_string(&manifest_path) {
        Ok(s) => s,
        Err(e) => {
            return error_response(404, format!("Manifest not found: {}", e));
        }
    };
    let manifest: FezzManifest = match serde_json::from_str(&manifest_str) {
        Ok(m) => m,
        Err(e) => {
            return error_response(400, format!("Invalid manifest: {}", e));
        }
    };
    println!(
        "[HHRF] Manifest loaded: id={}, version={}, entry={}",
        manifest.id, manifest.version, manifest.entry
    );
    let manifest_load_time = start_time.elapsed();
    println!("[HHRF] Manifest load time: {:?}", manifest_load_time);

    let so_path = format!("{root}/functions/{id}/{}", manifest.entry);

    // 2) Create FezzHttpRequest to send to the function process
    let req = FezzHttpRequest {
        method: manifest.routes[0].method.clone(),
        path: manifest.routes[0].path.clone(),
        headers: vec![],
        body: None,
    };

    let req_json = match serde_json::to_string(&req) {
        Ok(j) => j,
        Err(e) => {
            return error_response(500, format!("Failed to serialize request: {}", e));
        }
    };

    // 3) Execute function in an external process via fezz-runner
    let fetch_start = Instant::now();
    let resp_str = match execute_in_process(&so_path, &req_json).await {
        Ok(s) => s,
        Err(e) => {
            return error_response(500, format!("Function execution error: {}", e));
        }
    };

    let fetch_time = fetch_start.elapsed();
    println!("[HHRF] external function execution time: {:?}", fetch_time);

    let fezz_resp: FezzHttpResponse = match serde_json::from_str(&resp_str) {
        Ok(r) => r,
        Err(e) => {
            return error_response(500, format!("Invalid response JSON: {}", e));
        }
    };

    // 5) Convert to HTTP response
    let mut http_resp = axum::response::Response::builder().status(fezz_resp.status);

    for (k, v) in &fezz_resp.headers {
        http_resp = http_resp.header(k, v);
    }

    let body = fezz_resp.body.unwrap_or_default();

    let total_time = start_time.elapsed();
    println!("[HHRF] Total request time for '{}': {:?}", id, total_time);

    http_resp.body(axum::body::Body::from(body)).unwrap()
}

/// Execute a Fezz function as an external process via the fezz-runner helper.
///
/// `so_path` is the path to the dynamic library containing `fezz_fetch`.
/// `req_json` is the FezzHttpRequest JSON string to send on stdin.
async fn execute_in_process(so_path: &str, req_json: &str) -> Result<String, String> {
    // Allow overriding runner binary via env, so users can wrap it in a jailer
    // like nsjail / firejail / bwrap on Linux.
    let runner = std::env::var("FEZZ_RUNNER").unwrap_or_else(|_| "fezz-runner".to_string());

    println!("[HHRF] Spawning runner '{}' for {}", runner, so_path);

    let mut child = Command::new(&runner)
        .arg(so_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn runner '{}': {}", runner, e))?;

    // Write request JSON to stdin
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(req_json.as_bytes())
            .await
            .map_err(|e| format!("Failed to write to runner stdin: {}", e))?;
    } else {
        return Err("Runner stdin not available".to_string());
    }

    // Read response JSON from stdout
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Runner stdout not available".to_string())?;

    let mut buf = Vec::new();
    stdout
        .read_to_end(&mut buf)
        .await
        .map_err(|e| format!("Failed to read runner stdout: {}", e))?;

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed to wait for runner: {}", e))?;

    if !status.success() {
        return Err(format!("Runner exited with status {}", status));
    }

    String::from_utf8(buf).map_err(|e| format!("Invalid UTF-8 from runner: {}", e))
}

/// Creates an error HTTP response with the given status code and message.
fn error_response(status: u16, message: String) -> axum::response::Response {
    println!("[HHRF] Error: {}", message);
    axum::response::Response::builder()
        .status(status)
        .header("content-type", "text/plain")
        .body(axum::body::Body::from(message))
        .unwrap()
}
