use std::fs;
use std::path::Path;

use rulest_core::models::{OwnershipRule, OwnershipRuleKind};
use rulest_core::registry;

pub fn run(crate_name: &str, description: &str, kind: &str) -> Result<(), String> {
    let rule_kind = OwnershipRuleKind::from_str(kind)
        .ok_or_else(|| format!("Invalid rule kind '{}'. Use: must_own, must_not, shared_with", kind))?;

    let architect_dir = Path::new(".architect");
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
        crate_name,
        description.replace('\'', "''"),
        rule_kind.as_str()
    );

    let mut seed_content = fs::read_to_string(&seed_path).unwrap_or_default();
    seed_content.push_str(&sql);
    fs::write(&seed_path, seed_content)
        .map_err(|e| format!("Failed to update seed.sql: {}", e))?;

    println!("Added rule: {} {} '{}'", rule_kind.as_str(), crate_name, description);

    Ok(())
}
