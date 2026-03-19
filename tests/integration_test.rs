use std::path::PathBuf;

use rulest_core::models::{SymbolKind, Visibility};
use rulest_indexer::extractor::extract_symbols;

/// Return the absolute path to the sample-workspace fixture directory.
fn fixture_path(relative: &str) -> PathBuf {
    let mut base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    base.push("tests");
    base.push("fixtures");
    base.push("sample-workspace");
    base.push(relative);
    base
}

#[test]
fn test_extractor_on_fixture() {
    let path = fixture_path("crates/domain/src/types.rs");
    let extracted = extract_symbols(&path)
        .unwrap_or_else(|e| panic!("extract_symbols failed on {}: {}", path.display(), e));

    // Verify Price struct is extracted
    let price = extracted
        .symbols
        .iter()
        .find(|s| s.name == "Price" && s.kind == SymbolKind::Struct)
        .expect("should extract Price struct");
    assert_eq!(price.visibility, Visibility::Public);
    let sig = price.signature.as_ref().expect("Price should have a signature");
    assert!(
        sig.contains("struct Price"),
        "Price signature should contain 'struct Price', got: {}",
        sig
    );

    // Verify CurrencyFormat trait is extracted
    let trait_sym = extracted
        .symbols
        .iter()
        .find(|s| s.name == "CurrencyFormat" && s.kind == SymbolKind::Trait)
        .expect("should extract CurrencyFormat trait");
    assert_eq!(trait_sym.visibility, Visibility::Public);
    let sig = trait_sym
        .signature
        .as_ref()
        .expect("CurrencyFormat should have a signature");
    assert!(
        sig.contains("trait CurrencyFormat"),
        "CurrencyFormat signature should contain 'trait CurrencyFormat', got: {}",
        sig
    );

    // Verify calculate_fee function is extracted
    let fee_fn = extracted
        .symbols
        .iter()
        .find(|s| s.name == "calculate_fee" && s.kind == SymbolKind::Function)
        .expect("should extract calculate_fee function");
    assert_eq!(fee_fn.visibility, Visibility::Public);
    let sig = fee_fn
        .signature
        .as_ref()
        .expect("calculate_fee should have a signature");
    assert!(
        sig.contains("fn calculate_fee"),
        "calculate_fee signature should contain 'fn calculate_fee', got: {}",
        sig
    );

    // Verify impl CurrencyFormat for Price is captured in trait_impls
    let impl_found = extracted
        .trait_impls
        .iter()
        .any(|(trait_name, type_name)| trait_name == "CurrencyFormat" && type_name == "Price");
    assert!(
        impl_found,
        "should capture impl CurrencyFormat for Price in trait_impls, got: {:?}",
        extracted.trait_impls
    );
}

#[test]
fn test_extractor_pub_use() {
    let path = fixture_path("crates/domain/src/prelude.rs");
    let extracted = extract_symbols(&path)
        .unwrap_or_else(|e| panic!("extract_symbols failed on {}: {}", path.display(), e));

    // prelude.rs has two `pub use` re-exports: Price and CurrencyFormat
    let reexport_names: Vec<&str> = extracted
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::ReExport)
        .map(|s| s.name.as_str())
        .collect();

    assert!(
        reexport_names.contains(&"Price"),
        "should capture pub use re-export of Price, found: {:?}",
        reexport_names
    );
    assert!(
        reexport_names.contains(&"CurrencyFormat"),
        "should capture pub use re-export of CurrencyFormat, found: {:?}",
        reexport_names
    );

    // All re-exports should be public
    for sym in extracted
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::ReExport)
    {
        assert_eq!(
            sym.visibility,
            Visibility::Public,
            "pub use re-export '{}' should have Public visibility",
            sym.name
        );
    }

    // Each re-export should have a signature containing "pub use"
    for sym in extracted
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::ReExport)
    {
        let sig = sym
            .signature
            .as_ref()
            .expect("re-export should have a signature");
        assert!(
            sig.contains("pub use"),
            "re-export '{}' signature should contain 'pub use', got: {}",
            sym.name,
            sig
        );
    }
}

#[test]
fn test_extractor_macro() {
    let path = fixture_path("crates/infra/src/repository.rs");
    let extracted = extract_symbols(&path)
        .unwrap_or_else(|e| panic!("extract_symbols failed on {}: {}", path.display(), e));

    // Verify log_query macro is captured
    let macro_sym = extracted
        .symbols
        .iter()
        .find(|s| s.name == "log_query" && s.kind == SymbolKind::Macro)
        .expect("should extract log_query macro");

    let sig = macro_sym
        .signature
        .as_ref()
        .expect("log_query macro should have a signature");
    assert!(
        sig.contains("macro_rules!"),
        "log_query signature should contain 'macro_rules!', got: {}",
        sig
    );
    assert!(
        sig.contains("log_query"),
        "log_query signature should contain 'log_query', got: {}",
        sig
    );
}

// ============================================================================
// Issue #11: Indexer Pipeline Tests
// ============================================================================

/// Return the absolute path to the sample-workspace Cargo.toml fixture.
fn fixture_workspace_manifest() -> PathBuf {
    let mut base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    base.push("tests");
    base.push("fixtures");
    base.push("sample-workspace");
    base.push("Cargo.toml");
    base
}

#[test]
fn test_cargo_meta_extracts_workspace() {
    use rulest_indexer::cargo_meta;
    let fixture = fixture_workspace_manifest();
    let info = cargo_meta::extract_workspace(&fixture).expect("Should extract workspace");

    // Should find both crates
    assert!(
        info.crates.len() >= 2,
        "Should find at least 2 crates, got {}",
        info.crates.len()
    );
    let names: Vec<&str> = info.crates.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"domain"), "Should find 'domain' crate");
    assert!(names.contains(&"infra"), "Should find 'infra' crate");

    // Should find modules
    assert!(!info.modules.is_empty(), "Should find modules");

    // Should find cross-crate dependency (infra depends on domain)
    assert!(
        info.dependencies
            .iter()
            .any(|(from, to)| from == "infra" && to == "domain"),
        "Should detect infra -> domain dependency, got: {:?}",
        info.dependencies
    );
}

#[test]
fn test_cargo_meta_extracts_modules() {
    use rulest_indexer::cargo_meta;
    let fixture = fixture_workspace_manifest();
    let info = cargo_meta::extract_workspace(&fixture).unwrap();

    // Find domain crate modules
    let domain_modules = info
        .modules
        .iter()
        .find(|(name, _)| name == "domain")
        .map(|(_, modules)| modules)
        .expect("Should have domain modules");

    // Should find lib.rs, types.rs, prelude.rs
    let module_names: Vec<&str> = domain_modules.iter().map(|m| m.name.as_str()).collect();
    assert!(
        module_names.contains(&"lib"),
        "Should find lib module, got: {:?}",
        module_names
    );
    assert!(
        module_names.contains(&"types"),
        "Should find types module, got: {:?}",
        module_names
    );
    assert!(
        module_names.contains(&"prelude"),
        "Should find prelude module, got: {:?}",
        module_names
    );
}

#[test]
fn test_full_sync_pipeline() {
    use rulest_core::{queries, registry};
    use rulest_indexer::sync::sync_workspace;

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    registry::create_schema(&conn).unwrap();

    let fixture = fixture_workspace_manifest();
    let temp_dir = std::env::temp_dir().join("rulest_test_sync");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let stats =
        sync_workspace(&conn, &fixture, &temp_dir, true).expect("Sync should succeed");

    assert!(
        stats.crates_found >= 2,
        "Should find at least 2 crates, got {}",
        stats.crates_found
    );
    assert!(
        stats.modules_scanned > 0,
        "Should scan some modules, got {}",
        stats.modules_scanned
    );
    assert!(
        stats.symbols_added > 0,
        "Should add some symbols, got {}",
        stats.symbols_added
    );

    // Verify symbols are in the DB -- query for known symbols from fixture
    // The domain crate has: Price (struct), CurrencyFormat (trait), calculate_fee (fn)
    let advisories = queries::validate_creation(&conn, "calculate_fee", "other").unwrap();
    // Should find the existing symbol and advise reuse
    assert!(
        !advisories.is_empty(),
        "validate_creation should return advisories for calculate_fee after sync"
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ============================================================================
// Issue #9: CLI Integration Tests
// ============================================================================

#[test]
fn test_init_creates_registry() {
    use rulest_core::registry;

    let temp_dir = std::env::temp_dir().join("rulest_test_init");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let db_path = temp_dir.join("registry.db");
    let conn = registry::open_registry(&db_path).unwrap();
    registry::create_schema(&conn).unwrap();

    // Verify tables exist
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM crates", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_add_rule_and_validate_boundary() {
    use rulest_core::models::*;
    use rulest_core::{queries, registry};

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    registry::create_schema(&conn).unwrap();

    // Insert a crate
    let c = Crate {
        id: None,
        name: "domain".to_string(),
        path: "crates/domain".to_string(),
        description: None,
        bounded_context: Some("core".to_string()),
    };
    registry::insert_crate(&conn, &c).unwrap();

    // Add an ownership rule (simulates `rulest add-rule`)
    let rule = OwnershipRule {
        id: None,
        crate_name: "domain".to_string(),
        description: "No infrastructure concerns".to_string(),
        kind: OwnershipRuleKind::MustNot,
    };
    registry::insert_ownership_rule(&conn, &rule).unwrap();

    // Validate boundary (simulates `rulest query --validate-boundary`)
    let advisories = queries::validate_boundary(&conn, "HttpClient", "domain").unwrap();
    // Should detect a BoundaryViolation
    assert!(
        advisories
            .iter()
            .any(|a| matches!(a, rulest_core::advisory::Advisory::BoundaryViolation { .. })),
        "Should detect BoundaryViolation, got: {:?}",
        advisories
    );
}

#[test]
fn test_status_returns_stats() {
    use rulest_core::{queries, registry};

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    registry::create_schema(&conn).unwrap();

    let stats = queries::get_registry_stats(&conn);
    assert_eq!(stats.crate_count, 0);
    assert_eq!(stats.module_count, 0);
    assert_eq!(stats.symbol_count, 0);
    assert_eq!(stats.rule_count, 0);
}

#[test]
fn test_scaffold_templates_load() {
    // Verify that include_str! templates are non-empty
    let root_template = include_str!("../templates/CLAUDE.root.md");
    let module_template = include_str!("../templates/CLAUDE.module.md");
    let settings_template = include_str!("../templates/settings.json");
    let seed_template = include_str!("../templates/seed.sql");

    assert!(
        !root_template.is_empty(),
        "Root CLAUDE.md template should not be empty"
    );
    assert!(
        !module_template.is_empty(),
        "Module CLAUDE.md template should not be empty"
    );
    assert!(
        !settings_template.is_empty(),
        "settings.json template should not be empty"
    );
    assert!(
        !seed_template.is_empty(),
        "seed.sql template should not be empty"
    );

    // Verify templates contain expected placeholders
    assert!(root_template.contains("{{workspace_name}}"));
    assert!(module_template.contains("{{crate_name}}"));
    assert!(settings_template.contains("{{crate_list}}"));
}
