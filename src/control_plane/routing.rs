//! Route table for the control-plane.
//!
//! This module provides the route table for mapping incoming HTTP requests
//! to Fezz functions based on path patterns and HTTP methods.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// HTTP method for routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RouteMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
    Any,
}

impl RouteMethod {
    /// Check if this method matches the given method string.
    pub fn matches(&self, method: &str) -> bool {
        match self {
            RouteMethod::Any => true,
            RouteMethod::Get => method.eq_ignore_ascii_case("GET"),
            RouteMethod::Post => method.eq_ignore_ascii_case("POST"),
            RouteMethod::Put => method.eq_ignore_ascii_case("PUT"),
            RouteMethod::Delete => method.eq_ignore_ascii_case("DELETE"),
            RouteMethod::Patch => method.eq_ignore_ascii_case("PATCH"),
            RouteMethod::Head => method.eq_ignore_ascii_case("HEAD"),
            RouteMethod::Options => method.eq_ignore_ascii_case("OPTIONS"),
        }
    }
}

impl From<&str> for RouteMethod {
    fn from(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "GET" => RouteMethod::Get,
            "POST" => RouteMethod::Post,
            "PUT" => RouteMethod::Put,
            "DELETE" => RouteMethod::Delete,
            "PATCH" => RouteMethod::Patch,
            "HEAD" => RouteMethod::Head,
            "OPTIONS" => RouteMethod::Options,
            "*" | "ANY" => RouteMethod::Any,
            _ => RouteMethod::Get,
        }
    }
}

/// A route entry that maps a path pattern to a function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    /// Route path pattern (e.g., "/api/users/:id").
    pub path: String,
    /// HTTP method for this route.
    pub method: RouteMethod,
    /// Target function ID.
    pub function_id: String,
    /// Route priority (higher = more priority).
    pub priority: u32,
    /// Whether the route is enabled.
    pub enabled: bool,
}

impl Route {
    /// Create a new route.
    pub fn new(
        method: impl Into<RouteMethod>,
        path: impl Into<String>,
        function_id: impl Into<String>,
    ) -> Self {
        let method = match &method.into() {
            m @ RouteMethod::Get => *m,
            m @ RouteMethod::Post => *m,
            m @ RouteMethod::Put => *m,
            m @ RouteMethod::Delete => *m,
            m @ RouteMethod::Patch => *m,
            m @ RouteMethod::Head => *m,
            m @ RouteMethod::Options => *m,
            m @ RouteMethod::Any => *m,
        };
        Self {
            path: path.into(),
            method,
            function_id: function_id.into(),
            priority: 0,
            enabled: true,
        }
    }

    /// Set the route priority.
    pub fn priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    /// Set the enabled state.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Check if this route matches the given path and method.
    pub fn matches(&self, path: &str, method: &str) -> bool {
        if !self.enabled {
            return false;
        }

        if !self.method.matches(method) {
            return false;
        }

        // Simple path matching (exact match or prefix match for now)
        // TODO: Implement proper path parameter matching
        if self.path.ends_with("/*") {
            let prefix = &self.path[..self.path.len() - 2];
            path.starts_with(prefix)
        } else if self.path.contains(':') {
            // Simple pattern matching with :param placeholders
            let route_segments: Vec<&str> = self.path.split('/').collect();
            let path_segments: Vec<&str> = path.split('/').collect();

            if route_segments.len() != path_segments.len() {
                return false;
            }

            route_segments.iter().zip(path_segments.iter()).all(|(r, p)| {
                r.starts_with(':') || *r == *p
            })
        } else {
            self.path == path
        }
    }
}

/// Route table for routing requests to functions.
#[derive(Default)]
pub struct RouteTable {
    routes: Arc<RwLock<Vec<Route>>>,
}

impl RouteTable {
    /// Create a new route table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a route to the table.
    pub async fn add(&self, route: Route) {
        let mut routes = self.routes.write().await;
        routes.push(route);
        // Sort by priority (highest first)
        routes.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Remove routes for a function.
    pub async fn remove_function(&self, function_id: &str) {
        let mut routes = self.routes.write().await;
        routes.retain(|r| r.function_id != function_id);
    }

    /// Find a matching route for the given path and method.
    pub async fn find(&self, path: &str, method: &str) -> Option<Route> {
        let routes = self.routes.read().await;
        routes.iter().find(|r| r.matches(path, method)).cloned()
    }

    /// List all routes.
    pub async fn list(&self) -> Vec<Route> {
        let routes = self.routes.read().await;
        routes.clone()
    }

    /// Get routes for a specific function.
    pub async fn for_function(&self, function_id: &str) -> Vec<Route> {
        let routes = self.routes.read().await;
        routes
            .iter()
            .filter(|r| r.function_id == function_id)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_method_from_str() {
        assert_eq!(RouteMethod::from("GET"), RouteMethod::Get);
        assert_eq!(RouteMethod::from("post"), RouteMethod::Post);
        assert_eq!(RouteMethod::from("*"), RouteMethod::Any);
    }

    #[test]
    fn test_route_method_matches() {
        assert!(RouteMethod::Get.matches("GET"));
        assert!(RouteMethod::Get.matches("get"));
        assert!(!RouteMethod::Get.matches("POST"));
        assert!(RouteMethod::Any.matches("GET"));
        assert!(RouteMethod::Any.matches("POST"));
    }

    #[test]
    fn test_route_exact_match() {
        let route = Route::new(RouteMethod::Get, "/api/users", "users-list");
        
        assert!(route.matches("/api/users", "GET"));
        assert!(!route.matches("/api/users", "POST"));
        assert!(!route.matches("/api/users/1", "GET"));
    }

    #[test]
    fn test_route_wildcard_match() {
        let route = Route::new(RouteMethod::Get, "/api/*", "api-handler");
        
        assert!(route.matches("/api/users", "GET"));
        assert!(route.matches("/api/users/1", "GET"));
        assert!(!route.matches("/other", "GET"));
    }

    #[test]
    fn test_route_param_match() {
        let route = Route::new(RouteMethod::Get, "/api/users/:id", "user-get");
        
        assert!(route.matches("/api/users/123", "GET"));
        assert!(route.matches("/api/users/abc", "GET"));
        assert!(!route.matches("/api/users", "GET"));
        assert!(!route.matches("/api/users/1/details", "GET"));
    }

    #[tokio::test]
    async fn test_route_table_add_and_find() {
        let table = RouteTable::new();
        
        table.add(Route::new(RouteMethod::Get, "/api/users", "users-list")).await;
        table.add(Route::new(RouteMethod::Post, "/api/users", "users-create")).await;
        
        let route = table.find("/api/users", "GET").await;
        assert!(route.is_some());
        assert_eq!(route.unwrap().function_id, "users-list");
        
        let route = table.find("/api/users", "POST").await;
        assert!(route.is_some());
        assert_eq!(route.unwrap().function_id, "users-create");
    }

    #[tokio::test]
    async fn test_route_table_priority() {
        let table = RouteTable::new();
        
        table.add(Route::new(RouteMethod::Get, "/api/*", "api-catch-all").priority(0)).await;
        table.add(Route::new(RouteMethod::Get, "/api/users", "users-list").priority(10)).await;
        
        // Specific route should match first due to priority
        let route = table.find("/api/users", "GET").await;
        assert!(route.is_some());
        assert_eq!(route.unwrap().function_id, "users-list");
        
        // Catch-all should match other paths
        let route = table.find("/api/other", "GET").await;
        assert!(route.is_some());
        assert_eq!(route.unwrap().function_id, "api-catch-all");
    }

    #[tokio::test]
    async fn test_route_table_remove() {
        let table = RouteTable::new();
        
        table.add(Route::new(RouteMethod::Get, "/api/users", "users-list")).await;
        table.add(Route::new(RouteMethod::Get, "/api/posts", "posts-list")).await;
        
        table.remove_function("users-list").await;
        
        let routes = table.list().await;
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].function_id, "posts-list");
    }
}
