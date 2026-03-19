# {{workspace_name}}

## Architecture

This project uses [rulest](https://github.com/anthropics/rulest) for architecture enforcement.

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
{{crate_list}}

### Pre-flight Checklist

Before writing code:
1. Run `validate_creation` to check if the symbol already exists
2. Run `validate_dependency` to find existing types
3. Run `validate_boundary` to check ownership rules
4. Run `check_wip` to detect concurrent work

### Post-Build Sync

Run `rulest build` instead of `cargo build` to automatically sync the architecture registry after compilation.
