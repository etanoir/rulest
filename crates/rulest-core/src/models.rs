use serde::{Deserialize, Serialize};

/// A Cargo workspace member crate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Crate {
    pub id: Option<i64>,
    pub name: String,
    pub path: String,
    pub description: Option<String>,
}

/// A Rust module (source file) within a crate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Module {
    pub id: Option<i64>,
    pub crate_id: i64,
    pub path: String,
    pub name: String,
}

/// A symbol (function, struct, enum, trait, type alias) within a module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub id: Option<i64>,
    pub module_id: i64,
    pub name: String,
    pub kind: SymbolKind,
    pub visibility: Visibility,
    pub signature: Option<String>,
    pub status: SymbolStatus,
}

/// Relationships between symbols (calls, implements, depends_on).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    pub id: Option<i64>,
    pub from_symbol_id: i64,
    pub to_symbol_id: i64,
    pub kind: RelationshipKind,
}

/// A contract or invariant attached to a symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub id: Option<i64>,
    pub symbol_id: i64,
    pub kind: ContractKind,
    pub description: String,
}

/// An ownership rule for a crate/module boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnershipRule {
    pub id: Option<i64>,
    pub crate_name: String,
    pub description: String,
    pub kind: OwnershipRuleKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    TypeAlias,
    Const,
    Static,
    Macro,
}

impl SymbolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::TypeAlias => "type_alias",
            Self::Const => "const",
            Self::Static => "static",
            Self::Macro => "macro",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "function" => Some(Self::Function),
            "struct" => Some(Self::Struct),
            "enum" => Some(Self::Enum),
            "trait" => Some(Self::Trait),
            "type_alias" => Some(Self::TypeAlias),
            "const" => Some(Self::Const),
            "static" => Some(Self::Static),
            "macro" => Some(Self::Macro),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    CrateLocal,
    Private,
}

impl Visibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::CrateLocal => "crate_local",
            Self::Private => "private",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "public" => Some(Self::Public),
            "crate_local" => Some(Self::CrateLocal),
            "private" => Some(Self::Private),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolStatus {
    Stable,
    Planned,
    Wip,
}

impl SymbolStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Planned => "planned",
            Self::Wip => "wip",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "stable" => Some(Self::Stable),
            "planned" => Some(Self::Planned),
            "wip" => Some(Self::Wip),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    Calls,
    Implements,
    DependsOn,
}

impl RelationshipKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Calls => "calls",
            Self::Implements => "implements",
            Self::DependsOn => "depends_on",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "calls" => Some(Self::Calls),
            "implements" => Some(Self::Implements),
            "depends_on" => Some(Self::DependsOn),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractKind {
    Precondition,
    Postcondition,
    Invariant,
}

impl ContractKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Precondition => "precondition",
            Self::Postcondition => "postcondition",
            Self::Invariant => "invariant",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "precondition" => Some(Self::Precondition),
            "postcondition" => Some(Self::Postcondition),
            "invariant" => Some(Self::Invariant),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnershipRuleKind {
    MustOwn,
    MustNot,
    SharedWith,
}

impl OwnershipRuleKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MustOwn => "must_own",
            Self::MustNot => "must_not",
            Self::SharedWith => "shared_with",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "must_own" => Some(Self::MustOwn),
            "must_not" => Some(Self::MustNot),
            "shared_with" => Some(Self::SharedWith),
            _ => None,
        }
    }
}
