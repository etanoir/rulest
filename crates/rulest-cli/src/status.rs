use std::path::Path;

use rulest_core::{queries, registry};

pub fn run(db_path: &str) -> Result<(), String> {
    let path = Path::new(db_path);
    if !path.exists() {
        return Err(format!(
            "Registry not found at {}. Run `rulest init` first.",
            db_path
        ));
    }

    let conn = registry::open_registry(path)
        .map_err(|e| format!("Failed to open registry: {}", e))?;

    let stats = queries::get_registry_stats(&conn);

    println!("Registry: {}", db_path);
    println!();
    println!("  Crates:    {}", stats.crate_count);
    println!("  Modules:   {}", stats.module_count);
    println!("  Symbols:   {}", stats.symbol_count);
    println!("  Rules:     {}", stats.rule_count);

    if !stats.symbols_by_kind.is_empty() {
        println!();
        println!("  Symbols by kind:");
        for (kind, count) in &stats.symbols_by_kind {
            println!("    {:<12} {}", kind, count);
        }
    }

    if !stats.symbols_by_visibility.is_empty() {
        println!();
        println!("  Symbols by visibility:");
        for (vis, count) in &stats.symbols_by_visibility {
            println!("    {:<12} {}", vis, count);
        }
    }

    if !stats.symbols_by_status.is_empty() {
        println!();
        println!("  Symbols by status:");
        for (status, count) in &stats.symbols_by_status {
            println!("    {:<12} {}", status, count);
        }
    }

    Ok(())
}
