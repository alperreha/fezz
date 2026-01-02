# Fezz Callable HTTP Apps — Bytes-First ABI

This document describes the **intended** (target) Fezz design, focused on a **bytes-first** host↔function boundary.
Audience: Senior Software Engineer.

**Thesis:**
- **HHRF** is the *only* HTTP server (termination, routing, policy).
- Function artifacts (`.so`) are **callable handlers** (no port listening).
- The host↔plugin contract is **ABI-stable and bytes-based**: **bytes in → bytes out**, plus an explicit **free** function.
- Serialization/deserialization happens **inside the plugin** (macro-generated glue), not in HHRF, and not via Rust structs crossing ABI.

---

## 1) Why bytes-first ABI

Rust types (`axum::Router`, `http::Request`, `tower::Service`, generic trait objects) are not ABI-stable across dynamic library boundaries. Exporting “a Router you can `.oneshot()` from the host” is not viable.

**Therefore:** the boundary must be something like:
- `(*const u8, usize) -> (*mut u8, usize)`
and an explicit deallocator:
- `free(*mut u8, usize)`

This makes the boundary:
- language/runtime/toolchain agnostic (future-proof)
- allocator-safe (host never assumes it can free plugin memory)
- compatible with binary bodies (uploads, non-UTF8 payloads)

---

## 2) High-level runtime architecture

### Components
1) **Edge routing layer (future)**
   Envoy/xDS/DNS-based routing, service discovery, etc.

2) **HHRF (Host HTTP Runtime Frontier)**
   - Axum-based HTTP termination (HTTP/1.1–2, TLS termination upstream or here)
   - Routing: domain + path + version → function artifact ID
   - Policy: auth, rate limits, timeouts, observability, allowlists
   - **Transforms incoming HTTP → FezzWireRequest bytes**
   - Calls function plugin via ABI
   - **Transforms FezzWireResponse bytes → outgoing HTTP**

3) **Function artifact** (`.so`)
   - Exposes an ABI-stable entrypoint (bytes-in/out)
   - Contains business logic
   - No port listen, no HTTP server requirement
   - Optional advanced mode: contains an internal Axum `Router` and uses `oneshot` **inside the plugin**

---

## 3) The Fezz ABI (V2) — the contract

### Exported symbols from each `.so`
- `fezz_handle_v2`: processes one request
- `fezz_free_v2`: frees the returned buffer

**C ABI shape (conceptual):**
```c
typedef struct {
  const uint8_t* ptr;
  size_t len;
} FezzSlice;

typedef struct {
  uint8_t* ptr;
  size_t len;
} FezzOwned;

FezzOwned fezz_handle_v2(FezzSlice req);
void fezz_free_v2(FezzOwned buf);
```

### Behavioral contract
- Input: `req` is an opaque byte blob in **FezzWireRequest** format.
- Output: returned bytes are **FezzWireResponse** format.
- Memory:
  - Plugin allocates response buffer.
  - Host **must** call `fezz_free_v2` exactly once for each successful response.
- Safety:
  - Panics must **not** cross FFI boundary. Glue code must catch panics and return a valid error response blob.

---

## 4) Wire formats (bytes that cross the boundary)

We keep external HTTP standard; internally we carry an **HTTP-shaped envelope** in bytes.

### FezzWireRequest (recommended fields)
- `method`: string or small enum
- `scheme`: optional (`http`/`https`)
- `authority`: optional (Host)
- `path_and_query`: string (exact, preserve query)
- `headers`: list of (name, value) as bytes (not forced UTF-8)
- `body`: bytes (may be empty)
- `meta` (optional): trace_id, deadline_ms, client_ip, etc.

### FezzWireResponse
- `status`: u16
- `headers`: list of (name, value) bytes
- `body`: bytes

**Encoding choice:** CBOR or MessagePack are a good default (compact, binary-safe).
JSON is acceptable only for early prototypes (UTF-8 assumption + overhead), but bytes-first implies we should be able to carry arbitrary bytes.

---

## 5) Execution model

### “One request → one function call”
HHRF receives an HTTP request and performs:

1) Resolve target function ID/version (domain/path/etc.)
2) Load plugin:
   - from local disk cache / S3 fetch → disk
   - `libloading` load + in-memory cache with TTL eviction
3) Convert HTTP request → `FezzWireRequest` bytes
4) Invoke:
   - `fezz_handle_v2(req_bytes)`
5) Parse `FezzWireResponse` bytes
6) Build outgoing HTTP response
7) Ensure:
   - response buffer freed via `fezz_free_v2`

### Concurrency & blocking
- HHRF is async (Tokio/Axum).
- Plugin execution may block (e.g. sync I/O).
- Host must execute `fezz_handle_v2` in a dedicated blocking pool (`spawn_blocking`), or in an isolated runner process if we move to stronger sandboxing.

---

## 6) Developer experience (DX)

### Baseline: “Fezz function”
Developer writes business logic; macro generates ABI glue.

Conceptual signature the developer thinks in:
- `fn handle(req) -> resp` (typed or not)

What ships as `.so`:
- `fezz_handle_v2(bytes) -> bytes`

### Advanced DX: “Write Axum routes, but no listening”
We can support an Axum-native mode **inside** the plugin:

- Developer writes:
  - `fn app() -> axum::Router { ... }`
- Macro generates:
  - a static cached Router (`OnceCell<Router>`)
  - ABI handler that:
    1) converts `FezzWireRequest` bytes → `http::Request<Body>`
    2) calls `router.clone().oneshot(req)` **inside the plugin**
    3) converts `http::Response` → `FezzWireResponse` bytes

**Key:** the host never sees a `Router`. It only calls bytes-in/out.

---

## 7) HHRF responsibilities (strict)

HHRF is the “edge correctness” layer:

- HTTP parsing & validation
- hop-by-hop header rules (e.g. `connection`, `keep-alive`, `transfer-encoding` handling)
- request size limits, timeouts, concurrency caps
- authentication / authorization / tenancy isolation
- observability (trace IDs, metrics)
- routing + version resolution

HHRF must stay **stateless** relative to function logic. It should not own function-level serialization logic beyond the wire encoding.

---

## 8) Migration plan (from struct/JSON to bytes-first)

This is the recommended sequence:

1) **Introduce FezzWireRequest/Response bytes** (CBOR/MsgPack)
   - Add a `fezz-wire` crate (or inside `fezz-sdk`) that defines encoding/decoding helpers.

2) **Add V2 ABI to macros**
   - Generate `fezz_handle_v2` and `fezz_free_v2`.
   - Keep old `fezz_fetch` temporarily for backward compatibility.

3) **Update HHRF to V2**
   - Convert incoming `axum` request → FezzWireRequest bytes.
   - Call `fezz_handle_v2`.
   - Parse response bytes → outgoing HTTP response.
   - Always call `fezz_free_v2`.

4) **Deprecate the JSON/CString boundary**
   - Remove assumptions about UTF-8 bodies.
   - Replace `Option<String>` bodies with `Vec<u8>` bodies in internal representations.

5) Optional: **Axum-native plugin mode**
   - Add `#[fezz_app]` macro that wraps an internal `Router` + `oneshot` in plugin.

---

## 9) Security & policy notes (non-negotiable for prod)

Once plugins can make outbound calls, we must prevent SSRF and enforce policies:

- outbound allowlists (domain/CIDR), DNS-rebind defenses
- timeouts & max body size for outbound requests
- rate limiting per tenant/function
- structured audit logs per invocation
- strict resource caps (CPU/mem), ideally via microVM/container sandboxing (Firecracker, etc.)

Dynamic libraries are **not** a security boundary by themselves. Treat `.so` as trusted code unless sandboxed.

---

## 10) TL;DR

- External: **standard HTTP**.
- Internal: **bytes-first ABI**.
- Host (HHRF): parse HTTP, route, policy, call plugin.
- Plugin: bytes-in/out handler, serialization inside plugin, explicit free.
- Axum `.oneshot` is still usable, but only **inside the plugin**, never across ABI.
