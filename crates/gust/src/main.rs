//! Gust - A blazing fast Swift package manager.

use clap::{Parser, Subcommand};
use miette::Result;
use std::path::PathBuf;

mod commands;
mod install;

#[derive(Parser)]
#[command(name = "gust")]
#[command(version, about = "A blazing fast Swift package manager", long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(flatten)]
    global: GlobalOptions,

    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Args)]
struct GlobalOptions {
    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Suppress all output
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Disable colored output
    #[arg(long, global = true)]
    no_color: bool,

    /// Path to Gust.toml or Package.swift
    #[arg(long, global = true)]
    manifest: Option<PathBuf>,

    /// Number of parallel jobs
    #[arg(short, long, global = true)]
    jobs: Option<usize>,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new package
    New {
        /// Package name
        name: String,
        /// Package type
        #[arg(long, default_value = "library")]
        r#type: String,
    },

    /// Initialize a package in the current directory
    Init {
        /// Package name (defaults to directory name)
        #[arg(long)]
        name: Option<String>,
    },

    /// Add a dependency
    Add {
        /// Package name (optionally with version: package@1.0)
        package: String,
        /// Git repository URL
        #[arg(long)]
        git: Option<String>,
        /// Local path
        #[arg(long)]
        path: Option<PathBuf>,
        /// Add as dev dependency
        #[arg(long)]
        dev: bool,
    },

    /// Remove a dependency
    Remove {
        /// Package name
        package: String,
    },

    /// Update dependencies
    Update {
        /// Specific package to update
        package: Option<String>,
        /// Allow breaking version updates
        #[arg(long)]
        breaking: bool,
    },

    /// Install dependencies
    Install {
        /// Error if lockfile is out of date
        #[arg(long)]
        frozen: bool,
    },

    /// Build the package
    Build {
        /// Build in release mode
        #[arg(long, short)]
        release: bool,
        /// Specific target to build
        #[arg(long)]
        target: Option<String>,
        /// Disable binary artifact caching
        #[arg(long)]
        no_cache: bool,
    },

    /// Run the executable
    Run {
        /// Executable to run
        target: Option<String>,
        /// Arguments to pass to the executable
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Run tests
    Test {
        /// Specific test target
        target: Option<String>,
        /// Filter tests by name
        #[arg(long)]
        filter: Option<String>,
    },

    /// Clean build artifacts
    Clean {
        /// Also remove dependency checkouts
        #[arg(long)]
        deps: bool,
    },

    /// Show dependency tree
    Tree {
        /// Maximum depth to display
        #[arg(long)]
        depth: Option<usize>,
        /// Show duplicate versions
        #[arg(long)]
        duplicates: bool,
    },

    /// Check for outdated dependencies
    Outdated,

    /// Manage global cache
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },

    /// Migrate Package.swift to Gust.toml
    Migrate,

    /// Show package info
    Info {
        /// Package name
        package: String,
    },
}

#[derive(Subcommand)]
enum CacheAction {
    /// List cached packages
    List,
    /// Show cache statistics
    Stats,
    /// Clean cached packages
    Clean {
        /// Remove all cached packages
        #[arg(long)]
        all: bool,
        /// Only clear binary artifact cache
        #[arg(long)]
        binary: bool,
    },
    /// Print cache directory path
    Path,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Setup error handling
    gust_diagnostics::setup();

    let cli = Cli::parse();

    // Setup logging
    let log_level = match cli.global.verbose {
        0 => tracing::Level::WARN,
        1 => tracing::Level::INFO,
        2 => tracing::Level::DEBUG,
        _ => tracing::Level::TRACE,
    };

    if !cli.global.quiet {
        tracing_subscriber::fmt()
            .with_max_level(log_level)
            .with_target(false)
            .init();
    }

    match cli.command {
        Commands::New { name, r#type } => {
            commands::new_package(&name, &r#type).await?;
        }
        Commands::Init { name } => {
            commands::init(name.as_deref()).await?;
        }
        Commands::Build { release, target, no_cache } => {
            commands::build(release, target.as_deref(), cli.global.jobs, no_cache).await?;
        }
        Commands::Run { target, args } => {
            commands::run(target.as_deref(), &args).await?;
        }
        Commands::Test { target, filter } => {
            commands::test(target.as_deref(), filter.as_deref()).await?;
        }
        Commands::Clean { deps } => {
            commands::clean(deps).await?;
        }
        Commands::Add { package, git, path, dev } => {
            commands::add(&package, git.as_deref(), path.as_deref(), dev).await?;
        }
        Commands::Remove { package } => {
            commands::remove(&package).await?;
        }
        Commands::Install { frozen } => {
            commands::install(frozen).await?;
        }
        Commands::Update { package, breaking } => {
            commands::update(package.as_deref(), breaking).await?;
        }
        Commands::Tree { depth, duplicates } => {
            commands::tree(depth, duplicates).await?;
        }
        Commands::Outdated => {
            commands::outdated().await?;
        }
        Commands::Cache { action } => match action {
            CacheAction::List => commands::cache_list().await?,
            CacheAction::Stats => commands::cache_stats().await?,
            CacheAction::Clean { all, binary } => commands::cache_clean(all, binary).await?,
            CacheAction::Path => commands::cache_path().await?,
        },
        Commands::Migrate => {
            commands::migrate().await?;
        }
        Commands::Info { package } => {
            commands::info(&package).await?;
        }
    }

    Ok(())
}
