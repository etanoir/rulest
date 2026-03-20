use std::path::Path;

use rulest_core::registry;
use rulest_indexer::sync::sync_workspace;

pub fn run(workspace_path: &str, force_full: bool) -> Result<(), String> {
    let workspace = Path::new(workspace_path);
    let workspace_dir = if workspace.is_file() {
        if workspace.file_name().and_then(|n| n.to_str()) != Some("Cargo.toml") {
            return Err(format!(
                "Expected Cargo.toml but got '{}'",
                workspace.display()
            ));
        }
        workspace
            .parent()
            .ok_or("Cannot determine workspace directory")?
    } else {
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

    if !db_path.exists() {
        return Err("Registry not found. Run `rulest init` first.".to_string());
    }

    let conn = registry::open_registry(&db_path)
        .map_err(|e| format!("Failed to open registry: {}", e))?;

    println!("Syncing workspace...");
    let stats = sync_workspace(&conn, workspace, &architect_dir, force_full)?;

    println!("Sync complete:");
    println!("  Crates found:     {}", stats.crates_found);
    println!("  Modules scanned:  {}", stats.modules_scanned);
    println!("  Modules skipped:  {}", stats.modules_skipped);
    println!("  Symbols added:    {}", stats.symbols_added);
    println!("  Symbols removed:  {}", stats.symbols_removed);
    if !stats.parse_errors.is_empty() {
        println!("  Parse errors:     {}", stats.parse_errors.len());
        for (path, err) in &stats.parse_errors {
            println!("    - {}: {}", path, err);
        }
    }

    Ok(())
}
