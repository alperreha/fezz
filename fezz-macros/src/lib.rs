use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn fezz_function(_args: TokenStream, input: TokenStream) -> TokenStream {
    let func = parse_macro_input!(input as ItemFn);
    let func_name = &func.sig.ident;

    // Generate fezz_fetch wrapper with panic safety
    // The function is marked unsafe because it accepts a raw pointer that must
    // be valid and point to a null-terminated C string.
    let expanded = quote! {
        #func

        /// FFI entry point for the Fezz function.
        ///
        /// # Safety
        ///
        /// The caller must ensure that `req_json` is a valid pointer to a
        /// null-terminated C string containing valid UTF-8 JSON data.
        ///
        /// # Memory
        ///
        /// The returned pointer is allocated using `CString::into_raw()` and must be
        /// freed by the caller using `CString::from_raw()`.
        #[no_mangle]
        pub unsafe extern "C" fn fezz_fetch(req_json: *const std::os::raw::c_char) -> *mut std::os::raw::c_char {
            use std::ffi::{CStr, CString};
            use fezz_sdk::{FezzHttpRequest, FezzHttpResponse};

            // Wrap the entire function body in catch_unwind to prevent panics
            // from crossing the FFI boundary (which is undefined behavior).
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // C char* -> String
                let c_str = CStr::from_ptr(req_json);
                let req_str = c_str.to_str().unwrap_or("{}");
                let req: FezzHttpRequest = match serde_json::from_str(req_str) {
                    Ok(r) => r,
                    Err(e) => {
                        return FezzHttpResponse {
                            status: 400,
                            headers: vec![("content-type".into(), "application/json".into())],
                            body: Some(format!("{{\"error\":\"Invalid request: {}\"}}", e)),
                        };
                    }
                };

                // Call user function
                #func_name(req)
            }));

            let resp: FezzHttpResponse = match result {
                Ok(r) => r,
                Err(panic_info) => {
                    // A panic occurred - return an error response instead of crashing
                    let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = panic_info.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "Unknown panic".to_string()
                    };

                    FezzHttpResponse {
                        status: 500,
                        headers: vec![("content-type".into(), "application/json".into())],
                        body: Some(format!("{{\"error\":\"Function panicked: {}\"}}", panic_msg)),
                    }
                }
            };

            let resp_str = match serde_json::to_string(&resp) {
                Ok(s) => s,
                Err(_) => {
                    // Fallback: construct a valid FezzHttpResponse JSON manually
                    // Headers are serialized as an array of [key, value] tuples
                    r#"{"status":500,"headers":[["content-type","application/json"]],"body":"{\"error\":\"Serialization failed\"}"}"#.to_string()
                }
            };
            let c_string = match CString::new(resp_str) {
                Ok(s) => s,
                Err(_) => {
                    // Fallback for CString creation failure
                    CString::new(r#"{"status":500,"headers":[["content-type","application/json"]],"body":"{\"error\":\"CString creation failed\"}"}"#).unwrap()
                }
            };
            c_string.into_raw()
        }
    };

    expanded.into()
}
