use std::path::Path;

use rusqlite::{Connection, Result as SqlResult};

#[allow(unused_imports)]
use crate::models::*;

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
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS symbols (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    module_id INTEGER NOT NULL REFERENCES modules(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'private',
    signature TEXT,
    status TEXT NOT NULL DEFAULT 'stable',
    created_by TEXT,
    created_at TEXT
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

CREATE TABLE IF NOT EXISTS ownership_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    crate_name TEXT NOT NULL,
    description TEXT NOT NULL,
    kind TEXT NOT NULL
);
"#;

/// Open or create a registry database at the given path.
pub fn open_registry(path: &Path) -> SqlResult<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    Ok(conn)
}

/// Create the schema tables if they don't exist.
pub fn create_schema(conn: &Connection) -> SqlResult<()> {
    conn.execute_batch(SCHEMA_SQL)
}

/// Insert a crate, returning its id.
pub fn insert_crate(conn: &Connection, c: &Crate) -> SqlResult<i64> {
    conn.execute(
        "INSERT OR REPLACE INTO crates (name, path, description) VALUES (?1, ?2, ?3)",
        rusqlite::params![c.name, c.path, c.description],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Insert a module, returning its id.
pub fn insert_module(conn: &Connection, m: &Module) -> SqlResult<i64> {
    conn.execute(
        "INSERT OR REPLACE INTO modules (crate_id, path, name) VALUES (?1, ?2, ?3)",
        rusqlite::params![m.crate_id, m.path, m.name],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Insert a symbol, returning its id.
pub fn insert_symbol(conn: &Connection, s: &Symbol) -> SqlResult<i64> {
    conn.execute(
        "INSERT INTO symbols (module_id, name, kind, visibility, signature, status, created_by, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            s.module_id,
            s.name,
            s.kind.as_str(),
            s.visibility.as_str(),
            s.signature,
            s.status.as_str(),
            s.created_by,
            s.created_at,
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
            "UPDATE symbols SET visibility = ?1, signature = ?2, status = ?3 WHERE id = ?4",
            rusqlite::params![s.visibility.as_str(), s.signature, s.status.as_str(), id],
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
        "INSERT INTO ownership_rules (crate_name, description, kind) VALUES (?1, ?2, ?3)",
        rusqlite::params![r.crate_name, r.description, r.kind.as_str()],
    )?;
    Ok(conn.last_insert_rowid())
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
    let mut stmt = conn.prepare("SELECT id, crate_id, path, name FROM modules WHERE path = ?1")?;
    let result = stmt
        .query_row(rusqlite::params![path], |row| {
            Ok(Module {
                id: Some(row.get(0)?),
                crate_id: row.get(1)?,
                path: row.get(2)?,
                name: row.get(3)?,
            })
        })
        .ok();
    Ok(result)
}

/// Find a crate by name.
pub fn find_crate_by_name(conn: &Connection, name: &str) -> SqlResult<Option<Crate>> {
    let mut stmt =
        conn.prepare("SELECT id, name, path, description FROM crates WHERE name = ?1")?;
    let result = stmt
        .query_row(rusqlite::params![name], |row| {
            Ok(Crate {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                path: row.get(2)?,
                description: row.get(3)?,
            })
        })
        .ok();
    Ok(result)
}

/// Execute raw SQL (for seed.sql imports).
pub fn execute_sql(conn: &Connection, sql: &str) -> SqlResult<()> {
    conn.execute_batch(sql)
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
        };
        let crate_id = insert_crate(&conn, &c).unwrap();

        let m = Module {
            id: None,
            crate_id,
            path: "src/lib.rs".to_string(),
            name: "lib".to_string(),
        };
        let module_id = insert_module(&conn, &m).unwrap();

        let s = Symbol {
            id: None,
            module_id,
            name: "foo".to_string(),
            kind: SymbolKind::Function,
            visibility: Visibility::Public,
            signature: Some("fn foo() -> i32".to_string()),
            status: SymbolStatus::Planned,
            created_by: None,
            created_at: None,
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
}
