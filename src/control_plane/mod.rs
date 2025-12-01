//! Control-plane module for function metadata and routing.
//!
//! This module provides the structures and traits for integrating with
//! distributed configuration stores like etcd and Consul.

mod registry;
mod routing;

pub use registry::{FunctionEntry, FunctionStore, MemoryStore};
pub use routing::{Route, RouteTable};
