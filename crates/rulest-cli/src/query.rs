use std::path::Path;

use rulest_core::{queries, registry};

pub struct QueryArgs {
    pub symbol: Option<String>,
    pub db: String,
    pub validate_creation: Option<String>,
    pub target: Option<String>,
    pub validate_dependency: Option<String>,
    pub validate_boundary: Option<String>,
    pub crate_name: Option<String>,
    pub check_wip: Option<String>,
    pub suggest_reuse: Option<String>,
}

pub fn run(args: QueryArgs) -> Result<(), String> {
    let db_path = Path::new(&args.db);
    if !db_path.exists() {
        return Err(format!(
            "Registry not found at {}. Run `rulest init` first.",
            args.db
        ));
    }

    let conn = registry::open_registry(db_path)
        .map_err(|e| format!("Failed to open registry: {}", e))?;

    // Simple symbol lookup
    if let Some(ref name) = args.symbol {
        let advisories = queries::validate_creation(&conn, name, "");
        print_json(&advisories)?;
        return Ok(());
    }

    // Validate creation
    if let Some(ref name) = args.validate_creation {
        let target_module = args.target.as_deref().unwrap_or("");
        let advisories = queries::validate_creation(&conn, name, target_module);
        print_json(&advisories)?;
        return Ok(());
    }

    // Validate dependency
    if let Some(ref name) = args.validate_dependency {
        let advisories = queries::validate_dependency(&conn, name);
        print_json(&advisories)?;
        return Ok(());
    }

    // Validate boundary
    if let Some(ref name) = args.validate_boundary {
        let target_crate = args.crate_name.as_deref().unwrap_or("");
        let advisories = queries::validate_boundary(&conn, name, target_crate);
        print_json(&advisories)?;
        return Ok(());
    }

    // Check WIP
    if let Some(ref path) = args.check_wip {
        let advisories = queries::check_wip(&conn, path);
        print_json(&advisories)?;
        return Ok(());
    }

    // Suggest reuse
    if let Some(ref desc) = args.suggest_reuse {
        let advisories = queries::suggest_reuse(&conn, desc);
        print_json(&advisories)?;
        return Ok(());
    }

    Err("No query specified. Use --help for options.".to_string())
}

fn print_json(value: &impl serde::Serialize) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(value).map_err(|e| format!("Failed to serialize: {}", e))?;
    println!("{}", json);
    Ok(())
}
