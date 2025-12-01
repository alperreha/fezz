# Project: HHRF-based Fezz Functions (Cloudflare Workers-like Rust runtime)

## 1. One-liner (Architect-level)

A single Rust-based Host HTTP Runtime (HHRF) behind Envoy runs lightweight “fetch-like” Fezz function modules using a load-run-unload execution model, providing a serverless-style platform.

## 2. Goals

- Package each HTTP function as a ~1–3MB Fezz module (binary / .so / wasm).
- Give developers a Cloudflare Workers / Solana-style experience: write only a fetch() / process_instruction() function.
- Run a single HHRF process per worker node:
  - Envoy → HHRF (single upstream)
  - HHRF → many Fezz function modules (load/run/unload)
- Manage function and route metadata via a control-plane stored in etcd/Consul.
- Keep functions stateless and compatible with the load-run-unload runtime model.

## 3. Architecture Components

### 3.1 HHRF (Host-HTTP-Rust-Frontier)

- A long-lived Rust HTTP server running on each worker node.
- Minimal Axum/tower-based router.
- Receives all HTTP traffic from Envoy.
- Function router:
  - Determines target function-id.
  - Locates the Fezz module on disk/registry.
  - Loads the module and calls fezz_fetch().
  - Converts module response back to HTTP.
- Module loading strategies:
  - Phase 1: process spawn + stdin/stdout protocol.
  - Phase 2: .so plugin model using dlopen/dlsym.
  - Phase 3: wasm runtime with reusable instances.

### 3.2 Fezz Function Modules (mini-fezz-fetch-compatible)

- Small developer-written modules built via a Rust macro DSL.
- Standard entry point:
  - ABI: extern "C" fn fezz_fetch(...)
  - OR CLI-style: read FezzRequest JSON from stdin and write FezzResponse to stdout.
- Identified by function_id + version.
- Build generates a manifest (routes, methods, version).

### 3.3 Rust Macro DSL (Function-level API)

Example:

#[fezz_function(
id = "todo-list",
version = "v1",
method = "GET",
path = "/api/todos"
)]
async fn fetch(req: FezzRequest<ListTodosQuery>) -> FezzResponse<Json<Vec<Todo>>> {
// business logic
}

- Generates fezz_fetch() wrapper.
- Auto implements serde derives for DTOs.
- Writes metadata into a build-time manifest (function-id, routes, methods).

### 3.4 Control-plane & Registry

- Stores which function-id/version exists on which node.
- Holds manifest metadata (path, method, auth, timeout).
- Backend: etcd or Consul KV.
- Workflow:
  - Function build uploads artifact to registry.
  - Manifest POSTed to control-plane.
  - Control-plane updates etcd/Consul.
  - Updates Envoy xDS clusters/routes.
  - Notifies HHRF about available functions.

### 3.5 Envoy Data-plane

- Receives external HTTP traffic.
- Uses xDS dynamic configuration:
  - Cluster: HHRF instances.
  - Route: /api/... → HHRF.
- Multi-cluster setups:
  - Multiple HHRF pools across regions.
  - Shared control-plane for routing.

## 4. Technologies and Their Roles

- Rust + Axum/Tower — HHRF HTTP runtime.
- Rust Procedural Macros — #[fezz_function], #[fezz_request], #[fezz_response].
- Dynamic Loading / Wasm Engine — Process model → .so plugins → wasm isolation.
- Envoy Proxy — L7 proxy, TLS termination, observability, xDS.
- xDS Control-plane — ADS/CDS/RDS/EDS implementation.
- etcd/Consul — Distributed metadata/config store.

## 5. Key Terms

- Runtime/Host: HHRF, host runtime, load-run-unload, multi-tenant.
- Rust: axum, tower, tokio, proc_macro, syn, quote, dlopen.
- Serverless: fetch(), process_instruction(), stateless function.
- Envoy/xDS: xDS, ADS, CDS, RDS, EDS, control-plane, dynamic routing.
- Registry/Config: etcd, Consul, service registry, function manifest.
- Isolation: microVM, wasm, sandboxing, module isolation.

## 6. Non-goals (Phase 1)

- Separate container/microVM orchestration per function.
- UI/dashboard, multi-tenant billing.
- Full Kubernetes service mesh behavior (sidecars, mesh policies).
