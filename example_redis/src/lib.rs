use fezz_macros::fezz_function;
use fezz_sdk::{FezzWireHeader, FezzWireRequest, FezzWireResponse};
use redis::Commands;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Serialize, Deserialize)]
struct SetRequest {
    key: String,
    value: String,
}

#[derive(Serialize, Deserialize)]
struct GetRequest {
    key: String,
}

#[derive(Serialize, Deserialize)]
struct RedisResponse {
    success: bool,
    message: String,
    data: Option<String>,
}

/// Global Redis client using OnceLock for warm state persistence.
/// The client is initialized once and reused across all requests
/// for the lifetime of the cached library.
static REDIS_CLIENT: OnceLock<redis::Client> = OnceLock::new();

/// Gets or initializes the Redis client.
/// Uses OnceLock to ensure the client is only created once.
fn get_redis_client() -> Result<&'static redis::Client, redis::RedisError> {
    // Initialize the client only once
    let client = REDIS_CLIENT.get_or_init(|| {
        redis::Client::open("redis://127.0.0.1:6379/")
            .expect("Failed to create Redis client")
    });
    Ok(client)
}

fn get_redis_connection() -> Result<redis::Connection, redis::RedisError> {
    let client = get_redis_client()?;
    client.get_connection()
}

#[fezz_function]
pub fn redis_demo(req: FezzWireRequest) -> FezzWireResponse {
    let path = req.path_and_query.as_str();
    let method = req.method.as_str();

    match (method, path) {
        // POST /set - Write key-value to Redis
        ("POST", "/set") => {
            let body = match request_body_to_string(&req) {
                Ok(body) => body,
                Err(resp) => return resp,
            };

            let set_req: SetRequest = match serde_json::from_str(&body) {
                Ok(r) => r,
                Err(e) => {
                    return json_response(400, RedisResponse {
                        success: false,
                        message: format!("Invalid JSON: {}", e),
                        data: None,
                    });
                }
            };

            let mut conn = match get_redis_connection() {
                Ok(c) => c,
                Err(e) => {
                    return json_response(500, RedisResponse {
                        success: false,
                        message: format!("Redis connection error: {}", e),
                        data: None,
                    });
                }
            };

            match conn.set::<_, _, ()>(&set_req.key, &set_req.value) {
                Ok(_) => json_response(200, RedisResponse {
                    success: true,
                    message: format!("Key '{}' set successfully", set_req.key),
                    data: Some(set_req.value),
                }),
                Err(e) => json_response(500, RedisResponse {
                    success: false,
                    message: format!("Redis SET error: {}", e),
                    data: None,
                }),
            }
        }

        // GET /get?key=xxx or POST /get with body {"key": "xxx"}
        ("GET", p) if p.starts_with("/get") => {
            // Get key from query string: /get?key=mykey
            let key = p
                .split("key=")
                .nth(1)
                .map(|s| s.split('&').next().unwrap_or(s))
                .unwrap_or("");

            if key.is_empty() {
                return json_response(400, RedisResponse {
                    success: false,
                    message: "Key parameter required: /get?key=xxx".into(),
                    data: None,
                });
            }

            let mut conn = match get_redis_connection() {
                Ok(c) => c,
                Err(e) => {
                    return json_response(500, RedisResponse {
                        success: false,
                        message: format!("Redis connection error: {}", e),
                        data: None,
                    });
                }
            };

            match conn.get::<_, Option<String>>(key) {
                Ok(Some(value)) => json_response(200, RedisResponse {
                    success: true,
                    message: format!("Key '{}' found", key),
                    data: Some(value),
                }),
                Ok(None) => json_response(404, RedisResponse {
                    success: false,
                    message: format!("Key '{}' not found", key),
                    data: None,
                }),
                Err(e) => json_response(500, RedisResponse {
                    success: false,
                    message: format!("Redis GET error: {}", e),
                    data: None,
                }),
            }
        }

        // POST /get - Get key from body
        ("POST", "/get") => {
            let body = match request_body_to_string(&req) {
                Ok(body) => body,
                Err(resp) => return resp,
            };

            let get_req: GetRequest = match serde_json::from_str(&body) {
                Ok(r) => r,
                Err(e) => {
                    return json_response(400, RedisResponse {
                        success: false,
                        message: format!("Invalid JSON: {}", e),
                        data: None,
                    });
                }
            };

            let mut conn = match get_redis_connection() {
                Ok(c) => c,
                Err(e) => {
                    return json_response(500, RedisResponse {
                        success: false,
                        message: format!("Redis connection error: {}", e),
                        data: None,
                    });
                }
            };

            match conn.get::<_, Option<String>>(&get_req.key) {
                Ok(Some(value)) => json_response(200, RedisResponse {
                    success: true,
                    message: format!("Key '{}' found", get_req.key),
                    data: Some(value),
                }),
                Ok(None) => json_response(404, RedisResponse {
                    success: false,
                    message: format!("Key '{}' not found", get_req.key),
                    data: None,
                }),
                Err(e) => json_response(500, RedisResponse {
                    success: false,
                    message: format!("Redis GET error: {}", e),
                    data: None,
                }),
            }
        }

        // DELETE /del - Delete key
        ("DELETE", "/del") => {
            let body = match request_body_to_string(&req) {
                Ok(body) => body,
                Err(resp) => return resp,
            };

            let get_req: GetRequest = match serde_json::from_str(&body) {
                Ok(r) => r,
                Err(e) => {
                    return json_response(400, RedisResponse {
                        success: false,
                        message: format!("Invalid JSON: {}", e),
                        data: None,
                    });
                }
            };

            let mut conn = match get_redis_connection() {
                Ok(c) => c,
                Err(e) => {
                    return json_response(500, RedisResponse {
                        success: false,
                        message: format!("Redis connection error: {}", e),
                        data: None,
                    });
                }
            };

            match conn.del::<_, i32>(&get_req.key) {
                Ok(count) if count > 0 => json_response(200, RedisResponse {
                    success: true,
                    message: format!("Key '{}' deleted", get_req.key),
                    data: None,
                }),
                Ok(_) => json_response(404, RedisResponse {
                    success: false,
                    message: format!("Key '{}' not found", get_req.key),
                    data: None,
                }),
                Err(e) => json_response(500, RedisResponse {
                    success: false,
                    message: format!("Redis DEL error: {}", e),
                    data: None,
                }),
            }
        }

        // Default: usage info
        _ => json_response(200, RedisResponse {
            success: true,
            message: "Redis Demo API - Endpoints: POST /set, GET /get?key=xxx, POST /get, DELETE /del".into(),
            data: None,
        }),
    }
}

fn request_body_to_string(req: &FezzWireRequest) -> Result<String, FezzWireResponse> {
    if req.body.is_empty() {
        return Err(json_response(400, RedisResponse {
            success: false,
            message: "Body required".into(),
            data: None,
        }));
    }

    String::from_utf8(req.body.to_vec()).map_err(|e| {
        json_response(400, RedisResponse {
            success: false,
            message: format!("Invalid UTF-8 body: {}", e),
            data: None,
        })
    })
}

fn json_response(status: u16, body: RedisResponse) -> FezzWireResponse {
    let body = serde_json::to_vec(&body)
        .unwrap_or_else(|_| b"{\"success\":false,\"message\":\"Serialization failed\",\"data\":null}".to_vec());
    FezzWireResponse::new(
        status,
        vec![FezzWireHeader::new("content-type", "application/json")],
        body,
    )
}
