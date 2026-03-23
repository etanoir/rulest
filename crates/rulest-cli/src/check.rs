use std::path::Path;
use std::process::Command;

use rulest_core::{advisory::Advisory, queries, registry};
use rulest_indexer::extractor::extract_symbols_any;

/// Run check-file: parse a single file, detect new symbols, validate each.
pub fn run_check_file(file_path: &str, db_path: &str) -> Result<(), String> {
    let db = Path::new(db_path);
    if !db.exists() {
        return Err("Registry not found. Run `rulest init` first.".to_string());
    }

    let conn = registry::open_registry(db)
        .map_err(|e| format!("Failed to open registry: {}", e))?;

    let file = Path::new(file_path);
    if !file.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    // Extract crate name from file path (crates/<name>/...)
    let crate_name = extract_crate_name(file_path);

    // Extract symbols from the file
    let extracted = extract_symbols_any(file)
        .map_err(|e| format!("Failed to parse {}: {}", file_path, e))?;

    let mut has_warnings = false;
    let mut has_errors = false;
    let mut safe_count = 0;

    for sym in &extracted.symbols {
        // Check creation
        let creation_advisories = queries::validate_creation(&conn, &sym.name, file_path)
            .unwrap_or_default();

        // Check boundary if crate name is available
        let boundary_advisories = if let Some(ref cn) = crate_name {
            queries::validate_boundary(&conn, &sym.name, cn).unwrap_or_default()
        } else {
            Vec::new()
        };

        for advisory in creation_advisories.iter().chain(boundary_advisories.iter()) {
            match advisory {
                Advisory::SafeToCreate { .. } => {
                    safe_count += 1;
                }
                Advisory::ReuseExisting { existing, .. } => {
                    println!(
                        "WARNING: {} already exists in {}::{} (reuse_existing)",
                        sym.name, existing.crate_name, existing.module_path
                    );
                    has_warnings = true;
                }
                Advisory::BoundaryViolation { rule, crate_name, .. } => {
                    println!(
                        "ERROR: {} in {} violates boundary rule (must_not: {})",
                        sym.name, crate_name, rule
                    );
                    has_errors = true;
                }
                Advisory::AmbiguousMatch { candidates } => {
                    println!(
                        "WARNING: {} has {} ambiguous matches",
                        sym.name,
                        candidates.len()
                    );
                    has_warnings = true;
                }
                _ => {}
            }
        }
    }

    if !has_warnings && !has_errors {
        println!(
            "PASS: {} ({} symbols, all safe)",
            file_path, safe_count
        );
    }

    if has_errors {
        std::process::exit(2);
    } else if has_warnings {
        std::process::exit(1);
    }

    Ok(())
}

/// Run check --changed-only: validate only staged .rs files.
pub fn run_check_changed(db_path: &str) -> Result<(), String> {
    let db = Path::new(db_path);
    if !db.exists() {
        return Err("Registry not found. Run `rulest init` first.".to_string());
    }

    let conn = registry::open_registry(db)
        .map_err(|e| format!("Failed to open registry: {}", e))?;

    // Get staged .rs files from git
    let output = Command::new("git")
        .args(["diff", "--cached", "--name-only", "--diff-filter=ACM"])
        .output()
        .map_err(|e| format!("Failed to run git diff: {}", e))?;

    let files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| l.ends_with(".rs") || l.ends_with(".ts") || l.ends_with(".tsx"))
        .map(|l| l.to_string())
        .collect();

    if files.is_empty() {
        println!("No staged .rs files to check.");
        return Ok(());
    }

    let mut total_errors = 0;
    let mut total_warnings = 0;

    for file_path in &files {
        let file = Path::new(file_path);
        if !file.exists() {
            continue;
        }

        let crate_name = extract_crate_name(file_path);

        let extracted = match extract_symbols_any(file) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("SKIP: {} (parse error: {})", file_path, e);
                continue;
            }
        };

        let mut file_warnings = 0;
        let mut file_errors = 0;
        let mut file_safe = 0;

        for sym in &extracted.symbols {
            let creation = queries::validate_creation(&conn, &sym.name, file_path)
                .unwrap_or_default();
            let boundary = if let Some(ref cn) = crate_name {
                queries::validate_boundary(&conn, &sym.name, cn).unwrap_or_default()
            } else {
                Vec::new()
            };

            for advisory in creation.iter().chain(boundary.iter()) {
                match advisory {
                    Advisory::SafeToCreate { .. } => file_safe += 1,
                    Advisory::ReuseExisting { existing, .. } => {
                        println!(
                            "  - {}: reuse_existing (already in {}::{})",
                            sym.name, existing.crate_name, existing.module_path
                        );
                        file_warnings += 1;
                    }
                    Advisory::BoundaryViolation { rule, .. } => {
                        println!("  - {}: boundary_violation ({})", sym.name, rule);
                        file_errors += 1;
                    }
                    Advisory::AmbiguousMatch { .. } => {
                        println!("  - {}: ambiguous_match", sym.name);
                        file_warnings += 1;
                    }
                    _ => {}
                }
            }
        }

        if file_errors > 0 {
            println!("FAIL: {}", file_path);
            total_errors += file_errors;
        } else if file_warnings > 0 {
            println!("WARN: {}", file_path);
            total_warnings += file_warnings;
        } else {
            println!(
                "PASS: {} ({} new symbols, all safe)",
                file_path, file_safe
            );
        }

        total_errors += file_errors;
        total_warnings += file_warnings;
    }

    println!(
        "\nChecked {} files: {} errors, {} warnings",
        files.len(),
        total_errors,
        total_warnings
    );

    if total_errors > 0 {
        std::process::exit(2);
    } else if total_warnings > 0 {
        std::process::exit(1);
    }

    Ok(())
}

fn extract_crate_name(path: &str) -> Option<String> {
    if let Some(rest) = path.strip_prefix("crates/") {
        if let Some(idx) = rest.find('/') {
            return Some(rest[..idx].to_string());
        }
    }
    None
}
