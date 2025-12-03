# fezz

A single Rust-based Host HTTP Runtime (HHRF) runs lightweight "fetch-like" Fezz function modules using a load-run-unload execution model, providing a serverless-style platform.

## Architecture

### Hot-Swappable Library Cache

HHRF maintains a global cache of loaded function libraries for high-concurrency performance. Instead of loading and unloading `.so` files for every request, libraries are cached and reused across requests.

- **Cache Hit**: Uses the existing `Arc<Library>` (nearly zero RAM cost due to OS code segment sharing)
- **Cache Miss**: Loads the `.so` file, wraps in `Arc`, inserts into cache, then executes
- **TTL Cleanup**: Background task evicts libraries idle for more than 5 minutes

### Panic Safety

The macro wrapper uses `std::panic::catch_unwind` to catch panics in user code, preventing them from crossing FFI boundaries (which would cause undefined behavior). Panics are converted to HTTP 500 error responses.

### Async Runtime Isolation

The exported C function (`fezz_fetch`) is synchronous. For async operations (HTTP requests, Redis, etc.), use blocking clients within your function. The host runs FFI calls in `spawn_blocking` to prevent blocking the async runtime.

## Best Practices for User Functions

### Warm State Persistence

With the library caching mechanism, global variables persist between requests. Use `std::sync::OnceLock` for heavy clients:

```rust
use std::sync::OnceLock;
use redis::Client;

static REDIS_CLIENT: OnceLock<Client> = OnceLock::new();

fn get_redis_client() -> &'static Client {
    REDIS_CLIENT.get_or_init(|| {
        Client::open("redis://127.0.0.1:6379/").expect("Failed to create Redis client")
    })
}

#[fezz_function]
pub fn my_function(req: FezzHttpRequest) -> FezzHttpResponse {
    let client = get_redis_client();
    // Use the client - it will be reused across requests
    // ...
}
```

### Guidelines

1. **Use blocking clients**: Since the FFI boundary is synchronous, use blocking versions of HTTP/database clients
2. **Initialize once**: Use `OnceLock` for expensive resources like connection pools
3. **Handle errors gracefully**: Don't panic - return proper error responses instead
4. **Keep functions stateless**: Don't rely on mutable global state between requests

## Demo

1. Build sample function

```bash
cargo build -p example_todosapi --release
```

2. Copy the generated shared library to functions folder

```bash
# for todos@latest use same name as in fezz.json
mkdir -p ./functions/todos@latest
cp target/release/libexample_todosapi.dylib ./functions/todos@latest/libexample_todosapi.dylib
```

3. Create fezz.json for new function in functions/todos@latest/fezz.json

```json
{
  "id": "todos",
  "version": "latest",
  "entry": "libexample_todosapi.dylib",
  "routes": [
    {
      "path": "/hello",
      "method": "GET"
    }
  ]
}
```

4. Run HHRF server

```bash
cargo run -p hhrf
```

5. Test the function

```bash
curl http://127.0.0.1:3000/rpc/todos@latest
```
