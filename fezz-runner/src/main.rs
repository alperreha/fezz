use fezz_sdk::{FezzOwned, FezzSlice};
use libloading::{Library, Symbol};
use std::io::{Read, Write};
use std::process::exit;

// Same ABI as in HHRF and fezz-macros
type FezzHandleV2Fn = unsafe extern "C" fn(FezzSlice) -> FezzOwned;
type FezzFreeV2Fn = unsafe extern "C" fn(FezzOwned);

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

    // Read request bytes from stdin
    let mut buf = Vec::new();
    if let Err(e) = std::io::stdin().read_to_end(&mut buf) {
        eprintln!("Failed to read stdin: {}", e);
        exit(1);
    }

    eprintln!(
        "[fezz-runner] stdin read ok, bytes={}", 
        buf.len()
    );

    // Parse into FezzWireRequest just to validate; we then pass raw bytes to plugin
    if let Err(e) = fezz_sdk::decode_request(&buf) {
        eprintln!("Invalid request bytes: {}", e);
        exit(1);
    }

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

    // Resolve fezz_handle_v2 and fezz_free_v2 symbols
    let fezz_handle_v2: Symbol<FezzHandleV2Fn> = unsafe {
        match library.get(b"fezz_handle_v2") {
            Ok(sym) => {
                eprintln!("[fezz-runner] fezz_handle_v2 symbol resolved");
                sym
            }
            Err(e) => {
                eprintln!("Failed to get fezz_handle_v2 symbol: {}", e);
                exit(1);
            }
        }
    };

    let fezz_free_v2: Symbol<FezzFreeV2Fn> = unsafe {
        match library.get(b"fezz_free_v2") {
            Ok(sym) => {
                eprintln!("[fezz-runner] fezz_free_v2 symbol resolved");
                sym
            }
            Err(e) => {
                eprintln!("Failed to get fezz_free_v2 symbol: {}", e);
                exit(1);
            }
        }
    };

    // Call function
    eprintln!("[fezz-runner] calling fezz_handle_v2");
    let owned = unsafe { fezz_handle_v2(FezzSlice { ptr: buf.as_ptr(), len: buf.len() }) };
    if owned.ptr.is_null() && owned.len != 0 {
        eprintln!("fezz_handle_v2 returned null pointer");
        exit(1);
    }

    let resp_bytes = if owned.len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(owned.ptr, owned.len).to_vec() }
    };

    // Free response buffer allocated in plugin
    unsafe {
        fezz_free_v2(owned);
    }

    // Validate that it is a FezzWireResponse (optional but nice)
    if let Err(e) = fezz_sdk::decode_response(&resp_bytes) {
        eprintln!("Invalid response bytes from plugin: {}", e);
        exit(1);
    }

    // Write raw bytes to stdout for HHRF to consume
    eprintln!(
        "[fezz-runner] writing response bytes to stdout, bytes={}",
        resp_bytes.len()
    );

    if let Err(e) = std::io::stdout().write_all(&resp_bytes) {
        eprintln!("Failed to write stdout: {}", e);
        exit(1);
    }

    eprintln!("[fezz-runner] finished successfully");
}
