use clap::Parser;
use ruspell::commands;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "cargo", bin_name = "cargo")]
enum Cargo {
    /// Spell-check all files in the cargo workspace
    #[command(subcommand_negates_reqs = true, args_conflicts_with_subcommands = true)]
    Ruspell(RuspellArgs),
}

#[derive(clap::Args)]
struct RuspellArgs {
    #[command(subcommand)]
    action: Option<RuspellAction>,

    #[command(flatten)]
    check: CheckArgs,
}

#[derive(clap::Args)]
struct CheckArgs {
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
}

#[derive(clap::Subcommand)]
enum RuspellAction {
    /// Interactively review spelling issues
    Review {
        /// Config file path
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
}

/// Find the workspace root by walking up and picking the topmost Cargo.toml
/// that contains a [workspace] section. Falls back to the nearest Cargo.toml.
fn find_workspace_root() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    let mut nearest = None;
    let mut workspace_root = None;
    loop {
        let manifest = dir.join("Cargo.toml");
        if manifest.exists() {
            if nearest.is_none() {
                nearest = Some(dir.clone());
            }
            if let Ok(content) = std::fs::read_to_string(&manifest) {
                if content.contains("[workspace]") {
                    workspace_root = Some(dir.clone());
                }
            }
        }
        if !dir.pop() {
            break;
        }
    }
    workspace_root.or(nearest)
}

fn dict_base_dir() -> PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_default();
    PathBuf::from(home).join(".ruspell_cache").join("packages")
}

fn find_config_in_workspace(workspace_root: &Path) -> Option<PathBuf> {
    let candidates = [
        "ruspell.toml",
        ".ruspell.toml",
        ".cspell.json",
        "cspell.json",
        ".cspell.yaml",
        "cspell.yaml",
    ];
    candidates
        .iter()
        .map(|name| workspace_root.join(name))
        .find(|p| p.exists())
}

fn main() {
    let Cargo::Ruspell(args) = Cargo::parse();

    let workspace_root = match find_workspace_root() {
        Some(root) => root,
        None => {
            eprintln!("Error: could not find Cargo.toml in current or parent directories");
            std::process::exit(2);
        }
    };

    let result = match args.action {
        Some(RuspellAction::Review { config }) => {
            let config_path = config.or_else(|| find_config_in_workspace(&workspace_root));
            commands::review::run_review(
                &[workspace_root],
                config_path.as_deref(),
                commands::check::CheckOptions {
                    config_search: true,
                    use_gitignore_default: true,
                    dict_base_dir: Some(dict_base_dir()),
                    fallback_settings: Some(default_rust_settings()),
                    ..commands::check::CheckOptions::default()
                },
            )
        }
        None => run_check(&args.check, &workspace_root),
    };

    if let Err(e) = result {
        eprintln!("Error: {:#}", e);
        std::process::exit(2);
    }
}

fn run_check(args: &CheckArgs, workspace_root: &Path) -> anyhow::Result<()> {
    let config_path = args
        .config
        .clone()
        .or_else(|| find_config_in_workspace(workspace_root));

    let (settings, config_dir) = if let Some(ref config) = config_path {
        match ruspell_config::resolver::load_config_auto(config) {
            Ok(mut s) => {
                let has_rust = s
                    .dictionaries
                    .iter()
                    .any(|d| d.eq_ignore_ascii_case("rust"));
                if !has_rust {
                    s.dictionaries.push("rust".into());
                }
                let dir = config.parent().map(|p| p.to_path_buf());
                (s, dir)
            }
            Err(e) => {
                eprintln!("Warning: failed to load config: {:#}", e);
                (default_rust_settings(), None)
            }
        }
    } else {
        (default_rust_settings(), None)
    };

    let dict_base = if config_dir.is_some() {
        None
    } else {
        Some(dict_base_dir())
    };
    let options = commands::check::CheckOptions {
        use_gitignore_default: true,
        dict_base_dir: dict_base,
        ..commands::check::CheckOptions::default()
    };

    commands::check::run_check_with_settings(
        &[workspace_root.to_path_buf()],
        &settings,
        config_dir.as_deref(),
        &args.format,
        args.suggestions,
        args.unique,
        args.strict,
        options,
    )
    .map(|_| ())
}

fn default_rust_settings() -> ruspell_config::settings::CSpellSettings {
    let mut s = commands::check::default_settings();
    if !s
        .dictionaries
        .iter()
        .any(|d| d.eq_ignore_ascii_case("rust"))
    {
        s.dictionaries.insert(0, "rust".into());
    }
    s
}
