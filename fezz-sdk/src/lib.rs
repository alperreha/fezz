use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct FezzHttpRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct FezzHttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

// Jsonplaceholder /todos için özel payload’a ihtiyacın yok, body forward edilecek.
