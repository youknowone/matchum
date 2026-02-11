mod commands;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "ruspell", about = "High-performance spell checker")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Check files for spelling errors
    Check {
        /// Files or directories to check
        #[arg(default_value = ".")]
        paths: Vec<PathBuf>,

        /// Config file path
        #[arg(short, long)]
        config: Option<PathBuf>,

        /// Output format: text, json
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Show spelling suggestions
        #[arg(long)]
        suggestions: bool,

        /// Only show unique misspelled words
        #[arg(long)]
        unique: bool,

        /// Exit with code 1 if any spelling errors found
        #[arg(long)]
        strict: bool,
    },

    /// Check files (strict mode, exits 1 on errors)
    Lint {
        /// Files or directories to check
        #[arg(default_value = ".")]
        paths: Vec<PathBuf>,

        /// Config file path
        #[arg(short, long)]
        config: Option<PathBuf>,
    },

    /// Trace a word through dictionaries
    Trace {
        /// Word to trace
        word: String,

        /// Config file path
        #[arg(short, long)]
        config: Option<PathBuf>,
    },

    /// Manage dictionary packages
    Dict {
        #[command(subcommand)]
        action: DictAction,
    },

    /// cspell-compatible command interface
    Cspell {
        #[command(subcommand)]
        action: commands::cspell::CspellCommands,
    },
}

#[derive(Subcommand)]
enum DictAction {
    /// Fetch dictionary packages from npm registry
    Fetch {
        /// Package specs (e.g., @cspell/dict-rust, @cspell/dict-en_us@4.4.29)
        /// If none given, fetches all imports from cspell config
        packages: Vec<String>,

        /// Config file path
        #[arg(short, long)]
        config: Option<PathBuf>,
    },

    /// List installed dictionary packages
    List,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Check {
            paths,
            config,
            format,
            suggestions,
            unique,
            strict,
        } => commands::check::run_check(
            &paths,
            config.as_deref(),
            &format,
            suggestions,
            unique,
            strict,
            commands::check::CheckOptions::default(),
        )
        .map(|_| ()),

        Commands::Lint { paths, config } => commands::check::run_check(
            &paths,
            config.as_deref(),
            "text",
            true,
            false,
            true,
            commands::check::CheckOptions::default(),
        )
        .map(|_| ()),

        Commands::Trace { word, config } => {
            commands::trace::run_trace(&word, config.as_deref())
        }

        Commands::Dict { action } => match action {
            DictAction::Fetch { packages, config } => {
                let project_dir = commands::dict::find_project_dir(None);
                commands::dict::run_fetch(&packages, config.as_deref(), &project_dir)
            }
            DictAction::List => {
                let project_dir = commands::dict::find_project_dir(None);
                commands::dict::run_list(&project_dir)
            }
        },

        Commands::Cspell { action } => commands::cspell::dispatch(action),
    };

    if let Err(e) = result {
        eprintln!("Error: {:#}", e);
        std::process::exit(2);
    }
}
