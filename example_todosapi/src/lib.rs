use fezz_macros::fezz_function;
use fezz_sdk::{FezzWireHeader, FezzWireRequest, FezzWireResponse};

// Basit demo: req içeriğini çok umursamıyoruz, hep /todos çağıracağız
#[fezz_function]
pub fn proxy_todos(_req: FezzWireRequest) -> FezzWireResponse {
    let client = reqwest::blocking::Client::new();
    let res = client
        .get("https://jsonplaceholder.typicode.com/todos/1")
        .send()
        .unwrap();

    let status = res.status().as_u16();
    let body = res.bytes().unwrap_or_default().to_vec();

    FezzWireResponse::new(
        status,
        vec![FezzWireHeader::new("content-type", "application/json")],
        body,
    )
}
