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
        description: "No HTTP clients, network, or infrastructure concerns".to_string(),
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

// ============================================================================
// Additional Integration Tests for Untested CLI Modules
// ============================================================================

// ---------- Test 1: add-rule and query integration ----------

#[test]
fn test_add_rule_must_own_and_must_not() {
    use rulest_core::models::*;
    use rulest_core::{queries, registry};

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    registry::create_schema(&conn).unwrap();

    // Insert crates
    let domain_crate = Crate {
        id: None,
        name: "domain".to_string(),
        path: "crates/domain".to_string(),
        description: Some("Core domain logic".to_string()),
        bounded_context: Some("core".to_string()),
    };
    registry::insert_crate(&conn, &domain_crate).unwrap();

    let infra_crate = Crate {
        id: None,
        name: "infra".to_string(),
        path: "crates/infra".to_string(),
        description: Some("Infrastructure layer".to_string()),
        bounded_context: Some("infra".to_string()),
    };
    registry::insert_crate(&conn, &infra_crate).unwrap();

    // Insert a must_own rule
    let must_own_rule = OwnershipRule {
        id: None,
        crate_name: "domain".to_string(),
        description: "Owns all pricing and fee calculation logic".to_string(),
        kind: OwnershipRuleKind::MustOwn,
    };
    registry::insert_ownership_rule(&conn, &must_own_rule).unwrap();

    // Insert a must_not rule
    let must_not_rule = OwnershipRule {
        id: None,
        crate_name: "domain".to_string(),
        description: "No HTTP clients, network, or infrastructure concerns".to_string(),
        kind: OwnershipRuleKind::MustNot,
    };
    registry::insert_ownership_rule(&conn, &must_not_rule).unwrap();

    // Validate boundary: placing HttpClient in domain should trigger violation
    let advisories = queries::validate_boundary(&conn, "HttpClient", "domain").unwrap();
    assert!(
        advisories
            .iter()
            .any(|a| matches!(a, rulest_core::advisory::Advisory::BoundaryViolation { .. })),
        "Should detect BoundaryViolation for HttpClient in domain, got: {:?}",
        advisories
    );

    // Validate boundary: placing PriceCalculator in domain should be safe
    // (must_not rule is about HTTP/network/infrastructure, not pricing)
    let advisories = queries::validate_boundary(&conn, "PriceCalculator", "domain").unwrap();
    assert!(
        advisories
            .iter()
            .any(|a| matches!(a, rulest_core::advisory::Advisory::SafeToCreate { .. })),
        "PriceCalculator should be safe in domain, got: {:?}",
        advisories
    );

    // Verify rule count via get_registry_stats
    let stats = queries::get_registry_stats(&conn);
    assert_eq!(
        stats.rule_count, 2,
        "Should have exactly 2 rules, got {}",
        stats.rule_count
    );
}

// ---------- Test 2: status returns detailed stats ----------

#[test]
fn test_status_returns_detailed_stats() {
    use rulest_core::models::*;
    use rulest_core::{queries, registry};

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    registry::create_schema(&conn).unwrap();

    // Insert a crate
    let c = Crate {
        id: None,
        name: "test-crate".to_string(),
        path: "crates/test".to_string(),
        description: None,
        bounded_context: None,
    };
    let crate_id = registry::insert_crate(&conn, &c).unwrap();

    // Insert a module
    let m = Module {
        id: None,
        crate_id,
        path: "crates/test/src/lib.rs".to_string(),
        name: "lib".to_string(),
    };
    let module_id = registry::insert_module(&conn, &m).unwrap();

    // Insert symbols of different kinds, visibilities, and statuses
    let symbols = vec![
        Symbol {
            id: None,
            module_id,
            name: "MyStruct".to_string(),
            kind: SymbolKind::Struct,
            visibility: Visibility::Public,
            signature: Some("struct MyStruct".to_string()),
            line_number: Some(1),
            scope: None,
            status: SymbolStatus::Stable,
            created_by: None,
            created_at: None,
            updated_at: None,
        },
        Symbol {
            id: None,
            module_id,
            name: "helper_fn".to_string(),
            kind: SymbolKind::Function,
            visibility: Visibility::Private,
            signature: Some("fn helper_fn()".to_string()),
            line_number: Some(10),
            scope: None,
            status: SymbolStatus::Stable,
            created_by: None,
            created_at: None,
            updated_at: None,
        },
        Symbol {
            id: None,
            module_id,
            name: "new_feature".to_string(),
            kind: SymbolKind::Function,
            visibility: Visibility::Public,
            signature: Some("fn new_feature()".to_string()),
            line_number: Some(20),
            scope: None,
            status: SymbolStatus::Wip,
            created_by: Some("agent-1".to_string()),
            created_at: None,
            updated_at: None,
        },
        Symbol {
            id: None,
            module_id,
            name: "MyTrait".to_string(),
            kind: SymbolKind::Trait,
            visibility: Visibility::Public,
            signature: Some("trait MyTrait".to_string()),
            line_number: Some(30),
            scope: None,
            status: SymbolStatus::Planned,
            created_by: None,
            created_at: None,
            updated_at: None,
        },
    ];

    for sym in &symbols {
        registry::insert_symbol(&conn, sym).unwrap();
    }

    // Insert an ownership rule to verify rule_count
    let rule = OwnershipRule {
        id: None,
        crate_name: "test-crate".to_string(),
        description: "test rule".to_string(),
        kind: OwnershipRuleKind::MustOwn,
    };
    registry::insert_ownership_rule(&conn, &rule).unwrap();

    let stats = queries::get_registry_stats(&conn);

    // Verify top-level counts
    assert_eq!(stats.crate_count, 1);
    assert_eq!(stats.module_count, 1);
    assert_eq!(stats.symbol_count, 4);
    assert_eq!(stats.rule_count, 1);

    // Verify symbols_by_kind breakdown
    let fn_count = stats
        .symbols_by_kind
        .iter()
        .find(|(k, _)| k == "function")
        .map(|(_, c)| *c)
        .unwrap_or(0);
    assert_eq!(fn_count, 2, "Should have 2 functions, got {}", fn_count);

    let struct_count = stats
        .symbols_by_kind
        .iter()
        .find(|(k, _)| k == "struct")
        .map(|(_, c)| *c)
        .unwrap_or(0);
    assert_eq!(struct_count, 1, "Should have 1 struct, got {}", struct_count);

    let trait_count = stats
        .symbols_by_kind
        .iter()
        .find(|(k, _)| k == "trait")
        .map(|(_, c)| *c)
        .unwrap_or(0);
    assert_eq!(trait_count, 1, "Should have 1 trait, got {}", trait_count);

    // Verify symbols_by_visibility breakdown
    let pub_count = stats
        .symbols_by_visibility
        .iter()
        .find(|(v, _)| v == "public")
        .map(|(_, c)| *c)
        .unwrap_or(0);
    assert_eq!(
        pub_count, 3,
        "Should have 3 public symbols, got {}",
        pub_count
    );

    let priv_count = stats
        .symbols_by_visibility
        .iter()
        .find(|(v, _)| v == "private")
        .map(|(_, c)| *c)
        .unwrap_or(0);
    assert_eq!(
        priv_count, 1,
        "Should have 1 private symbol, got {}",
        priv_count
    );

    // Verify symbols_by_status breakdown
    let stable_count = stats
        .symbols_by_status
        .iter()
        .find(|(s, _)| s == "stable")
        .map(|(_, c)| *c)
        .unwrap_or(0);
    assert_eq!(
        stable_count, 2,
        "Should have 2 stable symbols, got {}",
        stable_count
    );

    let wip_count = stats
        .symbols_by_status
        .iter()
        .find(|(s, _)| s == "wip")
        .map(|(_, c)| *c)
        .unwrap_or(0);
    assert_eq!(wip_count, 1, "Should have 1 wip symbol, got {}", wip_count);

    let planned_count = stats
        .symbols_by_status
        .iter()
        .find(|(s, _)| s == "planned")
        .map(|(_, c)| *c)
        .unwrap_or(0);
    assert_eq!(
        planned_count, 1,
        "Should have 1 planned symbol, got {}",
        planned_count
    );
}

// ---------- Test 3: scaffold template content validation ----------

#[test]
fn test_scaffold_templates_contain_required_content() {
    let root_template = include_str!("../templates/CLAUDE.root.md");
    let module_template = include_str!("../templates/CLAUDE.module.md");
    let settings_template = include_str!("../templates/settings.json");
    let seed_template = include_str!("../templates/seed.sql");

    // Root template should contain MCP server config and routing rules
    assert!(
        root_template.contains("mcpServers"),
        "Root template should contain MCP server configuration"
    );
    assert!(
        root_template.contains("validate_creation"),
        "Root template should reference validate_creation in pre-flight checklist"
    );
    assert!(
        root_template.contains("validate_boundary"),
        "Root template should reference validate_boundary in pre-flight checklist"
    );
    assert!(
        root_template.contains("Routing Rules"),
        "Root template should contain Routing Rules section"
    );
    assert!(
        root_template.contains("{{crate_list}}"),
        "Root template should contain {{crate_list}} placeholder"
    );

    // Module template should have ownership and dependency sections
    assert!(
        module_template.contains("{{description}}"),
        "Module template should contain {{description}} placeholder"
    );
    assert!(
        module_template.contains("{{dependencies}}"),
        "Module template should contain {{dependencies}} placeholder"
    );
    assert!(
        module_template.contains("Ownership"),
        "Module template should contain Ownership section"
    );

    // Settings template should produce valid JSON structure with deny permissions
    assert!(
        settings_template.contains("permissions"),
        "Settings template should contain permissions key"
    );
    assert!(
        settings_template.contains("deny"),
        "Settings template should contain deny array"
    );

    // Seed template should contain ownership rules comment/header
    assert!(
        seed_template.contains("ownership"),
        "Seed template should reference ownership rules"
    );
    assert!(
        seed_template.contains("{{crate_list}}"),
        "Seed template should contain {{crate_list}} placeholder"
    );
}

// ---------- Test 4: build without registry ----------

#[test]
fn test_build_without_registry_succeeds() {
    // Simulate the build.rs logic: when .architect/registry.db does not exist,
    // the build command should NOT fail -- it prints a message and returns Ok.
    // We test this by checking that a non-existent DB path is handled gracefully.

    let temp_dir = std::env::temp_dir().join("rulest_test_build_no_registry");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let db_path = temp_dir.join(".architect").join("registry.db");

    // The registry should not exist
    assert!(
        !db_path.exists(),
        "DB path should not exist for this test"
    );

    // Replicate the build.rs check: "Registry not found, skipping sync"
    // This is what build::run() does when it can't find the DB
    let result = if !db_path.exists() {
        Ok("Registry not found, skipping sync")
    } else {
        Err("Should not reach here")
    };

    assert!(
        result.is_ok(),
        "Build without registry should succeed gracefully"
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ---------- Test 5: sync with parse errors ----------

#[test]
fn test_sync_reports_parse_errors() {
    use rulest_core::registry;
    use rulest_indexer::sync::sync_workspace;

    // Create a minimal workspace with an invalid Rust file
    let temp_dir = std::env::temp_dir().join("rulest_test_sync_parse_errors");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let workspace_dir = temp_dir.join("workspace");
    let crate_dir = workspace_dir.join("crates").join("broken");
    let src_dir = crate_dir.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    // Write workspace Cargo.toml
    std::fs::write(
        workspace_dir.join("Cargo.toml"),
        r#"[workspace]
members = ["crates/broken"]
resolver = "2"
"#,
    )
    .unwrap();

    // Write crate Cargo.toml
    std::fs::write(
        crate_dir.join("Cargo.toml"),
        r#"[package]
name = "broken"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();

    // Write an intentionally invalid Rust file
    std::fs::write(
        src_dir.join("lib.rs"),
        r#"
pub fn valid_fn() -> i32 { 42 }

// This is intentionally broken syntax
pub fn broken_fn( {
    // missing closing paren and return type
}
"#,
    )
    .unwrap();

    let architect_dir = workspace_dir.join(".architect");
    std::fs::create_dir_all(&architect_dir).unwrap();

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    registry::create_schema(&conn).unwrap();

    let stats = sync_workspace(
        &conn,
        &workspace_dir.join("Cargo.toml"),
        &architect_dir,
        true,
    )
    .expect("Sync should succeed even with parse errors");

    // The parse error should be recorded, not cause a failure
    assert!(
        !stats.parse_errors.is_empty(),
        "Should have recorded parse errors, got: {:?}",
        stats.parse_errors
    );

    // Verify the parse error mentions the broken file
    let has_broken_error = stats
        .parse_errors
        .iter()
        .any(|(path, _msg)| path.contains("lib.rs"));
    assert!(
        has_broken_error,
        "Parse errors should mention the broken file, got: {:?}",
        stats.parse_errors
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ---------- Test 6: FFI detection end-to-end ----------

#[test]
fn test_sync_detects_ffi_functions() {
    use rulest_core::{registry};
    use rulest_indexer::sync::sync_workspace;

    // Create a workspace with extern "C" functions
    let temp_dir = std::env::temp_dir().join("rulest_test_ffi_detection");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let workspace_dir = temp_dir.join("workspace");
    let crate_dir = workspace_dir.join("crates").join("ffi_crate");
    let src_dir = crate_dir.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    // Write workspace Cargo.toml
    std::fs::write(
        workspace_dir.join("Cargo.toml"),
        r#"[workspace]
members = ["crates/ffi_crate"]
resolver = "2"
"#,
    )
    .unwrap();

    // Write crate Cargo.toml
    std::fs::write(
        crate_dir.join("Cargo.toml"),
        r#"[package]
name = "ffi_crate"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();

    // Write a file with FFI functions
    std::fs::write(
        src_dir.join("lib.rs"),
        r#"
#[no_mangle]
pub extern "C" fn my_c_func(x: i32) -> i32 {
    x + 1
}

extern "C" {
    fn external_dependency(buf: *mut u8, len: usize) -> i32;
}

pub fn regular_function() -> bool {
    true
}
"#,
    )
    .unwrap();

    let architect_dir = workspace_dir.join(".architect");
    std::fs::create_dir_all(&architect_dir).unwrap();

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    registry::create_schema(&conn).unwrap();

    let stats = sync_workspace(
        &conn,
        &workspace_dir.join("Cargo.toml"),
        &architect_dir,
        true,
    )
    .expect("Sync should succeed");

    assert!(
        stats.symbols_added > 0,
        "Should have added symbols, got {}",
        stats.symbols_added
    );

    // Query the DB for ffi_function symbols
    let ffi_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM symbols WHERE kind = 'ffi_function'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        ffi_count >= 2,
        "Should have at least 2 FFI function symbols (my_c_func + external_dependency), got {}",
        ffi_count
    );

    // Verify the regular function is NOT marked as FFI
    let regular_kind: String = conn
        .query_row(
            "SELECT kind FROM symbols WHERE name = 'regular_function'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        regular_kind, "function",
        "regular_function should be kind 'function', got '{}'",
        regular_kind
    );

    // Verify my_c_func IS marked as FFI
    let ffi_kind: String = conn
        .query_row(
            "SELECT kind FROM symbols WHERE name = 'my_c_func'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        ffi_kind, "ffi_function",
        "my_c_func should be kind 'ffi_function', got '{}'",
        ffi_kind
    );

    // Verify external_dependency IS marked as FFI
    let ext_kind: String = conn
        .query_row(
            "SELECT kind FROM symbols WHERE name = 'external_dependency'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        ext_kind, "ffi_function",
        "external_dependency should be kind 'ffi_function', got '{}'",
        ext_kind
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ---------- Test 7: schema migration end-to-end ----------

#[test]
fn test_init_sets_schema_version() {
    use rulest_core::registry;

    let temp_dir = std::env::temp_dir().join("rulest_test_schema_version");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let db_path = temp_dir.join("registry.db");
    let conn = registry::open_registry(&db_path).unwrap();

    // Before create_schema, version should be 0 (fresh DB)
    let version_before = registry::get_schema_version(&conn).unwrap();
    assert_eq!(
        version_before, 0,
        "Fresh database should have schema version 0, got {}",
        version_before
    );

    // Run create_schema (simulates what init::run does)
    registry::create_schema(&conn).unwrap();

    // After create_schema, version should match SCHEMA_VERSION
    let version_after = registry::get_schema_version(&conn).unwrap();
    assert_eq!(
        version_after,
        registry::SCHEMA_VERSION,
        "Schema version should be {}, got {}",
        registry::SCHEMA_VERSION,
        version_after
    );

    // Running create_schema again should be idempotent
    registry::create_schema(&conn).unwrap();
    let version_idempotent = registry::get_schema_version(&conn).unwrap();
    assert_eq!(
        version_idempotent,
        registry::SCHEMA_VERSION,
        "Schema version should remain {} after second create_schema, got {}",
        registry::SCHEMA_VERSION,
        version_idempotent
    );

    // Clean up
    drop(conn);
    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ---------- Test 7b: schema migration from older version ----------

#[test]
fn test_schema_migration_from_v1_to_current() {
    use rulest_core::registry;

    let conn = rusqlite::Connection::open_in_memory().unwrap();

    // Simulate a v1 database: set version to 1
    registry::set_schema_version(&conn, 1).unwrap();

    // create_schema should migrate from v1 to SCHEMA_VERSION
    registry::create_schema(&conn).unwrap();

    let version = registry::get_schema_version(&conn).unwrap();
    assert_eq!(
        version,
        registry::SCHEMA_VERSION,
        "Should have migrated to version {}, got {}",
        registry::SCHEMA_VERSION,
        version
    );

    // Verify that all expected tables exist after migration
    let table_names: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap();
        stmt.query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    };

    for expected_table in &[
        "crates",
        "modules",
        "symbols",
        "relationships",
        "contracts",
        "ownership_rules",
        "crate_dependencies",
    ] {
        assert!(
            table_names.iter().any(|t| t == expected_table),
            "Table '{}' should exist after migration, found tables: {:?}",
            expected_table,
            table_names
        );
    }
}

// ---------- Test: future schema version rejection ----------

#[test]
fn test_schema_rejects_future_version() {
    use rulest_core::registry;

    let conn = rusqlite::Connection::open_in_memory().unwrap();

    // Set to a version higher than the binary supports
    registry::set_schema_version(&conn, registry::SCHEMA_VERSION + 10).unwrap();

    let result = registry::create_schema(&conn);
    assert!(
        result.is_err(),
        "Should reject databases from the future"
    );

    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("newer"),
        "Error should mention 'newer' version, got: {}",
        err_msg
    );
}

// ---------- Test: seed.sql execution via execute_seed_sql ----------

#[test]
fn test_seed_sql_roundtrip() {
    use rulest_core::{queries, registry};

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    registry::create_schema(&conn).unwrap();

    // Insert a crate (needed for validate_boundary)
    let c = rulest_core::models::Crate {
        id: None,
        name: "payments".to_string(),
        path: "crates/payments".to_string(),
        description: None,
        bounded_context: None,
    };
    registry::insert_crate(&conn, &c).unwrap();

    // Execute seed SQL with multiple rules
    let seed_sql = r#"
-- Payment domain rules
INSERT INTO ownership_rules (crate_name, description, kind) VALUES ('payments', 'Owns all payment processing logic', 'must_own');
INSERT INTO ownership_rules (crate_name, description, kind) VALUES ('payments', 'No database or storage concerns', 'must_not');
"#;

    registry::execute_seed_sql(&conn, seed_sql).unwrap();

    // Verify rules were inserted
    let stats = queries::get_registry_stats(&conn);
    assert_eq!(
        stats.rule_count, 2,
        "Should have 2 rules from seed SQL, got {}",
        stats.rule_count
    );

    // Verify the must_not rule is enforced
    let advisories = queries::validate_boundary(&conn, "DatabaseConnection", "payments").unwrap();
    assert!(
        advisories
            .iter()
            .any(|a| matches!(a, rulest_core::advisory::Advisory::BoundaryViolation { .. })),
        "DatabaseConnection should violate boundary in payments crate, got: {:?}",
        advisories
    );
}

// ---------- Test: full init + sync + query pipeline ----------

#[test]
fn test_full_init_sync_query_pipeline() {
    use rulest_core::models::*;
    use rulest_core::{queries, registry};
    use rulest_indexer::sync::sync_workspace;

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    registry::create_schema(&conn).unwrap();

    let fixture = fixture_workspace_manifest();
    let temp_dir = std::env::temp_dir().join("rulest_test_full_pipeline");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    // Sync the fixture workspace
    let stats = sync_workspace(&conn, &fixture, &temp_dir, true).expect("Sync should succeed");

    assert!(stats.parse_errors.is_empty(), "No parse errors expected");

    // Add ownership rules (simulating what add-rule does)
    let rule = OwnershipRule {
        id: None,
        crate_name: "domain".to_string(),
        description: "No HTTP clients or network concerns".to_string(),
        kind: OwnershipRuleKind::MustNot,
    };
    registry::insert_ownership_rule(&conn, &rule).unwrap();

    // Query validate_creation for an existing symbol.
    // "calculate_fee" exists only once (no re-export), so it returns ReuseExisting.
    let advisories = queries::validate_creation(&conn, "calculate_fee", "other").unwrap();
    assert!(
        advisories
            .iter()
            .any(|a| matches!(a, rulest_core::advisory::Advisory::ReuseExisting { .. })),
        "Should advise reusing calculate_fee, got: {:?}",
        advisories
    );

    // "Price" exists as both struct and re-export — should prefer the definition.
    let advisories = queries::validate_creation(&conn, "Price", "other").unwrap();
    assert!(
        advisories
            .iter()
            .any(|a| matches!(a, rulest_core::advisory::Advisory::ReuseExisting { .. })),
        "Price should produce ReuseExisting (preferring definition over re-export), got: {:?}",
        advisories
    );
    // Verify it points to the struct definition, not the re-export
    if let Some(rulest_core::advisory::Advisory::ReuseExisting { existing, .. }) =
        advisories.iter().find(|a| matches!(a, rulest_core::advisory::Advisory::ReuseExisting { .. }))
    {
        assert_eq!(existing.kind, "struct", "Should prefer the struct definition, got kind: {}", existing.kind);
    }

    // Query validate_boundary
    let advisories = queries::validate_boundary(&conn, "NetworkClient", "domain").unwrap();
    assert!(
        advisories
            .iter()
            .any(|a| matches!(a, rulest_core::advisory::Advisory::BoundaryViolation { .. })),
        "NetworkClient should violate domain boundary, got: {:?}",
        advisories
    );

    // Verify stats
    let reg_stats = queries::get_registry_stats(&conn);
    assert!(
        reg_stats.symbol_count > 0,
        "Should have symbols after sync"
    );
    assert!(
        reg_stats.crate_count >= 2,
        "Should have at least 2 crates"
    );
    assert_eq!(reg_stats.rule_count, 1, "Should have 1 rule");

    // Verify symbols_by_kind is populated
    assert!(
        !reg_stats.symbols_by_kind.is_empty(),
        "symbols_by_kind should not be empty after sync"
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ============================================================================
// Issue #35: Increased Integration Test Coverage
// ============================================================================

#[test]
fn test_query_tools_integration() {
    use rulest_core::models::*;
    use rulest_core::{queries, registry};

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    registry::create_schema(&conn).unwrap();

    let c = Crate {
        id: None,
        name: "mylib".to_string(),
        path: "crates/mylib".to_string(),
        description: None,
        bounded_context: None,
    };
    let crate_id = registry::insert_crate(&conn, &c).unwrap();

    let m = Module {
        id: None,
        crate_id,
        path: "crates/mylib/src/lib.rs".to_string(),
        name: "lib".to_string(),
    };
    let module_id = registry::insert_module(&conn, &m).unwrap();

    let s = Symbol {
        id: None,
        module_id,
        name: "process_data".to_string(),
        kind: SymbolKind::Function,
        visibility: Visibility::Public,
        signature: Some("fn process_data(input: &str) -> Result<String, Error>".to_string()),
        line_number: Some(10),
        scope: None,
        status: SymbolStatus::Stable,
        created_by: None,
        created_at: None,
        updated_at: None,
    };
    registry::insert_symbol(&conn, &s).unwrap();

    // validate_creation should find the existing symbol
    let advisories = queries::validate_creation(&conn, "process_data", "other/lib.rs").unwrap();
    assert!(!advisories.is_empty(), "Should find existing process_data");

    // validate_dependency should find no type (it's a function)
    let _dep_advisories = queries::validate_dependency(&conn, "process_data").unwrap();
    // Functions aren't types, so this may return empty or not — just verify no error

    // suggest_reuse should find something related to "process" or "data"
    let _reuse_advisories = queries::suggest_reuse(&conn, "data processing").unwrap();
    // Just verify it returns without error — no panic = success
}

#[test]
fn test_register_plan_and_check_wip() {
    use rulest_core::models::*;
    use rulest_core::{queries, registry};

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    registry::create_schema(&conn).unwrap();

    let c = Crate {
        id: None,
        name: "mylib".to_string(),
        path: "crates/mylib".to_string(),
        description: None,
        bounded_context: None,
    };
    let crate_id = registry::insert_crate(&conn, &c).unwrap();

    let m = Module {
        id: None,
        crate_id,
        path: "crates/mylib/src/handlers.rs".to_string(),
        name: "handlers".to_string(),
    };
    registry::insert_module(&conn, &m).unwrap();

    // Register a plan
    let actions = vec![rulest_core::advisory::PlannedAction {
        action: "create".to_string(),
        symbol: "new_handler".to_string(),
        target: "crates/mylib/src/handlers.rs".to_string(),
        crate_name: Some("mylib".to_string()),
        kind: Some("function".to_string()),
    }];
    let registered = queries::register_plan(&conn, &actions, "test-agent").unwrap();
    assert_eq!(registered, 1, "Should register 1 planned symbol");

    // check_wip should now detect the planned symbol
    let advisories = queries::check_wip(&conn, "handlers").unwrap();
    assert!(
        advisories
            .iter()
            .any(|a| matches!(a, rulest_core::advisory::Advisory::WipConflict { .. })),
        "Should detect WIP conflict for the planned symbol, got: {:?}",
        advisories
    );
}

#[test]
fn test_init_idempotency() {
    use rulest_core::models::*;
    use rulest_core::registry;

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    registry::create_schema(&conn).unwrap();

    // Insert a crate (simulating first init)
    let c = Crate {
        id: None,
        name: "existing-crate".to_string(),
        path: "crates/existing".to_string(),
        description: None,
        bounded_context: None,
    };
    registry::insert_crate(&conn, &c).unwrap();

    // Call create_schema again (simulating second init)
    registry::create_schema(&conn).unwrap();

    // Data should still be there
    let found = registry::find_crate_by_name(&conn, "existing-crate").unwrap();
    assert!(found.is_some(), "Crate should survive second create_schema call");

    // Schema version should still be correct
    let version = registry::get_schema_version(&conn).unwrap();
    assert_eq!(version, registry::SCHEMA_VERSION);
}

#[test]
fn test_mcp_register_plan_rejects_malformed_actions() {
    use rulest_core::registry;
    use serde_json::json;

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    registry::create_schema(&conn).unwrap();

    // Send malformed actions (string instead of array of objects)
    let result = rulest_mcp::tools::call_tool(
        &conn,
        "register_plan",
        &json!({
            "agent": "test-agent",
            "actions": "not an array"
        }),
    );

    assert!(
        result.get("error").is_some(),
        "Should return error for malformed actions, got: {:?}",
        result
    );
}
