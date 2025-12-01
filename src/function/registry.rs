//! Function registry for managing Fezz functions.

use crate::function::handler::{FezzError, FezzFunction, FunctionContext};
use crate::http::{FezzRequest, FezzResponse};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// State of a function in the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionState {
    /// Function is registered but not loaded.
    Unloaded,
    /// Function is being loaded.
    Loading,
    /// Function is loaded and ready to handle requests.
    Ready,
    /// Function is being unloaded.
    Unloading,
}

/// Function entry in the registry.
struct FunctionEntry {
    /// The function implementation.
    function: Arc<RwLock<Box<dyn FezzFunction>>>,
    /// Current state of the function.
    state: FunctionState,
    /// Function context.
    context: FunctionContext,
    /// Number of active invocations.
    active_invocations: usize,
}

/// Registry for managing Fezz functions with load-run-unload lifecycle.
pub struct FunctionRegistry {
    /// Registered functions.
    functions: RwLock<HashMap<String, FunctionEntry>>,
    /// Global environment variables.
    global_env: HashMap<String, String>,
}

impl FunctionRegistry {
    /// Create a new function registry.
    pub fn new() -> Self {
        Self {
            functions: RwLock::new(HashMap::new()),
            global_env: HashMap::new(),
        }
    }

    /// Create a new function registry with global environment variables.
    pub fn with_env(env: HashMap<String, String>) -> Self {
        Self {
            functions: RwLock::new(HashMap::new()),
            global_env: env,
        }
    }

    /// Register a new function.
    pub async fn register(
        &self,
        name: impl Into<String>,
        function: Box<dyn FezzFunction>,
    ) -> Result<(), FezzError> {
        let name = name.into();
        let mut functions = self.functions.write().await;

        if functions.contains_key(&name) {
            return Err(FezzError::new(format!(
                "Function '{}' is already registered",
                name
            )));
        }

        let mut context = FunctionContext::new(&name, "");
        for (k, v) in &self.global_env {
            context.env.insert(k.clone(), v.clone());
        }

        let entry = FunctionEntry {
            function: Arc::new(RwLock::new(function)),
            state: FunctionState::Unloaded,
            context,
            active_invocations: 0,
        };

        functions.insert(name.clone(), entry);
        info!("Registered function: {}", name);
        Ok(())
    }

    /// Load a function (initialize it).
    pub async fn load(&self, name: &str) -> Result<(), FezzError> {
        let mut functions = self.functions.write().await;

        let entry = functions
            .get_mut(name)
            .ok_or_else(|| FezzError::not_found(format!("Function '{}' not found", name)))?;

        if entry.state == FunctionState::Ready {
            debug!("Function '{}' is already loaded", name);
            return Ok(());
        }

        if entry.state == FunctionState::Loading {
            return Err(FezzError::new(format!(
                "Function '{}' is already being loaded",
                name
            )));
        }

        entry.state = FunctionState::Loading;
        let function = entry.function.clone();
        let context = entry.context.clone();

        // Release lock during async operation
        drop(functions);

        // Call on_load
        let mut func = function.write().await;
        if let Err(e) = func.on_load(&context).await {
            error!("Failed to load function '{}': {}", name, e);
            
            // Reset state
            let mut functions = self.functions.write().await;
            if let Some(entry) = functions.get_mut(name) {
                entry.state = FunctionState::Unloaded;
            }
            return Err(e);
        }

        // Update state
        let mut functions = self.functions.write().await;
        if let Some(entry) = functions.get_mut(name) {
            entry.state = FunctionState::Ready;
        }

        info!("Loaded function: {}", name);
        Ok(())
    }

    /// Unload a function (cleanup).
    pub async fn unload(&self, name: &str) -> Result<(), FezzError> {
        let mut functions = self.functions.write().await;

        let entry = functions
            .get_mut(name)
            .ok_or_else(|| FezzError::not_found(format!("Function '{}' not found", name)))?;

        if entry.state == FunctionState::Unloaded {
            debug!("Function '{}' is already unloaded", name);
            return Ok(());
        }

        if entry.active_invocations > 0 {
            warn!(
                "Function '{}' has {} active invocations, waiting...",
                name, entry.active_invocations
            );
        }

        entry.state = FunctionState::Unloading;
        let function = entry.function.clone();
        let context = entry.context.clone();

        // Release lock during async operation
        drop(functions);

        // Call on_unload
        let mut func = function.write().await;
        if let Err(e) = func.on_unload(&context).await {
            error!("Error during unload of function '{}': {}", name, e);
        }

        // Update state
        let mut functions = self.functions.write().await;
        if let Some(entry) = functions.get_mut(name) {
            entry.state = FunctionState::Unloaded;
        }

        info!("Unloaded function: {}", name);
        Ok(())
    }

    /// Execute a function (load-run-unload or just run if already loaded).
    pub async fn execute(
        &self,
        name: &str,
        request: FezzRequest,
        request_id: &str,
    ) -> Result<FezzResponse, FezzError> {
        // Ensure function is loaded
        self.load(name).await?;

        // Get function and increment active invocations
        let (function, context) = {
            let mut functions = self.functions.write().await;
            let entry = functions
                .get_mut(name)
                .ok_or_else(|| FezzError::not_found(format!("Function '{}' not found", name)))?;

            entry.active_invocations += 1;
            
            let mut ctx = entry.context.clone();
            ctx.request_id = request_id.to_string();
            
            (entry.function.clone(), ctx)
        };

        // Execute function
        let result = {
            let func = function.read().await;
            func.fetch(request, &context).await
        };

        // Decrement active invocations
        {
            let mut functions = self.functions.write().await;
            if let Some(entry) = functions.get_mut(name) {
                entry.active_invocations = entry.active_invocations.saturating_sub(1);
            }
        }

        result
    }

    /// Get the state of a function.
    pub async fn get_state(&self, name: &str) -> Option<FunctionState> {
        let functions = self.functions.read().await;
        functions.get(name).map(|e| e.state)
    }

    /// List all registered functions.
    pub async fn list(&self) -> Vec<(String, FunctionState)> {
        let functions = self.functions.read().await;
        functions
            .iter()
            .map(|(name, entry)| (name.clone(), entry.state))
            .collect()
    }

    /// Remove a function from the registry.
    pub async fn remove(&self, name: &str) -> Result<(), FezzError> {
        // Unload first
        if self.get_state(name).await == Some(FunctionState::Ready) {
            self.unload(name).await?;
        }

        let mut functions = self.functions.write().await;
        functions
            .remove(name)
            .ok_or_else(|| FezzError::not_found(format!("Function '{}' not found", name)))?;

        info!("Removed function: {}", name);
        Ok(())
    }
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}
