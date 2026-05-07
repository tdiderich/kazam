use anyhow::Result;
use std::io::{BufRead, Write};
use std::path::Path;

mod protocol;
mod tools;

use protocol::{JsonRpcRequest, JsonRpcResponse, ToolResult};

/// Whether a request requires a response on the wire.
/// MCP notifications (method starts with "notifications/") must NOT get a reply.
fn is_notification(method: &str) -> bool {
    method.starts_with("notifications/")
}

pub fn run(dir: &Path, allow_writes: bool) -> Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();

    eprintln!("kazam mcp: listening on stdio (dir={})", dir.display());

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let req = match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse::err(None, -32700, format!("parse error: {}", e));
                let out = serde_json::to_string(&resp)?;
                let mut locked = stdout.lock();
                writeln!(locked, "{}", out)?;
                locked.flush()?;
                continue;
            }
        };

        // MCP spec: notifications have no id and require no response.
        if is_notification(&req.method) {
            continue;
        }

        let response = dispatch(&req, dir, allow_writes);
        let out = serde_json::to_string(&response)?;
        let mut locked = stdout.lock();
        writeln!(locked, "{}", out)?;
        locked.flush()?;
    }

    Ok(())
}

pub fn run_http(
    dir: &Path,
    allow_writes: bool,
    port: u16,
    bind_host: &str,
    token: Option<&str>,
) -> Result<()> {
    let addr = format!("{}:{}", bind_host, port);
    let server = tiny_http::Server::http(&addr)
        .map_err(|e| anyhow::anyhow!("failed to bind {}: {}", addr, e))?;

    let mode = if bind_host == "127.0.0.1" {
        "local"
    } else {
        "remote"
    };
    let auth_status = if token.is_some() {
        "bearer-token"
    } else {
        "none"
    };
    eprintln!(
        "kazam mcp: listening on http://{} (dir={}, mode={}, auth={})",
        addr,
        dir.display(),
        mode,
        auth_status,
    );

    for mut request in server.incoming_requests() {
        let cors = tiny_http::Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap();
        let cors_headers = tiny_http::Header::from_bytes(
            "Access-Control-Allow-Headers",
            "Content-Type, Authorization",
        )
        .unwrap();
        let cors_methods =
            tiny_http::Header::from_bytes("Access-Control-Allow-Methods", "POST, OPTIONS").unwrap();
        let content_type =
            tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap();

        // Handle CORS preflight
        if request.method() == &tiny_http::Method::Options {
            let response = tiny_http::Response::from_string("")
                .with_status_code(204)
                .with_header(cors.clone())
                .with_header(cors_headers.clone())
                .with_header(cors_methods.clone());
            let _ = request.respond(response);
            continue;
        }

        // Bearer token check (when configured)
        if let Some(expected) = token {
            let auth_ok = request.headers().iter().any(|h| {
                h.field.as_str().to_ascii_lowercase() == "authorization"
                    && h.value
                        .as_str()
                        .strip_prefix("Bearer ")
                        .map(|t| t == expected)
                        .unwrap_or(false)
            });
            if !auth_ok {
                let body = r#"{"jsonrpc":"2.0","error":{"code":-32000,"message":"unauthorized: missing or invalid bearer token"},"id":null}"#;
                let response = tiny_http::Response::from_string(body)
                    .with_status_code(401)
                    .with_header(content_type.clone())
                    .with_header(cors.clone());
                let _ = request.respond(response);
                continue;
            }
        }

        // Only accept POST
        if request.method() != &tiny_http::Method::Post {
            let body = r#"{"jsonrpc":"2.0","error":{"code":-32600,"message":"only POST is accepted"},"id":null}"#;
            let response = tiny_http::Response::from_string(body)
                .with_status_code(405)
                .with_header(content_type.clone())
                .with_header(cors.clone());
            let _ = request.respond(response);
            continue;
        }

        // Read body
        let mut body = String::new();
        if let Err(e) = request.as_reader().read_to_string(&mut body) {
            eprintln!("kazam mcp: read error: {}", e);
            let err_body = format!(
                r#"{{"jsonrpc":"2.0","error":{{"code":-32700,"message":"read error: {}"}},"id":null}}"#,
                e
            );
            let response = tiny_http::Response::from_string(err_body)
                .with_status_code(400)
                .with_header(content_type.clone())
                .with_header(cors.clone());
            let _ = request.respond(response);
            continue;
        }

        // Parse JSON-RPC request
        let req = match serde_json::from_str::<JsonRpcRequest>(&body) {
            Ok(r) => r,
            Err(e) => {
                let err_body = format!(
                    r#"{{"jsonrpc":"2.0","error":{{"code":-32700,"message":"parse error: {}"}},"id":null}}"#,
                    e
                );
                let response = tiny_http::Response::from_string(err_body)
                    .with_status_code(400)
                    .with_header(content_type.clone())
                    .with_header(cors.clone());
                let _ = request.respond(response);
                continue;
            }
        };

        // Skip notifications (no response needed), but still send 204
        if is_notification(&req.method) {
            let response = tiny_http::Response::from_string("")
                .with_status_code(204)
                .with_header(cors.clone());
            let _ = request.respond(response);
            continue;
        }

        let rpc_response = dispatch(&req, dir, allow_writes);
        let out = serde_json::to_string(&rpc_response).unwrap_or_else(|_| {
            r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"serialization error"},"id":null}"#.into()
        });

        let response = tiny_http::Response::from_string(out)
            .with_status_code(200)
            .with_header(content_type.clone())
            .with_header(cors.clone());
        let _ = request.respond(response);
    }

    Ok(())
}

fn dispatch(req: &JsonRpcRequest, dir: &Path, allow_writes: bool) -> JsonRpcResponse {
    match req.method.as_str() {
        "initialize" => {
            let result = serde_json::json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": {
                    "name": "kazam-mcp",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "capabilities": {
                    "tools": {}
                }
            });
            JsonRpcResponse::ok(req.id.clone(), result)
        }

        "tools/list" => {
            let tool_list = tools::tool_definitions();
            let result = serde_json::json!({ "tools": tool_list });
            JsonRpcResponse::ok(req.id.clone(), result)
        }

        "tools/call" => {
            let params = req
                .params
                .as_ref()
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let name = params
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Object(Default::default()));

            let tool_result: Result<ToolResult> = match name {
                "read_page" => tools::read_page(dir, &args),
                "list_pages" => tools::list_pages(dir, &args),
                "get_config" => tools::get_config(dir),
                "search" => tools::search(dir, &args),
                "write_page" => tools::write_page(dir, &args, allow_writes),
                "annotate_page" => tools::annotate_page(dir, &args, allow_writes),
                "update_annotation" => tools::update_annotation(dir, &args, allow_writes),
                "list_annotations" => tools::list_annotations(dir, &args),
                other => Ok(ToolResult::error(format!("unknown tool: {}", other))),
            };

            match tool_result {
                Ok(tr) => {
                    let result = serde_json::to_value(&tr).unwrap_or(serde_json::Value::Null);
                    JsonRpcResponse::ok(req.id.clone(), result)
                }
                Err(e) => JsonRpcResponse::err(req.id.clone(), -32603, e.to_string()),
            }
        }

        other => JsonRpcResponse::err(
            req.id.clone(),
            -32601,
            format!("method not found: {}", other),
        ),
    }
}

// ── Tests ─────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_req(method: &str, params: Option<serde_json::Value>) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: method.into(),
            params,
        }
    }

    #[test]
    fn initialize_returns_capabilities() {
        let dir = tempfile::tempdir().unwrap();
        let resp = dispatch(&make_req("initialize", None), dir.path(), false);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert!(result.get("capabilities").is_some());
        assert!(result.get("serverInfo").is_some());
    }

    #[test]
    fn tools_list_returns_all_tools() {
        let dir = tempfile::tempdir().unwrap();
        let resp = dispatch(&make_req("tools/list", None), dir.path(), false);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let tool_list = result["tools"].as_array().unwrap();
        let names: Vec<&str> = tool_list
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"read_page"));
        assert!(names.contains(&"list_pages"));
        assert!(names.contains(&"get_config"));
        assert!(names.contains(&"search"));
        assert!(names.contains(&"write_page"));
    }

    #[test]
    fn tools_have_input_schema() {
        let dir = tempfile::tempdir().unwrap();
        let resp = dispatch(&make_req("tools/list", None), dir.path(), false);
        let result = resp.result.unwrap();
        let tool_list = result["tools"].as_array().unwrap();
        for tool in tool_list {
            assert!(
                tool.get("inputSchema").is_some(),
                "tool {} missing inputSchema",
                tool["name"]
            );
        }
    }

    #[test]
    fn unknown_method_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let resp = dispatch(&make_req("bogus/method", None), dir.path(), false);
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[test]
    fn tools_call_list_pages() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("index.yaml"),
            "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Hi\n",
        )
        .unwrap();

        let params = json!({
            "name": "list_pages",
            "arguments": {}
        });
        let resp = dispatch(&make_req("tools/call", Some(params)), dir.path(), false);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("index.yaml"));
    }

    #[test]
    fn jsonrpc_request_parse() {
        let raw = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":null}"#;
        let req: JsonRpcRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(req.method, "tools/list");
        assert_eq!(req.jsonrpc, "2.0");
    }

    #[test]
    fn response_always_has_jsonrpc_field() {
        let resp = JsonRpcResponse::ok(Some(json!(42)), json!({"ok": true}));
        let serialized = serde_json::to_string(&resp).unwrap();
        assert!(serialized.contains("\"jsonrpc\":\"2.0\""));
    }
}
