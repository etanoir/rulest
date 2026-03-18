use std::fs;
use std::path::Path;

use rulest_core::{queries, registry};

use crate::validate;

pub fn run(plan_file: &str, db_path: &str, agent: &str) -> Result<(), String> {
    let path = Path::new(db_path);
    if !path.exists() {
        return Err(format!(
            "Registry not found at {}. Run `rulest init` first.",
            db_path
        ));
    }

    let conn = registry::open_registry(path)
        .map_err(|e| format!("Failed to open registry: {}", e))?;

    let content = fs::read_to_string(plan_file)
        .map_err(|e| format!("Failed to read plan file '{}': {}", plan_file, e))?;

    let actions = validate::parse_plan_public(&content)?;

    if actions.is_empty() {
        return Err(
            "No actions found in plan. Expected lines like: CREATE: symbol_name in: path/to/file.rs"
                .to_string(),
        );
    }

    let count = queries::register_plan(&conn, &actions, agent)?;

    eprintln!(
        "Registered {} planned symbols from {} actions (agent: {})",
        count,
        actions.len(),
        agent
    );

    Ok(())
}
