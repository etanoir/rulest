use std::fs;
use std::path::Path;

use rulest_core::models::{OwnershipRule, OwnershipRuleKind};
use rulest_core::registry;

pub fn run(crate_name: &str, description: &str, kind: &str, workspace_path: &str) -> Result<(), String> {
    let rule_kind: OwnershipRuleKind = kind.parse()?;

    let workspace = Path::new(workspace_path);
    let workspace_dir = if workspace.is_file() {
        workspace
            .parent()
            .ok_or("Cannot determine workspace directory")?
    } else {
        workspace
    };

    let architect_dir = workspace_dir.join(".architect");
    let db_path = architect_dir.join("registry.db");
    let seed_path = architect_dir.join("seed.sql");

    if !db_path.exists() {
        return Err("Registry not found. Run `rulest init` first.".to_string());
    }

    let conn = registry::open_registry(&db_path)
        .map_err(|e| format!("Failed to open registry: {}", e))?;

    let rule = OwnershipRule {
        id: None,
        crate_name: crate_name.to_string(),
        description: description.to_string(),
        kind: rule_kind,
    };

    registry::insert_ownership_rule(&conn, &rule)
        .map_err(|e| format!("Failed to insert rule: {}", e))?;

    // Append to seed.sql
    let sql = format!(
        "INSERT INTO ownership_rules (crate_name, description, kind) VALUES ('{}', '{}', '{}');\n",
        crate_name.replace('\'', "''"),
        description.replace('\'', "''"),
        rule_kind.as_str()
    );

    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&seed_path)
        .map_err(|e| format!("Failed to open seed.sql: {}", e))?;
    file.write_all(sql.as_bytes())
        .map_err(|e| format!("Failed to append to seed.sql: {}", e))?;

    println!("Added rule: {} {} '{}'", rule_kind.as_str(), crate_name, description);

    Ok(())
}
