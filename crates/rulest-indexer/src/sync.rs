use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

use rulest_core::models::{Module, Symbol, SymbolStatus};
use rulest_core::registry;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

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
}

/// Persistent sync state tracking file mtimes.
#[derive(Debug, Default, Serialize, Deserialize)]
struct SyncLog {
    files: HashMap<String, u64>, // path -> mtime as secs since epoch
}

impl SyncLog {
    fn load(path: &Path) -> Self {
        if let Ok(contents) = fs::read_to_string(path) {
            serde_json::from_str(&contents).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize sync log: {}", e))?;
        fs::write(path, json).map_err(|e| format!("Failed to write sync log: {}", e))
    }

    fn needs_sync(&self, file_path: &str, current_mtime: u64) -> bool {
        match self.files.get(file_path) {
            Some(&stored_mtime) => current_mtime > stored_mtime,
            None => true,
        }
    }
}

/// Perform a full or incremental sync of a workspace into the registry.
pub fn sync_workspace(
    conn: &Connection,
    workspace_root: &Path,
    architect_dir: &Path,
    force_full: bool,
) -> Result<SyncStats, String> {
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

            // Check mtime for incremental sync
            let mtime = file_mtime(&file_path);
            if !sync_log.needs_sync(&module_info.path, mtime) {
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

            // Delete existing symbols for this module (will re-insert)
            let removed = registry::delete_symbols_for_module(conn, module_id)
                .map_err(|e| format!("Failed to clear symbols: {}", e))?;
            stats.symbols_removed += removed;

            // Extract symbols from source
            match extractor::extract_symbols(&file_path) {
                Ok(extracted) => {
                    for sym in extracted.symbols {
                        let symbol = Symbol {
                            id: None,
                            module_id,
                            name: sym.name,
                            kind: sym.kind,
                            visibility: sym.visibility,
                            signature: sym.signature,
                            status: SymbolStatus::Stable,
                        };
                        registry::insert_symbol(conn, &symbol)
                            .map_err(|e| format!("Failed to insert symbol: {}", e))?;
                        stats.symbols_added += 1;
                    }
                }
                Err(e) => {
                    eprintln!("Warning: {}", e);
                }
            }

            // Update sync log
            sync_log.files.insert(module_info.path.clone(), mtime);
        }
    }

    sync_log.save(&sync_log_path)?;
    Ok(stats)
}

fn file_mtime(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
