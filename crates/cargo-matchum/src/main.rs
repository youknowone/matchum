#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use clap::Parser;
use matchum::commands;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "cargo", bin_name = "cargo")]
enum Cargo {
    /// Spell-check all files in the cargo workspace
    #[command(subcommand_negates_reqs = true, args_conflicts_with_subcommands = true)]
    Matchum(MatchumArgs),
}

#[derive(clap::Args)]
struct MatchumArgs {
    #[command(subcommand)]
    action: Option<MatchumAction>,

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
enum MatchumAction {
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
            if let Ok(content) = std::fs::read_to_string(&manifest)
                && content.contains("[workspace]") {
                    workspace_root = Some(dir.clone());
                }
        }
        if !dir.pop() {
            break;
        }
    }
    workspace_root.or(nearest)
}

fn find_config_in_workspace(workspace_root: &Path) -> Option<PathBuf> {
    let candidates = [
        "matchum.toml",
        ".matchum.toml",
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
    let Cargo::Matchum(args) = Cargo::parse();

    let workspace_root = match find_workspace_root() {
        Some(root) => root,
        None => {
            eprintln!("Error: could not find Cargo.toml in current or parent directories");
            std::process::exit(2);
        }
    };

    let result = match args.action {
        Some(MatchumAction::Review { config }) => {
            let config_path = config.or_else(|| find_config_in_workspace(&workspace_root));
            let resolved = resolve_for_workspace(config_path.as_deref(), &workspace_root);
            match resolved {
                Ok((settings, config_dir, is_cspell, config_file, catalog)) => {
                    commands::review::run_review(
                        &[workspace_root],
                        config_path.as_deref(),
                        &settings,
                        config_dir.as_deref(),
                        catalog,
                        commands::check::CheckOptions {
                            config_search: true,
                            use_gitignore_default: true,
                            use_gitattributes: !is_cspell,
                            per_dir_config_search: is_cspell,
                            config_file,
                            compound_words_mode: None,
                            ..commands::check::CheckOptions::default()
                        },
                    )
                }
                Err(e) => Err(e),
            }
        }
        None => run_check(&args.check, &workspace_root),
    };

    if let Err(e) = result {
        eprintln!("Error: {:#}", e);
        std::process::exit(2);
    }
}

fn resolve_for_workspace(
    config_path: Option<&Path>,
    workspace_root: &Path,
) -> anyhow::Result<(
    matchum_config::settings::CSpellSettings,
    Option<PathBuf>,
    bool,
    Option<PathBuf>,
    commands::check::DictionaryCatalog,
)> {
    let resolved = {
        let r = commands::setup::resolve_config(config_path, Some(workspace_root), true, &[]);
        match r {
            Ok(r) if r.config_dir.is_some() => r,
            Ok(mut r) => {
                r.settings = default_rust_settings();
                r
            }
            Err(e) => return Err(e),
        }
    };

    let mut settings = resolved.settings;
    commands::check::apply_default_dictionaries(&mut settings);

    let is_cspell = resolved.is_cspell;
    let catalog = commands::cspell::catalog::build_dictionary_catalog(
        &settings,
        resolved.config_dir.as_deref(),
        Some(&commands::dict_cache_dir()),
        is_cspell,
    )?;

    Ok((
        settings,
        resolved.config_dir,
        is_cspell,
        resolved.config_file,
        catalog,
    ))
}

fn run_check(args: &CheckArgs, workspace_root: &Path) -> anyhow::Result<()> {
    let config_path = args
        .config
        .clone()
        .or_else(|| find_config_in_workspace(workspace_root));

    let (settings, config_dir, is_cspell, config_file, catalog) =
        resolve_for_workspace(config_path.as_deref(), workspace_root)?;

    commands::check::run_check(
        &[workspace_root.to_path_buf()],
        &settings,
        config_dir.as_deref(),
        catalog,
        &args.format,
        Some(args.suggestions),
        args.unique,
        args.strict,
        commands::check::CheckOptions {
            config_search: true,
            use_gitignore_default: true,
            use_gitattributes: !is_cspell,
            per_dir_config_search: is_cspell,
            config_file,
            compound_words_mode: None,
            ..commands::check::CheckOptions::default()
        },
    )
    .map(|_| ())
}

fn default_rust_settings() -> matchum_config::settings::CSpellSettings {
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
