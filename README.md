# fezz

A single Rust-based Host HTTP Runtime (HHRF) runs lightweight “fetch-like” Fezz function modules using a load-run-unload execution model, providing a serverless-style platform.

## Demo

1. Build sampletodoapisample function

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
