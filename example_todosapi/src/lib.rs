use fezz_macros::fezz_function;
use fezz_sdk::{FezzHttpRequest, FezzHttpResponse};

// Basit demo: req içeriğini çok umursamıyoruz, hep /todos çağıracağız
#[fezz_function]
pub fn proxy_todos(_req: FezzHttpRequest) -> FezzHttpResponse {
    // debug log all request details
    println!("[TODOS Demo] Received request: method='{}', path='{}'", _req.method, _req.path);
    println!("[TODOS Demo] Headers: {:?}", _req.headers);
    println!("[TODOS Demo] Body: {:?}", _req.body);

    let client = reqwest::blocking::Client::new();
    let res = client
        .get("https://jsonplaceholder.typicode.com/todos/1")
        .send()
        .unwrap();

    let status = res.status().as_u16();
    let body = res.text().unwrap();

    FezzHttpResponse {
        status,
        headers: vec![("content-type".into(), "application/json".into())],
        body: Some(body),
    }
}
