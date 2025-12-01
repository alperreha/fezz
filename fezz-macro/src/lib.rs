//! Procedural macros for the Fezz serverless runtime.
//!
//! This crate provides the `#[fezz_function]` attribute macro for defining
//! Fezz functions with a Cloudflare Workers-like API.
//!
//! # Example
//!
//! ```ignore
//! use fezz::prelude::*;
//!
//! #[fezz_function(
//!     id = "hello-world",
//!     version = "v1",
//!     method = "GET",
//!     path = "/api/hello"
//! )]
//! async fn fetch(req: FezzRequest, ctx: &FunctionContext) -> Result<FezzResponse, FezzError> {
//!     Ok(FezzResponse::text("Hello, World!"))
//! }
//! ```

use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::{
    parse_macro_input, ItemFn, Expr, ExprLit, Lit, Meta,
    punctuated::Punctuated, Token,
};

/// Attributes for the `#[fezz_function]` macro.
#[derive(Default, Debug)]
struct FezzFunctionAttrs {
    /// Function identifier.
    id: Option<String>,
    /// Function version.
    version: Option<String>,
    /// HTTP method.
    method: Option<String>,
    /// URL path pattern.
    path: Option<String>,
    /// Optional timeout in seconds.
    timeout: Option<u64>,
    /// Optional description.
    description: Option<String>,
}

impl FezzFunctionAttrs {
    fn parse_meta_list(metas: Punctuated<Meta, Token![,]>) -> syn::Result<Self> {
        let mut attrs = FezzFunctionAttrs::default();

        for meta in metas {
            match meta {
                Meta::NameValue(nv) => {
                    let ident = nv.path.get_ident()
                        .ok_or_else(|| syn::Error::new_spanned(&nv.path, "expected identifier"))?
                        .to_string();
                    
                    // Extract literal value from expression
                    let lit = match &nv.value {
                        Expr::Lit(ExprLit { lit, .. }) => lit.clone(),
                        _ => return Err(syn::Error::new_spanned(&nv.value, "expected literal")),
                    };
                    
                    match ident.as_str() {
                        "id" => {
                            if let Lit::Str(lit_str) = lit {
                                attrs.id = Some(lit_str.value());
                            }
                        }
                        "version" => {
                            if let Lit::Str(lit_str) = lit {
                                attrs.version = Some(lit_str.value());
                            }
                        }
                        "method" => {
                            if let Lit::Str(lit_str) = lit {
                                attrs.method = Some(lit_str.value());
                            }
                        }
                        "path" => {
                            if let Lit::Str(lit_str) = lit {
                                attrs.path = Some(lit_str.value());
                            }
                        }
                        "timeout" => {
                            if let Lit::Int(lit_int) = lit {
                                attrs.timeout = Some(lit_int.base10_parse()?);
                            }
                        }
                        "description" => {
                            if let Lit::Str(lit_str) = lit {
                                attrs.description = Some(lit_str.value());
                            }
                        }
                        _ => {
                            return Err(syn::Error::new_spanned(
                                nv.path,
                                format!("unknown attribute: {}", ident),
                            ));
                        }
                    }
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        meta,
                        "expected name = value",
                    ));
                }
            }
        }

        Ok(attrs)
    }
}

/// The `#[fezz_function]` attribute macro for defining Fezz serverless functions.
///
/// This macro generates a struct that implements the `FezzFunction` trait,
/// wrapping your async function with the proper lifecycle methods.
///
/// # Attributes
///
/// - `id` (required): Unique identifier for the function
/// - `version` (optional): Function version (default: "v1")  
/// - `method` (optional): HTTP method (default: "GET")
/// - `path` (optional): URL path pattern (default: "/")
/// - `timeout` (optional): Request timeout in seconds
/// - `description` (optional): Function description
///
/// # Example
///
/// ```ignore
/// #[fezz_function(
///     id = "hello-world",
///     version = "v1",
///     method = "GET",
///     path = "/api/hello"
/// )]
/// async fn fetch(req: FezzRequest, ctx: &FunctionContext) -> Result<FezzResponse, FezzError> {
///     Ok(FezzResponse::text("Hello, World!"))
/// }
/// ```
#[proc_macro_attribute]
pub fn fezz_function(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated);
    let input_fn = parse_macro_input!(input as ItemFn);

    match generate_fezz_function(args, input_fn) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn generate_fezz_function(
    args: Punctuated<Meta, Token![,]>,
    input_fn: ItemFn,
) -> syn::Result<proc_macro2::TokenStream> {
    let attrs = FezzFunctionAttrs::parse_meta_list(args)?;

    // Validate required attributes
    let function_id = attrs.id.ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "missing required attribute: id",
        )
    })?;

    let version = attrs.version.unwrap_or_else(|| "v1".to_string());
    let method = attrs.method.unwrap_or_else(|| "GET".to_string());
    let path = attrs.path.unwrap_or_else(|| "/".to_string());
    let timeout = attrs.timeout.unwrap_or(30);
    let description = attrs.description.unwrap_or_default();

    let fn_name = &input_fn.sig.ident;
    let struct_name = format_ident!("{}Function", to_pascal_case(&fn_name.to_string()));
    let manifest_name = format_ident!("{}_MANIFEST", fn_name.to_string().to_uppercase());

    let fn_vis = &input_fn.vis;
    let fn_block = &input_fn.block;
    let fn_asyncness = &input_fn.sig.asyncness;

    // Validate function signature
    if fn_asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &input_fn.sig,
            "fezz_function must be async",
        ));
    }

    // Generate the struct and trait implementation
    let expanded = quote! {
        /// Generated function manifest for compile-time metadata.
        #[allow(dead_code)]
        #fn_vis static #manifest_name: fezz::function::FunctionManifest = fezz::function::FunctionManifest {
            id: #function_id,
            version: #version,
            method: #method,
            path: #path,
            timeout: #timeout,
            description: #description,
        };

        /// Generated Fezz function struct.
        #[derive(Default)]
        #fn_vis struct #struct_name;

        impl #struct_name {
            /// Create a new instance of the function.
            pub fn new() -> Self {
                Self
            }

            /// Get the function manifest.
            pub fn manifest() -> &'static fezz::function::FunctionManifest {
                &#manifest_name
            }
        }

        #[fezz::prelude::async_trait]
        impl fezz::prelude::FezzFunction for #struct_name {
            async fn on_load(&mut self, ctx: &fezz::prelude::FunctionContext) -> Result<(), fezz::prelude::FezzError> {
                let _ = ctx;
                Ok(())
            }

            async fn fetch(
                &self,
                request: fezz::prelude::FezzRequest,
                ctx: &fezz::prelude::FunctionContext,
            ) -> Result<fezz::prelude::FezzResponse, fezz::prelude::FezzError> {
                // Call the user's function
                #fn_name(request, ctx).await
            }

            async fn on_unload(&mut self, ctx: &fezz::prelude::FunctionContext) -> Result<(), fezz::prelude::FezzError> {
                let _ = ctx;
                Ok(())
            }

            fn name(&self) -> &str {
                #function_id
            }
        }

        /// The user's implementation function.
        #fn_asyncness fn #fn_name(
            req: fezz::prelude::FezzRequest,
            ctx: &fezz::prelude::FunctionContext,
        ) -> Result<fezz::prelude::FezzResponse, fezz::prelude::FezzError>
        #fn_block
    };

    Ok(expanded)
}

/// Convert a snake_case string to PascalCase.
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}
