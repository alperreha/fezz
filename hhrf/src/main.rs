use axum::{extract::Path, routing::get, Router};
use fezz_sdk::{FezzHttpRequest, FezzHttpResponse};
use libloading::{Library, Symbol};
use serde::Deserialize;
use std::{
    collections::HashMap,
    ffi::{CStr, CString},
    os::raw::c_char,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{net::TcpListener, sync::RwLock};

/// Time-To-Live for cached libraries: libraries idle longer than this are unloaded.
const CACHE_IDLE_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

/// Interval for running the cache cleanup task.
const CACHE_CLEANUP_INTERVAL: Duration = Duration::from_secs(60); // 1 minute

/// A cached library entry containing the loaded library and usage timestamp.
struct CachedLibrary {
    library: Arc<Library>,
    last_used: Instant,
}

/// Global library cache type.
type LibraryCache = Arc<RwLock<HashMap<String, CachedLibrary>>>;

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

    // Initialize the global library cache
    let library_cache: LibraryCache = Arc::new(RwLock::new(HashMap::new()));

    // Spawn background task for TTL-based cache cleanup
    spawn_cache_cleanup_task(library_cache.clone());

    let app = Router::new().route(
        "/rpc/:id",
        get({
            let root = shared_root.clone();
            let cache = library_cache.clone();
            move |Path(id): Path<String>| handle_rpc(root.clone(), cache.clone(), id)
        }),
    );

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

/// Spawns a background task that periodically cleans up idle libraries from the cache.
fn spawn_cache_cleanup_task(cache: LibraryCache) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(CACHE_CLEANUP_INTERVAL).await;
            cleanup_expired_libraries(&cache).await;
        }
    });
}

/// Removes libraries from the cache that have not been used within the idle timeout.
async fn cleanup_expired_libraries(cache: &LibraryCache) {
    let now = Instant::now();
    let mut cache_guard = cache.write().await;
    let before_count = cache_guard.len();

    cache_guard.retain(|id, entry| {
        let age = now.duration_since(entry.last_used);
        if age > CACHE_IDLE_TIMEOUT {
            println!(
                "[HHRF] Evicting idle library '{}' (idle for {:?})",
                id, age
            );
            false
        } else {
            true
        }
    });

    let evicted = before_count - cache_guard.len();
    if evicted > 0 {
        println!(
            "[HHRF] Cache cleanup: evicted {} libraries, {} remaining",
            evicted,
            cache_guard.len()
        );
    }
}

async fn handle_rpc(
    root: Arc<String>,
    cache: LibraryCache,
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
    let cache_key = format!("{}:{}", id, manifest.entry);

    // 2) Try to get library from cache, or load and cache it
    let lib_load_start = Instant::now();
    let library = match get_or_load_library(&cache, &cache_key, &so_path).await {
        Ok(lib) => lib,
        Err(e) => {
            return error_response(500, format!("Failed to load library: {}", e));
        }
    };
    let lib_load_time = lib_load_start.elapsed();
    println!("[HHRF] Library access time: {:?}", lib_load_time);

    // 3) Create demo FezzHttpRequest
    let req = FezzHttpRequest {
        method: manifest.routes[0].method.clone(),
        path: manifest.routes[0].path.clone(),
        headers: vec![],
        body: None,
    };

    let req_json = serde_json::to_string(&req).unwrap();
    let c_req = CString::new(req_json).unwrap();

    // 4) Call fezz_fetch with panic safety barrier using spawn_blocking
    let fetch_start = Instant::now();
    let result = tokio::task::spawn_blocking(move || {
        // SAFETY: FFI calls are inherently unsafe. We use catch_unwind in the macro
        // to prevent panics from crossing the FFI boundary.
        let fezz_fetch: Symbol<FezzFetchFn> = unsafe {
            match library.get(b"fezz_fetch") {
                Ok(sym) => sym,
                Err(e) => {
                    return Err(format!("Failed to get fezz_fetch symbol: {}", e));
                }
            }
        };

        let raw_ptr = unsafe { fezz_fetch(c_req.as_ptr()) };

        if raw_ptr.is_null() {
            return Err("fezz_fetch returned null pointer".to_string());
        }

        // Convert C char* to String
        let c_str = unsafe { CStr::from_ptr(raw_ptr) };
        let resp_str = match c_str.to_str() {
            Ok(s) => s.to_string(),
            Err(e) => {
                return Err(format!("Invalid UTF-8 in response: {}", e));
            }
        };

        // Free the C string allocated by the plugin.
        // The fezz_fetch function (generated by the fezz_function macro) allocates
        // the response using CString::into_raw(), so we must free it with CString::from_raw().
        // This is part of the FFI contract between the host and plugins.
        unsafe {
            let _ = CString::from_raw(raw_ptr);
        }

        Ok(resp_str)
    })
    .await;

    let fetch_time = fetch_start.elapsed();
    println!("[HHRF] fezz_fetch execution time: {:?}", fetch_time);

    // Handle spawn_blocking result
    let resp_str = match result {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            return error_response(500, format!("Function execution error: {}", e));
        }
        Err(e) => {
            return error_response(500, format!("Task panicked: {}", e));
        }
    };

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

/// Gets a library from the cache or loads it if not present.
/// Updates the last_used timestamp on cache hit.
async fn get_or_load_library(
    cache: &LibraryCache,
    cache_key: &str,
    so_path: &str,
) -> Result<Arc<Library>, String> {
    // Try to get from cache with write lock (we'll update last_used anyway)
    {
        let mut cache_guard = cache.write().await;
        if let Some(entry) = cache_guard.get_mut(cache_key) {
            println!("[HHRF] Cache hit for '{}'", cache_key);
            // Update last_used timestamp atomically with the lookup
            entry.last_used = Instant::now();
            return Ok(entry.library.clone());
        }
    }

    // Cache miss - need to load the library
    println!("[HHRF] Cache miss for '{}', loading .so: {}", cache_key, so_path);

    let library = unsafe {
        Library::new(so_path).map_err(|e| format!("Failed to load {}: {}", so_path, e))?
    };
    let library = Arc::new(library);

    // Insert into cache
    {
        let mut cache_guard = cache.write().await;
        // Double-check in case another task loaded it while we were loading
        if let Some(entry) = cache_guard.get_mut(cache_key) {
            println!("[HHRF] Another task loaded '{}' concurrently", cache_key);
            entry.last_used = Instant::now();
            return Ok(entry.library.clone());
        }

        cache_guard.insert(
            cache_key.to_string(),
            CachedLibrary {
                library: library.clone(),
                last_used: Instant::now(),
            },
        );
        println!(
            "[HHRF] Library '{}' cached (total cached: {})",
            cache_key,
            cache_guard.len()
        );
    }

    Ok(library)
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
