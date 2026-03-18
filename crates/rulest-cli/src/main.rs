mod init;
mod query;
mod rule;
mod scaffold;
mod serve;
mod sync;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "rulest", version, about = "Architecture Registry & MCP Oracle for Rust projects")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize the architecture registry for a workspace
    Init {
        /// Path to the workspace Cargo.toml
        #[arg(short, long, default_value = "Cargo.toml")]
        workspace: String,
    },

    /// Add an ownership rule
    AddRule {
        /// Crate name the rule applies to
        crate_name: String,

        /// Description of the rule
        description: String,

        /// Rule kind: must_own, must_not, shared_with
        #[arg(short, long, default_value = "must_own")]
        kind: String,
    },

    /// Sync the registry from workspace source code
    Sync {
        /// Path to the workspace Cargo.toml
        #[arg(short, long, default_value = "Cargo.toml")]
        workspace: String,

        /// Force full resync (ignore mtime cache)
        #[arg(long)]
        full: bool,
    },

    /// Query the registry
    Query {
        /// Symbol name to search for
        symbol: Option<String>,

        /// Validate creation of a symbol in a target module
        #[arg(long)]
        validate_creation: Option<String>,

        /// Target module for creation validation
        #[arg(long)]
        target: Option<String>,

        /// Validate a type/dependency lookup
        #[arg(long)]
        validate_dependency: Option<String>,

        /// Validate boundary rules for a crate
        #[arg(long)]
        validate_boundary: Option<String>,

        /// Target crate for boundary validation
        #[arg(long)]
        crate_name: Option<String>,

        /// Check WIP symbols in a module path
        #[arg(long)]
        check_wip: Option<String>,

        /// Search for reusable symbols
        #[arg(long)]
        suggest_reuse: Option<String>,
    },

    /// Start the MCP server (JSON-RPC over stdio)
    Serve {
        /// Path to the registry database
        #[arg(short, long, default_value = ".architect/registry.db")]
        db: String,
    },

    /// Generate CLAUDE.md, settings.json, and seed.sql templates
    Scaffold {
        /// Path to the workspace Cargo.toml
        #[arg(short, long, default_value = "Cargo.toml")]
        workspace: String,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { workspace } => init::run(&workspace),
        Commands::AddRule {
            crate_name,
            description,
            kind,
        } => rule::run(&crate_name, &description, &kind),
        Commands::Sync { workspace, full } => sync::run(&workspace, full),
        Commands::Query {
            symbol,
            validate_creation,
            target,
            validate_dependency,
            validate_boundary,
            crate_name,
            check_wip,
            suggest_reuse,
        } => query::run(
            symbol.as_deref(),
            validate_creation.as_deref(),
            target.as_deref(),
            validate_dependency.as_deref(),
            validate_boundary.as_deref(),
            crate_name.as_deref(),
            check_wip.as_deref(),
            suggest_reuse.as_deref(),
        ),
        Commands::Serve { db } => serve::run(&db),
        Commands::Scaffold { workspace } => scaffold::run(&workspace),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
