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
