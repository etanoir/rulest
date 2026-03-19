use rusqlite::Connection;
use serde_json::{json, Value};

use rulest_core::queries;

/// Tool definitions for MCP `tools/list`.
pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "validate_creation",
            "description": "Check if a symbol can be created or if a similar one already exists. Returns SAFE_TO_CREATE, REUSE_EXISTING, or AMBIGUOUS_MATCH.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "symbol_name": {
                        "type": "string",
                        "description": "Name of the symbol to create"
                    },
                    "target_module": {
                        "type": "string",
                        "description": "Target module path (e.g., crates/trading/src/fees.rs)"
                    }
                },
                "required": ["symbol_name", "target_module"]
            }
        }),
        json!({
            "name": "validate_dependency",
            "description": "Look up a type (struct/trait/enum) across all crates. Returns USE_EXISTING_TYPE with prelude path if found.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "type_name": {
                        "type": "string",
                        "description": "Name of the type to look up"
                    }
                },
                "required": ["type_name"]
            }
        }),
        json!({
            "name": "validate_boundary",
            "description": "Check if placing a symbol in a crate violates ownership rules. Returns BOUNDARY_VIOLATION if rules are violated.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "symbol_name": {
                        "type": "string",
                        "description": "Name of the symbol to place"
                    },
                    "target_crate": {
                        "type": "string",
                        "description": "Target crate name"
                    }
                },
                "required": ["symbol_name", "target_crate"]
            }
        }),
        json!({
            "name": "check_wip",
            "description": "Scan for WIP or planned symbols in a module path. Returns WIP_CONFLICT if active work is detected.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "module_path": {
                        "type": "string",
                        "description": "Module path to check (e.g., src/fees)"
                    }
                },
                "required": ["module_path"]
            }
        }),
        json!({
            "name": "suggest_reuse",
            "description": "Search for reusable symbols matching a capability description. Returns suggestions for existing code that can be reused.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "capability": {
                        "type": "string",
                        "description": "Description of the capability needed (e.g., 'calculate trading fees')"
                    }
                },
                "required": ["capability"]
            }
        }),
        json!({
            "name": "register_plan",
            "description": "Register planned symbols from an AI plan into the registry. Enables conflict detection via check_wip for multi-agent coordination.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "actions": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "action": { "type": "string", "description": "create or modify" },
                                "symbol": { "type": "string", "description": "Symbol name" },
                                "target": { "type": "string", "description": "Target file path" },
                                "crate_name": { "type": "string", "description": "Target crate name (optional)" },
                                "kind": { "type": "string", "description": "Symbol kind: function, struct, enum, trait, type_alias, const, static, macro (optional, defaults to function)" }
                            },
                            "required": ["action", "symbol", "target"]
                        },
                        "description": "List of planned actions to register"
                    },
                    "agent": {
                        "type": "string",
                        "description": "Agent identifier for tracking"
                    }
                },
                "required": ["actions", "agent"]
            }
        }),
        json!({
            "name": "validate_plan",
            "description": "Validate an entire structured plan against the registry. Checks each action for conflicts, duplicates, boundary violations, and WIP conflicts. Returns a report with per-action advisories and a summary.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "actions": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "action": { "type": "string", "description": "create or modify" },
                                "symbol": { "type": "string", "description": "Symbol name" },
                                "target": { "type": "string", "description": "Target file path" },
                                "crate_name": { "type": "string", "description": "Target crate name (optional)" },
                                "kind": { "type": "string", "description": "Symbol kind: function, struct, enum, trait, type_alias, const, static, macro (optional, defaults to function)" }
                            },
                            "required": ["action", "symbol", "target"]
                        },
                        "description": "List of planned actions to validate"
                    }
                },
                "required": ["actions"]
            }
        }),
    ]
}

/// Execute a tool call and return the result.
pub fn call_tool(conn: &Connection, tool_name: &str, args: &Value) -> Value {
    match tool_name {
        "validate_creation" => {
            let symbol_name = args["symbol_name"].as_str().unwrap_or("");
            let target_module = args["target_module"].as_str().unwrap_or("");
            match queries::validate_creation(conn, symbol_name, target_module) {
                Ok(advisories) => json!({ "advisories": advisories }),
                Err(e) => json!({ "error": e }),
            }
        }
        "validate_dependency" => {
            let type_name = args["type_name"].as_str().unwrap_or("");
            match queries::validate_dependency(conn, type_name) {
                Ok(advisories) => json!({ "advisories": advisories }),
                Err(e) => json!({ "error": e }),
            }
        }
        "validate_boundary" => {
            let symbol_name = args["symbol_name"].as_str().unwrap_or("");
            let target_crate = args["target_crate"].as_str().unwrap_or("");
            match queries::validate_boundary(conn, symbol_name, target_crate) {
                Ok(advisories) => json!({ "advisories": advisories }),
                Err(e) => json!({ "error": e }),
            }
        }
        "check_wip" => {
            let module_path = args["module_path"].as_str().unwrap_or("");
            match queries::check_wip(conn, module_path) {
                Ok(advisories) => json!({ "advisories": advisories }),
                Err(e) => json!({ "error": e }),
            }
        }
        "suggest_reuse" => {
            let capability = args["capability"].as_str().unwrap_or("");
            match queries::suggest_reuse(conn, capability) {
                Ok(advisories) => json!({ "advisories": advisories }),
                Err(e) => json!({ "error": e }),
            }
        }
        "register_plan" => {
            let agent = args["agent"].as_str().unwrap_or("unknown");
            let actions: Vec<rulest_core::advisory::PlannedAction> = args
                .get("actions")
                .and_then(|a| serde_json::from_value(a.clone()).ok())
                .unwrap_or_default();
            match queries::register_plan(conn, &actions, agent) {
                Ok(count) => json!({ "registered": count, "total": actions.len() }),
                Err(e) => json!({ "error": e }),
            }
        }
        "validate_plan" => {
            let actions: Vec<rulest_core::advisory::PlannedAction> = args
                .get("actions")
                .and_then(|a| serde_json::from_value(a.clone()).ok())
                .unwrap_or_default();
            match queries::validate_plan(conn, &actions) {
                Ok(report) => json!(report),
                Err(e) => json!({ "error": e }),
            }
        }
        _ => {
            json!({ "error": format!("Unknown tool: {}", tool_name) })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rulest_core::models::*;
    use rulest_core::registry::*;

    /// Set up an in-memory DB with schema and test data.
    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        let c = Crate {
            id: None,
            name: "mylib".to_string(),
            path: "crates/mylib".to_string(),
            description: None,
            bounded_context: None,
        };
        let crate_id = insert_crate(&conn, &c).unwrap();

        let m = Module {
            id: None,
            crate_id,
            path: "crates/mylib/src/utils.rs".to_string(),
            name: "utils".to_string(),
        };
        let module_id = insert_module(&conn, &m).unwrap();

        let s = Symbol {
            id: None,
            module_id,
            name: "helper_fn".to_string(),
            kind: SymbolKind::Function,
            visibility: Visibility::Public,
            signature: Some("fn helper_fn() -> bool".to_string()),
            line_number: None,
            scope: None,
            status: SymbolStatus::Stable,
            created_by: None,
            created_at: None,
            updated_at: None,
        };
        insert_symbol(&conn, &s).unwrap();

        conn
    }

    #[test]
    fn test_validate_creation_tool() {
        let conn = setup_test_db();
        let result = call_tool(
            &conn,
            "validate_creation",
            &json!({
                "symbol_name": "helper_fn",
                "target_module": "crates/other/src/lib.rs"
            }),
        );

        assert!(result.get("advisories").is_some(), "Response should have 'advisories' key");
        let advisories = result["advisories"].as_array().expect("advisories should be an array");
        assert!(!advisories.is_empty(), "advisories should not be empty for an existing symbol");
    }

    #[test]
    fn test_unknown_tool() {
        let conn = setup_test_db();
        let result = call_tool(&conn, "nonexistent_tool", &json!({}));

        assert!(
            result.get("error").is_some(),
            "Response should have 'error' key for unknown tool"
        );
        let error_msg = result["error"].as_str().unwrap();
        assert!(
            error_msg.contains("Unknown tool"),
            "Error message should mention unknown tool, got: {}",
            error_msg
        );
    }

    #[test]
    fn test_register_plan_tool() {
        let conn = setup_test_db();
        let result = call_tool(
            &conn,
            "register_plan",
            &json!({
                "agent": "test-agent",
                "actions": [
                    {
                        "action": "create",
                        "symbol": "new_widget",
                        "target": "crates/mylib/src/utils.rs",
                        "kind": "function"
                    }
                ]
            }),
        );

        // register_plan should succeed since the module exists
        assert!(
            result.get("error").is_none(),
            "register_plan should not return an error, got: {:?}",
            result
        );
        assert!(
            result.get("registered").is_some(),
            "Response should have 'registered' count"
        );
        let registered = result["registered"].as_u64().unwrap();
        assert_eq!(registered, 1, "Should have registered 1 symbol");
    }
}
