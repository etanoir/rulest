use std::fs;
use std::path::Path;

use rulest_core::registry;
use rulest_indexer::cargo_meta;

pub fn run(workspace_path: &str) -> Result<(), String> {
    let workspace = Path::new(workspace_path);
    let workspace_dir = if workspace.is_file() {
        // Validate file is actually Cargo.toml
        if workspace.file_name().and_then(|n| n.to_str()) != Some("Cargo.toml") {
            return Err(format!(
                "Expected Cargo.toml, got '{}'",
                workspace.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
        workspace
            .parent()
            .ok_or("Cannot determine workspace directory")?
    } else {
        // Validate directory contains Cargo.toml
        if !workspace.join("Cargo.toml").exists() {
            return Err(format!(
                "No Cargo.toml found in '{}'",
                workspace.display()
            ));
        }
        workspace
    };

    let architect_dir = workspace_dir.join(".architect");
    let db_path = architect_dir.join("registry.db");
    let seed_path = architect_dir.join("seed.sql");

    // Create .architect/ directory
    fs::create_dir_all(&architect_dir)
        .map_err(|e| format!("Failed to create .architect/: {}", e))?;

    // Create registry database with schema (handles migration if needed)
    let conn = registry::open_registry(&db_path)
        .map_err(|e| format!("Failed to create registry: {}", e))?;
    registry::create_schema(&conn)
        .map_err(|e| format!("Failed to create schema: {}", e))?;

    // Check if registry already has data — if so, skip re-initialization
    let crate_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM crates", [], |row| row.get(0))
        .map_err(|e| format!("Failed to query crates: {}", e))?;

    if crate_count > 0 {
        println!("Registry already initialized at {}", architect_dir.display());
        return Ok(());
    }

    // Extract workspace info and populate crates
    let info = cargo_meta::extract_workspace(workspace)?;
    for c in &info.crates {
        registry::insert_crate(&conn, c)
            .map_err(|e| format!("Failed to insert crate '{}': {}", c.name, e))?;
    }

    // If seed.sql already exists (e.g. cloned repo), execute it against the fresh database.
    // Otherwise, generate a starter template.
    if seed_path.exists() {
        let sql = fs::read_to_string(&seed_path)
            .map_err(|e| format!("Failed to read seed.sql: {}", e))?;
        registry::execute_seed_sql(&conn, &sql)
            .map_err(|e| format!("Failed to execute seed.sql: {}", e))?;
        println!("Loaded existing seed.sql into registry");
    } else {
        let mut seed = String::from("-- Architecture ownership rules for this workspace\n");
        seed.push_str("-- Add rules with: rulest add-rule <crate> <description> --kind <must_own|must_not|shared_with>\n\n");

        for c in &info.crates {
            seed.push_str(&format!(
                "-- INSERT INTO ownership_rules (crate_name, description, kind) VALUES ('{}', 'TODO: describe ownership', 'must_own');\n",
                c.name
            ));
        }

        fs::write(&seed_path, seed)
            .map_err(|e| format!("Failed to write seed.sql: {}", e))?;
    }

    println!("Initialized architecture registry at {}", architect_dir.display());
    println!("  Database: {}", db_path.display());
    println!("  Seed file: {}", seed_path.display());
    println!("  Crates found: {}", info.crates.len());

    for c in &info.crates {
        println!("    - {}", c.name);
    }

    Ok(())
}
