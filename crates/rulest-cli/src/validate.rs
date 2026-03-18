use std::fs;
use std::path::Path;

use rulest_core::advisory::PlannedAction;
use rulest_core::{queries, registry};

pub fn run(plan_file: &str, db_path: &str) -> Result<(), String> {
    let path = Path::new(db_path);
    if !path.exists() {
        return Err(format!(
            "Registry not found at {}. Run `rulest init` first.",
            db_path
        ));
    }

    let conn = registry::open_registry(path)
        .map_err(|e| format!("Failed to open registry: {}", e))?;

    let content = fs::read_to_string(plan_file)
        .map_err(|e| format!("Failed to read plan file '{}': {}", plan_file, e))?;

    let actions = parse_plan(&content)?;

    if actions.is_empty() {
        return Err("No actions found in plan. Expected lines like: CREATE: symbol_name in: path/to/file.rs".to_string());
    }

    let report = queries::validate_plan(&conn, &actions);

    let json = serde_json::to_string_pretty(&report)
        .map_err(|e| format!("Failed to serialize: {}", e))?;
    println!("{}", json);

    // Print human-readable summary to stderr
    let s = &report.summary;
    eprintln!();
    eprintln!(
        "Plan: {} actions | {} safe | {} reuse | {} violations | {} conflicts | {} ambiguous",
        s.total_actions, s.safe, s.reuse, s.violations, s.conflicts, s.ambiguous
    );

    Ok(())
}

/// Parse a structured plan from text.
///
/// Accepts lines like:
///   CREATE: fn calculate_settlement_fee in: crates/trading/src/fees.rs
///   MODIFY: fn execute_settlement in: crates/trading/src/settlement.rs
///   CREATE: struct CurrencyAmount in: crates/trading/src/types.rs
/// Public re-export for use by register command.
pub fn parse_plan_public(content: &str) -> Result<Vec<PlannedAction>, String> {
    parse_plan(content)
}

fn parse_plan(content: &str) -> Result<Vec<PlannedAction>, String> {
    let mut actions = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }

        // Try to parse "ACTION: [kind] symbol in: target"
        let upper = trimmed.to_uppercase();
        let (action_type, rest) = if upper.starts_with("CREATE:") {
            ("create", &trimmed[7..])
        } else if upper.starts_with("MODIFY:") {
            ("modify", &trimmed[7..])
        } else {
            continue;
        };

        let rest = rest.trim();

        // Strip optional kind prefix (fn, struct, enum, trait, type, const)
        let rest = rest
            .strip_prefix("fn ")
            .or_else(|| rest.strip_prefix("struct "))
            .or_else(|| rest.strip_prefix("enum "))
            .or_else(|| rest.strip_prefix("trait "))
            .or_else(|| rest.strip_prefix("type "))
            .or_else(|| rest.strip_prefix("const "))
            .unwrap_or(rest)
            .trim();

        // Split on " in: " or " in "
        let (symbol, target) = if let Some(idx) = rest.find(" in: ") {
            (rest[..idx].trim(), rest[idx + 5..].trim())
        } else if let Some(idx) = rest.find(" in ") {
            (rest[..idx].trim(), rest[idx + 4..].trim())
        } else {
            (rest, "")
        };

        if symbol.is_empty() {
            continue;
        }

        // Derive crate name from target path (e.g. "crates/trading/src/fees.rs" -> "trading")
        let crate_name = extract_crate_name(target);

        actions.push(PlannedAction {
            action: action_type.to_string(),
            symbol: symbol.to_string(),
            target: target.to_string(),
            crate_name,
        });
    }

    Ok(actions)
}

fn extract_crate_name(target: &str) -> Option<String> {
    // Try "crates/<name>/..." pattern
    if let Some(rest) = target.strip_prefix("crates/") {
        if let Some(idx) = rest.find('/') {
            return Some(rest[..idx].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_plan() {
        let plan = r#"
# Settlement feature plan

CREATE: fn calculate_settlement_fee in: crates/trading/src/fees.rs
CREATE: struct CurrencyAmount in: crates/domain/src/types.rs
MODIFY: fn execute_settlement in: crates/trading/src/settlement.rs
"#;
        let actions = parse_plan(plan).unwrap();
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0].action, "create");
        assert_eq!(actions[0].symbol, "calculate_settlement_fee");
        assert_eq!(actions[0].target, "crates/trading/src/fees.rs");
        assert_eq!(actions[0].crate_name, Some("trading".to_string()));
        assert_eq!(actions[1].symbol, "CurrencyAmount");
        assert_eq!(actions[1].crate_name, Some("domain".to_string()));
        assert_eq!(actions[2].action, "modify");
    }

    #[test]
    fn test_parse_plan_skips_comments() {
        let plan = "# comment\n// another\n\nCREATE: foo in: bar.rs\n";
        let actions = parse_plan(plan).unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].symbol, "foo");
    }

    #[test]
    fn test_extract_crate_name() {
        assert_eq!(
            extract_crate_name("crates/trading/src/fees.rs"),
            Some("trading".to_string())
        );
        assert_eq!(extract_crate_name("src/main.rs"), None);
    }
}
