# rulest

## Architecture

This project uses itself (rulest) for architecture enforcement.

### MCP Oracle

Before creating new symbols, query the architecture registry:

```json
{
  "mcpServers": {
    "rulest": {
      "command": "rulest",
      "args": ["serve", "--db", ".architect/registry.db"]
    }
  }
}
```

### Routing Rules

| Concern | Crate | Notes |
|---------|-------|-------|
| Registry schema, models, queries, advisory types | `rulest-core` | No I/O or filesystem operations |
| Source code parsing and sync | `rulest-indexer` | syn-based extraction, incremental mtime sync |
| MCP protocol handling | `rulest-mcp` | JSON-RPC 2.0 over stdio |
| CLI argument parsing and dispatch | `rulest-cli` | Combines all crates into `rulest` binary |

### Pre-flight Checklist

Before writing code:
1. Run `validate_creation` to check if the symbol already exists
2. Run `validate_dependency` to find existing types
3. Run `validate_boundary` to check ownership rules
4. Run `check_wip` to detect concurrent work
