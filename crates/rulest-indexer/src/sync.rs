use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rulest_core::models::{Module, Relationship, RelationshipKind, Symbol, SymbolKind, SymbolStatus, Visibility};
use rulest_core::registry;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cargo_meta;
use crate::extractor;

/// Stats from a sync operation.
#[derive(Debug, Default)]
pub struct SyncStats {
    pub crates_found: usize,
    pub modules_scanned: usize,
    pub symbols_added: usize,
    pub symbols_updated: usize,
    pub symbols_removed: usize,
    pub modules_skipped: usize,
    /// Parse errors encountered: `(file_path, error_message)`.
    pub parse_errors: Vec<(String, String)>,
}

/// Persistent sync state tracking file content hashes.
#[derive(Debug, Default, Serialize, Deserialize)]
struct SyncLog {
    files: HashMap<String, String>, // path -> SHA-256 hex digest
}

impl SyncLog {
    fn load(path: &Path) -> Self {
        if let Ok(contents) = fs::read_to_string(path) {
            match serde_json::from_str(&contents) {
                Ok(log) => log,
                Err(e) => {
                    eprintln!("Warning: corrupted sync.log ({}), triggering full resync", e);
                    Self::default()
                }
            }
        } else {
            Self::default()
        }
    }

    fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize sync log: {}", e))?;
        fs::write(path, json).map_err(|e| format!("Failed to write sync log: {}", e))
    }

    fn needs_sync(&self, file_path: &str, current_hash: &str) -> bool {
        match self.files.get(file_path) {
            Some(stored_hash) => stored_hash != current_hash,
            None => true,
        }
    }
}

/// File-based lock to prevent concurrent sync operations.
///
/// Creates `.architect/sync.lock` containing the current PID. On acquire,
/// checks whether an existing lock is held by a live process. Locks older
/// than 10 minutes are treated as stale and reclaimed. The lock file is
/// automatically removed when the guard is dropped (RAII).
struct SyncLock {
    path: PathBuf,
}

/// Maximum age (in seconds) before a lock is considered stale.
const LOCK_STALE_SECS: u64 = 600; // 10 minutes

impl SyncLock {
    /// Attempt to acquire the sync lock.
    ///
    /// Returns `Err` if another sync process is actively holding the lock.
    fn acquire(architect_dir: &Path) -> Result<Self, String> {
        let lock_path = architect_dir.join("sync.lock");

        if lock_path.exists() {
            // Read existing lock
            let contents = fs::read_to_string(&lock_path).map_err(|e| {
                let context = match e.kind() {
                    std::io::ErrorKind::PermissionDenied => "permission denied",
                    _ => "I/O error",
                };
                format!("Failed to read lock file ({}): {}", context, e)
            })?;
            let existing_pid: u32 = match contents.trim().parse() {
                Ok(pid) => pid,
                Err(_) => {
                    eprintln!("Warning: corrupted lock file (non-numeric PID), treating as stale");
                    0
                }
            };

            // Check if the lock is stale by age
            let is_stale = match fs::metadata(&lock_path) {
                Ok(meta) => match meta.modified() {
                    Ok(modified) => match SystemTime::now().duration_since(modified) {
                        Ok(age) => age.as_secs() > LOCK_STALE_SECS,
                        Err(_) => false,
                    },
                    Err(_) => false,
                },
                Err(_) => false,
            };

            if !is_stale && existing_pid != 0 && is_process_alive(existing_pid) {
                return Err(format!(
                    "Another sync process (PID {}) is currently running. \
                     If this is incorrect, remove {}",
                    existing_pid,
                    lock_path.display()
                ));
            }
            // Lock is stale or owner is dead — reclaim it
        }

        // Write our PID to the lock file
        let pid = std::process::id();
        fs::write(&lock_path, pid.to_string())
            .map_err(|e| format!("Failed to create lock file: {}", e))?;

        Ok(SyncLock { path: lock_path })
    }
}

impl Drop for SyncLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Check whether a process with the given PID is alive.
///
/// Uses `kill -0` on Unix-like systems (macOS, Linux) to probe without
/// sending a signal. Falls back to assuming alive on other platforms
/// (the stale-time check provides the safety net).
fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use std::process::Command;
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        // On non-Unix platforms, rely on the stale-time heuristic.
        let _ = pid;
        true
    }
}

/// Perform a full or incremental sync of a workspace into the registry.
pub fn sync_workspace(
    conn: &Connection,
    workspace_root: &Path,
    architect_dir: &Path,
    force_full: bool,
) -> Result<SyncStats, String> {
    let _lock = SyncLock::acquire(architect_dir)?;

    let sync_log_path = architect_dir.join("sync.log");
    let mut sync_log = if force_full {
        SyncLog::default()
    } else {
        SyncLog::load(&sync_log_path)
    };

    let workspace_info = cargo_meta::extract_workspace(workspace_root)?;
    let mut stats = SyncStats {
        crates_found: workspace_info.crates.len(),
        ..SyncStats::default()
    };

    // Upsert crates
    let mut crate_ids: HashMap<String, i64> = HashMap::new();
    for c in &workspace_info.crates {
        let id = registry::insert_crate(conn, c)
            .map_err(|e| format!("Failed to insert crate '{}': {}", c.name, e))?;
        crate_ids.insert(c.name.clone(), id);
    }

    // Persist cross-crate workspace dependencies.
    // Clear existing deps first (full sync replaces them all).
    registry::clear_crate_dependencies(conn)
        .map_err(|e| format!("Failed to clear crate dependencies: {}", e))?;
    for (from_crate, to_crate) in &workspace_info.dependencies {
        if let (Some(&from_id), Some(&to_id)) =
            (crate_ids.get(from_crate), crate_ids.get(to_crate))
        {
            registry::insert_crate_dependency(conn, from_id, to_id)
                .map_err(|e| {
                    format!(
                        "Failed to insert crate dependency {} -> {}: {}",
                        from_crate, to_crate, e
                    )
                })?;
        }
    }

    // Collect all trait_impls across files for a second pass after all symbols are inserted
    let mut all_trait_impls: Vec<(String, String)> = Vec::new();

    // Process each crate's modules
    for (crate_name, module_infos) in &workspace_info.modules {
        let crate_id = match crate_ids.get(crate_name) {
            Some(&id) => id,
            None => continue,
        };

        for module_info in module_infos {
            // Resolve the actual file path
            let file_path = workspace_root
                .parent()
                .unwrap_or(workspace_root)
                .join(&module_info.path);

            if !file_path.exists() {
                continue;
            }

            // Check content hash for incremental sync
            let content_hash = match file_content_hash(&file_path) {
                Ok(hash) => hash,
                Err(e) => {
                    stats.parse_errors.push((module_info.path.clone(), e));
                    continue;
                }
            };
            if !sync_log.needs_sync(&module_info.path, &content_hash) {
                stats.modules_skipped += 1;
                continue;
            }

            stats.modules_scanned += 1;

            // Upsert module
            let module = Module {
                id: None,
                crate_id,
                path: module_info.path.clone(),
                name: module_info.name.clone(),
            };
            let module_id = registry::insert_module(conn, &module)
                .map_err(|e| format!("Failed to insert module '{}': {}", module_info.path, e))?;

            // Preserve planned/wip symbols before clearing
            let preserved_symbols = query_planned_wip_symbols(conn, module_id)
                .map_err(|e| format!("Failed to query planned/wip symbols: {}", e))?;

            // Delete existing symbols for this module (will re-insert)
            let removed = registry::delete_symbols_for_module(conn, module_id)
                .map_err(|e| format!("Failed to clear symbols: {}", e))?;
            stats.symbols_removed += removed;

            // Extract symbols from source
            let mut extracted_names: HashSet<String> = HashSet::new();
            match extractor::extract_symbols(&file_path) {
                Ok(extracted) => {
                    for sym in extracted.symbols {
                        extracted_names.insert(sym.name.clone());
                        let symbol = Symbol {
                            id: None,
                            module_id,
                            name: sym.name,
                            kind: sym.kind,
                            visibility: sym.visibility,
                            signature: sym.signature,
                            line_number: sym.line_number,
                            scope: sym.scope,
                            status: SymbolStatus::Stable,
                            created_by: None,
                            created_at: None,
                            updated_at: None,
                        };
                        registry::insert_symbol(conn, &symbol)
                            .map_err(|e| format!("Failed to insert symbol: {}", e))?;
                        stats.symbols_added += 1;
                    }

                    // Collect trait implementation relationships for second pass
                    all_trait_impls.extend(extracted.trait_impls);
                }
                Err(e) => {
                    stats.parse_errors.push((module_info.path.clone(), e));
                }
            }

            // Re-insert planned/wip symbols that haven't been implemented yet
            for sym in &preserved_symbols {
                if !extracted_names.contains(&sym.name) {
                    registry::insert_symbol(conn, sym)
                        .map_err(|e| format!("Failed to re-insert planned/wip symbol: {}", e))?;
                    // These symbols were removed then re-added, net zero change;
                    // adjust stats to not count them as removed.
                    stats.symbols_removed = stats.symbols_removed.saturating_sub(1);
                }
            }

            // Update sync log only if extraction succeeded (avoid caching partial/failed parses)
            let final_hash = match file_content_hash(&file_path) {
                Ok(h) => h,
                Err(_) => content_hash.clone(),
            };
            if final_hash == content_hash {
                sync_log.files.insert(module_info.path.clone(), content_hash);
            }
            // If hash changed between first read and now, don't cache — next sync will re-process
        }
    }

    // Second pass: persist trait implementation relationships.
    // Now that all symbols are inserted, we can look up trait and type symbol IDs.
    for (trait_name, type_name) in &all_trait_impls {
        let trait_id = registry::find_symbol_id_by_name_and_kind(conn, trait_name, "trait")
            .map_err(|e| format!("Failed to look up trait '{}': {}", trait_name, e))?;
        let type_id = registry::find_symbol_id_by_name(conn, type_name)
            .map_err(|e| format!("Failed to look up type '{}': {}", type_name, e))?;

        if let (Some(trait_sym_id), Some(type_sym_id)) = (trait_id, type_id) {
            let relationship = Relationship {
                id: None,
                from_symbol_id: type_sym_id,
                to_symbol_id: trait_sym_id,
                kind: RelationshipKind::Implements,
            };
            registry::insert_relationship(conn, &relationship)
                .map_err(|e| {
                    format!(
                        "Failed to insert trait impl relationship ({} -> {}): {}",
                        type_name, trait_name, e
                    )
                })?;
        }
    }

    sync_log.save(&sync_log_path)?;
    Ok(stats)
}

fn file_content_hash(path: &Path) -> Result<String, String> {
    let contents = fs::read(path).map_err(|e| {
        let context = match e.kind() {
            std::io::ErrorKind::PermissionDenied => "permission denied",
            std::io::ErrorKind::NotFound => "file not found",
            _ => "I/O error",
        };
        format!("Failed to read {} ({}): {}", path.display(), context, e)
    })?;
    let mut hasher = Sha256::new();
    hasher.update(&contents);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Query all planned/wip symbols for a module so they can be preserved across sync.
fn query_planned_wip_symbols(
    conn: &Connection,
    module_id: i64,
) -> Result<Vec<Symbol>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, module_id, name, kind, visibility, signature, line_number, scope, status, created_by, created_at, updated_at \
         FROM symbols WHERE module_id = ?1 AND status IN ('planned', 'wip')",
    )?;
    let rows = stmt.query_map(rusqlite::params![module_id], |row| {
        let kind_str: String = row.get(3)?;
        let vis_str: String = row.get(4)?;
        let status_str: String = row.get(8)?;
        Ok(Symbol {
            id: None, // Will be re-inserted with a new id
            module_id: row.get(1)?,
            name: row.get(2)?,
            kind: kind_str
                .parse::<SymbolKind>()
                .unwrap_or(SymbolKind::Function),
            visibility: vis_str
                .parse::<Visibility>()
                .unwrap_or(Visibility::Private),
            signature: row.get(5)?,
            line_number: row.get(6)?,
            scope: row.get(7)?,
            status: status_str
                .parse::<SymbolStatus>()
                .unwrap_or(SymbolStatus::Planned),
            created_by: row.get(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
        })
    })?;
    rows.collect()
}
