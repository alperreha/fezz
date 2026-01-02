use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn fezz_function(_args: TokenStream, input: TokenStream) -> TokenStream {
    let func = parse_macro_input!(input as ItemFn);
    let func_name = &func.sig.ident;

    // Generate bytes-first ABI wrapper with panic safety.
    let expanded = quote! {
        #func

        /// FFI entry point for the Fezz function (bytes-first ABI).
        ///
        /// # Safety
        ///
        /// The caller must ensure that `req` points to a valid byte slice of length `len`.
        #[no_mangle]
        pub unsafe extern "C" fn fezz_handle_v2(req: fezz_sdk::FezzSlice) -> fezz_sdk::FezzOwned {
            use fezz_sdk::{FezzWireHeader, FezzWireResponse};

            // Wrap the entire function body in catch_unwind to prevent panics
            // from crossing the FFI boundary (which is undefined behavior).
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if req.ptr.is_null() && req.len != 0 {
                    return Err("Null request pointer with non-zero length".to_string());
                }

                let req_bytes = if req.len == 0 {
                    &[][..]
                } else {
                    std::slice::from_raw_parts(req.ptr, req.len)
                };

                let req = match fezz_sdk::decode_request(req_bytes) {
                    Ok(r) => r,
                    Err(e) => {
                    let resp = FezzWireResponse::new(
                        400,
                        vec![FezzWireHeader::new("content-type", "application/json")],
                        format!("{{\"error\":\"Invalid request: {}\"}}", e).into_bytes(),
                    );
                    return Ok(resp);
                }
            };

                // Call user function
                Ok(#func_name(req))
            }));

            let resp: FezzWireResponse = match result {
                Ok(Ok(r)) => r,
                Ok(Err(message)) => FezzWireResponse::new(
                    400,
                    vec![FezzWireHeader::new("content-type", "application/json")],
                    format!("{{\"error\":\"{}\"}}", message).into_bytes(),
                ),
                Err(panic_info) => {
                    // A panic occurred - return an error response instead of crashing
                    let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = panic_info.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "Unknown panic".to_string()
                    };

                    FezzWireResponse::new(
                        500,
                        vec![FezzWireHeader::new("content-type", "application/json")],
                        format!("{{\"error\":\"Function panicked: {}\"}}", panic_msg).into_bytes(),
                    )
                }
            };

            let mut resp_bytes = match fezz_sdk::encode_response(&resp) {
                Ok(b) => b,
                Err(_) => Vec::new(),
            };
            let len = resp_bytes.len();
            let ptr = resp_bytes.as_mut_ptr();
            std::mem::forget(resp_bytes);
            fezz_sdk::FezzOwned { ptr, len }
        }

        #[no_mangle]
        pub unsafe extern "C" fn fezz_free_v2(buf: fezz_sdk::FezzOwned) {
            if buf.ptr.is_null() {
                return;
            }
            let _ = Vec::from_raw_parts(buf.ptr, buf.len, buf.len);
        }
    };

    expanded.into()
}
