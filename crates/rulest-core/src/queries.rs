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
        advisories.push(Advisory::UseExistingType {
            existing: existing.clone(),
            prelude_path: format!("{}::{}", existing.crate_name, existing.name),
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

    for (_id, crate_name, description, kind) in &rules {
        if kind == "must_not" {
            advisories.push(Advisory::BoundaryViolation {
                rule: description.clone(),
                crate_name: crate_name.clone(),
                suggestion: ModuleSuggestion {
                    module_path: String::new(),
                    crate_name: String::new(),
                    reason: format!(
                        "Crate '{}' has a must_not rule: {}. Consider placing '{}' elsewhere.",
                        crate_name, description, symbol_name
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
            "SELECT s.name FROM symbols s
             JOIN modules m ON s.module_id = m.id
             WHERE m.path LIKE ?1 AND s.status IN ('planned', 'wip')",
        )
        .unwrap();

    let symbols: Vec<String> = stmt
        .query_map(params![format!("%{}%", module_path)], |row| row.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    if !symbols.is_empty() {
        advisories.push(Advisory::WipConflict {
            agent: "unknown".to_string(),
            branch: None,
            symbols,
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
            advisories.push(Advisory::ReuseWithPattern {
                trait_name: existing.name.clone(),
                call_pattern: format!("impl {} for YourType", existing.name),
                example: format!(
                    "See {}::{} for the trait definition",
                    existing.crate_name, existing.module_path
                ),
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
            "SELECT s.name, s.kind, m.path, c.name, s.signature, s.visibility
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
        })
    })
    .unwrap()
    .filter_map(|r| r.ok())
    .collect()
}

fn find_symbols_fuzzy(conn: &Connection, pattern: &str) -> Vec<ExistingSymbol> {
    let mut stmt = conn
        .prepare(
            "SELECT s.name, s.kind, m.path, c.name, s.signature, s.visibility
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
        })
    })
    .unwrap()
    .filter_map(|r| r.ok())
    .collect()
}

fn find_type_symbols(conn: &Connection, type_name: &str) -> Vec<ExistingSymbol> {
    let mut stmt = conn
        .prepare(
            "SELECT s.name, s.kind, m.path, c.name, s.signature, s.visibility
             FROM symbols s
             JOIN modules m ON s.module_id = m.id
             JOIN crates c ON m.crate_id = c.id
             WHERE s.name = ?1 AND s.kind IN ('struct', 'enum', 'trait', 'type_alias')",
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
}
