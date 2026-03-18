# The Minesweeper Problem — Using Rust to Sandbox AI Unreliability in Vibe Coding

> **Field Notes from a Year of Vibe Coding**
>
> Enterprise Architecture × AI-Assisted Development | 2025 | ~40 min read

Guardrails ensure the AI does things right. The human ensures we build the right thing. This is a practitioner's account of building that division of labor — from the first compile error to a fully instrumented Plan-Validate-Execute pipeline with persistent architectural memory.

---

## Contents

### Part I — Foundation
1. [The Thesis and the Three-Node Model](#01--the-thesis-and-the-three-node-model)
2. [The Minesweeper Problem](#02--the-minesweeper-problem)
3. [Three Failure Classes](#03--three-failure-classes)
4. [Why Rust Specifically — The Typed-Language Comparison](#04--why-rust-specifically--the-typed-language-comparison)

### Part II — The Initial Defense Ladder
5. [The Compiler Firewall (L0–L1)](#05--the-compiler-firewall)
6. [The Tiered Manifest (L2–L5)](#06--the-tiered-manifest)
7. [The Enforcement Gap — CLAUDE.md vs. settings.json](#07--the-enforcement-gap--claudemd-vs-settingsjson)

### Part III — The Discovery Problem
8. [External Validation — codetracer and the Community Convergence](#08--external-validation--codetracer-and-the-community-convergence)

### Part IV — The Evolved Architecture
9. [Plan-Validate-Execute — From Reactive to Proactive](#09--plan-validate-execute--from-reactive-to-proactive)
10. [The MCP Oracle — Concrete Round-Trips](#10--the-mcp-oracle--concrete-round-trips)
11. [The Architecture Registry — Schema, Queries, and Lifecycle](#11--the-architecture-registry--schema-queries-and-lifecycle)
12. [The Evolved Defense Ladder](#12--the-evolved-defense-ladder)

### Part V — Assessment
13. [Cost Analysis](#13--cost-analysis)
14. [The Residual — What Still Requires Human Eyes](#14--the-residual--what-still-requires-human-eyes)
15. [Closing — From Compiler Bouncer to Institutional Memory](#15--closing--from-compiler-bouncer-to-institutional-memory)

### Part VI — Further Assessment (Proposed)
16. [Aspects for Further Assessment](#16--aspects-for-further-assessment)

---

# Part I — Foundation

*The model, the metaphor, and why language choice is a risk management decision.*

---

## 01 — The Thesis and the Three-Node Model

Vibe coding — guiding the architecture while AI generates implementation — is a separation of concerns across three agents: the human designs, the AI codes, the compiler checks. Each has a distinct strength and a distinct failure mode.

```
┌──────────────┐     Intent +      ┌──────────────┐     Generated     ┌──────────────┐
│  Architect   │───Constraints────▶│    Typist    │──────Code────────▶│   Verifier   │
│   (Human)    │                   │    (LLM)     │◀───Compile Err───│    (rustc)   │
└──────────────┘◀──Refined Prompt──└──────────────┘                   └──────┬───────┘
       ▲                                                                     │
       │                           ┌──────────────┐                          │
       └────────Logical Bug────────│   Runtime    │◀───Structurally Sound────┘
                                   └──────────────┘
```

*Fig 1 — The three-node model. The tight loop (AI ↔ compiler) is cheap and automated. The wide loop (runtime → human) fires only for business logic errors.*

The thesis: **by choosing Rust, we offload structural verification from the developer to the compiler.** The compiler catches memory, type, concurrency, and error-handling defects automatically, freeing the human to focus on whether the system does the *right thing*. Finding a logical error is vastly easier when you're not simultaneously hunting null dereferences and hidden race conditions.

---

## 02 — The Minesweeper Problem

The bottleneck in this model is the communication channel between Architect and Typist. The developer holds the complete Minesweeper board — every mine position, every intended path, every "don't do X because Y" lesson. The AI operates under fog of war: its context window is a fixed viewport with no memory across sessions.

| What You Hold | Why AI Can't Hold It | Consequence |
|---|---|---|
| **Complete board** — full architecture, all invariants | Context window is a viewport; it sees tiles, not the board | AI optimizes locally, violates global constraints |
| **Mine positions** — known pitfalls, past failures | No persistent memory; each prompt is a fresh game | You re-explain the same constraints repeatedly |
| **Path intent** — *why* this route over alternatives | AI infers from surface instruction, not reasoning chain | AI takes a "valid" path that steps on a mine you didn't mark |

That third row is the killer. You tell the AI "avoid C." But you chose to avoid C because six months ago it caused a cascade through D and E. When the AI encounters C' — a slight variant — it walks right through. **You're not transferring information; you're compressing an entire decision graph into a linear prompt.** That compression is inherently lossy.

Two strategies exist: **reveal more tiles** (better prompts, documentation — high ongoing cost, scales linearly) or **make the mines self-announcing** (type system, compiler rules — high upfront cost, near-zero marginal cost). The rest of this article is about building the second strategy into a complete system.

---

## 03 — Three Failure Classes

A year of practice reveals three distinct defect classes, each requiring a different mitigation:

```
┌─────────────────────────────┐  ┌─────────────────────────────┐  ┌─────────────────────────────┐
│  Class I — Structural       │  │  Class II — Semantic         │  │  Class III — Architectural   │
│  Memory, null deref,        │  │  Same type, different meaning│  │  Redundant functions,        │
│  data races, type mismatch  │  │  swapped params, wrong unit  │  │  duplicate logic, broken     │
│                             │  │                              │  │  boundaries                  │
│  rustc catches              │  │  newtypes catch              │  │  ???                         │
│  automatically → Zero cost  │  │  at compile time → One-time  │  │  → THE GAP                  │
└─────────────────────────────┘  └─────────────────────────────┘  └─────────────────────────────┘
```

*Fig 2 — Three failure classes. Closing the Class III gap is the central challenge of this article.*

Class I is Rust's core value proposition — well-documented and well-understood. Class II requires deliberate type design (covered in §5). Class III — where the AI generates code that compiles, runs correctly, but is architecturally redundant or misplaced — is the gap that the rest of this article progressively closes.

---

## 04 — Why Rust Specifically — The Typed-Language Comparison

An honest treatment of this thesis requires addressing the obvious counterargument: *why not TypeScript strict mode + comprehensive tests?* Or Haskell? Or any language with a sufficiently expressive type system?

| Defense Layer | Python / JS | TypeScript (strict) | Go | Haskell | Rust |
|---|---|---|---|---|---|
| Null safety | Runtime | Compile (`strictNullChecks`) | nil panics | `Maybe` | `Option<T>` |
| Error handling exhaustiveness | Uncaught exceptions | Partial (no enforced Result) | Convention (err check) | `Either` | `Result<T,E>` + `?` |
| Data race prevention | GIL hides, doesn't solve | Single-threaded (runtime) | Race detector (runtime) | STM (runtime) | **Compile-time** (Send/Sync) |
| Zero-cost newtypes | Class overhead | Branded types (workaround) | Type aliases (limited) | `newtype` | `struct Foo(T)` |
| Module visibility control | Convention only | Package exports (coarse) | Package-level | Module exports | `pub(crate)`, fine-grained |
| Typestate pattern | No | Possible but awkward | No | Phantom types | Zero-cost phantom types |
| DDD bounded context as crate | No module boundary enforcement | Package boundaries (npm workspaces) | Go modules | Cabal packages | Cargo workspace crates |

TypeScript strict comes closest. But three gaps are categorical, not a matter of degree: **compile-time data race prevention** (Rust is alone here among mainstream languages), **enforced error handling exhaustiveness** (TypeScript has no compiler-enforced Result equivalent), and **zero-cost newtypes** that don't add runtime overhead.

The practical implication for vibe coding: in TypeScript, you can build the manifest system and the oracle described later in this article. But you'll still need runtime tests and manual review for the concurrency and error-handling classes that Rust eliminates at compile time. Rust's advantage compounds — each layer of the defense ladder has fewer escapes.

---

# Part II — The Initial Defense Ladder

*Compiler firewall, newtype patterns, tiered manifests, and where this first-generation system breaks down.*

---

## 05 — The Compiler Firewall

Rust's compiler (`rustc`) eliminates Class I defects automatically:

| Defect | Permissive Language | Rust | AI-Specific Value |
|---|---|---|---|
| Null dereference | Runtime crash | Compile-time (`Option<T>`) | AI routinely forgets null checks |
| Unhandled error | Silent failure | Compile-time (`Result<T,E>`) | AI defaults to happy path unless forced |
| Data race | Heisenbugs | Compile-time (ownership) | AI can't track shared state across context |
| Use after free | Segfault / corruption | Compile-time (borrow checker) | AI loses track of lifetimes in long files |

The deeper value for vibe coding: Rust forces the AI to write out `match` arms and `?` operators explicitly, making error-handling logic **visible for review** rather than hidden behind implicit runtime behavior.

### Class II — The Newtype Firewall

The more insidious failure: the AI creates both a function and its call site, and swaps parameters of the same type.

**Primitive obsession — compiles, wrong:**

```rust
fn create_order(
    customer_id: u64,
    product_id: u64,
    quantity: u64,
    price: u64,
) { .. }

// AI swaps params. rustc says OK
create_order(
    product_id,  // wrong!
    customer_id, // wrong!
    qty, price,
);
```

**Newtype wrappers — compile error:**

```rust
struct CustomerId(u64);
struct ProductId(u64);
struct Quantity(u32);
struct Price(u64);

fn create_order(
    c: CustomerId, p: ProductId,
    q: Quantity, pr: Price,
) { .. }

// AI swaps params. rustc REJECTS
```

Zero runtime cost — the `u64` is still a `u64` in the binary. For stronger guarantees, **typestate** encodes valid state transitions into the type system (the AI can't call `.ship()` on an `Order<Draft>`), and the **builder pattern** eliminates positional ambiguity entirely. The decision heuristic: if you've explained the same constraint to the AI more than twice, encode it as a type.

---

## 06 — The Tiered Manifest

Class III — architectural rot — is where the AI creates redundant functions across context windows because it has no ambient codebase awareness. `rustc` doesn't care about redundancy. The first-generation solution is a **tiered manifest**:

| Layer | When Read | Token Cost | What It Provides | Can Drift? |
|---|---|---|---|---|
| **Root CLAUDE.md** — routing rules + ownership map | Every task | ~150 | "Before creating, read target module's CLAUDE.md and prelude.rs" | Yes, small surface |
| **Module CLAUDE.md** — local capabilities + deps | Entering a module | ~100 | "This module owns X, consumes Y from domain — don't reimplement" | Yes, needs maintenance |
| **`prelude.rs`** — actual `pub use` re-exports | Already reading code | 0 extra | The real types and traits in scope — source of truth | No — compiled code |
| **Pre-creation grep** | Before creating new fn/type | 0 (tool call) | `grep -r "fn <name>" crates/` | N/A |

The principle: **Layer 3 is the source of truth. Layer 2 is a human-readable index. Layer 1 is a router to Layer 2.** If the documentation drifts, the AI still encounters the real contracts in `prelude.rs`.

---

## 07 — The Enforcement Gap — CLAUDE.md vs. settings.json

The tiered manifest has a fundamental weakness: **CLAUDE.md is advisory.** Nothing prevents the AI from ignoring the protocol.

| Mechanism | Nature | Analogy | Scope |
|---|---|---|---|
| `CLAUDE.md` | Advisory — AI proceeds if it ignores the rule | "Please knock" sign | Can express workflow protocols ("check before creating") |
| `settings.json` | Enforced — operation is blocked at tool level | Locked door | Can only express command/file deny-allow rules |

The bridge: **you can't enforce "do X first," but you can remove the ability to do Y without going through Z.** Protect architectural control surfaces — `prelude.rs`, `mod.rs`, `lib.rs`, the domain crate — via `settings.json` deny rules.

```json
// settings.json — hard enforcement
{
  "permissions": {
    "deny": [
      "write:crates/domain/src/**",   // protect core types
      "write:**/prelude.rs",          // protect public API
      "write:**/mod.rs",              // protect module structure
      "write:**/lib.rs"               // protect crate entry points
    ]
  }
}
```

| Scenario | CLAUDE.md Only | + settings.json |
|---|---|---|
| AI creates duplicate type in domain crate | Happens silently | **Blocked** |
| AI modifies prelude to export duplicate | Happens silently | **Blocked** |
| AI creates duplicate helper in own module | Happens silently | Still possible — but isolated, discoverable via dead code |

> **KEY INSIGHT:** CLAUDE.md tells the AI what to do. settings.json removes the ability to do the wrong thing. The residual — a duplicate within the AI's own module — is contained; it can't spread into architectural control surfaces.

---

# Part III — The Discovery Problem

*The defense ladder's L5 is a bare grep. External tooling reveals how much better it can be.*

---

## 08 — External Validation — codetracer and the Community Convergence

While building the manifest system, we encountered [codetracer](https://github.com/mirzalazuardi/codetracer) by Mirza Lazuardi — a tool that had independently arrived at the same problem from a different ecosystem (Ruby/JS). Its philosophy: **use AI for reasoning, not for searching.** The convergence matters because two independent practitioners identified the same bottleneck: the AI's discovery capability, not its generation capability, is the problem.

**Our L5 — bare grep:**

```sh
$ grep -r "fn calculate_fee" crates/ \
    --include="*.rs" -l

crates/trading/src/fees.rs

# One file name. No context.
# AI doesn't know:
#  - what the function signature is
#  - what module it belongs to
#  - who calls it
#  - whether it's pub or private
# Result: AI creates a new one anyway
#   because "it might be different"
```

**codetracer-style discovery:**

```sh
$ codetracer calculate_fee crates/ \
    --mode flow --scope

=== DEFINITIONS ===
crates/trading/src/fees.rs:42
  pub fn calculate_fee(
    order: &Order,
    fee_type: FeeType
  ) -> SagasResult<Price>
  scope: mod trading > impl FeeCalculator

=== CALL SITES (3) ===
  crates/trading/src/execution.rs:87
  crates/trading/src/settlement.rs:34
  crates/analytics/src/cost.rs:22

=== PRELUDE EXPORT ===
  trading::prelude::FeeCalculator
```

The upgraded pre-creation protocol for Rust, borrowing codetracer's principles:

```sh
# Step 1 — Does this symbol exist? (multi-pattern)
rg "(fn |struct |enum |trait |type |const ).*calculate_fee" crates/ --include="*.rs" -n

# Step 2 — Who owns it? (scope context)
rg -n "(fn |impl |mod ).*calculate_fee" crates/ --include="*.rs" -B 5

# Step 3 — Who uses it? (call sites)
rg -n "calculate_fee\s*[\(\.\::<]" crates/ --include="*.rs" -l

# Step 4 — Is it publicly available?
rg "pub use.*calculate_fee" crates/ --include="*.rs"
```

---

# Part IV — The Evolved Architecture

*From reactive compiler checks to a proactive, oracle-guided pipeline with persistent architectural memory. This is the novel contribution of this article.*

---

## 09 — Plan-Validate-Execute — From Reactive to Proactive

Every guardrail described so far is **reactive**. The AI writes code, then something catches the defect after the fact. For structural defects, this is cheap (compiler feedback loop). For architectural defects, the damage is done before detection.

The shift: AI coding agents have a **plan mode** that produces a plan before execution. What if the plan was validated against the codebase by an external oracle before a single line was written?

### What a Structured Plan Looks Like

**Raw plan (unstructured):**

```
I'll add a settlement fee calculation
to the trading module. I'll create a
new function that takes an order and
computes the fee based on the broker's
rate schedule. I'll also need a type
for representing currency amounts.
```

**Structured plan (actionable):**

```
## Planned Actions

CREATE: fn calculate_settlement_fee
  in: crates/trading/src/fees.rs
  sig: (order: &Order) -> SagasResult<Price>
  purpose: compute settlement fee

CREATE: struct CurrencyAmount
  in: crates/trading/src/types.rs
  purpose: represent fee amounts

MODIFY: fn execute_settlement
  in: crates/trading/src/settlement.rs
  change: call calculate_settlement_fee
```

The structured plan is parseable. Each `CREATE` and `MODIFY` action becomes an MCP tool call to the oracle. The raw plan is a monologue the oracle can't act on.

### The Full Round-Trip

```
CREATE: fn calculate_settlement_fee ──▶ Oracle checks registry ──▶ REUSE_EXISTING
                                         + codebase search         → USE existing calculate_fee
                                                                     with FeeType::Settlement

CREATE: struct CurrencyAmount ─────────▶ Oracle checks ──────────▶ USE_EXISTING_TYPE
                                         domain::prelude            → IMPORT domain::Price

MODIFY: fn execute_settlement ─────────▶ Oracle checks ──────────▶ SAFE_TO_MODIFY
                                         git + registry             → Proceed as planned
```

*Fig 3 — Three planned actions, three oracle validations, two revisions, one proceed. The AI never writes a duplicate or creates a redundant type.*

---

## 10 — The MCP Oracle — Concrete Round-Trips

The Plan Validator is an MCP server exposing five tools. Each answers a specific planning question, returning structured JSON that the AI uses to revise its plan.

### Tool 1: validate_creation — "Does this already exist?"

Planned action: `CREATE fn calculate_settlement_fee in crates/trading/src/fees.rs`

```json
{
  "action": "create_function",
  "symbol": "calculate_settlement_fee",
  "target_module": "crates/trading/src/fees.rs",
  "advisory": "REUSE_EXISTING",
  "existing": {
    "name": "calculate_fee",
    "location": "crates/trading/src/fees.rs:42",
    "signature": "pub fn calculate_fee(order: &Order, fee_type: FeeType) -> SagasResult<Price>",
    "scope": "mod trading > impl FeeCalculator",
    "call_sites": 3,
    "suggestion": "Use FeeType::Settlement as the fee_type parameter"
  }
}
```

### Tool 2: validate_dependency — "Who provides this capability?"

Planned action: `CREATE struct CurrencyAmount in crates/trading/src/types.rs`

```json
{
  "action": "create_type",
  "symbol": "CurrencyAmount",
  "advisory": "USE_EXISTING_TYPE",
  "existing": {
    "type": "Price",
    "location": "crates/domain/src/money.rs:8",
    "prelude_path": "domain::prelude::Price",
    "traits": ["CurrencyFormat", "Add", "Display"],
    "note": "Newtype over u64 representing cents. Already implements CurrencyFormat for IDR display."
  }
}
```

### Tool 3: validate_boundary — "Is this in the right module?"

Planned action: `CREATE fn fetch_order_by_id in crates/domain/src/order.rs`

```json
{
  "action": "create_function",
  "symbol": "fetch_order_by_id",
  "advisory": "BOUNDARY_VIOLATION",
  "violation": {
    "rule": "No infrastructure concerns (DB, HTTP, filesystem)",
    "rule_kind": "must_not",
    "crate": "domain",
    "reason": "Database query is an infrastructure concern"
  },
  "suggestion": {
    "target": "crates/infra/src/repositories/order_repo.rs",
    "pattern": "Define a trait in domain, implement in infra"
  }
}
```

### Tool 4: check_wip — "Is someone else working here?"

Planned action: `MODIFY fn execute_settlement in crates/trading/src/execution.rs`

```json
{
  "action": "modify_function",
  "module": "crates/trading/src/execution.rs",
  "advisory": "WIP_CONFLICT",
  "conflict": {
    "agent": "agent-2",
    "branch": "feat/settlement-v2",
    "symbols_in_progress": ["execute_settlement", "validate_settlement_params"],
    "last_activity": "12 minutes ago"
  },
  "recommendation": "ESCALATE_TO_HUMAN — coordination decision required"
}
```

### Tool 5: suggest_reuse — "Here's the idiomatic path"

Planned action: `IMPLEMENT feature: format order total for display`

```json
{
  "action": "implement_feature",
  "description": "format order total for display",
  "advisory": "REUSE_WITH_PATTERN",
  "suggestion": {
    "use_trait": "domain::CurrencyFormat",
    "call_pattern": "order.total.format_idr()",
    "example_location": "crates/trading/src/reports.rs:67",
    "import": "use domain::prelude::CurrencyFormat;"
  }
}
```

### Advisory Resolution Summary

| Advisory | AI Action | Human? | Frequency (est.) |
|---|---|---|---|
| `SAFE_TO_CREATE` | Proceed | No | ~40% |
| `REUSE_EXISTING` | Revise → use existing | No | ~25% |
| `USE_EXISTING_TYPE` | Revise → import | No | ~15% |
| `REUSE_WITH_PATTERN` | Revise → follow pattern | No | ~8% |
| `BOUNDARY_VIOLATION` | Revise → correct module | No | ~5% |
| `WIP_CONFLICT` | **Pause → escalate** | Yes | ~4% |
| `AMBIGUOUS_MATCH` | **Present options** | Yes | ~3% |

Roughly 93% of advisories are auto-resolved — the AI revises its plan without human involvement. The human is only consulted for coordination decisions and genuine ambiguity.

### The Quadripartite Model

```
┌──────────┐   Intent    ┌──────────┐  Structured   ┌──────────┐  Advisories  ┌──────────┐
│ Architect │───────────▶│Plan Mode │──actions─────▶│  Oracle  │────────────▶│ Revised  │
│  (Human)  │            │(AI plans)│              │(MCP Val.)│            │  Plan    │
└────┬──────┘            └──────────┘              └────┬─────┘            └────┬─────┘
     ▲                                                  │                      │
     │  WIP/ambiguous                                   │                      ▼
     └──────────────────────────────────────────────────┘               ┌──────────┐  Code   ┌──────────┐
                                                                       │  Typist  │────────▶│ Verifier │
                                                                       │(AI exec.)│◀───err──│  (rustc) │
                                                                       └──────────┘         └────┬─────┘
                                                                              ▲                   │
                                                                              │              ┌────▼─────┐
                                                                              └──Logic Bug───│ Runtime  │
                                                                                             └──────────┘
```

*Fig 4 — The four-node model. The Oracle holds the minimap. The AI still operates under fog of war, but consults the oracle before every move.*

---

## 11 — The Architecture Registry — Schema, Queries, and Lifecycle

The Oracle needs something to query. The value is in a **structural metadata layer** that knows what exists, who owns it, and how things relate — without storing implementation.

### The Schema

```
CRATE (name PK, path, purpose, status, bounded_context)
  ├── MODULE (path PK, crate_name FK, purpose, status)
  │     ├── SYMBOL (id PK, name, kind, signature, module_path FK, visibility, status, created_by)
  │     │     ├── RELATIONSHIP (id PK, source_symbol FK, target_symbol FK, kind)
  │     │     └── RELATIONSHIP (target)
  │     └── CONTRACT (id PK, trait_name, implementor, module_path FK)
  └── OWNERSHIP_RULE (id PK, crate_name FK, rule, kind)
```

*Fig 5 — Six tables. Stores signatures, not bodies. A 50,000-line codebase produces ~500 rows (~300 KB).*

### The Queries the Oracle Actually Runs

**validate_creation → fuzzy symbol lookup:**

```sql
SELECT s.name, s.kind, s.signature, s.module_path,
       s.visibility, s.status, s.created_by
FROM symbol s
WHERE s.name LIKE '%settlement_fee%'
   OR s.name LIKE '%calculate_fee%';
-- Returns: calculate_fee, fn, pub, stable, crates/trading/src/fees.rs:42
```

**validate_dependency → trait/type capability lookup:**

```sql
SELECT s.name, s.signature, s.module_path, c.bounded_context
FROM symbol s
JOIN module m ON s.module_path = m.path
JOIN crate c ON m.crate_name = c.name
WHERE s.kind IN ('struct', 'trait')
  AND (s.name LIKE '%Currency%' OR s.name LIKE '%Price%' OR s.name LIKE '%Amount%');
-- Returns: Price, struct, domain; CurrencyFormat, trait, domain
```

**validate_boundary → ownership rule check:**

```sql
SELECT rule, kind
FROM ownership_rule
WHERE crate_name = 'domain' AND kind = 'must_not';
-- Returns: "No infrastructure concerns (DB, HTTP, filesystem)"
```

**check_wip → in-progress symbol scan:**

```sql
SELECT s.name, s.status, s.created_by, s.updated_at
FROM symbol s
WHERE s.module_path = 'crates/trading/src/execution.rs'
  AND s.status IN ('planned', 'wip');
-- Returns: execute_settlement, wip, agent-2, 12 min ago
```

**suggest_reuse → capability + contract lookup:**

```sql
SELECT s.name, s.signature, s.module_path,
       c.trait_name, c.implementor
FROM symbol s
LEFT JOIN contract c ON s.module_path = c.module_path
WHERE s.kind = 'fn'
  AND s.visibility IN ('pub', 'pub_crate')
  AND s.name LIKE '%format%';
-- Returns: format_idr, CurrencyFormat trait, impl for Price
```

All queries return in under 2ms against the expected data volume.

### Three Population Triggers

```
Trigger 1: Human Seed (Day 0)       Trigger 2: Post-Plan (before code)    Trigger 3: Post-Compile (after build)
Cargo.toml → crates                 Approved plan → PLANNED symbols       cargo metadata → signatures,
seed.sql → rules                    + agent ID                            traits, impl blocks, pub use
          │                                    │                                       │
          └────────────────────────────────────┼───────────────────────────────────────┘
                                               ▼
                                         registry.db
```

*Fig 6 — Three triggers, each firing at a different moment in the development cycle.*

**Trigger 1: Human Seed (Day 0, ~5 minutes)**

```sh
arch-registry init --workspace ./Cargo.toml
arch-registry add-rule domain "No infrastructure concerns"       --kind must_not
arch-registry add-rule domain "All types use newtype pattern"      --kind must
arch-registry add-rule trading "Consume domain::Price, not own type" --kind must
```

Result: 5 crates, 3–5 rules, 0 symbols. The oracle can already catch boundary violations before any function exists.

**Trigger 2: Post-Plan Registration (before code is written)**

When a plan is approved, planned symbols are registered with status `planned`. This is the **namespace reservation** that prevents duplicate planning by concurrent agents.

**Trigger 3: Post-Compile Sync (after successful build)**

After `cargo build`, a lightweight indexer syncs the registry. It parses `cargo metadata` output and runs targeted `syn`-based extraction on changed files only. It extracts:

| Extracted | Source | Not Extracted |
|---|---|---|
| Function signatures (`pub fn name(params) -> Return`) | `fn` declarations | Function bodies |
| Type definitions (`struct`, `enum`, `type`) | Item declarations | Impl block internals |
| Trait contracts (method signatures) | `trait` blocks | Default implementations |
| Trait implementations (`impl Trait for Type`) | `impl` blocks | Method bodies |
| Re-exports (`pub use`) | Prelude files | Internal `use` statements |
| Cross-crate dependencies | `Cargo.toml` deps | Transitive deps |

### File System Layout

```
.architect/
├── seed.sql          # committed — human-authored decisions, diffable, reviewable
├── registry.db       # gitignored — rebuilt from source + seed on checkout
└── sync.log          # gitignored — last sync timestamp
```

Ownership rules and crate purposes are versioned as code. The symbol index is derived metadata, rebuilt fresh.

---

## 12 — The Evolved Defense Ladder

| Phase | Layer | Verifier | Catches | Nature |
|---|---|---|---|---|
| **Pre-Execution** | Oracle: validate_creation | Registry + codebase search | Duplicate functions and types | Automated |
| | Oracle: validate_dependency | Registry symbol lookup | Reinventing existing types/traits | Automated |
| | Oracle: validate_boundary | Ownership rules | DDD boundary violations | Automated |
| | Oracle: check_wip | Registry + git state | Multi-agent conflicts | Semi-auto (escalates) |
| | Oracle: suggest_reuse | Registry + contracts | Suboptimal implementation paths | Automated |
| **Post-Execution** | L0: rustc | Compiler | Memory, ownership, types, concurrency | Automatic |
| | L1: Newtypes + Typestate | Compiler (type system) | Semantic type confusion, invalid transitions | Automatic |
| | L2: clippy + cargo udeps | Linter | Anti-patterns, dead code hinting at duplication | Semi-automatic |
| **Post-Build** | Registry sync | Post-compile hook | Keeps registry current; transitions planned → stable | Automatic |
| **Human** | Architectural review | Human judgment | Business logic, intent alignment, novel decisions | Manual — focused |

> **KEY INSIGHT:** The old L4 (CLAUDE.md manifest) and L5 (grep check) are **subsumed** by the Oracle's pre-execution validation. The human review layer narrows from "check everything" to "check business logic only."

---

# Part V — Assessment

*Economics, residual risk, and the direction forward.*

---

## 13 — Cost Analysis

| Cost Center | No Guardrails | Rust + Manifest | + Oracle + Registry |
|---|---|---|---|
| **Structural bug hunting** | Ongoing, per-task | Zero (compiler) | Zero (compiler) |
| **Semantic bug hunting** | Ongoing, hard to spot | Near-zero (newtypes) | Near-zero (newtypes) |
| **Architectural review** | Expensive, quadratic | Reduced (manifest) | Greatly reduced (oracle prevents at plan stage) |
| **Context prep per task** | ~2000 tokens | ~250 tokens | ~50 tokens (oracle provides JIT advisories) |
| **Duplicate detection** | Human review only | Keyword grep | Multi-convention, scope-aware, proactive |
| **Multi-agent coordination** | Manual chat | Manual chat | Semi-automatic (registry WIP tracking) |
| **Upfront investment** | Low | Medium | Higher (types, traits, registry, MCP, seed) |
| **Break-even point** | — | ~5K lines | ~10K lines |

> *The trade: upfront design cost for near-elimination of ongoing verification cost. In a workflow generating thousands of lines per week, this is overwhelmingly favorable.*

---

## 14 — The Residual — What Still Requires Human Eyes

| Residual Defect | Example | Why No Guardrail Catches It |
|---|---|---|
| **Wrong business logic** | Discount is 15% instead of 20% | Types correct, logic consistent — doesn't match requirements |
| **Wrong API contract** | External API returns cents, caller treats as whole currency | Newtypes help internally; external APIs return raw types |
| **Architectural misfit** | Flat struct where a state machine is needed | Compiles fine; requires judgment about modeling fitness |
| **Performance pathology** | O(n³) where O(n log n) is possible | Safe Rust prevents *unsafe*, not *slow* |
| **Missing requirements** | AI builds exactly what was asked, but the ask was incomplete | No system catches what was never stated |
| **Registry drift** | Seed rules reference refactored-away functions | Human-authored docs require human maintenance |
| **Novel architecture** | Should this feature be its own crate? | The Architect's core job — no oracle replaces it |

This residual is qualitatively different from the automated-away defects. Structural, semantic, and architectural defects are *mechanical* failures from the AI's context limitations. The residual defects are *judgment* failures requiring business understanding and domain expertise.

> **KEY INSIGHT:** The shift is from verification to validation. Verification — did we build it correctly? — is increasingly automated by the compiler, type system, and oracle. Validation — did we build the correct thing? — remains the human's irreducible contribution.

---

## 15 — Closing — From Compiler Bouncer to Institutional Memory

The system evolved through four phases, each solving a gap the previous one revealed:

```
Phase 1              Phase 2              Phase 3              Phase 4
Compiler      ──▶    Newtype +     ──▶    Manifest +    ──▶    Oracle +
Firewall             Typestate            Enforcement          Registry

structural ✓         semantic ✓           reactive —           proactive,
semantic ✗           architectural ✗      damage before        pre-execution,
                                          detection            institutional memory
```

*Fig 8 — Four phases. Each solves the gap revealed by the previous one.*

The Minesweeper metaphor evolved with the system. We started with "make the mines self-announcing" — use the compiler to reject invalid steps. We progressed to "give the AI a minimap" — manifests and discovery tools. We arrived at "build institutional memory" — a persistent, growing registry that ensures no agent operates in complete darkness.

The registry is the board. Not in the AI's head, not in a 2,000-token prompt, not in prose documentation. In a structured, indexed, incrementally-built database that any agent can query, any hook can update, and the architect governs through versioned seed rules.

> *Guardrails ensure the AI does things right. The human ensures we build the right thing.*

---

*Written from a year of practice building production systems with AI-assisted Rust development. The MCP Oracle and Architecture Registry represent the current frontier, actively being refined through implementation.*

*External reference: [codetracer](https://github.com/mirzalazuardi/codetracer) by Mirza Lazuardi provided independent validation of the discovery-layer thesis.*

---

# Part VI — Further Assessment (Proposed)

*Aspects to evaluate for advancing this vibe coding initiative with Rust.*

---

## 16 — Aspects for Further Assessment

The article presents a compelling four-phase evolution from compiler firewall to institutional memory. However, to move from a practitioner's narrative to a repeatable, scalable methodology, the following dimensions require deeper investigation.

### A. Empirical Validation & Metrics

| Aspect | What to Assess | Why It Matters |
|---|---|---|
| **Defect escape rate by class** | Measure actual Class I/II/III defect counts per 1,000 AI-generated lines, before and after each phase | The article's frequency estimates (~40% SAFE_TO_CREATE, ~25% REUSE_EXISTING, etc.) are stated without methodology. Empirical data turns anecdote into evidence. |
| **False positive/negative rate of the Oracle** | Track how often the Oracle blocks valid creations or misses true duplicates | An Oracle with high false-positive rate creates AI "learned helplessness" — it stops proposing things. High false-negative rate means duplicates still leak through. |
| **Time-to-detection vs. time-to-resolution** | Compare the wall-clock cost of finding vs. fixing a Class III defect reactively (grep) vs. proactively (Oracle) | The article claims proactive is better but doesn't quantify the actual developer-time savings. |
| **Break-even validation** | Verify the stated ~5K/~10K line break-even points with actual project data | These thresholds determine whether the system is viable for small/medium Rust projects or only large ones. |

### B. Oracle & Registry Robustness

| Aspect | What to Assess | Why It Matters |
|---|---|---|
| **Fuzzy matching quality** | How well does `LIKE '%settlement_fee%'` catch semantically equivalent but differently named symbols (e.g., `compute_settle_cost`)? | The SQL-based fuzzy match is the weakest link in the Oracle. Semantic similarity (embedding-based or AST-based) may be needed. |
| **Registry staleness and self-healing** | What happens when Trigger 3 (post-compile sync) fails, is skipped, or the developer refactors outside the registry-aware workflow? | The article acknowledges "registry drift" as a residual risk but doesn't propose mitigation beyond "human maintenance." Automated consistency checks (registry vs. actual codebase) need design. |
| **Scale behavior** | How does the registry perform at 100K, 500K, 1M lines? Does the `syn`-based extraction remain incremental? | The claim of ~500 rows for 50K lines and <2ms queries needs stress testing. Monorepo-scale Cargo workspaces with 50+ crates may surface bottlenecks. |
| **Cross-workspace and polyglot boundaries** | What happens when Rust crates depend on FFI, WASM modules, or external services with non-Rust interfaces? | The registry assumes a pure-Rust, Cargo-native world. Real systems have gRPC protos, OpenAPI specs, and database schemas that are equally important architectural surfaces. |

### C. AI Agent Behavioral Dynamics

| Aspect | What to Assess | Why It Matters |
|---|---|---|
| **Structured plan compliance** | How reliably do different LLMs (Claude, GPT-4, Codex, open-source) emit parseable structured plans vs. free-form prose? | The entire Oracle pipeline depends on the AI producing `CREATE`/`MODIFY` action blocks. If compliance is <90%, the system has a critical reliability gap. |
| **Advisory adherence** | When the Oracle says `REUSE_EXISTING`, does the AI actually reuse, or does it rationalize creating a new symbol anyway? | Advisory-is-advisory unless there's a hard enforcement mechanism. This is the same CLAUDE.md problem at a different layer. |
| **Context window pressure** | How does Oracle validation round-trip cost (5 tool calls per planned action) affect context budget for the actual code generation? | Each Oracle call consumes context. For complex features with 10+ planned actions, the validation pass alone could consume 30-50% of the context window. |
| **Prompt engineering fragility** | How sensitive is the system to prompt wording changes in CLAUDE.md for plan formatting, Oracle interaction protocol, etc.? | A system that breaks when you rephrase a sentence in CLAUDE.md is brittle. Assess robustness to paraphrasing. |

### D. Workflow Integration & Developer Experience

| Aspect | What to Assess | Why It Matters |
|---|---|---|
| **Onboarding friction** | How long does it take a new developer (or new AI agent) to become productive within this system? | High-ceremony systems risk adoption failure. Measure: time from `git clone` to first Oracle-validated contribution. |
| **Escape hatch design** | What happens when a legitimate task *requires* violating a boundary rule (e.g., emergency hotfix in domain crate)? | Every enforcement system needs a well-designed override. `settings.json` deny rules have no nuance — they block everything. The article doesn't address escalation for legitimate rule-breaking. |
| **IDE/toolchain integration** | Can the Oracle advisory surface in the editor (LSP diagnostics, inline hints) rather than only in the CLI agent loop? | If the Oracle only works inside Claude Code's agent loop, developers using other editors or AI tools get no benefit. |
| **CI/CD pipeline integration** | How does the Oracle + Registry fit into existing CI? Can the registry sync be a build step? Can Oracle violations fail a PR? | The article focuses on the interactive developer loop but doesn't address how this integrates with team-scale automation. |

### E. Comparative & Adversarial Analysis

| Aspect | What to Assess | Why It Matters |
|---|---|---|
| **Comparison with alternative approaches** | How does this system compare against: (a) pure test-driven vibe coding (no types, heavy tests), (b) formal verification tools like Prusti/Creusot, (c) AI-native code review tools (CodeRabbit, Sourcery)? | The article compares languages but not methodologies. A TypeScript project with comprehensive property-based tests might match Rust + Oracle for some defect classes. |
| **Adversarial robustness** | What happens when the AI actively works around the Oracle? (e.g., creates a symbol with a subtly different name to avoid `REUSE_EXISTING`) | AI systems are stochastic — they sometimes find creative paths around constraints. Stress-test the system with adversarial prompts. |
| **Non-Rust applicability** | How much of this architecture (tiered manifests, Oracle, Registry) transfers to TypeScript, Go, or other typed languages? | If the system only works with Rust, the audience is narrow. Identifying the Rust-specific vs. language-agnostic components expands impact. |

### F. Organizational & Economic Dimensions

| Aspect | What to Assess | Why It Matters |
|---|---|---|
| **Team-scale dynamics** | How does the system behave with 3-5 concurrent AI agents + 2-3 human developers? | The `check_wip` tool addresses this in theory, but concurrent registry writes, merge conflicts in `seed.sql`, and agent coordination at team scale are untested. |
| **Total cost of ownership** | What's the ongoing maintenance burden of `seed.sql`, registry sync scripts, MCP server, and CLAUDE.md hierarchy? | Upfront investment is acknowledged; TCO is not. Who maintains the Oracle when the domain model changes? |
| **Skill distribution shift** | How does this system change the skill profile needed in the team? More architects, fewer line-level coders? | The article implies the Architect role becomes dominant. Assess whether this creates a bottleneck or a leverage point. |
| **Failure mode cascades** | If the Oracle is down/wrong, does the system degrade gracefully to the Phase 2/3 layers, or does it fail open (no checks at all)? | Graceful degradation design is not addressed. A system that's either fully on or fully off is fragile in production. |

### G. Implementation Roadmap Priorities

Based on the above, the recommended assessment order for the `rulest` initiative:

1. **Oracle prototype with real codebase** — Build the MCP server against an actual Cargo workspace. Measure false positive/negative rates on real AI-generated plans.
2. **Structured plan compliance testing** — Test plan format compliance across Claude Opus, Sonnet, and Haiku. Establish minimum reliability threshold.
3. **Registry sync benchmarking** — Measure incremental `syn` extraction time on codebases of 10K, 50K, and 100K lines.
4. **Escape hatch and degradation design** — Define how the system behaves when the Oracle is unavailable or a rule needs temporary override.
5. **CI integration spike** — Prototype registry validation as a GitHub Actions step that fails PRs with unresolved `BOUNDARY_VIOLATION` advisories.
6. **Comparative benchmark** — Run the same 10 AI coding tasks with (a) Rust + Oracle, (b) Rust + manifest only, (c) TypeScript + tests. Measure defect rates and developer time.

---

> *This assessment framework transforms the Minesweeper Problem from a narrative into a testable hypothesis: that Rust's type system + structured pre-execution validation + persistent architectural memory produces measurably fewer defects per AI-generated line than alternative approaches, at a total cost that decreases with codebase scale.*
