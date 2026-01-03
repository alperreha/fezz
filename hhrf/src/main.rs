use axum::{
    extract::Path,
    http::{HeaderName, HeaderValue, Request},
    routing::any,
    Router,
};
use fezz_sdk::{ByteBuf, FezzWireHeader, FezzWireRequest, FezzWireResponse};
use http_body_util::BodyExt;
use libloading::{Library, Symbol};
use std::{
    path::Path as FsPath,
    sync::Arc,
    time::Instant,
};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    // HHRF_ROOT env'den gelsin
    let root = std::env::var("HHRF_ROOT").unwrap_or_else(|_| "./HHRF_ROOT".into());
    let shared_root = Arc::new(root);

    let app = Router::new().route(
        "/rpc/:org/:func/:version/*tail",
        any({
            let root = shared_root.clone();
            move |Path((org, func, version, _tail)): Path<(String, String, String, String)>,
                  req: Request<axum::body::Body>| {
                handle_rpc(root.clone(), org, func, version, req)
            }
        }),
    );

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handle_rpc(
    root: Arc<String>,
    org: String,
    func: String,
    version: String,
    req: Request<axum::body::Body>,
) -> axum::response::Response {
    let start_time = Instant::now();

    let (parts, body) = req.into_parts();
    let body_bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) => {
            return error_response(400, format!("Failed to read request body: {}", e));
        }
    };

    let prefix = format!("/rpc/{org}/{func}/{version}");
    let mut stripped_path = parts
        .uri
        .path()
        .strip_prefix(&prefix)
        .unwrap_or(parts.uri.path())
        .to_string();
    if stripped_path.is_empty() {
        stripped_path = "/".to_string();
    }
    if !stripped_path.starts_with('/') {
        stripped_path = format!("/{stripped_path}");
    }
    let path_and_query = if let Some(query) = parts.uri.query() {
        format!("{stripped_path}?{query}")
    } else {
        stripped_path
    };

    let function_root = format!("{root}/functions/{org}/{func}/{version}");
    let so_path = format!("{function_root}/fezz.so");
    let env_path = format!("{function_root}/.env");

    let env_vars = load_env_vars(&env_path);

    let headers = parts
        .headers
        .iter()
        .map(|(name, value)| FezzWireHeader::new(name.as_str(), value.as_bytes()))
        .collect::<Vec<_>>();

    // 2) Create FezzWireRequest to send to the function process
    let wire_req = FezzWireRequest {
        method: parts.method.to_string(),
        scheme: parts.uri.scheme_str().map(|s| s.to_string()),
        authority: parts
            .uri
            .authority()
            .map(|authority| authority.as_str().to_string())
            .or_else(|| {
                parts
                    .headers
                    .get(axum::http::header::HOST)
                    .and_then(|value| value.to_str().ok())
                    .map(|value| value.to_string())
            }),
        path_and_query,
        headers,
        body: ByteBuf::from(body_bytes.to_vec()),
        meta: None,
    };

    let req_bytes = match fezz_sdk::encode_request(&wire_req) {
        Ok(bytes) => bytes,
        Err(e) => {
            return error_response(500, format!("Failed to serialize request: {}", e));
        }
    };

    // 3) Execute function in-process via libloading
    let fetch_start = Instant::now();
    let resp_bytes = match execute_in_process(&so_path, &req_bytes, env_vars).await {
        Ok(bytes) => bytes,
        Err(e) => {
            return error_response(500, format!("Function execution error: {}", e));
        }
    };

    let fetch_time = fetch_start.elapsed();
    println!("[HHRF] external function execution time: {:?}", fetch_time);

    let fezz_resp: FezzWireResponse = match fezz_sdk::decode_response(&resp_bytes) {
        Ok(r) => r,
        Err(e) => {
            return error_response(500, format!("Invalid response bytes: {}", e));
        }
    };

    // 5) Convert to HTTP response
    let mut http_resp = axum::response::Response::builder().status(fezz_resp.status);

    for header in &fezz_resp.headers {
        let name = match HeaderName::from_bytes(&header.name) {
            Ok(name) => name,
            Err(_) => {
                println!("[HHRF] Skipping invalid header name");
                continue;
            }
        };
        let value = match HeaderValue::from_bytes(&header.value) {
            Ok(value) => value,
            Err(_) => {
                println!("[HHRF] Skipping invalid header value");
                continue;
            }
        };
        http_resp = http_resp.header(name, value);
    }

    let body = fezz_resp.body.into_vec();

    let total_time = start_time.elapsed();
    println!(
        "[HHRF] Total request time for '{}/{}/{}': {:?}",
        org, func, version, total_time
    );

    http_resp.body(axum::body::Body::from(body)).unwrap()
}

/// Execute a Fezz function in-process via libloading.
///
/// `so_path` is the path to the dynamic library containing `fezz_handle_v2`.
/// `req_bytes` is the FezzWireRequest bytes passed to the plugin.
async fn execute_in_process(
    so_path: &str,
    req_bytes: &[u8],
    env_vars: Vec<(String, String)>,
) -> Result<Vec<u8>, String> {
    if !FsPath::new(so_path).exists() {
        return Err(format!("Library not found at {}", so_path));
    }

    let so_path = so_path.to_string();
    let req_bytes = req_bytes.to_vec();

    tokio::task::spawn_blocking(move || unsafe {
        type FezzHandleV2Fn = unsafe extern "C" fn(fezz_sdk::FezzSlice) -> fezz_sdk::FezzOwned;
        type FezzFreeV2Fn = unsafe extern "C" fn(fezz_sdk::FezzOwned);

        for (key, value) in env_vars {
            std::env::set_var(key, value);
        }

        let library = Library::new(&so_path)
            .map_err(|e| format!("Failed to load library '{}': {}", so_path, e))?;

        let fezz_handle_v2: Symbol<FezzHandleV2Fn> = library
            .get(b"fezz_handle_v2")
            .map_err(|e| format!("Failed to resolve fezz_handle_v2: {}", e))?;

        let fezz_free_v2: Symbol<FezzFreeV2Fn> = library
            .get(b"fezz_free_v2")
            .map_err(|e| format!("Failed to resolve fezz_free_v2: {}", e))?;

        let owned = fezz_handle_v2(fezz_sdk::FezzSlice {
            ptr: req_bytes.as_ptr(),
            len: req_bytes.len(),
        });

        if owned.ptr.is_null() && owned.len != 0 {
            return Err("fezz_handle_v2 returned null pointer".to_string());
        }

        let resp_bytes = if owned.len == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(owned.ptr, owned.len).to_vec()
        };

        fezz_free_v2(owned);

        Ok(resp_bytes)
    })
    .await
    .map_err(|e| format!("Failed to join blocking task: {}", e))?
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

fn load_env_vars(env_path: &str) -> Vec<(String, String)> {
    let contents = match std::fs::read_to_string(env_path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(err) => {
            println!("[HHRF] Failed to read env file '{}': {}", env_path, err);
            return Vec::new();
        }
    };

    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            let value = value.trim().trim_matches('"').trim_matches('\'');
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}
