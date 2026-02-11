use crate::commands::check::{self, CheckOptions};
use anyhow::Result;
use std::path::PathBuf;

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
    pub show_suggestions: bool,
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

pub fn run(opts: LintOptions) -> Result<()> {
    let paths: Vec<PathBuf> = opts.globs.iter().map(PathBuf::from).collect();

    let format = if opts.words_only {
        "words-only"
    } else {
        "text"
    };

    let strict = !opts.no_exit_code;

    check::run_check(
        &paths,
        opts.config.as_deref(),
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
            locale: opts.locale,
            language_id: opts.language_id,
            continue_on_error: opts.continue_on_error,
            no_must_find_files: opts.no_must_find_files,
            no_default_configuration: opts.no_default_configuration,
            validate_directives: opts.validate_directives,
            cache: opts.cache,
            cache_reset: opts.cache_reset,
            cache_strategy: opts.cache_strategy,
            cache_location: opts.cache_location,
        },
    )
    .map(|_| ())
}
