use crate::commands::check::{self, CheckOptions};
use crate::commands::dict_cache_dir;
use anyhow::Result;
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

    check::run_check(
        &files,
        config.as_deref(),
        "text",
        true,  // show suggestions
        false, // unique
        strict,
        CheckOptions {
            no_default_configuration,
            validate_directives,
            config_search: true,
            dict_base_dir: Some(dict_cache_dir()),
            ..CheckOptions::default()
        },
    )
    .map(|_| ())
}
