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
) -> Result<Vec<Advisory>, String> {
    let mut advisories = Vec::new();

    // Exact match
    let exact_matches = find_symbols_by_name(conn, symbol_name)?;
    if exact_matches.len() == 1 {
        let existing = &exact_matches[0];
        advisories.push(Advisory::ReuseExisting {
            existing: existing.clone(),
            suggestion: format!(
                "Symbol '{}' already exists in {}::{}. Consider reusing it.",
                symbol_name, existing.crate_name, existing.module_path
            ),
        });
        return Ok(advisories);
    } else if exact_matches.len() > 1 {
        // Separate definitions from re-exports
        let (definitions, re_exports): (Vec<_>, Vec<_>) = exact_matches
            .into_iter()
            .partition(|s| s.kind != "re_export");

        if definitions.len() == 1 {
            // One definition + re-exports → prefer the definition
            let existing = &definitions[0];
            let note = if re_exports.is_empty() {
                String::new()
            } else {
                let paths: Vec<String> = re_exports
                    .iter()
                    .map(|r| format!("{}::{}", r.crate_name, r.module_path))
                    .collect();
                format!(" Also re-exported from: {}", paths.join(", "))
            };
            advisories.push(Advisory::ReuseExisting {
                existing: existing.clone(),
                suggestion: format!(
                    "Symbol '{}' already exists in {}::{}.{}",
                    symbol_name, existing.crate_name, existing.module_path, note
                ),
            });
            return Ok(advisories);
        } else if definitions.is_empty() {
            // Only re-exports exist — pick the first one
            let existing = &re_exports[0];
            advisories.push(Advisory::ReuseExisting {
                existing: existing.clone(),
                suggestion: format!(
                    "Symbol '{}' is re-exported from {}::{}.",
                    symbol_name, existing.crate_name, existing.module_path
                ),
            });
            return Ok(advisories);
        } else {
            // Multiple definitions — genuinely ambiguous
            advisories.push(Advisory::AmbiguousMatch {
                candidates: definitions,
            });
            return Ok(advisories);
        }
    }

    // Fuzzy match (LIKE with % wildcards)
    let fuzzy_matches = find_symbols_fuzzy(conn, symbol_name)?;
    if fuzzy_matches.len() == 1 {
        let existing = &fuzzy_matches[0];
        advisories.push(Advisory::ReuseExisting {
            existing: existing.clone(),
            suggestion: format!(
                "Similar symbol '{}' found in {}::{}. Did you mean to reuse it?",
                existing.name, existing.crate_name, existing.module_path
            ),
        });
        return Ok(advisories);
    } else if fuzzy_matches.len() > 1 {
        advisories.push(Advisory::AmbiguousMatch {
            candidates: fuzzy_matches,
        });
        return Ok(advisories);
    }

    // Check linked (external) registries
    if let Ok(linked) = find_linked_symbols(conn, symbol_name) {
        if !linked.is_empty() {
            let existing = &linked[0];
            advisories.push(Advisory::ReuseExisting {
                existing: ExistingSymbol {
                    name: existing.name.clone(),
                    kind: existing.kind.clone().unwrap_or_else(|| "unknown".to_string()),
                    module_path: existing.module_path.clone().unwrap_or_default(),
                    crate_name: format!("{}::{}", existing.source_name, existing.crate_name.as_deref().unwrap_or("?")),
                    signature: existing.signature.clone(),
                    visibility: "public".to_string(),
                    call_sites: None,
                    created_by: None,
                    line_number: None,
                    scope: None,
                    location: None,
                },
                suggestion: format!(
                    "Symbol '{}' exists in linked registry '{}'. Reuse it instead of creating a duplicate.",
                    symbol_name, existing.source_name
                ),
            });
            return Ok(advisories);
        }
    }

    advisories.push(Advisory::SafeToCreate {
        symbol: symbol_name.to_string(),
        target: target_module.to_string(),
    });
    Ok(advisories)
}

/// Validate whether a type/dependency exists in the registry.
///
/// Looks up struct/trait/enum across all crates.
pub fn validate_dependency(conn: &Connection, type_name: &str) -> Result<Vec<Advisory>, String> {
    let mut advisories = Vec::new();

    let matches = find_type_symbols(conn, type_name)?;
    if matches.is_empty() {
        advisories.push(Advisory::SafeToCreate {
            symbol: type_name.to_string(),
            target: "unknown".to_string(),
        });
    } else if matches.len() == 1 {
        let existing = &matches[0];

        // Find traits this type implements via relationships
        let traits = find_implemented_traits(conn, &existing.name, &existing.module_path)?;

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

    Ok(advisories)
}

/// Validate whether placing a symbol in a target crate violates ownership rules.
pub fn validate_boundary(
    conn: &Connection,
    symbol_name: &str,
    target_crate: &str,
) -> Result<Vec<Advisory>, String> {
    let mut advisories = Vec::new();

    let mut stmt = conn
        .prepare(
            "SELECT id, crate_name, description, kind, pattern, regex FROM ownership_rules WHERE crate_name = ?1",
        )
        .map_err(|e| format!("Failed to query ownership rules: {}", e))?;

    let rules: Vec<(i64, String, String, String, Option<String>, Option<String>)> = stmt
        .query_map(params![target_crate], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
        })
        .map_err(|e| format!("Failed to query ownership rules: {}", e))?
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

    for (_id, crate_name, description, kind, pattern, regex) in &rules {
        if kind == "must_not" {
            // Check pattern-based rules first (glob patterns)
            let pattern_match = pattern.as_ref().is_some_and(|p| glob_matches(symbol_name, p));

            // Check regex-based rules
            let regex_match = regex.as_ref().is_some_and(|r| regex_matches(symbol_name, r));

            // Fall back to semantic text-based matching if no pattern/regex defined
            let text_match = pattern.is_none()
                && regex.is_none()
                && symbol_matches_rule(symbol_name, description);

            if !pattern_match && !regex_match && !text_match {
                continue;
            }

            // Find the best alternative crate to suggest
            let (suggested_crate, suggested_path) = find_best_alternative(
                &alternative_crates,
                symbol_name,
                conn,
            );

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

    Ok(advisories)
}

/// Check for WIP or planned symbols in a given module path.
pub fn check_wip(conn: &Connection, module_path: &str) -> Result<Vec<Advisory>, String> {
    let mut advisories = Vec::new();

    let mut stmt = conn
        .prepare(
            "SELECT s.name, s.created_by, s.updated_at FROM symbols s
             JOIN modules m ON s.module_id = m.id
             WHERE m.path LIKE ?1 AND s.status IN ('planned', 'wip')",
        )
        .map_err(|e| format!("Failed to query WIP symbols: {}", e))?;

    let rows: Vec<(String, Option<String>, Option<String>)> = stmt
        .query_map(params![format!("%{}%", module_path)], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|e| format!("Failed to query WIP symbols: {}", e))?
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

    Ok(advisories)
}

/// Search for reusable symbols matching a capability description.
///
/// Performs keyword-based search across public symbols and contracts.
pub fn suggest_reuse(conn: &Connection, capability_description: &str) -> Result<Vec<Advisory>, String> {
    let mut advisories = Vec::new();

    // Split description into keywords and search
    let keywords: Vec<&str> = capability_description
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .collect();

    let mut all_matches: Vec<ExistingSymbol> = Vec::new();

    for keyword in &keywords {
        let matches = find_symbols_fuzzy(conn, keyword)?;
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
            .map_err(|e| format!("Failed to query contracts: {}", e))?;

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
                    line_number: None,
                    scope: None,
                    location: None,
                })
            })
            .map_err(|e| format!("Failed to query contracts: {}", e))?
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

    Ok(advisories)
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
            .map_err(|e| format!("Failed to find module: {}", e))?
        {
            Some(m) => m.id.ok_or_else(|| "Module found but has no ID".to_string())?,
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
            line_number: None,
            scope: None,
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

#[allow(clippy::manual_is_multiple_of)]
fn is_leap_year(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: u64, m: u64) -> u64 {
    match m {
        1 => 31,
        2 => {
            if is_leap_year(y) {
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

/// Convert epoch seconds to ISO-8601 formatted string (UTC).
fn epoch_to_iso8601(epoch_secs: u64) -> String {
    let secs_in_day: u64 = 86400;

    let time_of_day = epoch_secs % secs_in_day;
    let mut days = epoch_secs / secs_in_day;

    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;

    let mut year: u64 = 1970;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
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

    let mut total_days: u64 = 0;
    for y in 1970..year {
        total_days += if is_leap_year(y) { 366 } else { 365 };
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
) -> Result<crate::advisory::PlanReport, String> {
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
        let creation = validate_creation(conn, &action.symbol, &action.target)?;
        advisories.extend(creation);

        // Check if it's a type that already exists
        let dep = validate_dependency(conn, &action.symbol)?;
        // Only add USE_EXISTING_TYPE advisories (skip SafeToCreate duplicates)
        for a in &dep {
            if matches!(a, Advisory::UseExistingType { .. }) {
                advisories.push(a.clone());
            }
        }

        // Check boundary rules if crate name is provided
        if let Some(ref crate_name) = action.crate_name {
            let boundary = validate_boundary(conn, &action.symbol, crate_name)?;
            for a in &boundary {
                if matches!(a, Advisory::BoundaryViolation { .. }) {
                    advisories.push(a.clone());
                }
            }
        }

        // Check for WIP conflicts in the target module
        let wip = check_wip(conn, &action.target)?;
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

    Ok(PlanReport { results, summary })
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
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let result = match stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?))) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => Vec::new(),
    };
    result
}

// ---- Internal helpers ----

fn find_linked_symbols(conn: &Connection, name: &str) -> Result<Vec<crate::models::LinkedSymbol>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, source_name, name, kind, crate_name, module_path, signature, linked_at
             FROM linked_symbols WHERE name = ?1",
        )
        .map_err(|e| format!("Failed to query linked symbols: {}", e))?;
    let rows = stmt
        .query_map(params![name], |row| {
            Ok(crate::models::LinkedSymbol {
                id: Some(row.get(0)?),
                source_name: row.get(1)?,
                name: row.get(2)?,
                kind: row.get(3)?,
                crate_name: row.get(4)?,
                module_path: row.get(5)?,
                signature: row.get(6)?,
                linked_at: row.get(7)?,
            })
        })
        .map_err(|e| format!("Failed to query linked symbols: {}", e))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn find_symbols_by_name(conn: &Connection, name: &str) -> Result<Vec<ExistingSymbol>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT s.name, s.kind, m.path, c.name, s.signature, s.visibility, s.created_by,
                    (SELECT COUNT(*) FROM relationships r WHERE r.to_symbol_id = s.id AND r.kind = 'calls') as call_sites,
                    s.line_number, s.scope
             FROM symbols s
             JOIN modules m ON s.module_id = m.id
             JOIN crates c ON m.crate_id = c.id
             WHERE s.name = ?1",
        )
        .map_err(|e| format!("Failed to query symbols by name: {}", e))?;

    let rows = stmt
        .query_map(params![name], |row| {
            let module_path: String = row.get(2)?;
            let line_number: Option<u32> = row.get(8)?;
            let location = line_number.map(|ln| format!("{}:{}", module_path, ln));
            Ok(ExistingSymbol {
                name: row.get(0)?,
                kind: row.get(1)?,
                module_path,
                crate_name: row.get(3)?,
                signature: row.get(4)?,
                visibility: row.get(5)?,
                created_by: row.get(6)?,
                call_sites: row.get::<_, i64>(7).ok().map(|n| n as usize),
                line_number,
                scope: row.get(9)?,
                location,
            })
        })
        .map_err(|e| format!("Failed to query symbols by name: {}", e))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn find_symbols_fuzzy(conn: &Connection, pattern: &str) -> Result<Vec<ExistingSymbol>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT s.name, s.kind, m.path, c.name, s.signature, s.visibility, s.created_by,
                    (SELECT COUNT(*) FROM relationships r WHERE r.to_symbol_id = s.id AND r.kind = 'calls') as call_sites,
                    s.line_number, s.scope
             FROM symbols s
             JOIN modules m ON s.module_id = m.id
             JOIN crates c ON m.crate_id = c.id
             WHERE s.name LIKE ?1",
        )
        .map_err(|e| format!("Failed to search symbols: {}", e))?;

    let rows = stmt
        .query_map(params![format!("%{}%", pattern)], |row| {
            let module_path: String = row.get(2)?;
            let line_number: Option<u32> = row.get(8)?;
            let location = line_number.map(|ln| format!("{}:{}", module_path, ln));
            Ok(ExistingSymbol {
                name: row.get(0)?,
                kind: row.get(1)?,
                module_path,
                crate_name: row.get(3)?,
                signature: row.get(4)?,
                visibility: row.get(5)?,
                created_by: row.get(6)?,
                call_sites: row.get::<_, i64>(7).ok().map(|n| n as usize),
                line_number,
                scope: row.get(9)?,
                location,
            })
        })
        .map_err(|e| format!("Failed to search symbols: {}", e))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Find traits implemented by a given symbol (via `Implements` relationships).
fn find_implemented_traits(
    conn: &Connection,
    symbol_name: &str,
    module_path: &str,
) -> Result<Vec<String>, String> {
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
        return Ok(Vec::new());
    };

    // Look up Implements relationships where this symbol is the from_symbol
    let mut stmt = conn
        .prepare(
            "SELECT s2.name FROM relationships r
             JOIN symbols s2 ON r.to_symbol_id = s2.id
             WHERE r.from_symbol_id = ?1 AND r.kind = 'implements'",
        )
        .map_err(|e| format!("Failed to query trait implementations: {}", e))?;

    let rows = stmt
        .query_map(params![sid], |row| row.get::<_, String>(0))
        .map_err(|e| format!("Failed to query trait implementations: {}", e))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
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

/// Get all crate-level dependencies as (from_crate_name, to_crate_name) pairs.
pub fn get_crate_dependencies(conn: &Connection) -> Result<Vec<(String, String)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT c1.name, c2.name
             FROM crate_dependencies cd
             JOIN crates c1 ON cd.from_crate_id = c1.id
             JOIN crates c2 ON cd.to_crate_id = c2.id",
        )
        .map_err(|e| format!("Failed to query crate dependencies: {}", e))?;

    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| format!("Failed to query crate dependencies: {}", e))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn find_type_symbols(conn: &Connection, type_name: &str) -> Result<Vec<ExistingSymbol>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT s.name, s.kind, m.path, c.name, s.signature, s.visibility, s.created_by,
                    (SELECT COUNT(*) FROM relationships r WHERE r.to_symbol_id = s.id AND r.kind = 'calls') as call_sites,
                    s.line_number, s.scope
             FROM symbols s
             JOIN modules m ON s.module_id = m.id
             JOIN crates c ON m.crate_id = c.id
             WHERE s.name = ?1 AND s.kind IN ('struct', 'enum', 'trait', 'type_alias', 're_export', 'class', 'interface')",
        )
        .map_err(|e| format!("Failed to query type symbols: {}", e))?;

    let rows = stmt
        .query_map(params![type_name], |row| {
            let module_path: String = row.get(2)?;
            let line_number: Option<u32> = row.get(8)?;
            let location = line_number.map(|ln| format!("{}:{}", module_path, ln));
            Ok(ExistingSymbol {
                name: row.get(0)?,
                kind: row.get(1)?,
                module_path,
                crate_name: row.get(3)?,
                signature: row.get(4)?,
                visibility: row.get(5)?,
                created_by: row.get(6)?,
                call_sites: row.get::<_, i64>(7).ok().map(|n| n as usize),
                line_number,
                scope: row.get(9)?,
                location,
            })
        })
        .map_err(|e| format!("Failed to query type symbols: {}", e))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Match a symbol name against comma-separated glob patterns.
/// Supports `*` as wildcard (e.g., "Sql*,*Repository,Pg*").
fn glob_matches(symbol_name: &str, patterns: &str) -> bool {
    for pattern in patterns.split(',') {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            continue;
        }
        if glob_match_single(symbol_name, pattern) {
            return true;
        }
    }
    false
}

/// Match a single glob pattern (supports `*` wildcard only).
fn glob_match_single(name: &str, pattern: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        // No wildcard — exact match
        return name == pattern;
    }

    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if let Some(found) = name[pos..].find(part) {
            if i == 0 && found != 0 {
                // First part must match start
                return false;
            }
            pos += found + part.len();
        } else {
            return false;
        }
    }
    // If pattern doesn't end with *, last part must match end
    if !pattern.ends_with('*') {
        return name.ends_with(parts.last().unwrap_or(&""));
    }
    true
}

/// Match a symbol name against a regex-like pattern.
/// Supports: `^` (start anchor), `$` (end anchor), `|` (alternation), `()` (grouping).
/// For more complex regex, users should use glob patterns instead.
fn regex_matches(symbol_name: &str, pattern: &str) -> bool {
    let anchored_start = pattern.starts_with('^');
    let anchored_end = pattern.ends_with('$');

    // Strip outer anchors
    let inner = pattern
        .trim_start_matches('^')
        .trim_end_matches('$');

    // Strip outer parens if present: ^(A|B|C)$ -> A|B|C
    let inner = if inner.starts_with('(') && inner.ends_with(')') {
        &inner[1..inner.len() - 1]
    } else {
        inner
    };

    // Split alternatives on `|`
    for alt in inner.split('|') {
        let alt = alt.trim();
        if alt.is_empty() {
            continue;
        }

        let matched = if anchored_start && anchored_end {
            symbol_name == alt
        } else if anchored_start {
            symbol_name.starts_with(alt)
        } else if anchored_end {
            symbol_name.ends_with(alt)
        } else {
            symbol_name.contains(alt)
        };

        if matched {
            return true;
        }
    }
    false
}

/// Check if a symbol name is related to the concerns described in a must_not rule.
///
/// Extracts semantic keywords from the rule description and checks whether the
/// symbol name (case-insensitively) contains any of them. This prevents false
/// positives where unrelated symbols trigger boundary violations.
///
/// For example, rule "No HTTP routing or database schema" would extract keywords
/// like ["http", "routing", "database", "schema"], and only symbols containing
/// those substrings (e.g. "HttpClient", "DatabaseSchema", "Router") would match.
fn symbol_matches_rule(symbol_name: &str, rule_description: &str) -> bool {
    // Common stop words to ignore when extracting keywords
    let stop_words: &[&str] = &[
        "no", "or", "and", "the", "a", "an", "in", "of", "to", "for",
        "not", "with", "from", "by", "on", "at", "is", "be", "as",
        "do", "does", "should", "must", "direct", "operations", "concerns",
        "logic", "definitions", "handling",
    ];

    let symbol_lower = symbol_name.to_lowercase();

    // Also split the symbol name by CamelCase boundaries for matching
    let symbol_parts = split_camel_case(&symbol_lower);

    // Extract keywords from the rule description
    let rule_words: Vec<String> = rule_description
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 2)
        .filter(|w| !stop_words.contains(w))
        .map(|w| w.to_string())
        .collect();

    // Check if any keyword from the rule matches the symbol name
    for keyword in &rule_words {
        // Direct substring match against full symbol name
        if symbol_lower.contains(keyword.as_str()) {
            return true;
        }
        // Match against individual CamelCase parts
        for part in &symbol_parts {
            if part == keyword {
                return true;
            }
            // Stem-aware matching: check if the keyword and part share a common
            // root of at least 4 chars (e.g. "routing" / "router" share "rout")
            let min_stem = 4.min(keyword.len()).min(part.len());
            if min_stem >= 4 && keyword[..min_stem] == part[..min_stem] {
                return true;
            }
        }
    }

    false
}

/// Split a CamelCase identifier into lowercase parts.
/// e.g. "HttpClient" -> ["http", "client"]
/// e.g. "postservice" -> ["postservice"] (already lowercase)
fn split_camel_case(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = s.chars().collect();

    for &c in chars.iter() {
        if c.is_uppercase() && !current.is_empty() {
            parts.push(current.to_lowercase());
            current = String::new();
        }
        current.push(c);
    }
    if !current.is_empty() {
        parts.push(current.to_lowercase());
    }

    // If no split happened, return the original
    if parts.is_empty() {
        parts.push(s.to_lowercase());
    }

    parts
}

/// Find the best alternative crate to suggest for a symbol that violates a boundary.
///
/// Tries to find a crate whose `must_own` rule description semantically matches
/// the symbol. Falls back to the first alternative crate.
fn find_best_alternative(
    alternatives: &[(String, String)],
    symbol_name: &str,
    conn: &Connection,
) -> (String, String) {
    // Try to find a crate with a must_own rule that matches the symbol
    if let Ok(mut stmt) = conn.prepare(
        "SELECT crate_name, description FROM ownership_rules WHERE kind = 'must_own'"
    ) {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            for row in rows.flatten() {
                let (crate_name, description) = row;
                if symbol_matches_rule(symbol_name, &description) {
                    // Find this crate in alternatives
                    if let Some((name, path)) = alternatives.iter().find(|(n, _)| n == &crate_name) {
                        return (name.clone(), path.clone());
                    }
                }
            }
        }
    }

    // Fallback to first alternative
    alternatives
        .first()
        .map(|(n, p)| (n.clone(), p.clone()))
        .unwrap_or_else(|| ("other".to_string(), "crates/other/src/lib.rs".to_string()))
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
            language: Language::Rust,
        };
        let module_id = insert_module(&conn, &m).unwrap();

        let s = Symbol {
            id: None,
            module_id,
            name: "calculate_fee".to_string(),
            kind: SymbolKind::Function,
            visibility: Visibility::Public,
            signature: Some("fn calculate_fee(amount: f64, rate: f64) -> f64".to_string()),
            line_number: None,
            scope: None,
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
            line_number: None,
            scope: None,
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
        let advisories =
            validate_creation(&conn, "calculate_fee", "crates/other/src/lib.rs").unwrap();
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
        let advisories =
            validate_creation(&conn, "totally_new_function", "crates/other/src/lib.rs").unwrap();
        assert_eq!(advisories.len(), 1);
        matches!(&advisories[0], Advisory::SafeToCreate { .. });
    }

    #[test]
    fn test_validate_dependency() {
        let conn = setup_test_db();
        let advisories = validate_dependency(&conn, "FeeSchedule").unwrap();
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
                pattern: None,
                regex: None,
            },
        )
        .unwrap();

        let advisories = validate_boundary(&conn, "HttpClient", "domain").unwrap();
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
                line_number: None,
                scope: None,
                status: SymbolStatus::Wip,
                created_by: None,
                created_at: None,
                updated_at: None,
            },
        )
        .unwrap();

        let advisories = check_wip(&conn, "fees").unwrap();
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
        let advisories = suggest_reuse(&conn, "calculate fee").unwrap();

        assert!(
            !advisories.is_empty(),
            "suggest_reuse should return advisories for a matching keyword"
        );
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

        let report = validate_plan(&conn, &actions).unwrap();
        assert_eq!(report.results.len(), 2, "Plan report should have 2 results");
        assert_eq!(
            report.summary.total_actions, 2,
            "Summary total_actions should be 2"
        );

        // First action is brand new, should have SafeToCreate
        let first = &report.results[0];
        assert!(
            first
                .advisories
                .iter()
                .any(|a| matches!(a, Advisory::SafeToCreate { .. })),
            "brand_new_fn should be SafeToCreate, got: {:?}",
            first.advisories
        );

        // Second action collides with existing "calculate_fee"
        let second = &report.results[1];
        assert!(
            second
                .advisories
                .iter()
                .any(|a| matches!(a, Advisory::ReuseExisting { .. })),
            "calculate_fee should trigger ReuseExisting, got: {:?}",
            second.advisories
        );

        // Summary should reflect the reuse
        assert!(
            report.summary.reuse >= 1,
            "Summary should count at least 1 reuse"
        );
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

    #[test]
    fn test_symbol_matches_rule() {
        // Should match: symbol name contains keyword from rule
        assert!(symbol_matches_rule("HttpClient", "No HTTP routing or network operations"));
        assert!(symbol_matches_rule("DatabaseSchema", "No database or filesystem operations"));
        assert!(symbol_matches_rule("Router", "No HTTP routing or network operations"));
        assert!(symbol_matches_rule("TemplateRenderer", "No template rendering"));
        assert!(symbol_matches_rule("FileWriter", "No I/O or filesystem operations"));

        // Should NOT match: symbol name unrelated to rule
        assert!(!symbol_matches_rule("PostService", "No HTTP routing, no direct database schema, no template rendering"));
        assert!(!symbol_matches_rule("UserRole", "No I/O or filesystem operations"));
        assert!(!symbol_matches_rule("CommentService", "No HTTP routing or network operations"));
        assert!(!symbol_matches_rule("HookRegistry", "No database or filesystem operations"));
    }

    #[test]
    fn test_split_camel_case() {
        assert_eq!(split_camel_case("HttpClient"), vec!["http", "client"]);
        assert_eq!(split_camel_case("postservice"), vec!["postservice"]);
        assert_eq!(split_camel_case("DatabaseSchema"), vec!["database", "schema"]);
        assert_eq!(split_camel_case("URLParser"), vec!["u", "r", "l", "parser"]);
    }

    #[test]
    fn test_validate_creation_prefers_definition_over_reexport() {
        let conn = setup_test_db();

        // Add a second crate with a lib.rs module for re-exports
        let c2 = Crate {
            id: None,
            name: "crypto-types".to_string(),
            path: "crates/crypto-types".to_string(),
            description: None,
            bounded_context: None,
        };
        let crate_id2 = insert_crate(&conn, &c2).unwrap();

        let m_algo = Module {
            id: None,
            crate_id: crate_id2,
            path: "crates/crypto-types/src/algorithm.rs".to_string(),
            name: "algorithm".to_string(),
            language: Language::Rust,
        };
        let module_algo_id = insert_module(&conn, &m_algo).unwrap();

        let m_lib = Module {
            id: None,
            crate_id: crate_id2,
            path: "crates/crypto-types/src/lib.rs".to_string(),
            name: "lib".to_string(),
            language: Language::Rust,
        };
        let module_lib_id = insert_module(&conn, &m_lib).unwrap();

        // Definition: enum AlgorithmId in algorithm.rs
        insert_symbol(
            &conn,
            &Symbol {
                id: None,
                module_id: module_algo_id,
                name: "AlgorithmId".to_string(),
                kind: SymbolKind::Enum,
                visibility: Visibility::Public,
                signature: Some("enum AlgorithmId".to_string()),
                line_number: Some(5),
                scope: None,
                status: SymbolStatus::Stable,
                created_by: None,
                created_at: None,
                updated_at: None,
            },
        )
        .unwrap();

        // Re-export: pub use in lib.rs
        insert_symbol(
            &conn,
            &Symbol {
                id: None,
                module_id: module_lib_id,
                name: "AlgorithmId".to_string(),
                kind: SymbolKind::ReExport,
                visibility: Visibility::Public,
                signature: Some("pub use algorithm::AlgorithmId".to_string()),
                line_number: Some(1),
                scope: None,
                status: SymbolStatus::Stable,
                created_by: None,
                created_at: None,
                updated_at: None,
            },
        )
        .unwrap();

        let advisories =
            validate_creation(&conn, "AlgorithmId", "crates/other/src/lib.rs").unwrap();
        assert_eq!(advisories.len(), 1);
        match &advisories[0] {
            Advisory::ReuseExisting { existing, suggestion } => {
                assert_eq!(existing.kind, "enum", "Should prefer the definition (enum), not re_export");
                assert_eq!(existing.module_path, "crates/crypto-types/src/algorithm.rs");
                assert!(suggestion.contains("re-exported"), "Suggestion should mention re-export: {}", suggestion);
            }
            other => panic!("Expected ReuseExisting pointing to definition, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_creation_ambiguous_when_multiple_definitions() {
        let conn = setup_test_db();

        // Create two different crates each with a definition of the same name
        let c2 = Crate {
            id: None,
            name: "crate-a".to_string(),
            path: "crates/crate-a".to_string(),
            description: None,
            bounded_context: None,
        };
        let crate_a_id = insert_crate(&conn, &c2).unwrap();
        let m_a = Module {
            id: None,
            crate_id: crate_a_id,
            path: "crates/crate-a/src/lib.rs".to_string(),
            name: "lib".to_string(),
            language: Language::Rust,
        };
        let mod_a_id = insert_module(&conn, &m_a).unwrap();
        insert_symbol(
            &conn,
            &Symbol {
                id: None,
                module_id: mod_a_id,
                name: "DuplicateType".to_string(),
                kind: SymbolKind::Struct,
                visibility: Visibility::Public,
                signature: None,
                line_number: None,
                scope: None,
                status: SymbolStatus::Stable,
                created_by: None,
                created_at: None,
                updated_at: None,
            },
        )
        .unwrap();

        let c3 = Crate {
            id: None,
            name: "crate-b".to_string(),
            path: "crates/crate-b".to_string(),
            description: None,
            bounded_context: None,
        };
        let crate_b_id = insert_crate(&conn, &c3).unwrap();
        let m_b = Module {
            id: None,
            crate_id: crate_b_id,
            path: "crates/crate-b/src/lib.rs".to_string(),
            name: "lib".to_string(),
            language: Language::Rust,
        };
        let mod_b_id = insert_module(&conn, &m_b).unwrap();
        insert_symbol(
            &conn,
            &Symbol {
                id: None,
                module_id: mod_b_id,
                name: "DuplicateType".to_string(),
                kind: SymbolKind::Struct,
                visibility: Visibility::Public,
                signature: None,
                line_number: None,
                scope: None,
                status: SymbolStatus::Stable,
                created_by: None,
                created_at: None,
                updated_at: None,
            },
        )
        .unwrap();

        let advisories =
            validate_creation(&conn, "DuplicateType", "crates/other/src/lib.rs").unwrap();
        assert_eq!(advisories.len(), 1);
        match &advisories[0] {
            Advisory::AmbiguousMatch { candidates } => {
                assert_eq!(candidates.len(), 2, "Should have 2 definition candidates");
                assert!(candidates.iter().all(|c| c.kind == "struct"), "All candidates should be definitions");
            }
            other => panic!("Expected AmbiguousMatch for multiple definitions, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_boundary_does_not_false_positive() {
        let conn = setup_test_db();

        // Add a must_not rule
        conn.execute(
            "INSERT INTO ownership_rules (crate_name, description, kind) VALUES (?1, ?2, ?3)",
            params!["domain", "No HTTP routing or network operations", "must_not"],
        )
        .unwrap();

        // PostService should NOT trigger a violation (unrelated to HTTP/network)
        let advisories = validate_boundary(&conn, "PostService", "domain").unwrap();
        assert!(
            advisories.iter().all(|a| matches!(a, Advisory::SafeToCreate { .. })),
            "PostService should be safe to create in domain, got: {:?}",
            advisories
        );

        // HttpClient SHOULD trigger a violation (matches "http" keyword)
        let advisories = validate_boundary(&conn, "HttpClient", "domain").unwrap();
        assert!(
            advisories.iter().any(|a| matches!(a, Advisory::BoundaryViolation { .. })),
            "HttpClient should trigger boundary violation in domain, got: {:?}",
            advisories
        );
    }
}
