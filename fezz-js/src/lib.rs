use anyhow::{anyhow, Context, Result};
use deno_core::{
    futures::executor::block_on,
    op,
    v8,
    Extension, FsModuleLoader, JsRuntime, ModuleSpecifier, PollEventLoopOptions, RuntimeOptions,
};
use futures_timer::Delay;
use std::{collections::HashMap, fs, path::Path, rc::Rc, time::Duration};
use tokio::sync::Mutex;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct JsKey {
    pub org: String,
    pub func: String,
    pub version: String,
}

#[derive(Clone, Debug)]
pub struct JsInvoke {
    pub method: String,
    pub path_and_query: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub env: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub struct JsResult {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

pub struct JsRuntimeManager {
    script_cache: Mutex<HashMap<JsKey, String>>,
}

impl JsRuntimeManager {
    pub fn new() -> Self {
        Self {
            script_cache: Mutex::new(HashMap::new()),
        }
    }

    pub async fn invoke(&self, key: &JsKey, script_path: &str, req: JsInvoke) -> Result<JsResult> {
        if !Path::new(script_path).exists() {
            return Err(anyhow!("JS bundle not found at {}", script_path));
        }

        {
            let mut cache = self.script_cache.lock().await;
            cache
                .entry(key.clone())
                .or_insert_with(|| script_path.to_string());
        }

        let script_path = script_path.to_string();
        tokio::task::spawn_blocking(move || run_js(&script_path, req))
            .await
            .context("Failed to join JS task")?
    }
}

const BOOTSTRAP: &str = r#"
class Response {
  constructor(body, init = {}) {
    this.status = init.status ?? 200;
    this.headers = init.headers ?? [];
    this.body = body ?? "";
  }
}

function __fezz_normalize_response(resp) {
  if (resp instanceof Response) {
    return {
      status: resp.status ?? 200,
      headers: normalizeHeaders(resp.headers),
      body: normalizeBody(resp.body),
    };
  }

  if (resp && typeof resp === "object") {
    return {
      status: resp.status ?? 200,
      headers: normalizeHeaders(resp.headers ?? []),
      body: normalizeBody(resp.body),
    };
  }

  return { status: 200, headers: [], body: normalizeBody(resp) };
}

function normalizeHeaders(headers) {
  if (headers instanceof Map) {
    return Array.from(headers.entries());
  }

  if (Array.isArray(headers)) {
    return headers;
  }

  if (headers && typeof headers === "object") {
    return Object.entries(headers);
  }

  return [];
}

function normalizeBody(body) {
  if (body instanceof Uint8Array) {
    return { type: "bytes", value: Array.from(body) };
  }

  return { type: "text", value: body ?? "" };
}

globalThis.Response = Response;
globalThis.__fezz_normalize_response = __fezz_normalize_response;
globalThis.setTimeout = (callback, ms = 0, ...args) => {
  const delay = Number(ms) || 0;
  const id = ++globalThis.__fezz_timeout_id;
  Deno.core.opAsync("op_sleep", delay).then(() => {
    if (globalThis.__fezz_cleared_timeouts.has(id)) {
      return;
    }
    callback(...args);
  });
  return id;
};
globalThis.clearTimeout = (id) => {
  globalThis.__fezz_cleared_timeouts.add(id);
};
globalThis.__fezz_timeout_id = 0;
globalThis.__fezz_cleared_timeouts = new Set();
"#;

#[op]
async fn op_sleep(ms: u64) -> Result<(), anyhow::Error> {
    Delay::new(Duration::from_millis(ms)).await;
    Ok(())
}

fn run_js(script_path: &str, req: JsInvoke) -> Result<JsResult> {
    let timer_ext = Extension::builder().ops(vec![op_sleep::decl()]).build();
    let mut runtime = JsRuntime::new(RuntimeOptions {
        module_loader: Some(Rc::new(FsModuleLoader)),
        extensions: vec![timer_ext],
        ..Default::default()
    });

    runtime
        .execute_script("<fezz-bootstrap>", BOOTSTRAP)
        .context("Failed to execute JS bootstrap")?;

    let canonical_path = fs::canonicalize(script_path)
        .with_context(|| format!("Failed to canonicalize JS module path: {}", script_path))?;
    let module_specifier = ModuleSpecifier::from_file_path(&canonical_path).map_err(|_| {
        anyhow!(
            "Invalid JS module path: {}",
            canonical_path.display()
        )
    })?;

    let module_id = block_on(runtime.load_main_es_module(&module_specifier))
        .context("Failed to load JS module")?;

    let evaluation = runtime.mod_evaluate(module_id);
    block_on(async {
        runtime
            .run_event_loop(PollEventLoopOptions::default())
            .await?;
        evaluation.await?;
        Ok::<(), anyhow::Error>(())
    })
    .context("Failed to evaluate JS module")?;

    let module_namespace = runtime
        .get_module_namespace(module_id)
        .context("Failed to get module namespace")?;
    let mut scope = runtime.handle_scope();
    let module_namespace = v8::Local::new(&mut scope, module_namespace);

    let fetch_fn = resolve_fetch(&mut scope, module_namespace)?;

    let req_value = build_request(&mut scope, &req)?;
    let env_value = build_env(&mut scope, &req.env)?;
    let ctx_value = v8::Object::new(&mut scope);

    let undefined = v8::undefined(&mut scope).into();
    let result = fetch_fn
        .call(
            &mut scope,
            undefined,
            &[req_value.into(), env_value.into(), ctx_value.into()],
        )
        .ok_or_else(|| anyhow!("JS fetch handler threw an exception"))?;

    if result.is_promise() {
        let promise = unsafe { v8::Local::<v8::Promise>::cast(result) };
        let promise = v8::Global::new(&mut scope, promise);
        drop(scope);
        let resolved = resolve_promise(&mut runtime, promise)?;
        let mut scope = runtime.handle_scope();
        let resolved_value = v8::Local::new(&mut scope, &resolved);
        let normalized = normalize_response(&mut scope, resolved_value)?;
        return extract_response(&mut scope, normalized);
    }

    let normalized = normalize_response(&mut scope, result)?;
    extract_response(&mut scope, normalized)
}

fn resolve_promise(
    runtime: &mut JsRuntime,
    promise: v8::Global<v8::Promise>,
) -> Result<v8::Global<v8::Value>> {
    loop {
        block_on(runtime.run_event_loop(PollEventLoopOptions::default()))
            .context("Failed to run JS event loop")?;
        let mut scope = runtime.handle_scope();
        let promise = v8::Local::new(&mut scope, &promise);
        match promise.state() {
            v8::PromiseState::Pending => continue,
            v8::PromiseState::Fulfilled => {
                let value = promise.result(&mut scope);
                return Ok(v8::Global::new(&mut scope, value));
            }
            v8::PromiseState::Rejected => {
                let reason = promise.result(&mut scope);
                let reason = format_js_error(&mut scope, reason);
                return Err(anyhow!("JS fetch promise rejected: {}", reason));
            }
        }
    }
}

fn format_js_error<'a>(
    scope: &mut v8::HandleScope<'a>,
    value: v8::Local<'a, v8::Value>,
) -> String {
    if let Some(string) = value.to_string(scope) {
        return string.to_rust_string_lossy(scope);
    }
    if let Some(json) = v8::json::stringify(scope, value) {
        return json.to_rust_string_lossy(scope);
    }
    "<non-string rejection>".to_string()
}

fn resolve_fetch<'a>(
    scope: &mut v8::HandleScope<'a>,
    module_namespace: v8::Local<'a, v8::Object>,
) -> Result<v8::Local<'a, v8::Function>> {
    let fetch_key = v8::String::new(scope, "fetch").unwrap();
    if let Some(fetch_value) = module_namespace.get(scope, fetch_key.into()) {
        if fetch_value.is_function() {
            return Ok(unsafe { v8::Local::<v8::Function>::cast(fetch_value) });
        }
    }

    let default_key = v8::String::new(scope, "default").unwrap();
    let default_value = module_namespace
        .get(scope, default_key.into())
        .ok_or_else(|| anyhow!("No default export found"))?;

    if !default_value.is_object() {
        return Err(anyhow!("Default export is not an object"));
    }

    let default_obj = unsafe { v8::Local::<v8::Object>::cast(default_value) };
    let fetch_value = default_obj
        .get(scope, fetch_key.into())
        .ok_or_else(|| anyhow!("Default export missing fetch handler"))?;

    if !fetch_value.is_function() {
        return Err(anyhow!("Default export fetch is not a function"));
    }

    Ok(unsafe { v8::Local::<v8::Function>::cast(fetch_value) })
}

fn build_request<'a>(
    scope: &mut v8::HandleScope<'a>,
    req: &JsInvoke,
) -> Result<v8::Local<'a, v8::Object>> {
    let obj = v8::Object::new(scope);

    let method_key = v8::String::new(scope, "method").unwrap();
    let method_value = v8::String::new(scope, &req.method).unwrap();
    obj.set(scope, method_key.into(), method_value.into());

    let path_key = v8::String::new(scope, "path").unwrap();
    let path_value = v8::String::new(scope, &req.path_and_query).unwrap();
    obj.set(scope, path_key.into(), path_value.into());

    let headers_key = v8::String::new(scope, "headers").unwrap();
    let headers_value = build_headers(scope, &req.headers)?;
    obj.set(scope, headers_key.into(), headers_value.into());

    let body_key = v8::String::new(scope, "body").unwrap();
    let body_value = build_body(scope, &req.body)?;
    obj.set(scope, body_key.into(), body_value.into());

    Ok(obj)
}

fn build_headers<'a>(
    scope: &mut v8::HandleScope<'a>,
    headers: &[(String, String)],
) -> Result<v8::Local<'a, v8::Array>> {
    let array = v8::Array::new(scope, headers.len() as i32);
    for (idx, (name, value)) in headers.iter().enumerate() {
        let entry = v8::Array::new(scope, 2);
        let name_value = v8::String::new(scope, name).unwrap();
        let value_value = v8::String::new(scope, value).unwrap();
        let key_index = v8::Integer::new(scope, 0);
        entry.set(scope, key_index.into(), name_value.into());
        let value_index = v8::Integer::new(scope, 1);
        entry.set(scope, value_index.into(), value_value.into());
        let array_index = v8::Integer::new(scope, idx as i32);
        array.set(scope, array_index.into(), entry.into());
    }
    Ok(array)
}

fn build_env<'a>(
    scope: &mut v8::HandleScope<'a>,
    env: &[(String, String)],
) -> Result<v8::Local<'a, v8::Object>> {
    let obj = v8::Object::new(scope);
    for (key, value) in env {
        let key_value = v8::String::new(scope, key).unwrap();
        let value_value = v8::String::new(scope, value).unwrap();
        obj.set(scope, key_value.into(), value_value.into());
    }
    Ok(obj)
}

fn build_body<'a>(
    scope: &mut v8::HandleScope<'a>,
    body: &[u8],
) -> Result<v8::Local<'a, v8::Value>> {
    if body.is_empty() {
        return Ok(v8::String::new(scope, "").unwrap().into());
    }

    let backing_store =
        v8::ArrayBuffer::new_backing_store_from_boxed_slice(body.to_vec().into_boxed_slice());
    let backing_store = backing_store.make_shared();
    let array_buffer = v8::ArrayBuffer::with_backing_store(scope, &backing_store);
    let uint8_array = v8::Uint8Array::new(scope, array_buffer, 0, body.len())
        .ok_or_else(|| anyhow!("Failed to create Uint8Array"))?;
    Ok(uint8_array.into())
}

fn normalize_response<'a>(
    scope: &mut v8::HandleScope<'a>,
    value: v8::Local<'a, v8::Value>,
) -> Result<v8::Local<'a, v8::Object>> {
    let global = scope.get_current_context().global(scope);
    let normalizer_key = v8::String::new(scope, "__fezz_normalize_response").unwrap();
    let normalizer_value = global
        .get(scope, normalizer_key.into())
        .ok_or_else(|| anyhow!("Missing response normalizer"))?;
    if !normalizer_value.is_function() {
        return Err(anyhow!("Response normalizer is not callable"));
    }
    let normalizer_fn = unsafe { v8::Local::<v8::Function>::cast(normalizer_value) };
    let normalized = normalizer_fn
        .call(scope, global.into(), &[value])
        .ok_or_else(|| anyhow!("Failed to normalize response"))?;

    if !normalized.is_object() {
        return Err(anyhow!("Normalized response is not an object"));
    }

    Ok(unsafe { v8::Local::<v8::Object>::cast(normalized) })
}

fn extract_response<'a>(
    scope: &mut v8::HandleScope<'a>,
    response: v8::Local<'a, v8::Object>,
) -> Result<JsResult> {
    let status = get_u16_property(scope, response, "status")?.unwrap_or(200);
    let headers = get_headers(scope, response)?;
    let body = get_body(scope, response)?;

    Ok(JsResult {
        status,
        headers,
        body,
    })
}

fn get_u16_property<'a>(
    scope: &mut v8::HandleScope<'a>,
    obj: v8::Local<'a, v8::Object>,
    key: &str,
) -> Result<Option<u16>> {
    let key_value = v8::String::new(scope, key).unwrap();
    let value = match obj.get(scope, key_value.into()) {
        Some(value) => value,
        None => return Ok(None),
    };
    if value.is_number() {
        let number = value.number_value(scope).unwrap_or(200.0);
        return Ok(Some(number as u16));
    }
    Ok(None)
}

fn get_headers<'a>(
    scope: &mut v8::HandleScope<'a>,
    obj: v8::Local<'a, v8::Object>,
) -> Result<Vec<(String, String)>> {
    let key_value = v8::String::new(scope, "headers").unwrap();
    let value = obj
        .get(scope, key_value.into())
        .unwrap_or_else(|| v8::Array::new(scope, 0).into());

    if !value.is_array() {
        return Ok(Vec::new());
    }

    let array = unsafe { v8::Local::<v8::Array>::cast(value) };
    let mut headers = Vec::new();
    for idx in 0..array.length() {
        let entry_index = v8::Integer::new(scope, idx as i32);
        let entry = array
            .get(scope, entry_index.into())
            .unwrap_or_else(|| v8::undefined(scope).into());
        if !entry.is_array() {
            continue;
        }
        let entry_array = unsafe { v8::Local::<v8::Array>::cast(entry) };
        let key_index = v8::Integer::new(scope, 0);
        let name = entry_array
            .get(scope, key_index.into())
            .and_then(|val| val.to_string(scope))
            .map(|s| s.to_rust_string_lossy(scope));
        let value_index = v8::Integer::new(scope, 1);
        let value = entry_array
            .get(scope, value_index.into())
            .and_then(|val| val.to_string(scope))
            .map(|s| s.to_rust_string_lossy(scope));
        if let (Some(name), Some(value)) = (name, value) {
            headers.push((name, value));
        }
    }

    Ok(headers)
}

fn get_body<'a>(
    scope: &mut v8::HandleScope<'a>,
    obj: v8::Local<'a, v8::Object>,
) -> Result<Vec<u8>> {
    let key_value = v8::String::new(scope, "body").unwrap();
    let value = match obj.get(scope, key_value.into()) {
        Some(value) => value,
        None => return Ok(Vec::new()),
    };

    if value.is_null_or_undefined() {
        return Ok(Vec::new());
    }

    if value.is_string() {
        let string = value
            .to_string(scope)
            .ok_or_else(|| anyhow!("Failed to read response body string"))?;
        return Ok(string.to_rust_string_lossy(scope).into_bytes());
    }

    if value.is_object() {
        let body_obj = unsafe { v8::Local::<v8::Object>::cast(value) };
        let body_type = get_string_property(scope, body_obj, "type")?;
        let value_key = v8::String::new(scope, "value").unwrap();
        let body_value = body_obj.get(scope, value_key.into());

        if let Some(body_type) = body_type {
            match body_type.as_str() {
                "bytes" => {
                    if let Some(body_value) = body_value {
                        if body_value.is_array() {
                            let array = unsafe { v8::Local::<v8::Array>::cast(body_value) };
                            let mut bytes = Vec::with_capacity(array.length() as usize);
                            for idx in 0..array.length() {
                                let entry_index = v8::Integer::new(scope, idx as i32);
                                let entry = array
                                    .get(scope, entry_index.into())
                                    .and_then(|val| val.integer_value(scope));
                                if let Some(value) = entry {
                                    bytes.push(value as u8);
                                }
                            }
                            return Ok(bytes);
                        }
                    }
                }
                "text" => {
                    if let Some(body_value) = body_value {
                        if body_value.is_string() {
                            let string = body_value
                                .to_string(scope)
                                .ok_or_else(|| anyhow!("Failed to read response body string"))?;
                            return Ok(string.to_rust_string_lossy(scope).into_bytes());
                        }
                        if let Some(json) = v8::json::stringify(scope, body_value) {
                            return Ok(json.to_rust_string_lossy(scope).into_bytes());
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(json) = v8::json::stringify(scope, value) {
            return Ok(json.to_rust_string_lossy(scope).into_bytes());
        }
    }

    Ok(Vec::new())
}

fn get_string_property<'a>(
    scope: &mut v8::HandleScope<'a>,
    obj: v8::Local<'a, v8::Object>,
    key: &str,
) -> Result<Option<String>> {
    let key_value = v8::String::new(scope, key).unwrap();
    let value = match obj.get(scope, key_value.into()) {
        Some(value) => value,
        None => return Ok(None),
    };
    if value.is_string() {
        let value = value
            .to_string(scope)
            .ok_or_else(|| anyhow!("Failed to read string property"))?;
        return Ok(Some(value.to_rust_string_lossy(scope)));
    }
    Ok(None)
}
