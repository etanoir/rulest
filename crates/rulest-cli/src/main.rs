mod build;
mod init;
mod query;
mod register;
mod rule;
mod scaffold;
mod serve;
mod status;
mod sync;
mod validate;

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

        /// Path to the workspace Cargo.toml
        #[arg(short, long, default_value = "Cargo.toml")]
        workspace: String,
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

        /// Path to the registry database
        #[arg(short, long, default_value = ".architect/registry.db")]
        db: String,

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

    /// Validate a structured plan file against the registry
    Validate {
        /// Path to the plan file
        plan: String,

        /// Path to the registry database
        #[arg(short, long, default_value = ".architect/registry.db")]
        db: String,
    },

    /// Register planned symbols from a plan file into the registry
    RegisterPlan {
        /// Path to the plan file
        plan: String,

        /// Agent identifier (e.g. "claude-agent-1")
        #[arg(short, long, default_value = "cli")]
        agent: String,

        /// Path to the registry database
        #[arg(short, long, default_value = ".architect/registry.db")]
        db: String,
    },

    /// Show registry statistics
    Status {
        /// Path to the registry database
        #[arg(short, long, default_value = ".architect/registry.db")]
        db: String,
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

    /// Build the workspace and auto-sync the registry
    Build {
        /// Path to the workspace Cargo.toml
        #[arg(short, long, default_value = "Cargo.toml")]
        workspace: String,

        /// Additional arguments to pass to cargo build
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        cargo_args: Vec<String>,
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
            workspace,
        } => rule::run(&crate_name, &description, &kind, &workspace),
        Commands::Sync { workspace, full } => sync::run(&workspace, full),
        Commands::Query {
            symbol,
            db,
            validate_creation,
            target,
            validate_dependency,
            validate_boundary,
            crate_name,
            check_wip,
            suggest_reuse,
        } => query::run(query::QueryArgs {
            symbol,
            db,
            validate_creation,
            target,
            validate_dependency,
            validate_boundary,
            crate_name,
            check_wip,
            suggest_reuse,
        }),
        Commands::Validate { plan, db } => validate::run(&plan, &db),
        Commands::RegisterPlan { plan, agent, db } => register::run(&plan, &db, &agent),
        Commands::Status { db } => status::run(&db),
        Commands::Serve { db } => serve::run(&db),
        Commands::Scaffold { workspace } => scaffold::run(&workspace),
        Commands::Build {
            workspace,
            cargo_args,
        } => build::run(&workspace, &cargo_args),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
