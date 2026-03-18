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
                                "crate_name": { "type": "string", "description": "Target crate name (optional)" }
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
    ]
}

/// Execute a tool call and return the result.
pub fn call_tool(conn: &Connection, tool_name: &str, args: &Value) -> Value {
    match tool_name {
        "validate_creation" => {
            let symbol_name = args["symbol_name"].as_str().unwrap_or("");
            let target_module = args["target_module"].as_str().unwrap_or("");
            let advisories = queries::validate_creation(conn, symbol_name, target_module);
            json!({ "advisories": advisories })
        }
        "validate_dependency" => {
            let type_name = args["type_name"].as_str().unwrap_or("");
            let advisories = queries::validate_dependency(conn, type_name);
            json!({ "advisories": advisories })
        }
        "validate_boundary" => {
            let symbol_name = args["symbol_name"].as_str().unwrap_or("");
            let target_crate = args["target_crate"].as_str().unwrap_or("");
            let advisories = queries::validate_boundary(conn, symbol_name, target_crate);
            json!({ "advisories": advisories })
        }
        "check_wip" => {
            let module_path = args["module_path"].as_str().unwrap_or("");
            let advisories = queries::check_wip(conn, module_path);
            json!({ "advisories": advisories })
        }
        "suggest_reuse" => {
            let capability = args["capability"].as_str().unwrap_or("");
            let advisories = queries::suggest_reuse(conn, capability);
            json!({ "advisories": advisories })
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
        _ => {
            json!({ "error": format!("Unknown tool: {}", tool_name) })
        }
    }
}
