use fezz_sdk::{FezzHttpRequest, FezzHttpResponse};
use libloading::{Library, Symbol};
use std::ffi::{CStr, CString};
use std::io::{Read, Write};
use std::os::raw::c_char;
use std::process::exit;

// Same ABI as in HHRF and fezz-macros
type FezzFetchFn = unsafe extern "C" fn(*const c_char) -> *mut c_char;

fn main() {
    // Args: <path-to-dylib>
    let so_path = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("Usage: fezz-runner <path-to-dylib>");
            exit(1);
        }
    };

    eprintln!("[fezz-runner] starting, so_path={}", so_path);

    // Read request JSON from stdin
    let mut buf = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut buf) {
        eprintln!("Failed to read stdin: {}", e);
        exit(1);
    }

    eprintln!(
        "[fezz-runner] stdin read ok, bytes={}", 
        buf.len()
    );

    // Parse into FezzHttpRequest just to validate; we then pass raw JSON to plugin
    let _req: FezzHttpRequest = match serde_json::from_str(&buf) {
        Ok(r) => {
            eprintln!("[fezz-runner] request JSON parsed as FezzHttpRequest");
            r
        }
        Err(e) => {
            eprintln!("Invalid request JSON: {}", e);
            exit(1);
        }
    };

    // Load library
    eprintln!("[fezz-runner] loading library");

    let library = unsafe {
        match Library::new(&so_path) {
            Ok(lib) => {
                eprintln!("[fezz-runner] library loaded successfully");
                lib
            }
            Err(e) => {
                eprintln!("Failed to load {}: {}", so_path, e);
                exit(1);
            }
        }
    };

    // Resolve fezz_fetch symbol
    let fezz_fetch: Symbol<FezzFetchFn> = unsafe {
        match library.get(b"fezz_fetch") {
            Ok(sym) => {
                eprintln!("[fezz-runner] fezz_fetch symbol resolved");
                sym
            }
            Err(e) => {
                eprintln!("Failed to get fezz_fetch symbol: {}", e);
                exit(1);
            }
        }
    };

    // Call function
    let c_req = match CString::new(buf) {
        Ok(c) => {
            eprintln!("[fezz-runner] CString built from request JSON");
            c
        }
        Err(e) => {
            eprintln!("Failed to build CString from request: {}", e);
            exit(1);
        }
    };

    eprintln!("[fezz-runner] calling fezz_fetch");
    let raw_ptr = unsafe { fezz_fetch(c_req.as_ptr()) };
    if raw_ptr.is_null() {
        eprintln!("fezz_fetch returned null pointer");
        exit(1);
    }

    // Convert result to string
    let c_str = unsafe { CStr::from_ptr(raw_ptr) };
    let resp_str = match c_str.to_str() {
        Ok(s) => {
            eprintln!("[fezz-runner] response C string converted to UTF-8");
            s.to_string()
        }
        Err(e) => {
            eprintln!("Invalid UTF-8 in response: {}", e);
            exit(1);
        }
    };

    // Free response string allocated in plugin
    unsafe {
        let _ = CString::from_raw(raw_ptr);
    }

    // Validate that it is a FezzHttpResponse JSON (optional but nice)
    let _resp: FezzHttpResponse = match serde_json::from_str(&resp_str) {
        Ok(r) => {
            eprintln!("[fezz-runner] response JSON parsed as FezzHttpResponse");
            r
        }
        Err(e) => {
            eprintln!("Invalid response JSON from plugin: {}", e);
            exit(1);
        }
    };

    // Write raw JSON to stdout for HHRF to consume
    eprintln!("[fezz-runner] writing response JSON to stdout, bytes={}", resp_str.len());

    if let Err(e) = std::io::stdout().write_all(resp_str.as_bytes()) {
        eprintln!("Failed to write stdout: {}", e);
        exit(1);
    }

    eprintln!("[fezz-runner] finished successfully");
}
