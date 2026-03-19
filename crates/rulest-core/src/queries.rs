use rusqlite::{params, Connection};

use crate::advisory::*;

/// Validate whether a symbol can be created in the target module.
///
/// Checks for exact and fuzzy matches across the registry.
/// Returns `SafeToCreate` if no conflicts, `ReuseExisting` or `AmbiguousMatch` otherwise.
pub fn validate_creation(
    conn: &Connection,
    symbol_name: &str,
    target_module: &str,
) -> Vec<Advisory> {
    let mut advisories = Vec::new();

    // Exact match
    let exact_matches = find_symbols_by_name(conn, symbol_name);
    if exact_matches.len() == 1 {
        let existing = &exact_matches[0];
        advisories.push(Advisory::ReuseExisting {
            existing: existing.clone(),
            suggestion: format!(
                "Symbol '{}' already exists in {}::{}. Consider reusing it.",
                symbol_name, existing.crate_name, existing.module_path
            ),
        });
        return advisories;
    } else if exact_matches.len() > 1 {
        advisories.push(Advisory::AmbiguousMatch {
            candidates: exact_matches,
        });
        return advisories;
    }

    // Fuzzy match (LIKE with % wildcards)
    let fuzzy_matches = find_symbols_fuzzy(conn, symbol_name);
    if fuzzy_matches.len() == 1 {
        let existing = &fuzzy_matches[0];
        advisories.push(Advisory::ReuseExisting {
            existing: existing.clone(),
            suggestion: format!(
                "Similar symbol '{}' found in {}::{}. Did you mean to reuse it?",
                existing.name, existing.crate_name, existing.module_path
            ),
        });
        return advisories;
    } else if fuzzy_matches.len() > 1 {
        advisories.push(Advisory::AmbiguousMatch {
            candidates: fuzzy_matches,
        });
        return advisories;
    }

    advisories.push(Advisory::SafeToCreate {
        symbol: symbol_name.to_string(),
        target: target_module.to_string(),
    });
    advisories
}

/// Validate whether a type/dependency exists in the registry.
///
/// Looks up struct/trait/enum across all crates.
pub fn validate_dependency(conn: &Connection, type_name: &str) -> Vec<Advisory> {
    let mut advisories = Vec::new();

    let matches = find_type_symbols(conn, type_name);
    if matches.is_empty() {
        advisories.push(Advisory::SafeToCreate {
            symbol: type_name.to_string(),
            target: "unknown".to_string(),
        });
    } else if matches.len() == 1 {
        let existing = &matches[0];

        // Find traits this type implements via relationships
        let traits = find_implemented_traits(conn, &existing.name, &existing.module_path);

        // Try to find a pub use re-export path for a nicer prelude path
        let prelude_path = find_reexport_path(conn, &existing.name, &existing.crate_name)
            .unwrap_or_else(|| format!("{}::{}", existing.crate_name, existing.name));

        advisories.push(Advisory::UseExistingType {
            existing: existing.clone(),
            prelude_path,
            traits,
        });
    } else {
        advisories.push(Advisory::AmbiguousMatch {
            candidates: matches,
        });
    }

    advisories
}

/// Validate whether placing a symbol in a target crate violates ownership rules.
pub fn validate_boundary(
    conn: &Connection,
    symbol_name: &str,
    target_crate: &str,
) -> Vec<Advisory> {
    let mut advisories = Vec::new();

    let mut stmt = conn
        .prepare(
            "SELECT id, crate_name, description, kind FROM ownership_rules WHERE crate_name = ?1",
        )
        .unwrap();

    let rules: Vec<(i64, String, String, String)> = stmt
        .query_map(params![target_crate], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    // Collect alternative crates for suggestions
    let alternative_crates: Vec<(String, String)> = conn
        .prepare("SELECT name, path FROM crates WHERE name != ?1")
        .and_then(|mut stmt| {
            stmt.query_map(params![target_crate], |row| Ok((row.get(0)?, row.get(1)?)))
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
        })
        .unwrap_or_default();

    for (_id, crate_name, description, kind) in &rules {
        if kind == "must_not" {
            // Find the best alternative crate to suggest
            let (suggested_crate, suggested_path) = alternative_crates
                .first()
                .map(|(n, p)| (n.clone(), p.clone()))
                .unwrap_or_else(|| ("other".to_string(), "crates/other/src/lib.rs".to_string()));

            advisories.push(Advisory::BoundaryViolation {
                rule: description.clone(),
                crate_name: crate_name.clone(),
                suggestion: ModuleSuggestion {
                    module_path: format!("{}/src/lib.rs", suggested_path),
                    crate_name: suggested_crate.clone(),
                    reason: format!(
                        "Crate '{}' has a must_not rule: {}. Consider placing '{}' in crate '{}'.",
                        crate_name, description, symbol_name, suggested_crate
                    ),
                },
            });
        }
    }

    if advisories.is_empty() {
        advisories.push(Advisory::SafeToCreate {
            symbol: symbol_name.to_string(),
            target: target_crate.to_string(),
        });
    }

    advisories
}

/// Check for WIP or planned symbols in a given module path.
pub fn check_wip(conn: &Connection, module_path: &str) -> Vec<Advisory> {
    let mut advisories = Vec::new();

    let mut stmt = conn
        .prepare(
            "SELECT s.name, s.created_by, s.updated_at FROM symbols s
             JOIN modules m ON s.module_id = m.id
             WHERE m.path LIKE ?1 AND s.status IN ('planned', 'wip')",
        )
        .unwrap();

    let rows: Vec<(String, Option<String>, Option<String>)> = stmt
        .query_map(params![format!("%{}%", module_path)], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    if !rows.is_empty() {
        // Group by agent
        let symbols: Vec<String> = rows.iter().map(|(name, _, _)| name.clone()).collect();
        let agent = rows
            .iter()
            .find_map(|(_, a, _)| a.clone())
            .unwrap_or_else(|| "unknown".to_string());

        // Compute relative time from the most recent updated_at timestamp
        let last_activity_relative = rows
            .iter()
            .filter_map(|(_, _, u)| u.clone())
            .max()
            .map(|ts| relative_time(&ts));

        advisories.push(Advisory::WipConflict {
            agent,
            branch: None,
            symbols,
            last_activity: last_activity_relative,
        });
    }

    advisories
}

/// Search for reusable symbols matching a capability description.
///
/// Performs keyword-based search across public symbols and contracts.
pub fn suggest_reuse(conn: &Connection, capability_description: &str) -> Vec<Advisory> {
    let mut advisories = Vec::new();

    // Split description into keywords and search
    let keywords: Vec<&str> = capability_description
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .collect();

    let mut all_matches: Vec<ExistingSymbol> = Vec::new();

    for keyword in &keywords {
        let matches = find_symbols_fuzzy(conn, keyword);
        for m in matches {
            if !all_matches.iter().any(|e| e.name == m.name && e.module_path == m.module_path) {
                all_matches.push(m);
            }
        }
    }

    // Also search contracts
    for keyword in &keywords {
        let mut stmt = conn
            .prepare(
                "SELECT s.name, s.kind, m.path, c2.name, s.signature, s.visibility
                 FROM contracts ct
                 JOIN symbols s ON ct.symbol_id = s.id
                 JOIN modules m ON s.module_id = m.id
                 JOIN crates c2 ON m.crate_id = c2.id
                 WHERE ct.description LIKE ?1 AND s.visibility = 'public'",
            )
            .unwrap();

        let matches: Vec<ExistingSymbol> = stmt
            .query_map(params![format!("%{}%", keyword)], |row| {
                Ok(ExistingSymbol {
                    name: row.get(0)?,
                    kind: row.get(1)?,
                    module_path: row.get(2)?,
                    crate_name: row.get(3)?,
                    signature: row.get(4)?,
                    visibility: row.get(5)?,
                    call_sites: None,
                    created_by: None,
                })
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        for m in matches {
            if !all_matches.iter().any(|e| e.name == m.name && e.module_path == m.module_path) {
                all_matches.push(m);
            }
        }
    }

    if all_matches.is_empty() {
        // No suggestions found
    } else if all_matches.len() == 1 {
        let existing = &all_matches[0];
        if existing.kind == "trait" {
            let import_path = format!(
                "use {}::{}::{};",
                existing.crate_name,
                existing.module_path.rsplit('/').next().unwrap_or(&existing.module_path).trim_end_matches(".rs"),
                existing.name
            );
            advisories.push(Advisory::ReuseWithPattern {
                trait_name: existing.name.clone(),
                call_pattern: format!("impl {} for YourType", existing.name),
                example: format!(
                    "See {}::{} for the trait definition",
                    existing.crate_name, existing.module_path
                ),
                import: import_path,
            });
        } else {
            advisories.push(Advisory::ReuseExisting {
                existing: existing.clone(),
                suggestion: format!(
                    "Consider reusing '{}' from {}::{}",
                    existing.name, existing.crate_name, existing.module_path
                ),
            });
        }
    } else {
        advisories.push(Advisory::AmbiguousMatch {
            candidates: all_matches,
        });
    }

    advisories
}

/// Register planned actions into the registry as planned/wip symbols.
///
/// This is Trigger 2 (Post-Plan Registration) from the Minesweeper architecture:
/// after AI creates a plan, register the intended symbols so other agents can
/// detect conflicts via `check_wip`.
pub fn register_plan(
    conn: &Connection,
    actions: &[crate::advisory::PlannedAction],
    agent: &str,
) -> Result<usize, String> {
    let mut registered = 0;

    for action in actions {
        // Find or create the module for the target path
        let module_id = match crate::registry::find_module_by_path(conn, &action.target)
            .map_err(|e| format!("DB error: {}", e))?
        {
            Some(m) => m.id.unwrap(),
            None => {
                // Module doesn't exist yet — skip (will be created on sync)
                continue;
            }
        };

        let kind = action
            .kind
            .as_deref()
            .and_then(|k| k.parse::<crate::models::SymbolKind>().ok())
            .unwrap_or(crate::models::SymbolKind::Function);
        let status = if action.action == "create" {
            crate::models::SymbolStatus::Planned
        } else {
            crate::models::SymbolStatus::Wip
        };

        let now = chrono_now();
        let symbol = crate::models::Symbol {
            id: None,
            module_id,
            name: action.symbol.clone(),
            kind,
            visibility: crate::models::Visibility::Public,
            signature: None,
            status,
            created_by: Some(agent.to_string()),
            created_at: Some(now.clone()),
            updated_at: Some(now),
        };

        crate::registry::upsert_symbol(conn, &symbol)
            .map_err(|e| format!("Failed to register symbol '{}': {}", action.symbol, e))?;
        registered += 1;
    }

    Ok(registered)
}

fn chrono_now() -> String {
    // ISO-8601 timestamp without external dependency
    use std::time::SystemTime;
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    epoch_to_iso8601(duration.as_secs())
}

/// Convert epoch seconds to ISO-8601 formatted string (UTC).
fn epoch_to_iso8601(epoch_secs: u64) -> String {
    fn is_leap(y: u64) -> bool {
        (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
    }
    fn days_in_month(y: u64, m: u64) -> u64 {
        match m {
            1 => 31,
            2 => {
                if is_leap(y) {
                    29
                } else {
                    28
                }
            }
            3 => 31,
            4 => 30,
            5 => 31,
            6 => 30,
            7 => 31,
            8 => 31,
            9 => 30,
            10 => 31,
            11 => 30,
            12 => 31,
            _ => 30,
        }
    }

    let secs_in_day: u64 = 86400;

    let time_of_day = epoch_secs % secs_in_day;
    let mut days = epoch_secs / secs_in_day;

    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;

    let mut year: u64 = 1970;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let mut month: u64 = 1;
    loop {
        let dim = days_in_month(year, month);
        if days < dim {
            break;
        }
        days -= dim;
        month += 1;
    }
    let day = days + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, minute, second
    )
}

/// Convert a timestamp string to a human-readable relative time string like "12 minutes ago".
/// Accepts either epoch seconds (legacy) or ISO-8601 format.
fn relative_time(timestamp: &str) -> String {
    use std::time::SystemTime;

    let now_secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Try parsing as epoch seconds first (legacy format)
    let ts_secs = if let Ok(epoch) = timestamp.parse::<u64>() {
        epoch
    } else if let Some(secs) = iso8601_to_epoch(timestamp) {
        secs
    } else {
        return "unknown".to_string();
    };

    if now_secs <= ts_secs {
        return "just now".to_string();
    }

    let diff = now_secs - ts_secs;

    if diff < 60 {
        return "just now".to_string();
    }
    if diff < 3600 {
        let mins = diff / 60;
        return if mins == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{} minutes ago", mins)
        };
    }
    if diff < 86400 {
        let hours = diff / 3600;
        return if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", hours)
        };
    }
    let days = diff / 86400;
    if days == 1 {
        "1 day ago".to_string()
    } else {
        format!("{} days ago", days)
    }
}

/// Parse a simple ISO-8601 timestamp (YYYY-MM-DDTHH:MM:SSZ) to epoch seconds.
fn iso8601_to_epoch(s: &str) -> Option<u64> {
    let s = s.trim_end_matches('Z');
    let (date_part, time_part) = s.split_once('T')?;
    let mut date_parts = date_part.split('-');
    let year: u64 = date_parts.next()?.parse().ok()?;
    let month: u64 = date_parts.next()?.parse().ok()?;
    let day: u64 = date_parts.next()?.parse().ok()?;

    let mut time_parts = time_part.split(':');
    let hour: u64 = time_parts.next()?.parse().ok()?;
    let minute: u64 = time_parts.next()?.parse().ok()?;
    let second: u64 = time_parts.next()?.parse().ok()?;

    fn is_leap(y: u64) -> bool {
        (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
    }
    fn days_in_month(y: u64, m: u64) -> u64 {
        match m {
            1 => 31,
            2 => {
                if is_leap(y) {
                    29
                } else {
                    28
                }
            }
            3 => 31,
            4 => 30,
            5 => 31,
            6 => 30,
            7 => 31,
            8 => 31,
            9 => 30,
            10 => 31,
            11 => 30,
            12 => 31,
            _ => 30,
        }
    }

    let mut total_days: u64 = 0;
    for y in 1970..year {
        total_days += if is_leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        total_days += days_in_month(year, m);
    }
    total_days += day - 1;

    Some(total_days * 86400 + hour * 3600 + minute * 60 + second)
}

/// Validate an entire structured plan against the registry.
///
/// Runs validate_creation, validate_dependency, validate_boundary, and
/// check_wip for each planned action, composing the existing query functions.
pub fn validate_plan(
    conn: &Connection,
    actions: &[crate::advisory::PlannedAction],
) -> crate::advisory::PlanReport {
    use crate::advisory::*;

    let mut results = Vec::new();
    let mut summary = PlanSummary {
        total_actions: actions.len(),
        safe: 0,
        reuse: 0,
        violations: 0,
        conflicts: 0,
        ambiguous: 0,
    };

    for action in actions {
        let mut advisories = Vec::new();

        // Check if symbol already exists
        let creation = validate_creation(conn, &action.symbol, &action.target);
        advisories.extend(creation);

        // Check if it's a type that already exists
        let dep = validate_dependency(conn, &action.symbol);
        // Only add USE_EXISTING_TYPE advisories (skip SafeToCreate duplicates)
        for a in &dep {
            if matches!(a, Advisory::UseExistingType { .. }) {
                advisories.push(a.clone());
            }
        }

        // Check boundary rules if crate name is provided
        if let Some(ref crate_name) = action.crate_name {
            let boundary = validate_boundary(conn, &action.symbol, crate_name);
            for a in &boundary {
                if matches!(a, Advisory::BoundaryViolation { .. }) {
                    advisories.push(a.clone());
                }
            }
        }

        // Check for WIP conflicts in the target module
        let wip = check_wip(conn, &action.target);
        advisories.extend(wip);

        // Tally summary
        for a in &advisories {
            match a {
                Advisory::SafeToCreate { .. } => summary.safe += 1,
                Advisory::ReuseExisting { .. }
                | Advisory::UseExistingType { .. }
                | Advisory::ReuseWithPattern { .. } => summary.reuse += 1,
                Advisory::BoundaryViolation { .. } => summary.violations += 1,
                Advisory::WipConflict { .. } => summary.conflicts += 1,
                Advisory::AmbiguousMatch { .. } => summary.ambiguous += 1,
            }
        }

        results.push(PlanValidationResult {
            action: action.clone(),
            advisories,
        });
    }

    PlanReport { results, summary }
}

/// Aggregate stats from the registry.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RegistryStats {
    pub crate_count: usize,
    pub module_count: usize,
    pub symbol_count: usize,
    pub rule_count: usize,
    pub symbols_by_kind: Vec<(String, usize)>,
    pub symbols_by_visibility: Vec<(String, usize)>,
    pub symbols_by_status: Vec<(String, usize)>,
}

/// Get aggregate statistics from the registry.
pub fn get_registry_stats(conn: &Connection) -> RegistryStats {
    let crate_count: usize = conn
        .query_row("SELECT COUNT(*) FROM crates", [], |row| row.get(0))
        .unwrap_or(0);

    let module_count: usize = conn
        .query_row("SELECT COUNT(*) FROM modules", [], |row| row.get(0))
        .unwrap_or(0);

    let symbol_count: usize = conn
        .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
        .unwrap_or(0);

    let rule_count: usize = conn
        .query_row("SELECT COUNT(*) FROM ownership_rules", [], |row| row.get(0))
        .unwrap_or(0);

    let symbols_by_kind = group_count(conn, "SELECT kind, COUNT(*) FROM symbols GROUP BY kind");
    let symbols_by_visibility =
        group_count(conn, "SELECT visibility, COUNT(*) FROM symbols GROUP BY visibility");
    let symbols_by_status =
        group_count(conn, "SELECT status, COUNT(*) FROM symbols GROUP BY status");

    RegistryStats {
        crate_count,
        module_count,
        symbol_count,
        rule_count,
        symbols_by_kind,
        symbols_by_visibility,
        symbols_by_status,
    }
}

fn group_count(conn: &Connection, sql: &str) -> Vec<(String, usize)> {
    let mut stmt = conn.prepare(sql).unwrap();
    stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
}

// ---- Internal helpers ----

fn find_symbols_by_name(conn: &Connection, name: &str) -> Vec<ExistingSymbol> {
    let mut stmt = conn
        .prepare(
            "SELECT s.name, s.kind, m.path, c.name, s.signature, s.visibility, s.created_by,
                    (SELECT COUNT(*) FROM relationships r WHERE r.to_symbol_id = s.id) as call_sites
             FROM symbols s
             JOIN modules m ON s.module_id = m.id
             JOIN crates c ON m.crate_id = c.id
             WHERE s.name = ?1",
        )
        .unwrap();

    stmt.query_map(params![name], |row| {
        Ok(ExistingSymbol {
            name: row.get(0)?,
            kind: row.get(1)?,
            module_path: row.get(2)?,
            crate_name: row.get(3)?,
            signature: row.get(4)?,
            visibility: row.get(5)?,
            created_by: row.get(6)?,
            call_sites: row.get::<_, i64>(7).ok().map(|n| n as usize),
        })
    })
    .unwrap()
    .filter_map(|r| r.ok())
    .collect()
}

fn find_symbols_fuzzy(conn: &Connection, pattern: &str) -> Vec<ExistingSymbol> {
    let mut stmt = conn
        .prepare(
            "SELECT s.name, s.kind, m.path, c.name, s.signature, s.visibility, s.created_by,
                    (SELECT COUNT(*) FROM relationships r WHERE r.to_symbol_id = s.id) as call_sites
             FROM symbols s
             JOIN modules m ON s.module_id = m.id
             JOIN crates c ON m.crate_id = c.id
             WHERE s.name LIKE ?1",
        )
        .unwrap();

    stmt.query_map(params![format!("%{}%", pattern)], |row| {
        Ok(ExistingSymbol {
            name: row.get(0)?,
            kind: row.get(1)?,
            module_path: row.get(2)?,
            crate_name: row.get(3)?,
            signature: row.get(4)?,
            visibility: row.get(5)?,
            created_by: row.get(6)?,
            call_sites: row.get::<_, i64>(7).ok().map(|n| n as usize),
        })
    })
    .unwrap()
    .filter_map(|r| r.ok())
    .collect()
}

/// Find traits implemented by a given symbol (via `Implements` relationships).
fn find_implemented_traits(conn: &Connection, symbol_name: &str, module_path: &str) -> Vec<String> {
    // Find the symbol's id first
    let symbol_id: Option<i64> = conn
        .query_row(
            "SELECT s.id FROM symbols s
             JOIN modules m ON s.module_id = m.id
             WHERE s.name = ?1 AND m.path = ?2",
            params![symbol_name, module_path],
            |row| row.get(0),
        )
        .ok();

    let Some(sid) = symbol_id else {
        return Vec::new();
    };

    // Look up Implements relationships where this symbol is the from_symbol
    let mut stmt = conn
        .prepare(
            "SELECT s2.name FROM relationships r
             JOIN symbols s2 ON r.to_symbol_id = s2.id
             WHERE r.from_symbol_id = ?1 AND r.kind = 'implements'",
        )
        .unwrap();

    stmt.query_map(params![sid], |row| row.get::<_, String>(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
}

/// Try to find a `pub use` re-export path for a symbol (e.g., prelude path).
fn find_reexport_path(conn: &Connection, symbol_name: &str, crate_name: &str) -> Option<String> {
    // Look for a re-export symbol (kind = 're_export' or a symbol in a prelude/lib module)
    // that references this name
    let result: Option<(String, String)> = conn
        .query_row(
            "SELECT m.path, c.name FROM symbols s
             JOIN modules m ON s.module_id = m.id
             JOIN crates c ON m.crate_id = c.id
             WHERE s.name = ?1 AND s.kind = 're_export' AND c.name = ?2",
            params![symbol_name, crate_name],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    if let Some((mod_path, cn)) = result {
        // Build a Rust-style prelude path from the module path
        let module_name = mod_path
            .rsplit('/')
            .next()
            .unwrap_or(&mod_path)
            .trim_end_matches(".rs");
        Some(format!("{}::{}::{}", cn, module_name, symbol_name))
    } else {
        None
    }
}

fn find_type_symbols(conn: &Connection, type_name: &str) -> Vec<ExistingSymbol> {
    let mut stmt = conn
        .prepare(
            "SELECT s.name, s.kind, m.path, c.name, s.signature, s.visibility, s.created_by,
                    (SELECT COUNT(*) FROM relationships r WHERE r.to_symbol_id = s.id) as call_sites
             FROM symbols s
             JOIN modules m ON s.module_id = m.id
             JOIN crates c ON m.crate_id = c.id
             WHERE s.name = ?1 AND s.kind IN ('struct', 'enum', 'trait', 'type_alias', 're_export')",
        )
        .unwrap();

    stmt.query_map(params![type_name], |row| {
        Ok(ExistingSymbol {
            name: row.get(0)?,
            kind: row.get(1)?,
            module_path: row.get(2)?,
            crate_name: row.get(3)?,
            signature: row.get(4)?,
            visibility: row.get(5)?,
            created_by: row.get(6)?,
            call_sites: row.get::<_, i64>(7).ok().map(|n| n as usize),
        })
    })
    .unwrap()
    .filter_map(|r| r.ok())
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::*;
    use crate::registry::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        let c = Crate {
            id: None,
            name: "trading".to_string(),
            path: "crates/trading".to_string(),
            description: None,
            bounded_context: None,
        };
        let crate_id = insert_crate(&conn, &c).unwrap();

        let m = Module {
            id: None,
            crate_id,
            path: "crates/trading/src/fees.rs".to_string(),
            name: "fees".to_string(),
        };
        let module_id = insert_module(&conn, &m).unwrap();

        let s = Symbol {
            id: None,
            module_id,
            name: "calculate_fee".to_string(),
            kind: SymbolKind::Function,
            visibility: Visibility::Public,
            signature: Some("fn calculate_fee(amount: f64, rate: f64) -> f64".to_string()),
            status: SymbolStatus::Stable,
            created_by: None,
            created_at: None,
            updated_at: None,
        };
        insert_symbol(&conn, &s).unwrap();

        let s2 = Symbol {
            id: None,
            module_id,
            name: "FeeSchedule".to_string(),
            kind: SymbolKind::Struct,
            visibility: Visibility::Public,
            signature: Some("struct FeeSchedule".to_string()),
            status: SymbolStatus::Stable,
            created_by: None,
            created_at: None,
            updated_at: None,
        };
        insert_symbol(&conn, &s2).unwrap();

        conn
    }

    #[test]
    fn test_validate_creation_finds_exact_match() {
        let conn = setup_test_db();
        let advisories = validate_creation(&conn, "calculate_fee", "crates/other/src/lib.rs");
        assert_eq!(advisories.len(), 1);
        match &advisories[0] {
            Advisory::ReuseExisting { existing, .. } => {
                assert_eq!(existing.name, "calculate_fee");
            }
            other => panic!("Expected ReuseExisting, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_creation_safe() {
        let conn = setup_test_db();
        let advisories = validate_creation(&conn, "totally_new_function", "crates/other/src/lib.rs");
        assert_eq!(advisories.len(), 1);
        matches!(&advisories[0], Advisory::SafeToCreate { .. });
    }

    #[test]
    fn test_validate_dependency() {
        let conn = setup_test_db();
        let advisories = validate_dependency(&conn, "FeeSchedule");
        assert_eq!(advisories.len(), 1);
        match &advisories[0] {
            Advisory::UseExistingType { existing, .. } => {
                assert_eq!(existing.name, "FeeSchedule");
            }
            other => panic!("Expected UseExistingType, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_boundary_violation() {
        let conn = setup_test_db();
        insert_ownership_rule(
            &conn,
            &OwnershipRule {
                id: None,
                crate_name: "domain".to_string(),
                description: "No infrastructure concerns".to_string(),
                kind: OwnershipRuleKind::MustNot,
            },
        )
        .unwrap();

        let advisories = validate_boundary(&conn, "HttpClient", "domain");
        assert_eq!(advisories.len(), 1);
        matches!(&advisories[0], Advisory::BoundaryViolation { .. });
    }

    #[test]
    fn test_check_wip() {
        let conn = setup_test_db();

        // Add a WIP symbol
        let module_id: i64 = conn
            .query_row(
                "SELECT id FROM modules WHERE name = 'fees'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        insert_symbol(
            &conn,
            &Symbol {
                id: None,
                module_id,
                name: "new_fee_calculator".to_string(),
                kind: SymbolKind::Function,
                visibility: Visibility::Public,
                signature: None,
                status: SymbolStatus::Wip,
                created_by: None,
                created_at: None,
                updated_at: None,
            },
        )
        .unwrap();

        let advisories = check_wip(&conn, "fees");
        assert_eq!(advisories.len(), 1);
        match &advisories[0] {
            Advisory::WipConflict { symbols, .. } => {
                assert!(symbols.contains(&"new_fee_calculator".to_string()));
            }
            other => panic!("Expected WipConflict, got {:?}", other),
        }
    }

    #[test]
    fn test_suggest_reuse() {
        let conn = setup_test_db();
        // "calculate_fee" exists in the test DB, search for "calculate fee"
        let advisories = suggest_reuse(&conn, "calculate fee");

        assert!(!advisories.is_empty(), "suggest_reuse should return advisories for a matching keyword");
        // The keyword "calculate" should fuzzy-match "calculate_fee"
        match &advisories[0] {
            Advisory::ReuseExisting { existing, .. } => {
                assert_eq!(existing.name, "calculate_fee");
            }
            Advisory::AmbiguousMatch { candidates } => {
                assert!(
                    candidates.iter().any(|c| c.name == "calculate_fee"),
                    "Candidates should include 'calculate_fee', got: {:?}",
                    candidates.iter().map(|c| &c.name).collect::<Vec<_>>()
                );
            }
            other => panic!(
                "Expected ReuseExisting or AmbiguousMatch, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn test_validate_plan() {
        let conn = setup_test_db();
        let actions = vec![
            PlannedAction {
                action: "create".to_string(),
                symbol: "brand_new_fn".to_string(),
                target: "crates/other/src/lib.rs".to_string(),
                crate_name: None,
                kind: Some("function".to_string()),
            },
            PlannedAction {
                action: "create".to_string(),
                symbol: "calculate_fee".to_string(),
                target: "crates/other/src/lib.rs".to_string(),
                crate_name: None,
                kind: Some("function".to_string()),
            },
        ];

        let report = validate_plan(&conn, &actions);
        assert_eq!(report.results.len(), 2, "Plan report should have 2 results");
        assert_eq!(report.summary.total_actions, 2, "Summary total_actions should be 2");

        // First action is brand new, should have SafeToCreate
        let first = &report.results[0];
        assert!(
            first.advisories.iter().any(|a| matches!(a, Advisory::SafeToCreate { .. })),
            "brand_new_fn should be SafeToCreate, got: {:?}",
            first.advisories
        );

        // Second action collides with existing "calculate_fee"
        let second = &report.results[1];
        assert!(
            second.advisories.iter().any(|a| matches!(a, Advisory::ReuseExisting { .. })),
            "calculate_fee should trigger ReuseExisting, got: {:?}",
            second.advisories
        );

        // Summary should reflect the reuse
        assert!(report.summary.reuse >= 1, "Summary should count at least 1 reuse");
    }

    #[test]
    fn test_register_plan() {
        let conn = setup_test_db();
        let actions = vec![PlannedAction {
            action: "create".to_string(),
            symbol: "planned_symbol".to_string(),
            target: "crates/trading/src/fees.rs".to_string(),
            crate_name: Some("trading".to_string()),
            kind: Some("function".to_string()),
        }];

        let count = register_plan(&conn, &actions, "test-agent").unwrap();
        assert_eq!(count, 1, "Should register 1 symbol");

        // Verify the symbol was inserted with status=planned and created_by=test-agent
        let (status, created_by): (String, Option<String>) = conn
            .query_row(
                "SELECT status, created_by FROM symbols WHERE name = 'planned_symbol'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("planned_symbol should exist in DB");

        assert_eq!(status, "planned", "Status should be 'planned'");
        assert_eq!(
            created_by.as_deref(),
            Some("test-agent"),
            "created_by should be 'test-agent'"
        );
    }
}
