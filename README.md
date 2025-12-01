# fezz

A single Rust-based Host HTTP Runtime (HHRF) runs lightweight “fetch-like” Fezz function modules using a load-run-unload execution model, providing a serverless-style platform.

## Demo

1. Build sampletodoapisample function

```bash
cargo build -p todoapisample --release
```

2. Copy the generated shared library to functions folder

```bash
# for todoapisample use same name as in fezz.json
cp target/release/libtodoapisample.so ./functions/todoapisample/todoapisample.so
```

3. Create fezz.json for new function in functions/todoapisample/fezz.json

```json
{
  "id": "todoapisample",
  "version": "latest",
  "entry": "todoapisample.so",
  "routes": [
    {
      "path": "/todos",
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
curl http://127.0.0.1:3000/rpc/latest/todoapisample
```
