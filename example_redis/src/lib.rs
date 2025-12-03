use fezz_macros::fezz_function;
use fezz_sdk::{FezzHttpRequest, FezzHttpResponse};
use redis::Commands;
use serde::{Deserialize, Serialize};

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

fn get_redis_connection() -> Result<redis::Connection, redis::RedisError> {
    let client = redis::Client::open("redis://127.0.0.1:6379/")?;
    client.get_connection()
}

fn with_redis<F, T>(f: F) -> Result<T, redis::RedisError>
where
    F: FnOnce(&mut redis::Connection) -> Result<T, redis::RedisError>,
{
    let mut conn = get_redis_connection()?;
    let result = f(&mut conn);
    // Connection is automatically closed when dropped
    drop(conn);
    result
}

#[fezz_function]
pub fn redis_demo(req: FezzHttpRequest) -> FezzHttpResponse {
    let path = req.path.as_str();
    let method = req.method.as_str();

    match (method, path) {
        // POST /set - Redis'e key-value yaz
        ("POST", "/set") => {
            let body = match &req.body {
                Some(b) => b,
                None => {
                    return json_response(400, RedisResponse {
                        success: false,
                        message: "Body required".into(),
                        data: None,
                    });
                }
            };

            let set_req: SetRequest = match serde_json::from_str(body) {
                Ok(r) => r,
                Err(e) => {
                    return json_response(400, RedisResponse {
                        success: false,
                        message: format!("Invalid JSON: {}", e),
                        data: None,
                    });
                }
            };

            let result = with_redis(|conn| {
                conn.set::<_, _, ()>(&set_req.key, &set_req.value)
            });

            match result {
                Ok(_) => json_response(200, RedisResponse {
                    success: true,
                    message: format!("Key '{}' set successfully", set_req.key),
                    data: Some(set_req.value),
                }),
                Err(e) => json_response(500, RedisResponse {
                    success: false,
                    message: format!("Redis error: {}", e),
                    data: None,
                }),
            }
        }

        // GET /get?key=xxx veya POST /get ile body {"key": "xxx"}
        ("GET", p) if p.starts_with("/get") => {
            // Query string'den key al: /get?key=mykey
            let key = p.split("key=")
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

            let key_owned = key.to_string();
            let result = with_redis(|conn| conn.get::<_, Option<String>>(&key_owned));

            match result {
                Ok(Some(value)) => json_response(200, RedisResponse {
                    success: true,
                    message: format!("Key '{}' found", key_owned),
                    data: Some(value),
                }),
                Ok(None) => json_response(404, RedisResponse {
                    success: false,
                    message: format!("Key '{}' not found", key_owned),
                    data: None,
                }),
                Err(e) => json_response(500, RedisResponse {
                    success: false,
                    message: format!("Redis error: {}", e),
                    data: None,
                }),
            }
        }

        // POST /get - Body'den key al
        ("POST", "/get") => {
            let body = match &req.body {
                Some(b) => b,
                None => {
                    return json_response(400, RedisResponse {
                        success: false,
                        message: "Body required".into(),
                        data: None,
                    });
                }
            };

            let get_req: GetRequest = match serde_json::from_str(body) {
                Ok(r) => r,
                Err(e) => {
                    return json_response(400, RedisResponse {
                        success: false,
                        message: format!("Invalid JSON: {}", e),
                        data: None,
                    });
                }
            };

            let result = with_redis(|conn| conn.get::<_, Option<String>>(&get_req.key));

            match result {
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
                    message: format!("Redis error: {}", e),
                    data: None,
                }),
            }
        }

        // DELETE /del - Key sil
        ("DELETE", "/del") => {
            let body = match &req.body {
                Some(b) => b,
                None => {
                    return json_response(400, RedisResponse {
                        success: false,
                        message: "Body required".into(),
                        data: None,
                    });
                }
            };

            let del_req: GetRequest = match serde_json::from_str(body) {
                Ok(r) => r,
                Err(e) => {
                    return json_response(400, RedisResponse {
                        success: false,
                        message: format!("Invalid JSON: {}", e),
                        data: None,
                    });
                }
            };

            let result = with_redis(|conn| conn.del::<_, i32>(&del_req.key));

            match result {
                Ok(count) if count > 0 => json_response(200, RedisResponse {
                    success: true,
                    message: format!("Key '{}' deleted", del_req.key),
                    data: None,
                }),
                Ok(_) => json_response(404, RedisResponse {
                    success: false,
                    message: format!("Key '{}' not found", del_req.key),
                    data: None,
                }),
                Err(e) => json_response(500, RedisResponse {
                    success: false,
                    message: format!("Redis error: {}", e),
                    data: None,
                }),
            }
        }

        // Default: kullanım bilgisi
        _ => json_response(200, RedisResponse {
            success: true,
            message: "Redis Demo API - Endpoints: POST /set, GET /get?key=xxx, POST /get, DELETE /del".into(),
            data: None,
        }),
    }
}

fn json_response(status: u16, body: RedisResponse) -> FezzHttpResponse {
    FezzHttpResponse {
        status,
        headers: vec![("content-type".into(), "application/json".into())],
        body: Some(serde_json::to_string(&body).unwrap()),
    }
}
