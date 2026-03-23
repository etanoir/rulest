//! # rulest-indexer
//!
//! Source code indexer for the rulest architecture registry. Uses `syn` to parse
//! Rust source files and extract symbols (functions, structs, traits, etc.),
//! `cargo metadata` to discover workspace structure, and incremental mtime-based
//! sync to keep the registry up to date.

pub mod cargo_meta;
pub mod extractor;
pub mod sync;
#[cfg(feature = "typescript")]
pub mod ts_extractor;
