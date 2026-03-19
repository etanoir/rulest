//! # rulest-mcp
//!
//! MCP (Model Context Protocol) server for the rulest architecture oracle.
//! Exposes seven JSON-RPC 2.0 tools over stdio that AI coding agents use to
//! validate plans against the architecture registry before writing code.

pub mod server;
pub mod tools;
