use matchum::commands;
use matchum::diff::DiffFilter;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "matchum", about = "High-performance spell checker")]
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

        /// Output format: text, json, github, words-only
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

        /// Only check files of this type (e.g., rust, python, js)
        #[arg(short = 't', long = "type")]
        file_type: Option<String>,

        /// Show execution statistics
        #[arg(long)]
        stats: bool,

        /// Read unified diff from stdin; only report issues on added lines
        #[arg(long)]
        diff: bool,

        /// Exclude files matching the glob pattern (can be used multiple times)
        #[arg(short = 'e', long = "exclude")]
        exclude: Vec<String>,

        /// Set language locales (e.g., "en,fr" or "en-GB")
        #[arg(long)]
        locale: Option<String>,

        /// Force programming language for unknown extensions
        #[arg(long)]
        language_id: Option<String>,

        /// Do not show the spelling errors
        #[arg(long)]
        no_issues: bool,

        /// Turn off progress messages
        #[arg(long)]
        no_progress: bool,

        /// Turn off summary message
        #[arg(long)]
        no_summary: bool,

        /// Only show spelling issues or errors
        #[arg(long)]
        quiet: bool,

        /// Silent mode, suppress error messages
        #[arg(short = 's', long)]
        silent: bool,

        /// Use paths relative to the config file or root
        #[arg(long)]
        relative: bool,

        /// Exit after first file with an issue or error
        #[arg(long)]
        fail_fast: bool,

        /// Show hidden files
        #[arg(long)]
        dot: bool,

        /// Do not use .gitignore
        #[arg(long)]
        no_gitignore: bool,

        /// Set the root directory for relative paths
        #[arg(long)]
        root: Option<PathBuf>,

        /// Specify a list of files to be spell checked
        #[arg(long = "file-list")]
        file_list: Vec<String>,
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

    /// Initialize a matchum.toml config file
    Init {
        /// Migrate from an existing cspell config
        #[arg(long)]
        from_cspell: bool,

        /// Source config file path (for --from-cspell)
        #[arg(short, long)]
        config: Option<PathBuf>,
    },

    /// Interactively review spelling issues
    Review {
        /// Files or directories to check
        #[arg(default_value = ".")]
        paths: Vec<PathBuf>,

        /// Config file path
        #[arg(short, long)]
        config: Option<PathBuf>,
    },

    /// Add words to the project dictionary
    Add {
        /// Words to add
        words: Vec<String>,

        /// Add as forbidden words instead
        #[arg(long)]
        forbidden: bool,

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
            file_type,
            stats,
            diff,
            exclude,
            locale,
            language_id,
            no_issues,
            no_progress,
            no_summary,
            quiet,
            silent,
            relative: _,
            fail_fast,
            dot,
            no_gitignore,
            root,
            file_list,
        } => {
            let diff_filter = if diff {
                let mut buf = String::new();
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)
                    .expect("failed to read stdin");
                Some(Arc::new(DiffFilter::parse(&buf)))
            } else {
                None
            };
            let use_gitignore = if no_gitignore { Some(false) } else { None };
            commands::check::run_check(
                &paths,
                config.as_deref(),
                &format,
                suggestions,
                unique,
                strict,
                commands::check::CheckOptions {
                    config_search: true,
                    use_gitignore_default: true,
                    dict_base_dir: Some(commands::dict_cache_dir()),
                    fallback_settings: Some(commands::check::default_settings()),
                    file_type,
                    stats,
                    diff_filter,
                    exclude,
                    locale,
                    language_id,
                    no_issues,
                    no_progress,
                    no_summary,
                    quiet,
                    silent,
                    fail_fast,
                    dot,
                    use_gitignore,
                    root,
                    file_list,
                    ..commands::check::CheckOptions::default()
                },
            )
            .map(|_| ())
        }

        Commands::Lint { paths, config } => commands::check::run_check(
            &paths,
            config.as_deref(),
            "text",
            true,
            false,
            true,
            commands::check::CheckOptions {
                config_search: true,
                use_gitignore_default: true,
                dict_base_dir: Some(commands::dict_cache_dir()),
                fallback_settings: Some(commands::check::default_settings()),
                ..commands::check::CheckOptions::default()
            },
        )
        .map(|_| ()),

        Commands::Trace { word, config } => commands::trace::run_trace(&word, config.as_deref()),

        Commands::Review { paths, config } => commands::review::run_review(
            &paths,
            config.as_deref(),
            commands::check::CheckOptions {
                config_search: true,
                use_gitignore_default: true,
                dict_base_dir: Some(commands::dict_cache_dir()),
                fallback_settings: Some(commands::check::default_settings()),
                ..commands::check::CheckOptions::default()
            },
        ),

        Commands::Init {
            from_cspell,
            config,
        } => commands::init::run_init(from_cspell, config.as_deref()),

        Commands::Add {
            words,
            forbidden,
            config,
        } => commands::add::run_add(&words, forbidden, config.as_deref()),

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
