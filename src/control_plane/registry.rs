//! Function store for the control-plane.
//!
//! This module provides traits and implementations for storing function
//! metadata in distributed configuration stores like etcd and Consul.

use crate::function::manifest::OwnedFunctionManifest;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A function entry in the control-plane store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionEntry {
    /// Function manifest with metadata.
    pub manifest: OwnedFunctionManifest,
    /// Node IDs where this function is available.
    pub nodes: Vec<String>,
    /// Whether the function is enabled.
    pub enabled: bool,
    /// Last updated timestamp (Unix epoch milliseconds).
    pub updated_at: u64,
}

impl FunctionEntry {
    /// Create a new function entry.
    pub fn new(manifest: OwnedFunctionManifest) -> Self {
        Self {
            manifest,
            nodes: Vec::new(),
            enabled: true,
            updated_at: current_timestamp(),
        }
    }

    /// Create a function entry with a specific node.
    pub fn on_node(mut self, node_id: impl Into<String>) -> Self {
        self.nodes.push(node_id.into());
        self
    }

    /// Set the enabled state.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Trait for function metadata storage backends.
///
/// This trait allows different backends like etcd, Consul, or in-memory
/// stores to be used for the control-plane.
#[async_trait]
pub trait FunctionStore: Send + Sync {
    /// Register a function in the store.
    async fn register(&self, entry: FunctionEntry) -> Result<(), StoreError>;

    /// Update an existing function entry.
    async fn update(&self, entry: FunctionEntry) -> Result<(), StoreError>;

    /// Remove a function from the store.
    async fn remove(&self, function_id: &str) -> Result<(), StoreError>;

    /// Get a function entry by ID.
    async fn get(&self, function_id: &str) -> Result<Option<FunctionEntry>, StoreError>;

    /// List all function entries.
    async fn list(&self) -> Result<Vec<FunctionEntry>, StoreError>;

    /// Watch for changes to the store (returns a receiver).
    async fn watch(&self) -> Result<tokio::sync::mpsc::Receiver<StoreEvent>, StoreError>;
}

/// Events from the function store.
#[derive(Debug, Clone)]
pub enum StoreEvent {
    /// A function was added.
    Added(FunctionEntry),
    /// A function was updated.
    Updated(FunctionEntry),
    /// A function was removed.
    Removed(String),
}

/// Error type for store operations.
#[derive(Debug, Clone)]
pub struct StoreError {
    /// Error message.
    pub message: String,
}

impl StoreError {
    /// Create a new store error.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StoreError: {}", self.message)
    }
}

impl std::error::Error for StoreError {}

/// In-memory implementation of FunctionStore.
///
/// This is useful for development and testing, or for single-node deployments.
#[derive(Default)]
pub struct MemoryStore {
    entries: Arc<RwLock<HashMap<String, FunctionEntry>>>,
    watchers: Arc<RwLock<Vec<tokio::sync::mpsc::Sender<StoreEvent>>>>,
}

impl MemoryStore {
    /// Create a new in-memory store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Notify all watchers of an event.
    async fn notify(&self, event: StoreEvent) {
        let watchers = self.watchers.read().await;
        for sender in watchers.iter() {
            let _ = sender.send(event.clone()).await;
        }
    }
}

#[async_trait]
impl FunctionStore for MemoryStore {
    async fn register(&self, entry: FunctionEntry) -> Result<(), StoreError> {
        let function_id = entry.manifest.id.to_string();
        let mut entries = self.entries.write().await;
        
        if entries.contains_key(&function_id) {
            return Err(StoreError::new(format!(
                "Function '{}' already exists",
                function_id
            )));
        }
        
        entries.insert(function_id, entry.clone());
        drop(entries);
        
        self.notify(StoreEvent::Added(entry)).await;
        Ok(())
    }

    async fn update(&self, entry: FunctionEntry) -> Result<(), StoreError> {
        let function_id = entry.manifest.id.to_string();
        let mut entries = self.entries.write().await;
        
        if !entries.contains_key(&function_id) {
            return Err(StoreError::new(format!(
                "Function '{}' not found",
                function_id
            )));
        }
        
        entries.insert(function_id, entry.clone());
        drop(entries);
        
        self.notify(StoreEvent::Updated(entry)).await;
        Ok(())
    }

    async fn remove(&self, function_id: &str) -> Result<(), StoreError> {
        let mut entries = self.entries.write().await;
        
        entries
            .remove(function_id)
            .ok_or_else(|| StoreError::new(format!("Function '{}' not found", function_id)))?;
        
        drop(entries);
        
        self.notify(StoreEvent::Removed(function_id.to_string())).await;
        Ok(())
    }

    async fn get(&self, function_id: &str) -> Result<Option<FunctionEntry>, StoreError> {
        let entries = self.entries.read().await;
        Ok(entries.get(function_id).cloned())
    }

    async fn list(&self) -> Result<Vec<FunctionEntry>, StoreError> {
        let entries = self.entries.read().await;
        Ok(entries.values().cloned().collect())
    }

    async fn watch(&self) -> Result<tokio::sync::mpsc::Receiver<StoreEvent>, StoreError> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let mut watchers = self.watchers.write().await;
        watchers.push(tx);
        Ok(rx)
    }
}

/// Get current timestamp in milliseconds.
fn current_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manifest() -> OwnedFunctionManifest {
        OwnedFunctionManifest::new("test-fn", "v1", "GET", "/api/test")
    }

    #[tokio::test]
    async fn test_memory_store_register() {
        let store = MemoryStore::new();
        let entry = FunctionEntry::new(test_manifest()).on_node("node-1");
        
        store.register(entry).await.unwrap();
        
        let stored = store.get("test-fn").await.unwrap();
        assert!(stored.is_some());
        assert_eq!(stored.unwrap().manifest.id, "test-fn");
    }

    #[tokio::test]
    async fn test_memory_store_duplicate_register() {
        let store = MemoryStore::new();
        let entry = FunctionEntry::new(test_manifest());
        
        store.register(entry.clone()).await.unwrap();
        let result = store.register(entry).await;
        
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_memory_store_update() {
        let store = MemoryStore::new();
        let entry = FunctionEntry::new(test_manifest());
        
        store.register(entry.clone()).await.unwrap();
        
        let updated = entry.on_node("node-2");
        store.update(updated).await.unwrap();
        
        let stored = store.get("test-fn").await.unwrap().unwrap();
        assert_eq!(stored.nodes.len(), 1);
        assert_eq!(stored.nodes[0], "node-2");
    }

    #[tokio::test]
    async fn test_memory_store_remove() {
        let store = MemoryStore::new();
        let entry = FunctionEntry::new(test_manifest());
        
        store.register(entry).await.unwrap();
        store.remove("test-fn").await.unwrap();
        
        let stored = store.get("test-fn").await.unwrap();
        assert!(stored.is_none());
    }

    #[tokio::test]
    async fn test_memory_store_list() {
        let store = MemoryStore::new();
        
        store.register(FunctionEntry::new(
            OwnedFunctionManifest::new("fn-1", "v1", "GET", "/a")
        )).await.unwrap();
        
        store.register(FunctionEntry::new(
            OwnedFunctionManifest::new("fn-2", "v1", "POST", "/b")
        )).await.unwrap();
        
        let entries = store.list().await.unwrap();
        assert_eq!(entries.len(), 2);
    }
}
