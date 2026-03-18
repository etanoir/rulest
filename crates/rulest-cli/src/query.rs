use std::path::Path;

use rulest_core::{queries, registry};

pub fn run(
    symbol: Option<&str>,
    validate_creation: Option<&str>,
    target: Option<&str>,
    validate_dependency: Option<&str>,
    validate_boundary: Option<&str>,
    crate_name: Option<&str>,
    check_wip: Option<&str>,
    suggest_reuse: Option<&str>,
) -> Result<(), String> {
    let db_path = Path::new(".architect/registry.db");
    if !db_path.exists() {
        return Err("Registry not found. Run `rulest init` first.".to_string());
    }

    let conn = registry::open_registry(db_path)
        .map_err(|e| format!("Failed to open registry: {}", e))?;

    // Simple symbol lookup
    if let Some(name) = symbol {
        let advisories = queries::validate_creation(&conn, name, "");
        let json = serde_json::to_string_pretty(&advisories)
            .map_err(|e| format!("Failed to serialize: {}", e))?;
        println!("{}", json);
        return Ok(());
    }

    // Validate creation
    if let Some(name) = validate_creation {
        let target_module = target.unwrap_or("");
        let advisories = queries::validate_creation(&conn, name, target_module);
        let json = serde_json::to_string_pretty(&advisories)
            .map_err(|e| format!("Failed to serialize: {}", e))?;
        println!("{}", json);
        return Ok(());
    }

    // Validate dependency
    if let Some(name) = validate_dependency {
        let advisories = queries::validate_dependency(&conn, name);
        let json = serde_json::to_string_pretty(&advisories)
            .map_err(|e| format!("Failed to serialize: {}", e))?;
        println!("{}", json);
        return Ok(());
    }

    // Validate boundary
    if let Some(name) = validate_boundary {
        let target_crate = crate_name.unwrap_or("");
        let advisories = queries::validate_boundary(&conn, name, target_crate);
        let json = serde_json::to_string_pretty(&advisories)
            .map_err(|e| format!("Failed to serialize: {}", e))?;
        println!("{}", json);
        return Ok(());
    }

    // Check WIP
    if let Some(path) = check_wip {
        let advisories = queries::check_wip(&conn, path);
        let json = serde_json::to_string_pretty(&advisories)
            .map_err(|e| format!("Failed to serialize: {}", e))?;
        println!("{}", json);
        return Ok(());
    }

    // Suggest reuse
    if let Some(desc) = suggest_reuse {
        let advisories = queries::suggest_reuse(&conn, desc);
        let json = serde_json::to_string_pretty(&advisories)
            .map_err(|e| format!("Failed to serialize: {}", e))?;
        println!("{}", json);
        return Ok(());
    }

    Err("No query specified. Use --help for options.".to_string())
}
