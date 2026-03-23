use std::path::Path;

use rusqlite::{Connection, Result as SqlResult};

#[allow(unused_imports)]
use crate::models::*;

/// Current schema version. Set to 4 for multi-language module support.
/// Versions 0 and 1 are considered pre-migration databases.
pub const SCHEMA_VERSION: i32 = 4;

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS crates (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    path TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    bounded_context TEXT
);

CREATE TABLE IF NOT EXISTS modules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    crate_id INTEGER NOT NULL REFERENCES crates(id) ON DELETE CASCADE,
    path TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    language TEXT NOT NULL DEFAULT 'rust'
);

-- NOTE: The article schema includes purpose/status on modules, but we omit these
-- because module metadata is better captured at the crate level (crates.description)
-- and symbol level (symbols.status). Modules are structural containers, not semantic units.

CREATE TABLE IF NOT EXISTS symbols (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    module_id INTEGER NOT NULL REFERENCES modules(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'private',
    signature TEXT,
    line_number INTEGER,
    scope TEXT,
    status TEXT NOT NULL DEFAULT 'stable',
    created_by TEXT,
    created_at TEXT,
    updated_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
CREATE INDEX IF NOT EXISTS idx_symbols_kind ON symbols(kind);
CREATE INDEX IF NOT EXISTS idx_symbols_module ON symbols(module_id);

CREATE TABLE IF NOT EXISTS relationships (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_symbol_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
    to_symbol_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
    kind TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS contracts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    symbol_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    description TEXT NOT NULL
);

-- NOTE: The article schema uses CONTRACT (trait_name, implementor, module_path) for
-- trait-specific contracts. Our schema generalizes this: trait implementations are
-- captured in the relationships table (kind='implements'), while the contracts table
-- stores broader invariants (preconditions, postconditions, invariants) attached to
-- any symbol. This is intentionally more flexible than the article's design.

CREATE TABLE IF NOT EXISTS ownership_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    crate_name TEXT NOT NULL,
    description TEXT NOT NULL,
    kind TEXT NOT NULL,
    pattern TEXT,
    regex TEXT
);

CREATE TABLE IF NOT EXISTS linked_registries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    path TEXT NOT NULL,
    linked_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS linked_symbols (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_name TEXT NOT NULL,
    name TEXT NOT NULL,
    kind TEXT,
    crate_name TEXT,
    module_path TEXT,
    signature TEXT,
    linked_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_linked_name ON linked_symbols(name);

CREATE TABLE IF NOT EXISTS crate_dependencies (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_crate_id INTEGER NOT NULL REFERENCES crates(id) ON DELETE CASCADE,
    to_crate_id INTEGER NOT NULL REFERENCES crates(id) ON DELETE CASCADE,
    UNIQUE(from_crate_id, to_crate_id)
);
"#;

/// Open or create a registry database at the given path.
pub fn open_registry(path: &Path) -> SqlResult<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    Ok(conn)
}

/// Read the current schema version from the database using `PRAGMA user_version`.
pub fn get_schema_version(conn: &Connection) -> SqlResult<i32> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

/// Set the schema version in the database using `PRAGMA user_version`.
pub fn set_schema_version(conn: &Connection, version: i32) -> SqlResult<()> {
    // PRAGMA statements don't support parameter binding, so we format directly.
    // The version is an i32 so there is no injection risk.
    conn.execute_batch(&format!("PRAGMA user_version = {version}"))
}

/// Create or migrate the schema to the current version.
///
/// - If the database is fresh (user_version == 0): runs full SCHEMA_SQL and sets version.
/// - If the database is behind: runs incremental migrations.
/// - If already current: does nothing.
/// - If ahead of this binary: returns an error.
pub fn create_schema(conn: &Connection) -> SqlResult<()> {
    let current = get_schema_version(conn)?;

    if current == 0 {
        // Fresh database or pre-migration database: apply full schema
        conn.execute_batch(SCHEMA_SQL)?;
        set_schema_version(conn, SCHEMA_VERSION)?;
    } else if current < SCHEMA_VERSION {
        // Existing database that needs migration
        migrate(conn, current)?;
        set_schema_version(conn, SCHEMA_VERSION)?;
    } else if current == SCHEMA_VERSION {
        // Already up to date — nothing to do
    } else {
        // Database is from a newer version of rulest
        return Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!(
                "database schema version {current} is newer than supported version {SCHEMA_VERSION}"
            )),
        ));
    }

    Ok(())
}

/// Run migrations sequentially from `from_version` up to SCHEMA_VERSION.
fn migrate(conn: &Connection, from_version: i32) -> SqlResult<()> {
    if from_version < 2 {
        migrate_to_v2(conn)?;
    }
    if from_version < 3 {
        migrate_to_v3(conn)?;
    }
    if from_version < 4 {
        migrate_to_v4(conn)?;
    }
    Ok(())
}

/// Migration to v2: establishes migration framework.
/// This is a no-op since the actual table schema hasn't changed —
/// it just marks the database as migration-aware.
fn migrate_to_v2(conn: &Connection) -> SqlResult<()> {
    // Ensure all tables exist (idempotent due to IF NOT EXISTS)
    conn.execute_batch(SCHEMA_SQL)
}

/// Migration to v3: adds pattern/regex columns to ownership_rules,
/// linked_registries and linked_symbols tables.
fn migrate_to_v3(conn: &Connection) -> SqlResult<()> {
    // Add pattern/regex columns (ignore error if they already exist)
    let _ = conn.execute_batch("ALTER TABLE ownership_rules ADD COLUMN pattern TEXT");
    let _ = conn.execute_batch("ALTER TABLE ownership_rules ADD COLUMN regex TEXT");
    // Create new tables (IF NOT EXISTS is idempotent)
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS linked_registries (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            path TEXT NOT NULL,
            linked_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS linked_symbols (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_name TEXT NOT NULL,
            name TEXT NOT NULL,
            kind TEXT,
            crate_name TEXT,
            module_path TEXT,
            signature TEXT,
            linked_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_linked_name ON linked_symbols(name);"
    )
}

/// Migration to v4: adds language column to modules table.
fn migrate_to_v4(conn: &Connection) -> SqlResult<()> {
    let _ = conn.execute_batch(
        "ALTER TABLE modules ADD COLUMN language TEXT NOT NULL DEFAULT 'rust'"
    );
    Ok(())
}

/// Insert a crate, returning its id.
pub fn insert_crate(conn: &Connection, c: &Crate) -> SqlResult<i64> {
    conn.execute(
        "INSERT OR REPLACE INTO crates (name, path, description, bounded_context) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![c.name, c.path, c.description, c.bounded_context],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Insert a module, returning its id.
pub fn insert_module(conn: &Connection, m: &Module) -> SqlResult<i64> {
    conn.execute(
        "INSERT OR REPLACE INTO modules (crate_id, path, name, language) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![m.crate_id, m.path, m.name, m.language.as_str()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Insert a symbol, returning its id.
pub fn insert_symbol(conn: &Connection, s: &Symbol) -> SqlResult<i64> {
    conn.execute(
        "INSERT INTO symbols (module_id, name, kind, visibility, signature, line_number, scope, status, created_by, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            s.module_id,
            s.name,
            s.kind.as_str(),
            s.visibility.as_str(),
            s.signature,
            s.line_number,
            s.scope,
            s.status.as_str(),
            s.created_by,
            s.created_at,
            s.updated_at,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Upsert a symbol by module_id + name + kind. Updates signature/visibility/status if exists.
pub fn upsert_symbol(conn: &Connection, s: &Symbol) -> SqlResult<i64> {
    // Try to find existing
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM symbols WHERE module_id = ?1 AND name = ?2 AND kind = ?3",
            rusqlite::params![s.module_id, s.name, s.kind.as_str()],
            |row| row.get(0),
        )
        .ok();

    if let Some(id) = existing {
        conn.execute(
            "UPDATE symbols SET visibility = ?1, signature = ?2, line_number = ?3, scope = ?4, status = ?5, updated_at = ?6 WHERE id = ?7",
            rusqlite::params![s.visibility.as_str(), s.signature, s.line_number, s.scope, s.status.as_str(), s.updated_at, id],
        )?;
        Ok(id)
    } else {
        insert_symbol(conn, s)
    }
}

/// Insert a relationship.
pub fn insert_relationship(conn: &Connection, r: &Relationship) -> SqlResult<i64> {
    conn.execute(
        "INSERT INTO relationships (from_symbol_id, to_symbol_id, kind) VALUES (?1, ?2, ?3)",
        rusqlite::params![r.from_symbol_id, r.to_symbol_id, r.kind.as_str()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Insert a contract.
pub fn insert_contract(conn: &Connection, c: &Contract) -> SqlResult<i64> {
    conn.execute(
        "INSERT INTO contracts (symbol_id, kind, description) VALUES (?1, ?2, ?3)",
        rusqlite::params![c.symbol_id, c.kind.as_str(), c.description],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Insert an ownership rule.
pub fn insert_ownership_rule(conn: &Connection, r: &OwnershipRule) -> SqlResult<i64> {
    conn.execute(
        "INSERT INTO ownership_rules (crate_name, description, kind, pattern, regex) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![r.crate_name, r.description, r.kind.as_str(), r.pattern, r.regex],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Insert a linked registry entry.
pub fn insert_linked_registry(conn: &Connection, r: &LinkedRegistry) -> SqlResult<i64> {
    conn.execute(
        "INSERT OR REPLACE INTO linked_registries (name, path, linked_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![r.name, r.path, r.linked_at],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Remove a linked registry and its symbols.
pub fn remove_linked_registry(conn: &Connection, name: &str) -> SqlResult<()> {
    conn.execute("DELETE FROM linked_symbols WHERE source_name = ?1", rusqlite::params![name])?;
    conn.execute("DELETE FROM linked_registries WHERE name = ?1", rusqlite::params![name])?;
    Ok(())
}

/// List all linked registries.
pub fn list_linked_registries(conn: &Connection) -> SqlResult<Vec<LinkedRegistry>> {
    let mut stmt = conn.prepare("SELECT id, name, path, linked_at FROM linked_registries")?;
    let rows = stmt.query_map([], |row| {
        Ok(LinkedRegistry {
            id: Some(row.get(0)?),
            name: row.get(1)?,
            path: row.get(2)?,
            linked_at: row.get(3)?,
        })
    })?;
    rows.collect()
}

/// Insert a linked symbol from an external registry.
pub fn insert_linked_symbol(conn: &Connection, s: &LinkedSymbol) -> SqlResult<i64> {
    conn.execute(
        "INSERT INTO linked_symbols (source_name, name, kind, crate_name, module_path, signature, linked_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![s.source_name, s.name, s.kind, s.crate_name, s.module_path, s.signature, s.linked_at],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Clear all linked symbols from a given source.
pub fn clear_linked_symbols(conn: &Connection, source_name: &str) -> SqlResult<()> {
    conn.execute("DELETE FROM linked_symbols WHERE source_name = ?1", rusqlite::params![source_name])?;
    Ok(())
}

/// Query all public symbols from a registry (for cross-repo linking).
/// Returns (name, kind, module_path, crate_name, signature).
pub fn query_public_symbols(conn: &Connection) -> SqlResult<Vec<(String, String, String, String, Option<String>)>> {
    let mut stmt = conn.prepare(
        "SELECT s.name, s.kind, m.path, c.name, s.signature
         FROM symbols s
         JOIN modules m ON s.module_id = m.id
         JOIN crates c ON m.crate_id = c.id
         WHERE s.visibility = 'public'"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
    })?;
    rows.collect()
}

/// Insert a crate dependency, ignoring duplicates.
pub fn insert_crate_dependency(
    conn: &Connection,
    from_crate_id: i64,
    to_crate_id: i64,
) -> SqlResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO crate_dependencies (from_crate_id, to_crate_id) VALUES (?1, ?2)",
        rusqlite::params![from_crate_id, to_crate_id],
    )?;
    Ok(())
}

/// Clear all crate dependencies (used during full sync).
pub fn clear_crate_dependencies(conn: &Connection) -> SqlResult<()> {
    conn.execute("DELETE FROM crate_dependencies", [])?;
    Ok(())
}

/// Delete all symbols belonging to a given module.
pub fn delete_symbols_for_module(conn: &Connection, module_id: i64) -> SqlResult<usize> {
    conn.execute(
        "DELETE FROM symbols WHERE module_id = ?1",
        rusqlite::params![module_id],
    )
}

/// Find a module by its file path.
pub fn find_module_by_path(conn: &Connection, path: &str) -> SqlResult<Option<Module>> {
    let mut stmt = conn.prepare("SELECT id, crate_id, path, name, language FROM modules WHERE path = ?1")?;
    let result = stmt
        .query_row(rusqlite::params![path], |row| {
            let lang_str: String = row.get::<_, String>(4).unwrap_or_else(|_| "rust".to_string());
            Ok(Module {
                id: Some(row.get(0)?),
                crate_id: row.get(1)?,
                path: row.get(2)?,
                name: row.get(3)?,
                language: lang_str.parse().unwrap_or(Language::Rust),
            })
        })
        .ok();
    Ok(result)
}

/// Find a crate by name.
pub fn find_crate_by_name(conn: &Connection, name: &str) -> SqlResult<Option<Crate>> {
    let mut stmt =
        conn.prepare("SELECT id, name, path, description, bounded_context FROM crates WHERE name = ?1")?;
    let result = stmt
        .query_row(rusqlite::params![name], |row| {
            Ok(Crate {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                path: row.get(2)?,
                description: row.get(3)?,
                bounded_context: row.get(4)?,
            })
        })
        .ok();
    Ok(result)
}

/// Find the first symbol ID matching a name and kind.
pub fn find_symbol_id_by_name_and_kind(
    conn: &Connection,
    name: &str,
    kind: &str,
) -> SqlResult<Option<i64>> {
    let result = conn
        .query_row(
            "SELECT id FROM symbols WHERE name = ?1 AND kind = ?2 LIMIT 1",
            rusqlite::params![name, kind],
            |row| row.get(0),
        )
        .ok();
    Ok(result)
}

/// Find the first symbol ID matching a name (any kind).
pub fn find_symbol_id_by_name(conn: &Connection, name: &str) -> SqlResult<Option<i64>> {
    let result = conn
        .query_row(
            "SELECT id FROM symbols WHERE name = ?1 LIMIT 1",
            rusqlite::params![name],
            |row| row.get(0),
        )
        .ok();
    Ok(result)
}

/// Execute validated seed SQL (for seed.sql imports).
///
/// Only allows:
/// - `INSERT INTO ownership_rules ...` statements
/// - SQL comments (`--` and `/* ... */`)
/// - Empty/whitespace-only lines
///
/// Rejects any other statement (DROP, DELETE, UPDATE, ALTER, ATTACH, PRAGMA, CREATE,
/// or INSERTs targeting tables other than `ownership_rules`).
pub fn execute_seed_sql(conn: &Connection, sql: &str) -> SqlResult<()> {
    let mut rejected = Vec::new();

    for raw_stmt in sql.split(';') {
        let stmt = strip_comments(raw_stmt).trim().to_string();
        if stmt.is_empty() {
            continue;
        }
        let upper = stmt.to_uppercase();
        let first_word = upper.split_whitespace().next().unwrap_or("");

        match first_word {
            "INSERT" => {
                let normalised: String = upper.split_whitespace().collect::<Vec<_>>().join(" ");
                if !normalised.starts_with("INSERT INTO OWNERSHIP_RULES") {
                    rejected.push(stmt);
                }
            }
            _ => {
                rejected.push(stmt);
            }
        }
    }

    if !rejected.is_empty() {
        let msg = format!(
            "seed.sql contains {} rejected statement(s):\n{}",
            rejected.len(),
            rejected
                .iter()
                .enumerate()
                .map(|(i, s)| format!("  {}: {}", i + 1, s))
                .collect::<Vec<_>>()
                .join("\n")
        );
        return Err(rusqlite::Error::InvalidParameterName(msg));
    }

    for raw_stmt in sql.split(';') {
        let stmt = strip_comments(raw_stmt).trim().to_string();
        if stmt.is_empty() {
            continue;
        }
        conn.execute_batch(&stmt)?;
    }

    Ok(())
}

/// Remove SQL comments (both `--` line comments and `/* ... */` block comments)
/// from a SQL fragment, preserving string literals.
fn strip_comments(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len());
    let chars: Vec<char> = sql.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '\'' {
            result.push(chars[i]);
            i += 1;
            while i < len {
                result.push(chars[i]);
                if chars[i] == '\'' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if chars[i] == '-' && i + 1 < len && chars[i + 1] == '-' {
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if chars[i] == '/' && i + 1 < len && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_creation() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        // Verify tables exist by querying them
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM crates", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_insert_and_find_crate() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        let c = Crate {
            id: None,
            name: "my-crate".to_string(),
            path: "/path/to/crate".to_string(),
            description: Some("A test crate".to_string()),
            bounded_context: None,
        };
        let id = insert_crate(&conn, &c).unwrap();
        assert!(id > 0);

        let found = find_crate_by_name(&conn, "my-crate").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "my-crate");
    }

    #[test]
    fn test_upsert_symbol() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        let c = Crate {
            id: None,
            name: "test".to_string(),
            path: ".".to_string(),
            description: None,
            bounded_context: None,
        };
        let crate_id = insert_crate(&conn, &c).unwrap();

        let m = Module {
            id: None,
            crate_id,
            path: "src/lib.rs".to_string(),
            name: "lib".to_string(),
            language: Language::Rust,
        };
        let module_id = insert_module(&conn, &m).unwrap();

        let s = Symbol {
            id: None,
            module_id,
            name: "foo".to_string(),
            kind: SymbolKind::Function,
            visibility: Visibility::Public,
            signature: Some("fn foo() -> i32".to_string()),
            line_number: None,
            scope: None,
            status: SymbolStatus::Planned,
            created_by: None,
            created_at: None,
            updated_at: None,
        };
        let id1 = upsert_symbol(&conn, &s).unwrap();

        // Upsert same symbol with updated status
        let s2 = Symbol {
            status: SymbolStatus::Stable,
            ..s.clone()
        };
        let id2 = upsert_symbol(&conn, &s2).unwrap();
        assert_eq!(id1, id2);

        // Verify status was updated
        let status: String = conn
            .query_row(
                "SELECT status FROM symbols WHERE id = ?1",
                rusqlite::params![id1],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "stable");
    }

    #[test]
    fn test_seed_sql_valid_insert() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        let sql = "INSERT INTO ownership_rules (crate_name, description, kind) VALUES ('my-crate', 'owns core logic', 'must_own');";
        execute_seed_sql(&conn, sql).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM ownership_rules", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_seed_sql_rejects_drop() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        let sql = "DROP TABLE ownership_rules;";
        let result = execute_seed_sql(&conn, sql);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("rejected"), "error should mention rejected statements: {err_msg}");
    }

    #[test]
    fn test_seed_sql_rejects_delete() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        let sql = "DELETE FROM ownership_rules;";
        let result = execute_seed_sql(&conn, sql);
        assert!(result.is_err());
    }

    #[test]
    fn test_seed_sql_rejects_update() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        let sql = "UPDATE ownership_rules SET kind = 'shared_with' WHERE id = 1;";
        let result = execute_seed_sql(&conn, sql);
        assert!(result.is_err());
    }

    #[test]
    fn test_seed_sql_allows_comments_and_whitespace() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        let sql = r#"
-- This is a comment
/* Block comment */

INSERT INTO ownership_rules (crate_name, description, kind) VALUES ('a', 'rule a', 'must_own');

-- Another comment
INSERT INTO ownership_rules (crate_name, description, kind) VALUES ('b', 'rule b', 'must_not');
"#;
        execute_seed_sql(&conn, sql).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM ownership_rules", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_seed_sql_rejects_insert_into_other_table() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        let sql = "INSERT INTO crates (name, path) VALUES ('evil', '/tmp');";
        let result = execute_seed_sql(&conn, sql);
        assert!(result.is_err());
    }

    #[test]
    fn test_fresh_database_gets_schema_version() {
        let conn = Connection::open_in_memory().unwrap();
        assert_eq!(get_schema_version(&conn).unwrap(), 0);

        create_schema(&conn).unwrap();
        assert_eq!(get_schema_version(&conn).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn test_create_schema_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        // Insert some data
        let c = Crate {
            id: None,
            name: "idempotent-test".to_string(),
            path: "/tmp".to_string(),
            description: None,
            bounded_context: None,
        };
        insert_crate(&conn, &c).unwrap();

        // Run create_schema again — should be a no-op
        create_schema(&conn).unwrap();

        // Data should still be there
        let found = find_crate_by_name(&conn, "idempotent-test").unwrap();
        assert!(found.is_some());
        assert_eq!(get_schema_version(&conn).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn test_get_set_schema_version() {
        let conn = Connection::open_in_memory().unwrap();
        assert_eq!(get_schema_version(&conn).unwrap(), 0);

        set_schema_version(&conn, 5).unwrap();
        assert_eq!(get_schema_version(&conn).unwrap(), 5);

        set_schema_version(&conn, 42).unwrap();
        assert_eq!(get_schema_version(&conn).unwrap(), 42);
    }

    #[test]
    fn test_newer_version_returns_error() {
        let conn = Connection::open_in_memory().unwrap();
        set_schema_version(&conn, SCHEMA_VERSION + 1).unwrap();

        let result = create_schema(&conn);
        assert!(result.is_err());
    }

    #[test]
    fn test_migration_from_pre_migration_database() {
        let conn = Connection::open_in_memory().unwrap();
        // Simulate a pre-migration database: tables exist but no user_version set
        conn.execute_batch(SCHEMA_SQL).unwrap();
        assert_eq!(get_schema_version(&conn).unwrap(), 0);

        // create_schema should detect version 0 and set it to SCHEMA_VERSION
        create_schema(&conn).unwrap();
        assert_eq!(get_schema_version(&conn).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn test_migration_from_v1() {
        let conn = Connection::open_in_memory().unwrap();
        // Simulate a v1 database
        conn.execute_batch(SCHEMA_SQL).unwrap();
        set_schema_version(&conn, 1).unwrap();

        // create_schema should migrate from v1 to SCHEMA_VERSION
        create_schema(&conn).unwrap();
        assert_eq!(get_schema_version(&conn).unwrap(), SCHEMA_VERSION);
    }
}
