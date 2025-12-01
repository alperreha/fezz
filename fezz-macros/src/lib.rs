use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn fezz_function(_args: TokenStream, input: TokenStream) -> TokenStream {
    let func = parse_macro_input!(input as ItemFn);
    let func_name = &func.sig.ident;

    // fezz_fetch wrapper’ı generate et
    let expanded = quote! {
        #func

        #[no_mangle]
        pub extern "C" fn fezz_fetch(req_json: *const std::os::raw::c_char) -> *mut std::os::raw::c_char {
            use std::ffi::{CStr, CString};
            use fezz_sdk::{FezzHttpRequest, FezzHttpResponse};

            // C char* -> String
            let c_str = unsafe { CStr::from_ptr(req_json) };
            let req_str = c_str.to_str().unwrap_or("{}");
            let req: FezzHttpRequest = serde_json::from_str(req_str).unwrap();

            // user function çağrısı
            let resp: FezzHttpResponse = #func_name(req);

            let resp_str = serde_json::to_string(&resp).unwrap();
            let c_string = CString::new(resp_str).unwrap();
            c_string.into_raw()
        }
    };

    expanded.into()
}
