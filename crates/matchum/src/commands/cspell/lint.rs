use crate::commands::check::{self, CheckOptions};
use crate::commands::cspell::catalog;
use crate::commands::dict_cache_dir;
use crate::commands::setup;
use anyhow::Result;
use matchum_core::validator::CompoundWordsMode;
use std::path::{Path, PathBuf};

pub struct LintOptions {
    pub globs: Vec<String>,
    pub config: Option<PathBuf>,
    pub config_search: bool,
    pub stop_config_search_at: Vec<PathBuf>,
    pub exclude: Vec<String>,
    pub file_list: Vec<String>,
    pub file: Vec<PathBuf>,
    pub max_file_size: Option<String>,
    pub dictionary: Vec<String>,
    pub disable_dictionary: Vec<String>,
    pub allow_compound_words: Option<bool>,
    pub unique: bool,
    pub words_only: bool,
    pub no_exit_code: bool,
    pub show_suggestions: Option<bool>,
    pub root: Option<PathBuf>,
    pub quiet: bool,
    pub silent: bool,
    pub no_issues: bool,
    pub no_progress: bool,
    pub no_summary: bool,
    pub no_relative: bool,
    pub show_context: bool,
    pub fail_fast: bool,
    pub dot: bool,
    pub use_gitignore: Option<bool>,
    pub gitignore_root: Option<PathBuf>,
    pub verbose: u8,
    pub locale: Option<String>,
    pub language_id: Option<String>,
    pub continue_on_error: bool,
    pub no_must_find_files: bool,
    pub no_default_configuration: bool,
    pub validate_directives: bool,
    pub cache: bool,
    pub cache_reset: bool,
    pub cache_strategy: Option<String>,
    pub cache_location: Option<PathBuf>,
}

fn config_search_start(root: Option<&Path>, cwd: &Path) -> PathBuf {
    root.map(Path::to_path_buf)
        .unwrap_or_else(|| cwd.to_path_buf())
}

pub fn run(opts: LintOptions) -> Result<()> {
    let paths: Vec<PathBuf> = opts.globs.iter().map(PathBuf::from).collect();

    let format = if opts.words_only {
        "words-only"
    } else {
        "text"
    };

    let strict = !opts.no_exit_code;

    // Resolve config
    let resolved = if opts.no_default_configuration {
        setup::ResolvedConfig {
            settings: Default::default(),
            config_dir: None,
            is_cspell: true,
            config_file: None,
        }
    } else {
        let cwd = std::env::current_dir().unwrap_or_default();
        let config_search_start = config_search_start(opts.root.as_deref(), &cwd);
        let result = setup::resolve_config(
            opts.config.as_deref(),
            Some(config_search_start.as_path()),
            opts.config_search,
            &opts.stop_config_search_at,
        );
        match result {
            Ok(r) => {
                if r.config_dir.is_some() {
                    r
                } else {
                    setup::ResolvedConfig {
                        settings: Default::default(),
                        config_dir: None,
                        is_cspell: true,
                        config_file: None,
                    }
                }
            }
            Err(e) => {
                if opts.continue_on_error {
                    if !opts.silent {
                        eprintln!("Warning: {:#}", e);
                    }
                    setup::ResolvedConfig {
                        settings: Default::default(),
                        config_dir: None,
                        is_cspell: true,
                        config_file: None,
                    }
                } else {
                    return Err(e);
                }
            }
        }
    };

    let mut settings = resolved.settings;
    if let Some(ref locale) = opts.locale {
        settings.language = Some(locale.clone());
    }
    check::apply_default_dictionaries(&mut settings);
    check::apply_default_patterns(&mut settings);

    let catalog = catalog::build_dictionary_catalog(
        &settings,
        resolved.config_dir.as_deref(),
        Some(&dict_cache_dir()),
        true,
    )?;

    check::run_check(
        &paths,
        &settings,
        resolved.config_dir.as_deref(),
        catalog,
        format,
        opts.show_suggestions,
        opts.unique,
        strict,
        CheckOptions {
            exclude: opts.exclude,
            file_list: opts.file_list,
            config_search: opts.config_search,
            stop_config_search_at: opts.stop_config_search_at,
            max_file_size: opts.max_file_size,
            dictionary: opts.dictionary,
            disable_dictionary: opts.disable_dictionary,
            allow_compound_words: opts.allow_compound_words,
            compound_words_mode: Some(CompoundWordsMode::SeparateWords),
            cspell_compat_mode: true,
            no_issues: opts.no_issues,
            no_summary: opts.no_summary,
            no_progress: opts.no_progress,
            quiet: opts.quiet,
            silent: opts.silent,
            no_relative: opts.no_relative,
            show_context: opts.show_context,
            root: opts.root,
            fail_fast: opts.fail_fast,
            dot: opts.dot,
            use_gitignore: opts.use_gitignore,
            gitignore_root: opts.gitignore_root,
            file: opts.file,
            verbose: opts.verbose,
            language_id: opts.language_id,
            continue_on_error: opts.continue_on_error,
            no_must_find_files: opts.no_must_find_files,
            no_default_configuration: opts.no_default_configuration,
            validate_directives: opts.validate_directives,
            cache: opts.cache,
            cache_reset: opts.cache_reset,
            cache_strategy: opts.cache_strategy,
            cache_location: opts.cache_location,
            use_gitignore_default: true,
            file_type: None,
            stats: false,
            diff_filter: None,
            config_file: resolved.config_file,
            use_gitattributes: false,
            per_dir_config_search: true,
            ..CheckOptions::default()
        },
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_search_start_uses_root_when_provided() {
        let cwd = Path::new("/tmp/current");
        let root = Path::new("/tmp/root");
        assert_eq!(config_search_start(Some(root), cwd), root);
    }

    #[test]
    fn config_search_start_defaults_to_cwd_for_glob_runs() {
        let cwd = Path::new("/tmp/current");
        assert_eq!(config_search_start(None, cwd), cwd);
    }

    #[test]
    fn config_search_from_repo_root_finds_parent_vendor_config() {
        let dir = tempfile::tempdir().unwrap();
        let repos = dir.path().join("repositories");
        let repo = repos.join("temp").join("AdaDoom3").join("AdaDoom3");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(
            repos.join("cspell.yaml"),
            "version: '0.2'\nignorePaths:\n  - '*.idmap'\n",
        )
        .unwrap();

        let start = config_search_start(None, &repo);
        let resolved = setup::resolve_config(None, Some(start.as_path()), true, &[]).unwrap();

        assert_eq!(resolved.config_dir.as_deref(), Some(repos.as_path()));
        assert!(
            resolved
                .settings
                .ignore_paths
                .iter()
                .any(|p| p == "*.idmap"),
            "expected parent repositories/cspell.yaml ignorePaths to be loaded"
        );
    }
}
