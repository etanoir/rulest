use std::path::Path;
use std::process::Command;

use rulest_core::registry;
use rulest_indexer::sync::sync_workspace;

pub fn run(workspace_path: &str, cargo_args: &[String]) -> Result<(), String> {
    let workspace = Path::new(workspace_path);
    let workspace_dir = if workspace.is_file() {
        workspace
            .parent()
            .ok_or("Cannot determine workspace directory")?
    } else {
        workspace
    };

    // Step 1: Run cargo build
    println!("Running cargo build...");
    let mut cmd = Command::new("cargo");
    cmd.arg("build");

    if workspace.is_file() {
        cmd.arg("--manifest-path").arg(workspace_path);
    }

    for arg in cargo_args {
        cmd.arg(arg);
    }

    let status = cmd
        .status()
        .map_err(|e| format!("Failed to run cargo build: {}", e))?;

    if !status.success() {
        return Err(format!(
            "cargo build failed with exit code: {}",
            status.code().unwrap_or(-1)
        ));
    }

    // Step 2: Auto-sync registry
    let architect_dir = workspace_dir.join(".architect");
    let db_path = architect_dir.join("registry.db");

    if !db_path.exists() {
        println!("Registry not found, skipping sync. Run `rulest init` first.");
        return Ok(());
    }

    println!("Syncing registry...");
    let conn =
        registry::open_registry(&db_path).map_err(|e| format!("Failed to open registry: {}", e))?;

    let stats = sync_workspace(&conn, workspace, &architect_dir, false)?;

    println!("Post-build sync complete:");
    println!("  Modules scanned:  {}", stats.modules_scanned);
    println!("  Modules skipped:  {}", stats.modules_skipped);
    println!("  Symbols added:    {}", stats.symbols_added);
    println!("  Symbols removed:  {}", stats.symbols_removed);

    Ok(())
}
