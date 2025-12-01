use axum::{extract::Path, routing::get, Router};
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
    // HHRF_ROOT env’den gelsin
    let root = std::env::var("HHRF_ROOT").unwrap_or_else(|_| "./HHRF_ROOT".into());
    let shared_root = Arc::new(root);

    let app = Router::new()
        .route("/rpc/latest/:id", get({
            let root = shared_root.clone();
            move |Path(id): Path<String>| handle_rpc(root.clone(), id)
        }));

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handle_rpc(root: Arc<String>, id: String) -> axum::response::Response {
    // 1) fezz.json oku
    let manifest_path = format!("{root}/functions/{id}/fezz.json");
    let manifest_str = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: FezzManifest = serde_json::from_str(&manifest_str).unwrap();

    let so_path = format!("{root}/functions/{id}/{}", manifest.entry);

    // 2) .so load et
    let lib = unsafe { Library::new(&so_path).unwrap() };
    let fezz_fetch: Symbol<FezzFetchFn> = unsafe { lib.get(b"fezz_fetch").unwrap() };

    // 3) Demo için sabit bir FezzHttpRequest oluştur (/todos GET)
    let req = FezzHttpRequest {
        method: "GET".into(),
        path: "/todos".into(),
        headers: vec![],
        body: None,
    };

    let req_json = serde_json::to_string(&req).unwrap();
    let c_req = CString::new(req_json).unwrap();

    // 4) fezz_fetch çağır
    let raw_ptr = unsafe { fezz_fetch(c_req.as_ptr()) };

    // 5) C char* → String
    let c_str = unsafe { CStr::from_ptr(raw_ptr) };
    let resp_str = c_str.to_str().unwrap().to_string();

    // (Not: CString::from_raw(raw_ptr) ile free etmek gerekir; demo için geçiyoruz.)

    let fezz_resp: FezzHttpResponse = serde_json::from_str(&resp_str).unwrap();

    // 6) HTTP response’a çevir
    let mut http_resp = axum::response::Response::builder()
        .status(fezz_resp.status);

    for (k, v) in &fezz_resp.headers {
        http_resp = http_resp.header(k, v);
    }

    let body = fezz_resp.body.unwrap_or_default();
    http_resp.body(axum::body::Body::from(body)).unwrap()
}
