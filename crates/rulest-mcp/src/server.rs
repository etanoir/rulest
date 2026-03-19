use std::path::Path;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use rulest_core::registry;

use crate::tools;

const SERVER_NAME: &str = "rulest";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
const PROTOCOL_VERSION: &str = "2024-11-05";

/// Run the MCP server over stdio (JSON-RPC 2.0).
pub async fn run_stdio(db_path: &Path) -> Result<(), String> {
    let conn = registry::open_registry(db_path)
        .map_err(|e| format!("Failed to open registry: {}", e))?;

    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .await
            .map_err(|e| format!("Failed to read stdin: {}", e))?;

        if bytes_read == 0 {
            break; // EOF
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                let error_response = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {}", e)
                    }
                });
                write_response(&mut stdout, &error_response).await?;
                continue;
            }
        };

        let id = request.get("id").cloned();
        let method = request["method"].as_str().unwrap_or("");
        let is_notification = id.is_none() || id.as_ref() == Some(&Value::Null);

        let response = match method {
            "initialize" => Some(handle_initialize(id)),
            "tools/list" => Some(handle_tools_list(id)),
            "tools/call" => {
                let params = request.get("params").cloned().unwrap_or(json!({}));
                Some(handle_tools_call(id, &params, &conn))
            }
            _ if is_notification => None, // Notifications get no response per JSON-RPC 2.0
            _ => {
                Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("Method not found: {}", method)
                    }
                }))
            }
        };

        if let Some(ref resp) = response {
            write_response(&mut stdout, resp).await?;
        }
    }

    Ok(())
}

fn handle_initialize(id: Option<Value>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": SERVER_NAME,
                "version": SERVER_VERSION
            }
        }
    })
}

fn handle_tools_list(id: Option<Value>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": tools::tool_definitions()
        }
    })
}

fn handle_tools_call(
    id: Option<Value>,
    params: &Value,
    conn: &rusqlite::Connection,
) -> Value {
    let tool_name = params["name"].as_str().unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    let result = tools::call_tool(conn, tool_name, &arguments);

    let is_error = result.get("error").is_some();

    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [
                {
                    "type": "text",
                    "text": serde_json::to_string_pretty(&result).unwrap_or_default()
                }
            ],
            "isError": is_error
        }
    })
}

async fn write_response(
    stdout: &mut tokio::io::Stdout,
    response: &Value,
) -> Result<(), String> {
    let serialized =
        serde_json::to_string(response).map_err(|e| format!("Failed to serialize: {}", e))?;
    stdout
        .write_all(serialized.as_bytes())
        .await
        .map_err(|e| format!("Failed to write: {}", e))?;
    stdout
        .write_all(b"\n")
        .await
        .map_err(|e| format!("Failed to write newline: {}", e))?;
    stdout
        .flush()
        .await
        .map_err(|e| format!("Failed to flush: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rulest_core::registry;

    fn setup_test_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        registry::create_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn test_handle_initialize() {
        let response = handle_initialize(Some(json!(1)));
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 1);
        assert!(response["result"]["protocolVersion"].is_string());
        assert!(response["result"]["capabilities"]["tools"].is_object());
        assert_eq!(response["result"]["serverInfo"]["name"], "rulest");
    }

    #[test]
    fn test_handle_tools_list() {
        let response = handle_tools_list(Some(json!(2)));
        assert_eq!(response["id"], 2);
        let tools = response["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 7, "Should expose 7 MCP tools");
        // Verify all tool names are present
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"validate_creation"));
        assert!(names.contains(&"validate_dependency"));
        assert!(names.contains(&"validate_boundary"));
        assert!(names.contains(&"check_wip"));
        assert!(names.contains(&"suggest_reuse"));
        assert!(names.contains(&"register_plan"));
        assert!(names.contains(&"validate_plan"));
    }

    #[test]
    fn test_handle_tools_call_valid() {
        let conn = setup_test_db();
        let params = json!({
            "name": "validate_creation",
            "arguments": {
                "symbol_name": "nonexistent_fn",
                "target_module": "src/lib.rs"
            }
        });
        let response = handle_tools_call(Some(json!(3)), &params, &conn);
        assert_eq!(response["id"], 3);
        assert!(response["result"]["content"].is_array());
        assert_eq!(response["result"]["isError"], false);
    }

    #[test]
    fn test_handle_tools_call_unknown_tool() {
        let conn = setup_test_db();
        let params = json!({
            "name": "nonexistent_tool",
            "arguments": {}
        });
        let response = handle_tools_call(Some(json!(4)), &params, &conn);
        assert_eq!(response["result"]["isError"], true);
    }

    #[test]
    fn test_handle_tools_call_validate_plan() {
        let conn = setup_test_db();
        let params = json!({
            "name": "validate_plan",
            "arguments": {
                "actions": [{
                    "action": "create",
                    "symbol": "new_fn",
                    "target": "src/lib.rs"
                }]
            }
        });
        let response = handle_tools_call(Some(json!(5)), &params, &conn);
        assert_eq!(response["result"]["isError"], false);
    }
}
