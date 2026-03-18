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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_sites: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

/// A suggestion for which module to use instead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleSuggestion {
    pub module_path: String,
    pub crate_name: String,
    pub reason: String,
}

/// A planned action from a structured AI plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlannedAction {
    /// "create" or "modify"
    pub action: String,
    /// Symbol name (e.g. "calculate_settlement_fee")
    pub symbol: String,
    /// Target file path (e.g. "crates/trading/src/fees.rs")
    pub target: String,
    /// Target crate name (e.g. "trading"), derived from target path
    pub crate_name: Option<String>,
}

/// Result of validating a single planned action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanValidationResult {
    pub action: PlannedAction,
    pub advisories: Vec<Advisory>,
}

/// Result of validating an entire plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanReport {
    pub results: Vec<PlanValidationResult>,
    pub summary: PlanSummary,
}

/// Summary counts for a plan validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanSummary {
    pub total_actions: usize,
    pub safe: usize,
    pub reuse: usize,
    pub violations: usize,
    pub conflicts: usize,
    pub ambiguous: usize,
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
