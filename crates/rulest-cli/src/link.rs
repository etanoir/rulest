use std::path::Path;

use rulest_core::models::{LinkedRegistry, LinkedSymbol};
use rulest_core::registry;

/// Link an external registry.
pub fn run_link(
    name: &str,
    external_db: &str,
    local_db: &str,
) -> Result<(), String> {
    let local = Path::new(local_db);
    if !local.exists() {
        return Err("Local registry not found. Run `rulest init` first.".to_string());
    }

    let external = Path::new(external_db);
    if !external.exists() {
        return Err(format!("External registry not found: {}", external_db));
    }

    let conn = registry::open_registry(local)
        .map_err(|e| format!("Failed to open local registry: {}", e))?;
    registry::create_schema(&conn)
        .map_err(|e| format!("Failed to ensure schema: {}", e))?;

    let now = chrono_now();

    // Register the link
    let link = LinkedRegistry {
        id: None,
        name: name.to_string(),
        path: external_db.to_string(),
        linked_at: now.clone(),
    };
    registry::insert_linked_registry(&conn, &link)
        .map_err(|e| format!("Failed to register link: {}", e))?;

    // Import symbols from external registry
    import_symbols(local, external, name, &now)?;

    println!("Linked registry '{}' from {}", name, external_db);

    Ok(())
}

/// Refresh all linked registries.
pub fn run_refresh(local_db: &str) -> Result<(), String> {
    let local = Path::new(local_db);
    if !local.exists() {
        return Err("Registry not found. Run `rulest init` first.".to_string());
    }

    let conn = registry::open_registry(local)
        .map_err(|e| format!("Failed to open registry: {}", e))?;
    registry::create_schema(&conn)
        .map_err(|e| format!("Failed to ensure schema: {}", e))?;

    let links = registry::list_linked_registries(&conn)
        .map_err(|e| format!("Failed to list linked registries: {}", e))?;

    if links.is_empty() {
        println!("No linked registries.");
        return Ok(());
    }

    let now = chrono_now();

    for link in &links {
        let ext_path = Path::new(&link.path);
        if !ext_path.exists() {
            eprintln!("WARNING: Linked registry '{}' not found at {}", link.name, link.path);
            continue;
        }

        registry::clear_linked_symbols(&conn, &link.name)
            .map_err(|e| format!("Failed to clear symbols for '{}': {}", link.name, e))?;

        import_symbols(local, ext_path, &link.name, &now)?;
        println!("Refreshed: {}", link.name);
    }

    Ok(())
}

/// List all linked registries.
pub fn run_list(local_db: &str) -> Result<(), String> {
    let local = Path::new(local_db);
    if !local.exists() {
        return Err("Registry not found. Run `rulest init` first.".to_string());
    }

    let conn = registry::open_registry(local)
        .map_err(|e| format!("Failed to open registry: {}", e))?;
    registry::create_schema(&conn)
        .map_err(|e| format!("Failed to ensure schema: {}", e))?;

    let links = registry::list_linked_registries(&conn)
        .map_err(|e| format!("Failed to list linked registries: {}", e))?;

    if links.is_empty() {
        println!("No linked registries.");
    } else {
        for link in &links {
            println!("  {} -> {} (linked: {})", link.name, link.path, link.linked_at);
        }
    }

    Ok(())
}

/// Remove a linked registry.
pub fn run_remove(name: &str, local_db: &str) -> Result<(), String> {
    let local = Path::new(local_db);
    if !local.exists() {
        return Err("Registry not found.".to_string());
    }

    let conn = registry::open_registry(local)
        .map_err(|e| format!("Failed to open registry: {}", e))?;
    registry::create_schema(&conn)
        .map_err(|e| format!("Failed to ensure schema: {}", e))?;

    registry::remove_linked_registry(&conn, name)
        .map_err(|e| format!("Failed to remove link: {}", e))?;

    println!("Removed linked registry '{}'", name);
    Ok(())
}

fn import_symbols(
    local_db: &Path,
    ext_db: &Path,
    source_name: &str,
    now: &str,
) -> Result<(), String> {
    let ext_conn = registry::open_registry(ext_db)
        .map_err(|e| format!("Failed to open external registry: {}", e))?;

    let local_conn = registry::open_registry(local_db)
        .map_err(|e| format!("Failed to open local registry: {}", e))?;

    let symbols = registry::query_public_symbols(&ext_conn)
        .map_err(|e| format!("Failed to query external symbols: {}", e))?;

    let mut count = 0;
    for (name, kind, module_path, crate_name, signature) in &symbols {
        let sym = LinkedSymbol {
            id: None,
            source_name: source_name.to_string(),
            name: name.clone(),
            kind: Some(kind.clone()),
            crate_name: Some(crate_name.clone()),
            module_path: Some(module_path.clone()),
            signature: signature.clone(),
            linked_at: now.to_string(),
        };
        registry::insert_linked_symbol(&local_conn, &sym)
            .map_err(|e| format!("Failed to import symbol: {}", e))?;
        count += 1;
    }

    println!("  Imported {} public symbols from '{}'", count, source_name);
    Ok(())
}

fn chrono_now() -> String {
    use std::time::SystemTime;
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}
