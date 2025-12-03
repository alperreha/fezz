use axum::{
    body::Bytes,
    extract::Path,
    http::{HeaderMap, Method},
    routing::{delete, get, patch, post, put},
    Router,
};
use fezz_sdk::{FezzHttpRequest, FezzHttpResponse};
use libloading::{Library, Symbol};
use serde::Deserialize;
use std::{ffi::{CString, CStr}, os::raw::c_char, sync::Arc};
use tokio::net::TcpListener;

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

type FezzFetchFn = unsafe extern "C" fn(*const c_char) -> *mut c_char;

#[tokio::main]
async fn main() {
    // HHRF_ROOT env'den gelsin
    let root = std::env::var("HHRF_ROOT").unwrap_or_else(|_| "./HHRF_ROOT".into());
    let shared_root = Arc::new(root);

    let app = Router::new()
        .route("/rpc/:id", get({
            let root = shared_root.clone();
            move |Path(id): Path<String>, method: Method, headers: HeaderMap| {
                handle_rpc(root.clone(), id, None, method, headers, None)
            }
        }))
        .route("/rpc/:id", post({
            let root = shared_root.clone();
            move |Path(id): Path<String>, method: Method, headers: HeaderMap, body: Bytes| {
                handle_rpc(root.clone(), id, None, method, headers, Some(body))
            }
        }))
        .route("/rpc/:id", put({
            let root = shared_root.clone();
            move |Path(id): Path<String>, method: Method, headers: HeaderMap, body: Bytes| {
                handle_rpc(root.clone(), id, None, method, headers, Some(body))
            }
        }))
        .route("/rpc/:id", patch({
            let root = shared_root.clone();
            move |Path(id): Path<String>, method: Method, headers: HeaderMap, body: Bytes| {
                handle_rpc(root.clone(), id, None, method, headers, Some(body))
            }
        }))
        .route("/rpc/:id", delete({
            let root = shared_root.clone();
            move |Path(id): Path<String>, method: Method, headers: HeaderMap, body: Bytes| {
                handle_rpc(root.clone(), id, None, method, headers, Some(body))
            }
        }))
        .route("/rpc/:id/*rest", get({
            let root = shared_root.clone();
            move |Path((id, rest)): Path<(String, String)>, method: Method, headers: HeaderMap| {
                handle_rpc(root.clone(), id, Some(rest), method, headers, None)
            }
        }))
        .route("/rpc/:id/*rest", post({
            let root = shared_root.clone();
            move |Path((id, rest)): Path<(String, String)>, method: Method, headers: HeaderMap, body: Bytes| {
                handle_rpc(root.clone(), id, Some(rest), method, headers, Some(body))
            }
        }))
        .route("/rpc/:id/*rest", put({
            let root = shared_root.clone();
            move |Path((id, rest)): Path<(String, String)>, method: Method, headers: HeaderMap, body: Bytes| {
                handle_rpc(root.clone(), id, Some(rest), method, headers, Some(body))
            }
        }))
        .route("/rpc/:id/*rest", patch({
            let root = shared_root.clone();
            move |Path((id, rest)): Path<(String, String)>, method: Method, headers: HeaderMap, body: Bytes| {
                handle_rpc(root.clone(), id, Some(rest), method, headers, Some(body))
            }
        }))
        .route("/rpc/:id/*rest", delete({
            let root = shared_root.clone();
            move |Path((id, rest)): Path<(String, String)>, method: Method, headers: HeaderMap, body: Bytes| {
                handle_rpc(root.clone(), id, Some(rest), method, headers, Some(body))
            }
        }));

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handle_rpc(
    root: Arc<String>,
    id: String,
    rest_path: Option<String>,
    method: Method,
    headers: HeaderMap,
    body: Option<Bytes>,
) -> axum::response::Response {
    let start_time = std::time::Instant::now();

    // 1) fezz.json oku
    let manifest_path = format!("{root}/functions/{id}/fezz.json");
    println!("[HHRF] Loading manifest: {}", manifest_path);
    let manifest_str = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: FezzManifest = serde_json::from_str(&manifest_str).unwrap();
    println!("[HHRF] Manifest loaded: id={}, version={}, entry={}", manifest.id, manifest.version, manifest.entry);
    let manifest_load_time = start_time.elapsed();
    println!("[HHRF] Manifest load time: {:?}", manifest_load_time);
    let so_path = format!("{root}/functions/{id}/{}", manifest.entry);
    println!("[HHRF] Loading .so: {}", so_path);

    // 2) .so load et
    let lib_load_start = std::time::Instant::now();
    let lib = unsafe { Library::new(&so_path).unwrap() };
    let fezz_fetch: Symbol<FezzFetchFn> = unsafe { lib.get(b"fezz_fetch").unwrap() };
    let lib_load_time = lib_load_start.elapsed();
    println!("[HHRF] Library load time: {:?}", lib_load_time);

    // debug log for id and rest_path
    println!("[HHRF] Handling RPC for id='{}' with rest_path='{:?}'", id, rest_path);

    // 3) Axum'dan gelen headers'ı Vec<(String, String)>'e çevir
    let fezz_headers: Vec<(String, String)> = headers
        .iter()
        .filter_map(|(k, v)| {
            v.to_str().ok().map(|val| (k.as_str().to_string(), val.to_string()))
        })
        .collect();

    // 4) Body'yi String'e çevir
    let fezz_body: Option<String> = body.and_then(|b| {
        if b.is_empty() {
            None
        } else {
            String::from_utf8(b.to_vec()).ok()
        }
    });

    // 5) FezzHttpRequest oluştur (gerçek method, headers, body ile)
    let req = FezzHttpRequest {
        method: method.as_str().to_string(),
        path: match rest_path {
            Some(p) => format!("/{}", p),
            None => manifest.routes[0].path.clone(),
        },
        headers: fezz_headers,
        body: fezz_body,
    };

    println!("[HHRF] FezzHttpRequest: method={}, path={}, headers_count={}, has_body={}", 
        req.method, req.path, req.headers.len(), req.body.is_some());

    let req_json = serde_json::to_string(&req).unwrap();
    let c_req = CString::new(req_json).unwrap();

    // 6) fezz_fetch çağır
    let fetch_start = std::time::Instant::now();
    let raw_ptr = unsafe { fezz_fetch(c_req.as_ptr()) };
    let fetch_time = fetch_start.elapsed();
    println!("[HHRF] fezz_fetch execution time: {:?}", fetch_time);

    // 7) C char* → String
    let c_str = unsafe { CStr::from_ptr(raw_ptr) };
    let resp_str = c_str.to_str().unwrap().to_string();

    // (Not: CString::from_raw(raw_ptr) ile free etmek gerekir; demo için geçiyoruz.)

    let fezz_resp: FezzHttpResponse = serde_json::from_str(&resp_str).unwrap();

    // 8) HTTP response'a çevir
    let mut http_resp = axum::response::Response::builder()
        .status(fezz_resp.status);

    for (k, v) in &fezz_resp.headers {
        http_resp = http_resp.header(k, v);
    }

    let body = fezz_resp.body.unwrap_or_default();

    let total_time = start_time.elapsed();
    println!("[HHRF] Total request time for '{}': {:?}", id, total_time);

    http_resp.body(axum::body::Body::from(body)).unwrap()
}
