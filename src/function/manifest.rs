//! Function manifest for compile-time metadata.
//!
//! The manifest contains metadata about a Fezz function including
//! its route, method, version, and other properties.

use serde::{Deserialize, Serialize};

/// Function manifest containing metadata for a Fezz function.
///
/// This is generated at compile time by the `#[fezz_function]` macro
/// and can be used by the control-plane for routing and configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionManifest {
    /// Unique identifier for the function.
    pub id: &'static str,
    /// Function version (e.g., "v1", "v2").
    pub version: &'static str,
    /// HTTP method this function handles (e.g., "GET", "POST").
    pub method: &'static str,
    /// URL path pattern for routing.
    pub path: &'static str,
    /// Request timeout in seconds.
    pub timeout: u64,
    /// Optional description of the function.
    pub description: &'static str,
}

impl FunctionManifest {
    /// Create a new function manifest.
    pub const fn new(
        id: &'static str,
        version: &'static str,
        method: &'static str,
        path: &'static str,
    ) -> Self {
        Self {
            id,
            version,
            method,
            path,
            timeout: 30,
            description: "",
        }
    }

    /// Create a manifest with custom timeout.
    pub const fn with_timeout(mut self, timeout: u64) -> Self {
        self.timeout = timeout;
        self
    }

    /// Create a manifest with description.
    pub const fn with_description(mut self, description: &'static str) -> Self {
        self.description = description;
        self
    }

    /// Convert to an owned manifest.
    pub fn to_owned(&self) -> OwnedFunctionManifest {
        OwnedFunctionManifest {
            id: self.id.to_string(),
            version: self.version.to_string(),
            method: self.method.to_string(),
            path: self.path.to_string(),
            timeout: self.timeout,
            description: self.description.to_string(),
        }
    }
}

impl Default for FunctionManifest {
    fn default() -> Self {
        Self {
            id: "",
            version: "v1",
            method: "GET",
            path: "/",
            timeout: 30,
            description: "",
        }
    }
}

/// Owned version of FunctionManifest for runtime use.
///
/// This version uses owned `String` types instead of `&'static str`
/// for use in dynamic contexts like the control-plane.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OwnedFunctionManifest {
    /// Unique identifier for the function.
    pub id: String,
    /// Function version (e.g., "v1", "v2").
    pub version: String,
    /// HTTP method this function handles (e.g., "GET", "POST").
    pub method: String,
    /// URL path pattern for routing.
    pub path: String,
    /// Request timeout in seconds.
    pub timeout: u64,
    /// Optional description of the function.
    pub description: String,
}

impl OwnedFunctionManifest {
    /// Create a new owned function manifest.
    pub fn new(
        id: impl Into<String>,
        version: impl Into<String>,
        method: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
            method: method.into(),
            path: path.into(),
            timeout: 30,
            description: String::new(),
        }
    }

    /// Set the timeout.
    pub fn with_timeout(mut self, timeout: u64) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }
}

impl Default for OwnedFunctionManifest {
    fn default() -> Self {
        Self {
            id: String::new(),
            version: "v1".to_string(),
            method: "GET".to_string(),
            path: "/".to_string(),
            timeout: 30,
            description: String::new(),
        }
    }
}

impl From<&FunctionManifest> for OwnedFunctionManifest {
    fn from(manifest: &FunctionManifest) -> Self {
        manifest.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_creation() {
        let manifest = FunctionManifest::new("test-fn", "v1", "GET", "/api/test");
        assert_eq!(manifest.id, "test-fn");
        assert_eq!(manifest.version, "v1");
        assert_eq!(manifest.method, "GET");
        assert_eq!(manifest.path, "/api/test");
        assert_eq!(manifest.timeout, 30);
    }

    #[test]
    fn test_manifest_with_timeout() {
        let manifest = FunctionManifest::new("test-fn", "v1", "POST", "/api/submit")
            .with_timeout(60);
        assert_eq!(manifest.timeout, 60);
    }

    #[test]
    fn test_manifest_serialization() {
        let manifest = FunctionManifest::new("test-fn", "v1", "GET", "/api/test");
        let json = serde_json::to_string(&manifest).unwrap();
        assert!(json.contains("test-fn"));
        assert!(json.contains("v1"));
    }

    #[test]
    fn test_owned_manifest_creation() {
        let manifest = OwnedFunctionManifest::new("test-fn", "v1", "GET", "/api/test");
        assert_eq!(manifest.id, "test-fn");
        assert_eq!(manifest.version, "v1");
    }

    #[test]
    fn test_manifest_to_owned() {
        let manifest = FunctionManifest::new("test-fn", "v1", "GET", "/api/test");
        let owned = manifest.to_owned();
        assert_eq!(owned.id, "test-fn");
        assert_eq!(owned.version, "v1");
    }

    #[test]
    fn test_owned_manifest_serialization() {
        let manifest = OwnedFunctionManifest::new("test-fn", "v1", "GET", "/api/test");
        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: OwnedFunctionManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, manifest);
    }
}
