use serde::{Deserialize, Serialize};

/// An existing symbol found during validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExistingSymbol {
    pub name: String,
    pub kind: String,
    pub module_path: String,
    pub crate_name: String,
    pub signature: Option<String>,
    pub visibility: String,
}

/// A suggestion for which module to use instead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleSuggestion {
    pub module_path: String,
    pub crate_name: String,
    pub reason: String,
}

/// Advisory responses from Oracle queries.
///
/// Each variant tells the AI agent what to do based on the registry state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Advisory {
    /// No conflicts — safe to create the new symbol.
    SafeToCreate {
        symbol: String,
        target: String,
    },

    /// An existing symbol matches — reuse it instead of creating a new one.
    ReuseExisting {
        existing: ExistingSymbol,
        suggestion: String,
    },

    /// A type already exists — use it via the prelude/re-export path.
    UseExistingType {
        existing: ExistingSymbol,
        prelude_path: String,
    },

    /// Creating this symbol in the target crate violates an ownership rule.
    BoundaryViolation {
        rule: String,
        crate_name: String,
        suggestion: ModuleSuggestion,
    },

    /// Another agent has WIP or planned symbols in this area.
    WipConflict {
        agent: String,
        branch: Option<String>,
        symbols: Vec<String>,
    },

    /// Multiple candidates match — human/agent must disambiguate.
    AmbiguousMatch {
        candidates: Vec<ExistingSymbol>,
    },

    /// A reusable trait/pattern exists — use it with this call pattern.
    ReuseWithPattern {
        trait_name: String,
        call_pattern: String,
        example: String,
    },
}
