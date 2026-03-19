use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A Cargo workspace member crate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Crate {
    pub id: Option<i64>,
    pub name: String,
    pub path: String,
    pub description: Option<String>,
    pub bounded_context: Option<String>,
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
    pub line_number: Option<u32>,
    pub scope: Option<String>,
    pub status: SymbolStatus,
    pub created_by: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
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
    ReExport,
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
            Self::ReExport => "re_export",
        }
    }
}

impl FromStr for SymbolKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "function" => Ok(Self::Function),
            "struct" => Ok(Self::Struct),
            "enum" => Ok(Self::Enum),
            "trait" => Ok(Self::Trait),
            "type_alias" => Ok(Self::TypeAlias),
            "const" => Ok(Self::Const),
            "static" => Ok(Self::Static),
            "macro" => Ok(Self::Macro),
            "re_export" => Ok(Self::ReExport),
            _ => Err(format!("Invalid symbol kind: '{}'", s)),
        }
    }
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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
}

impl FromStr for Visibility {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "public" => Ok(Self::Public),
            "crate_local" => Ok(Self::CrateLocal),
            "private" => Ok(Self::Private),
            _ => Err(format!("Invalid visibility: '{}'", s)),
        }
    }
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolStatus {
    Stable,
    Planned,
    Wip,
    Deprecated,
}

impl SymbolStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Planned => "planned",
            Self::Wip => "wip",
            Self::Deprecated => "deprecated",
        }
    }
}

impl FromStr for SymbolStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "stable" => Ok(Self::Stable),
            "planned" => Ok(Self::Planned),
            "wip" => Ok(Self::Wip),
            "deprecated" => Ok(Self::Deprecated),
            _ => Err(format!("Invalid symbol status: '{}'", s)),
        }
    }
}

impl fmt::Display for SymbolStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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
}

impl FromStr for RelationshipKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "calls" => Ok(Self::Calls),
            "implements" => Ok(Self::Implements),
            "depends_on" => Ok(Self::DependsOn),
            _ => Err(format!("Invalid relationship kind: '{}'", s)),
        }
    }
}

impl fmt::Display for RelationshipKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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
}

impl FromStr for ContractKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "precondition" => Ok(Self::Precondition),
            "postcondition" => Ok(Self::Postcondition),
            "invariant" => Ok(Self::Invariant),
            _ => Err(format!("Invalid contract kind: '{}'", s)),
        }
    }
}

impl fmt::Display for ContractKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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
}

impl FromStr for OwnershipRuleKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "must_own" => Ok(Self::MustOwn),
            "must_not" => Ok(Self::MustNot),
            "shared_with" => Ok(Self::SharedWith),
            _ => Err(format!("Invalid ownership rule kind: '{}'. Use: must_own, must_not, shared_with", s)),
        }
    }
}

impl fmt::Display for OwnershipRuleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
