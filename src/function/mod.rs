//! Fezz function module providing the load-run-unload execution model.

pub mod handler;
pub mod registry;

pub use handler::{FezzError, FezzFunction, FunctionContext};
pub use registry::FunctionRegistry;
