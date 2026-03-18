# rulest

Architecture Registry & MCP Oracle for Rust projects.

A CLI tool and MCP server that any Rust workspace can adopt to prevent AI agents from creating duplicate symbols, violating module boundaries, or reinventing existing types. Think `cargo-audit` but for architectural integrity during vibe coding.

For the principles and theory behind this tool, read [The Minesweeper Problem](the-minesweeper-problem.md).

## Installation

```sh
cargo install --path crates/rulest-cli
```

Or build from source:

```sh
git clone https://github.com/etanoir/rulest.git
cd rulest
cargo build --release
# binary is at target/release/rulest
```

## Quick Start

### 1. Initialize the registry

From your Rust workspace root:

```sh
rulest init
```

This creates an `.architect/` directory containing:

- `registry.db` — SQLite database with the architecture schema (gitignore this)
- `seed.sql` — ownership rules as versionable SQL (commit this)

### 2. Define ownership rules

Tell the registry what each crate is responsible for:

```sh
rulest add-rule domain "No infrastructure concerns (DB, HTTP, filesystem)" --kind must_not
rulest add-rule domain "All business types use newtype pattern" --kind must_own
rulest add-rule trading "Consume domain::Price, do not define own currency type" --kind must_not
```

Rule kinds:

| Kind | Meaning |
|------|---------|
| `must_own` | This crate is the authoritative owner of this concern |
| `must_not` | This crate must not contain this kind of code |
| `shared_with` | This concern is shared across crates |

### 3. Sync source code into the registry

```sh
rulest sync
```

This parses every `.rs` file in the workspace using `syn` and populates the registry with symbols — functions, structs, enums, traits, type aliases, constants. Only signatures are stored, never function bodies.

Sync is incremental by default (based on file mtime). Force a full resync with:

```sh
rulest sync --full
```

### 4. Query the registry

Look up a symbol:

```sh
rulest query calculate_fee
```

Validate before creating a new function:

```sh
rulest query --validate-creation calculate_settlement_fee --target crates/trading/src/fees.rs
```

Check if a type already exists:

```sh
rulest query --validate-dependency CurrencyAmount
```

Check boundary rules:

```sh
rulest query --validate-boundary HttpClient --crate-name domain
```

Check for work-in-progress conflicts:

```sh
rulest query --check-wip src/fees
```

Search for reusable code:

```sh
rulest query --suggest-reuse "calculate trading fees"
```

All queries return structured JSON with advisory types: `safe_to_create`, `reuse_existing`, `use_existing_type`, `boundary_violation`, `wip_conflict`, `ambiguous_match`, or `reuse_with_pattern`.

### 5. Connect to Claude Code as an MCP server

Add to your Claude Code MCP configuration:

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

The MCP server exposes five tools over JSON-RPC (stdio):

| Tool | Question it answers |
|------|-------------------|
| `validate_creation` | Does this symbol already exist? |
| `validate_dependency` | Who provides this type/trait? |
| `validate_boundary` | Does placing this here violate ownership rules? |
| `check_wip` | Is someone else working in this area? |
| `suggest_reuse` | What existing code can I reuse for this? |

### 6. Scaffold CLAUDE.md and settings for a project

```sh
rulest scaffold
```

Generates `CLAUDE.md` (root and per-crate), `.claude/settings.json` with deny rules, and `.architect/seed.sql` — all pre-filled with your workspace's actual crate names. Existing files are not overwritten.

## Important Usage Notes

### What to commit, what to gitignore

```gitignore
# .gitignore
.architect/registry.db
.architect/registry.db-*
.architect/sync.log
```

Commit `.architect/seed.sql` — it contains your architectural decisions and is the source of truth for ownership rules. The database is rebuilt from source code + seed on any machine.

### Keep the registry in sync

Run `rulest sync` after significant code changes. The registry is only as useful as it is current. A stale registry will miss new symbols and may give incorrect advisories.

For best results, sync after every successful build:

```sh
cargo build && rulest sync
```

### Ownership rules are your main lever

The registry's power comes from the rules you define. Start with a few high-value rules:

- Mark your domain crate as `must_not` for infrastructure concerns
- Mark shared crates as `shared_with` to signal intentional cross-cutting
- Revisit rules as the architecture evolves

### Advisory responses guide, not block

Queries return advisories, not hard errors. The AI agent (or human) decides what to do with them. This is by design — the tool provides architectural awareness, not enforcement. Use `settings.json` deny rules for hard enforcement on critical files.

### The registry stores signatures, not code

The indexer extracts names, types, visibility, and signatures. It never stores function bodies or implementation details. A 50,000-line codebase produces roughly 500 registry rows (~300 KB).

## CLI Reference

```
rulest init       [-w <Cargo.toml>]          Initialize the registry
rulest add-rule   <crate> <desc> [-k <kind>] Add an ownership rule
rulest sync       [-w <Cargo.toml>] [--full] Sync source code into registry
rulest query      [symbol]                   Look up a symbol
rulest query      --validate-creation <name> --target <module>
rulest query      --validate-dependency <type>
rulest query      --validate-boundary <name> --crate-name <crate>
rulest query      --check-wip <module_path>
rulest query      --suggest-reuse <description>
rulest serve      [-d <registry.db>]         Start MCP server (stdio)
rulest scaffold   [-w <Cargo.toml>]          Generate project templates
```

## License

MIT
