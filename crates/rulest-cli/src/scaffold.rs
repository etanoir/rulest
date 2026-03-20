use std::fs;
use std::path::Path;

use rulest_indexer::cargo_meta;

const ROOT_CLAUDE_MD: &str = include_str!("../../../templates/CLAUDE.root.md");
const MODULE_CLAUDE_MD: &str = include_str!("../../../templates/CLAUDE.module.md");
const SETTINGS_JSON: &str = include_str!("../../../templates/settings.json");
const SEED_SQL: &str = include_str!("../../../templates/seed.sql");

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

    let info = cargo_meta::extract_workspace(workspace)?;

    let workspace_name = workspace_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());

    // Generate root CLAUDE.md
    let crate_rows: Vec<String> = info
        .crates
        .iter()
        .map(|c| {
            format!(
                "| {} | `{}` | {} |",
                c.name,
                c.name,
                c.description.as_deref().unwrap_or("—")
            )
        })
        .collect();

    let root_md = ROOT_CLAUDE_MD
        .replace("{{workspace_name}}", &workspace_name)
        .replace("{{crate_list}}", &crate_rows.join("\n"));

    let claude_md_path = workspace_dir.join("CLAUDE.md");
    if !claude_md_path.exists() {
        fs::write(&claude_md_path, &root_md)
            .map_err(|e| format!("Failed to write CLAUDE.md: {}", e))?;
        println!("Created {}", claude_md_path.display());
    } else {
        println!("Skipped {} (already exists)", claude_md_path.display());
    }

    // Generate per-crate CLAUDE.md files
    for c in &info.crates {
        let crate_dir = workspace_dir.join(&c.path);
        let crate_md_path = crate_dir.join("CLAUDE.md");

        if !crate_md_path.exists() && crate_dir.exists() {
            let content = MODULE_CLAUDE_MD
                .replace("{{crate_name}}", &c.name)
                .replace(
                    "{{description}}",
                    c.description.as_deref().unwrap_or("TODO: Add description"),
                )
                .replace("{{dependencies}}", "TODO: list dependencies");

            fs::write(&crate_md_path, &content)
                .map_err(|e| format!("Failed to write {}: {}", crate_md_path.display(), e))?;
            println!("Created {}", crate_md_path.display());
        }
    }

    // Generate .claude/settings.json
    let claude_dir = workspace_dir.join(".claude");
    fs::create_dir_all(&claude_dir)
        .map_err(|e| format!("Failed to create .claude/: {}", e))?;

    let settings_path = claude_dir.join("settings.json");
    if !settings_path.exists() {
        // Generate one deny rule per crate
        let deny_rules: Vec<String> = info
            .crates
            .iter()
            .map(|c| format!("      \"Edit(crates/{}/src/**)\",", c.name))
            .collect();

        let settings_content =
            SETTINGS_JSON.replace("{{crate_list}}", &deny_rules.join("\n"));

        fs::write(&settings_path, &settings_content)
            .map_err(|e| format!("Failed to write settings.json: {}", e))?;
        println!("Created {}", settings_path.display());
    }

    // Generate seed.sql
    let architect_dir = workspace_dir.join(".architect");
    fs::create_dir_all(&architect_dir)
        .map_err(|e| format!("Failed to create .architect/: {}", e))?;

    let seed_path = architect_dir.join("seed.sql");
    if !seed_path.exists() {
        let crate_inserts: Vec<String> = info
            .crates
            .iter()
            .map(|c| {
                format!(
                    "INSERT INTO ownership_rules (crate_name, description, kind) VALUES ('{}', '{} owns its domain logic', 'must_own');",
                    c.name.replace('\'', "''"), c.name.replace('\'', "''")
                )
            })
            .collect();

        let seed = SEED_SQL.replace("{{crate_list}}", &crate_inserts.join("\n"));

        fs::write(&seed_path, &seed)
            .map_err(|e| format!("Failed to write seed.sql: {}", e))?;
        println!("Created {}", seed_path.display());
    }

    println!("\nScaffold complete! Review the generated files and customize as needed.");

    Ok(())
}
