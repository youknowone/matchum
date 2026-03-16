use crate::commands::check::{self, CheckOptions};
use crate::commands::cspell::catalog;
use crate::commands::dict_cache_dir;
use crate::commands::setup;
use anyhow::Result;
use matchum_core::validator::CompoundWordsMode;
use std::path::PathBuf;

/// cspell check: spell check file(s) and display the result with the full file in context.
pub fn run(
    files: Vec<PathBuf>,
    config: Option<PathBuf>,
    no_exit_code: bool,
    no_default_configuration: bool,
    validate_directives: bool,
) -> Result<()> {
    if files.is_empty() {
        anyhow::bail!("error: missing required argument '<files...>'");
    }

    let strict = !no_exit_code;

    let resolved = if no_default_configuration {
        setup::ResolvedConfig {
            settings: Default::default(),
            config_dir: None,
            is_cspell: true,
            config_file: None,
        }
    } else {
        let start = files.first().map(|p| p.as_path());
        let mut r = setup::resolve_config(config.as_deref(), start, true, &[])?;
        if r.config_dir.is_none() {
            r.settings = Default::default();
        }
        r
    };

    let mut settings = resolved.settings;
    check::apply_default_dictionaries(&mut settings);
    check::apply_default_patterns(&mut settings);

    let catalog = catalog::build_dictionary_catalog(
        &settings,
        resolved.config_dir.as_deref(),
        Some(&dict_cache_dir()),
        true,
    )?;

    check::run_check(
        &files,
        &settings,
        resolved.config_dir.as_deref(),
        catalog,
        "text",
        None,
        false,
        strict,
        CheckOptions {
            no_default_configuration,
            validate_directives,
            config_search: true,
            compound_words_mode: Some(CompoundWordsMode::SeparateWords),
            cspell_compat_mode: true,
            per_dir_config_search: true,
            config_file: resolved.config_file,
            ..CheckOptions::default()
        },
    )
    .map(|_| ())
}
