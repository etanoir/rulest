//! # rulest-core
//!
//! Core library for the rulest architecture registry. Provides the SQLite schema,
//! data models, Oracle query functions, and structured advisory types used by the
//! MCP server and CLI.

pub mod advisory;
pub mod models;
pub mod queries;
pub mod registry;
