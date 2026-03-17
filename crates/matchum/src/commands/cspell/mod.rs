pub mod catalog;
pub mod check;
pub mod config_search;
pub mod defaults;
pub mod dictionaries;
pub mod init;
pub mod link;
pub mod lint;
pub mod patterns;
pub mod resolve;
pub mod suggestions;
pub mod trace;

use anyhow::{Context, Result};
use clap::Subcommand;
use matchum_config::resolver;
use matchum_config::settings::CSpellSettings;
use matchum_dict::dictionary::Dictionary;
use matchum_dict::loader;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum CspellCommands {
    /// Check spelling
    Lint {
        /// Glob patterns of files to check
        globs: Vec<String>,

        /// Configuration file to use
        #[arg(short, long, value_name = "cspell.json")]
        config: Option<PathBuf>,

        /// Disable automatic searching for additional configuration files
        #[arg(long = "no-config-search")]
        no_config_search: bool,

        /// Stop searching for config at this directory
        #[arg(long = "stop-config-search-at", value_name = "dir")]
        stop_config_search_at: Vec<PathBuf>,

        /// Display more information about files being checked
        #[arg(short, long, action = clap::ArgAction::Count)]
        verbose: u8,

        /// Set language locales (e.g. "en,fr" or "en-GB")
        #[arg(long)]
        locale: Option<String>,

        /// Force programming language for unknown extensions
        #[arg(long = "language-id", value_name = "file-type")]
        language_id: Option<String>,

        /// Only output the words not found in the dictionaries
        #[arg(long = "words-only")]
        words_only: bool,

        /// Only output the first instance of a word not found
        #[arg(short, long)]
        unique: bool,

        /// Exclude files matching the glob pattern
        #[arg(short = 'e', long = "exclude", value_name = "glob")]
        exclude: Vec<String>,

        /// Specify a list of files to be spell checked
        #[arg(long = "file-list", value_name = "path or stdin")]
        file_list: Vec<String>,

        /// Specify files to spell check (filtered by globs)
        #[arg(long = "file", value_name = "file")]
        file: Vec<PathBuf>,

        /// Do not show the spelling errors
        #[arg(long = "no-issues")]
        no_issues: bool,

        /// Turn off progress messages
        #[arg(long = "no-progress")]
        no_progress: bool,

        /// Turn off summary message in console
        #[arg(long = "no-summary")]
        no_summary: bool,

        /// Silent mode, suppress error messages
        #[arg(short = 's', long)]
        silent: bool,

        /// Do not return an exit code if issues are found
        #[arg(long = "no-exit-code")]
        no_exit_code: bool,

        /// Only show spelling issues or errors
        #[arg(long)]
        quiet: bool,

        /// Exit after first file with an issue or error
        #[arg(long = "fail-fast")]
        fail_fast: bool,

        /// Continue processing files even if there is a configuration error
        #[arg(long = "continue-on-error")]
        continue_on_error: bool,

        /// Root directory, defaults to current directory
        #[arg(short = 'r', long, value_name = "root folder")]
        root: Option<PathBuf>,

        /// Display issues with relative paths
        #[arg(long = "relative", conflicts_with = "no_relative")]
        relative: bool,

        /// Display issues with absolute path instead of relative
        #[arg(long = "no-relative", conflicts_with = "relative")]
        no_relative: bool,

        /// Show the surrounding text around an issue
        #[arg(long = "show-context")]
        show_context: bool,

        /// Show spelling suggestions
        #[arg(long = "show-suggestions")]
        show_suggestions: bool,

        /// Do not show spelling suggestions or fixes
        #[arg(long = "no-show-suggestions")]
        no_show_suggestions: bool,

        /// Do not error if no files are found
        #[arg(long = "no-must-find-files")]
        no_must_find_files: bool,

        /// Use cache to only check changed files
        #[arg(long = "cache")]
        cache: bool,

        /// Do not use cache
        #[arg(long = "no-cache")]
        no_cache: bool,

        /// Reset the cache file
        #[arg(long = "cache-reset")]
        cache_reset: bool,

        /// Strategy to use for detecting changed files
        #[arg(long = "cache-strategy", value_name = "strategy")]
        cache_strategy: Option<String>,

        /// Path to the cache file or directory
        #[arg(long = "cache-location", value_name = "path")]
        cache_location: Option<PathBuf>,

        /// Include files and directories starting with . (period)
        #[arg(long)]
        dot: bool,

        /// Ignore files matching glob patterns found in .gitignore files
        #[arg(long)]
        gitignore: bool,

        /// Do NOT use .gitignore files
        #[arg(long = "no-gitignore")]
        no_gitignore: bool,

        /// Prevent searching for .gitignore files past root
        #[arg(long = "gitignore-root", value_name = "path")]
        gitignore_root: Option<PathBuf>,

        /// Validate in-document CSpell directives
        #[arg(long = "validate-directives")]
        validate_directives: bool,

        /// Prevent checking large files (e.g. 1MB, 50KB)
        #[arg(long = "max-file-size", value_name = "size")]
        max_file_size: Option<String>,

        /// Do not load the default configuration and dictionaries
        #[arg(long = "no-default-configuration")]
        no_default_configuration: bool,

        /// Enable a dictionary by name
        #[arg(long = "dictionary", value_name = "name")]
        dictionary: Vec<String>,

        /// Disable a dictionary by name
        #[arg(long = "disable-dictionary", value_name = "name")]
        disable_dictionary: Vec<String>,

        /// Turn on allowCompoundWords
        #[arg(
            long = "allow-compound-words",
            conflicts_with = "no_allow_compound_words"
        )]
        allow_compound_words: bool,

        /// Turn off allowCompoundWords
        #[arg(long = "no-allow-compound-words")]
        no_allow_compound_words: bool,

        /// Specify one or more reporters to use
        #[arg(long, value_name = "module|path")]
        reporter: Vec<String>,

        /// Output a summary of issues found
        #[arg(long = "issues-summary-report", hide = true)]
        issues_summary_report: bool,

        /// Output a performance summary report
        #[arg(long = "show-perf-summary", hide = true)]
        show_perf_summary: bool,
    },

    /// Spell check file(s) and display the result. The full file is displayed in color.
    Check {
        /// Files to check
        files: Vec<PathBuf>,

        /// Configuration file to use
        #[arg(short, long, value_name = "cspell.json")]
        config: Option<PathBuf>,

        /// Validate in-document CSpell directives
        #[arg(long = "validate-directives")]
        validate_directives: bool,

        /// Do not validate in-document CSpell directives
        #[arg(long = "no-validate-directives")]
        no_validate_directives: bool,

        /// Do not return an exit code if issues are found
        #[arg(long = "no-exit-code")]
        no_exit_code: bool,

        /// Do not load the default configuration and dictionaries
        #[arg(long = "no-default-configuration")]
        no_default_configuration: bool,
    },

    /// Trace words -- Search for words in the configuration and dictionaries
    Trace {
        /// Words to trace
        words: Vec<String>,

        /// Configuration file to use
        #[arg(short, long, value_name = "cspell.json")]
        config: Option<PathBuf>,

        /// Set language locales
        #[arg(long)]
        locale: Option<String>,

        /// Use programming language
        #[arg(long = "language-id")]
        language_id: Option<String>,

        /// Turn on allowCompoundWords
        #[arg(
            long = "allow-compound-words",
            conflicts_with = "no_allow_compound_words"
        )]
        allow_compound_words: bool,

        /// Turn off allowCompoundWords
        #[arg(long = "no-allow-compound-words")]
        no_allow_compound_words: bool,

        /// Ignore case and accents when searching for words
        #[arg(long = "ignore-case")]
        ignore_case: bool,

        /// Do not ignore case and accents
        #[arg(long = "no-ignore-case")]
        no_ignore_case: bool,

        /// Enable a dictionary by name
        #[arg(long = "dictionary", value_name = "name")]
        dictionary: Vec<String>,

        /// Read words from stdin
        #[arg(long)]
        stdin: bool,

        /// Show all dictionaries
        #[arg(long)]
        all: bool,

        /// Show only dictionaries that have the words
        #[arg(long = "only-found")]
        only_found: bool,

        /// Do not load the default configuration and dictionaries
        #[arg(long = "no-default-configuration")]
        no_default_configuration: bool,
    },

    /// Spelling Suggestions for words
    #[command(alias = "sug")]
    Suggestions {
        /// Words to get suggestions for
        words: Vec<String>,

        /// Configuration file to use
        #[arg(short, long, value_name = "cspell.json")]
        config: Option<PathBuf>,

        /// Set language locales
        #[arg(long)]
        locale: Option<String>,

        /// Use programming language
        #[arg(long = "language-id")]
        language_id: Option<String>,

        /// Ignore case and accents when searching for words
        #[arg(short = 's', long = "no-strict")]
        no_strict: bool,

        /// Alias of --no-strict
        #[arg(long = "ignore-case")]
        ignore_case: bool,

        /// Number of changes allowed to a word
        #[arg(long = "num-changes", default_value = "4")]
        num_changes: usize,

        /// Number of suggestions
        #[arg(long = "num-suggestions", default_value = "8")]
        num_suggestions: usize,

        /// Use stdin for input
        #[arg(long)]
        stdin: bool,

        /// REPL interface for looking up suggestions
        #[arg(long)]
        repl: bool,

        /// Show detailed output
        #[arg(short, long)]
        verbose: bool,

        /// Use the dictionary specified
        #[arg(short = 'd', long = "dictionary", value_name = "name")]
        dictionary: Vec<String>,

        /// Use the dictionaries specified
        #[arg(long = "dictionaries", value_name = "names")]
        dictionaries: Vec<String>,
    },

    /// Initialize a CSpell configuration file
    Init {
        /// Path to the CSpell configuration file
        #[arg(short, long, value_name = "path")]
        config: Option<PathBuf>,

        /// Define where to write file
        #[arg(short, long, value_name = "path")]
        output: Option<PathBuf>,

        /// Define the format of the file (yaml, yml, json, jsonc)
        #[arg(long, default_value = "yaml")]
        format: String,

        /// Import a configuration file or dictionary package
        #[arg(long = "import", value_name = "path|package")]
        imports: Vec<String>,

        /// Define the locale to use when spell checking
        #[arg(long)]
        locale: Option<String>,

        /// Enable a dictionary
        #[arg(long = "dictionary")]
        dictionary: Vec<String>,

        /// Do not add comments to the config file
        #[arg(long = "no-comments")]
        no_comments: bool,

        /// Write the configuration to stdout instead of a file
        #[arg(long)]
        stdout: bool,
    },

    /// Link dictionaries and other settings to the cspell global config
    Link {
        #[command(subcommand)]
        action: Option<LinkCommands>,
    },

    /// List dictionaries
    Dictionaries {
        /// Configuration file to use
        #[arg(short, long, value_name = "cspell.json")]
        config: Option<PathBuf>,

        /// Configure how to display the dictionary path
        #[arg(long = "path-format", value_name = "format")]
        path_format: Option<String>,

        /// Show only enabled dictionaries
        #[arg(long)]
        enabled: bool,

        /// Set language locales
        #[arg(long)]
        locale: Option<String>,

        /// File type to use
        #[arg(long = "file-type", value_name = "fileType")]
        file_type: Option<String>,

        /// Do not show the location of the dictionary
        #[arg(long = "no-show-location")]
        no_show_location: bool,

        /// Do not load the default configuration and dictionaries
        #[arg(long = "no-default-configuration")]
        no_default_configuration: bool,
    },
}

#[derive(Subcommand)]
pub enum LinkCommands {
    /// List currently linked configurations
    #[command(alias = "ls")]
    List,

    /// Add dictionaries to the cspell global config
    #[command(alias = "a")]
    Add {
        /// Dictionary packages to add
        dictionaries: Vec<String>,
    },

    /// Remove matching paths/packages from the global config
    #[command(alias = "r")]
    Remove {
        /// Paths to remove
        paths: Vec<String>,
    },
}

fn normalize_lint_globs(
    globs: Vec<String>,
    _file_list: &[String],
    _file: &[PathBuf],
) -> Vec<String> {
    // cspell leaves positional globs empty when the user does not pass any file sources.
    // That allows lint.ts to fall back to config.files instead of scanning "." implicitly.
    globs
}

pub fn dispatch(cmd: CspellCommands) -> anyhow::Result<()> {
    match cmd {
        CspellCommands::Lint {
            globs,
            config,
            no_config_search,
            stop_config_search_at,
            verbose,
            locale,
            language_id,
            words_only,
            unique,
            exclude,
            file_list,
            file,
            no_issues,
            no_progress,
            no_summary,
            silent,
            no_exit_code,
            quiet,
            fail_fast,
            continue_on_error,
            root,
            relative,
            no_relative,
            show_context,
            show_suggestions,
            no_show_suggestions,
            no_must_find_files,
            cache,
            no_cache,
            cache_reset,
            cache_strategy,
            cache_location,
            dot,
            gitignore,
            no_gitignore,
            gitignore_root,
            validate_directives,
            max_file_size,
            no_default_configuration,
            dictionary,
            disable_dictionary,
            allow_compound_words,
            no_allow_compound_words,
            reporter,
            issues_summary_report: _,
            show_perf_summary: _,
        } => {
            if !reporter.is_empty() {
                eprintln!(
                    "Warning: custom reporters are not yet supported, using default reporter"
                );
            }
            let globs = normalize_lint_globs(globs, &file_list, &file);
            lint::run(lint::LintOptions {
                globs,
                config,
                config_search: !no_config_search,
                stop_config_search_at,
                exclude,
                file_list,
                file,
                max_file_size,
                dictionary,
                disable_dictionary,
                allow_compound_words: compound_flag(allow_compound_words, no_allow_compound_words),
                unique,
                words_only,
                no_exit_code,
                show_suggestions: if show_suggestions {
                    Some(true)
                } else if no_show_suggestions {
                    Some(false)
                } else {
                    None
                },
                root,
                quiet,
                silent,
                no_issues,
                no_progress,
                no_summary,
                no_relative: no_relative && !relative,
                show_context,
                fail_fast,
                dot,
                use_gitignore: if gitignore {
                    Some(true)
                } else if no_gitignore {
                    Some(false)
                } else {
                    None
                },
                gitignore_root,
                verbose,
                locale,
                language_id,
                continue_on_error,
                no_must_find_files,
                no_default_configuration,
                validate_directives,
                cache: cache && !no_cache,
                cache_reset,
                cache_strategy,
                cache_location,
            })
        }

        CspellCommands::Check {
            files,
            config,
            validate_directives,
            no_validate_directives,
            no_exit_code,
            no_default_configuration,
        } => check::run(
            files,
            config,
            no_exit_code,
            no_default_configuration,
            validate_directives && !no_validate_directives,
        ),

        CspellCommands::Trace {
            words,
            config,
            locale,
            language_id,
            allow_compound_words,
            no_allow_compound_words,
            ignore_case,
            no_ignore_case,
            dictionary,
            stdin,
            all,
            only_found,
            no_default_configuration,
        } => trace::run(trace::TraceOptions {
            words,
            config,
            locale,
            language_id,
            allow_compound_words: compound_flag(allow_compound_words, no_allow_compound_words),
            ignore_case: ignore_case && !no_ignore_case,
            dictionary,
            stdin,
            show_all: all,
            only_found,
            no_default_configuration,
        }),

        CspellCommands::Suggestions {
            words,
            config,
            locale,
            language_id,
            no_strict,
            ignore_case,
            num_changes,
            num_suggestions,
            stdin,
            repl,
            verbose,
            dictionary,
            dictionaries,
        } => {
            let mut all_dicts = dictionary;
            all_dicts.extend(dictionaries);
            suggestions::run(suggestions::SuggestionsOptions {
                words,
                config,
                locale,
                language_id,
                ignore_case: no_strict || ignore_case,
                num_changes,
                num_suggestions,
                stdin,
                repl,
                verbose,
                filter_dicts: all_dicts,
            })
        }

        CspellCommands::Init {
            config,
            output,
            format,
            imports,
            locale,
            dictionary,
            no_comments,
            stdout,
        } => init::run(init::InitOptions {
            config,
            output,
            format,
            imports,
            locale,
            dictionary,
            no_comments,
            stdout,
        }),

        CspellCommands::Link { action } => link::run(action),

        CspellCommands::Dictionaries {
            config,
            path_format,
            enabled,
            locale,
            file_type,
            no_show_location,
            no_default_configuration,
        } => dictionaries::run(dictionaries::DictListOptions {
            config,
            path_format,
            enabled,
            locale,
            file_type,
            no_show_location,
            no_default_configuration,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_lint_globs;
    use std::path::PathBuf;

    #[test]
    fn lint_preserves_empty_globs_without_file_sources() {
        let globs = normalize_lint_globs(Vec::new(), &[], &[]);
        assert!(
            globs.is_empty(),
            "cspell lint should keep empty positional globs so config.files can drive discovery"
        );
    }

    #[test]
    fn lint_preserves_explicit_globs() {
        let globs = normalize_lint_globs(
            vec!["src/**/*.ts".into()],
            &[],
            &[PathBuf::from("README.md")],
        );
        assert_eq!(globs, vec!["src/**/*.ts"]);
    }
}

/// Load cspell settings from a config path or by searching from cwd.
pub(crate) fn load_settings(
    config_path: Option<&Path>,
) -> Result<(CSpellSettings, Option<PathBuf>)> {
    if let Some(path) = config_path {
        let config_dir = path.parent().map(|p| p.to_path_buf());
        let settings = resolver::load_config(path).context("failed to load config file")?;
        Ok((settings, config_dir))
    } else {
        let cwd = std::env::current_dir()?;
        match resolver::find_config(&cwd) {
            Some(path) => {
                let config_dir = path.parent().map(|p| p.to_path_buf());
                let settings =
                    resolver::load_config(&path).context("failed to load config file")?;
                Ok((settings, config_dir))
            }
            None => Ok((CSpellSettings::default(), None)),
        }
    }
}

/// Build named dictionaries from config dictionary_definitions.
pub(crate) fn build_named_dictionaries(
    settings: &CSpellSettings,
    config_dir: Option<&Path>,
) -> Result<Vec<(String, Box<dyn Dictionary>)>> {
    let mut dicts: Vec<(String, Box<dyn Dictionary>)> = Vec::new();
    for def in &settings.dictionary_definitions {
        if let Some(ref path_str) = def.path {
            let path = Path::new(path_str);
            let resolved = if path.is_absolute() {
                path.to_path_buf()
            } else if let Some(base) = config_dir {
                base.join(path)
            } else {
                path.to_path_buf()
            };
            if resolved.exists() {
                match loader::load_dictionary(&resolved) {
                    Ok(dict) => dicts.push((def.name.clone(), Box::new(dict))),
                    Err(e) => {
                        eprintln!("Warning: failed to load dictionary {}: {}", path_str, e)
                    }
                }
            }
        }
    }
    Ok(dicts)
}

fn compound_flag(allow: bool, no_allow: bool) -> Option<bool> {
    if allow {
        Some(true)
    } else if no_allow {
        Some(false)
    } else {
        None
    }
}
