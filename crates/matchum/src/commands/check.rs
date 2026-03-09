use crate::diff::DiffFilter;
use anyhow::{Context, Result};
use ignore::WalkBuilder;
use md5::{Digest, Md5};
use rayon::prelude::*;
use matchum_config::overrides;
use matchum_config::resolver;
use matchum_config::settings::{CSpellSettings, PatternDefinition, StringOrList};
use matchum_core::issue::ValidationIssue;
use matchum_core::validator::{Validator, ValidatorConfig};
use matchum_dict::dictionary::Dictionary;
use matchum_dict::hashdict::HashDictionary;
use matchum_dict::loader;
use std::collections::{HashMap, HashSet};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[allow(dead_code)]
pub struct CheckResult {
    pub files_checked: usize,
    pub files_with_issues: usize,
    pub total_issues: usize,
}

#[derive(Debug, Default, Clone)]
#[allow(dead_code)]
pub struct CheckOptions {
    pub exclude: Vec<String>,
    pub file_list: Vec<String>,
    pub config_search: bool,
    pub stop_config_search_at: Vec<PathBuf>,
    pub max_file_size: Option<String>,
    pub dictionary: Vec<String>,
    pub disable_dictionary: Vec<String>,
    pub allow_compound_words: Option<bool>,
    // Output control
    pub no_issues: bool,
    pub no_summary: bool,
    pub no_progress: bool,
    pub quiet: bool,
    pub silent: bool,
    pub no_relative: bool,
    pub show_context: bool,
    pub verbose: u8,
    // Behavior control
    pub root: Option<PathBuf>,
    pub fail_fast: bool,
    pub dot: bool,
    pub use_gitignore: Option<bool>,
    pub gitignore_root: Option<PathBuf>,
    pub file: Vec<PathBuf>,
    pub locale: Option<String>,
    pub language_id: Option<String>,
    pub continue_on_error: bool,
    pub no_must_find_files: bool,
    pub no_default_configuration: bool,
    pub validate_directives: bool,
    // Cache
    pub cache: bool,
    pub cache_reset: bool,
    pub cache_strategy: Option<String>,
    pub cache_location: Option<PathBuf>,
    /// Default value for use_gitignore when not explicitly set.
    /// Native mode: true, cspell compat mode: false.
    pub use_gitignore_default: bool,
    /// Base directory for dictionary package resolution.
    /// When set, npm packages are resolved/downloaded here instead of config_dir.
    pub dict_base_dir: Option<PathBuf>,
    /// Only check files of the given type (e.g., "rust", "python").
    pub file_type: Option<String>,
    /// Show execution statistics.
    pub stats: bool,
    /// When set, only report issues on lines present in this diff filter.
    pub diff_filter: Option<Arc<DiffFilter>>,
    /// Settings to use when no config file is found.
    /// `matchum check` uses `default_settings()`, `matchum cspell` uses `CSpellSettings::default()`.
    pub fallback_settings: Option<CSpellSettings>,
}

const DEFAULT_DICTIONARIES: &[&str] = &[
    "en_us",
    "softwareTerms",
    "companies",
    "public-licenses",
    "filetypes",
];

/// Fill default dictionaries when config doesn't specify any.
fn apply_default_dictionaries(settings: &mut CSpellSettings) {
    if settings.dictionaries.is_empty() {
        settings.dictionaries = DEFAULT_DICTIONARIES.iter().map(|&s| s.into()).collect();
    }
}

/// Default settings when no config file is found.
pub fn default_settings() -> CSpellSettings {
    CSpellSettings {
        dictionaries: DEFAULT_DICTIONARIES.iter().map(|&s| s.into()).collect(),
        language: Some("en".into()),
        ignore_paths: vec!["**/node_modules/**".into(), "**/*.lock".into()],
        ..Default::default()
    }
}

pub fn run_check(
    paths: &[PathBuf],
    config_path: Option<&Path>,
    format: &str,
    show_suggestions: bool,
    unique: bool,
    strict: bool,
    options: CheckOptions,
) -> Result<CheckResult> {
    // Handle --root: resolve paths relative to root
    let effective_paths: Vec<PathBuf>;
    let effective_root: Option<&Path>;
    if let Some(ref root) = options.root {
        effective_root = Some(root.as_path());
        effective_paths = paths
            .iter()
            .map(|p| {
                if p.is_absolute() {
                    p.clone()
                } else {
                    root.join(p)
                }
            })
            .collect();
    } else {
        effective_root = None;
        effective_paths = paths.to_vec();
    }

    // Load settings (or skip if --no-default-configuration)
    let fallback = options.fallback_settings.clone().unwrap_or_default();
    let (mut settings, config_dir) = if options.no_default_configuration {
        (CSpellSettings::default(), None)
    } else {
        let config_search_start = effective_paths.first().map(|p| p.as_path());
        let result = load_settings(
            config_path,
            config_search_start.or(effective_root),
            options.config_search,
            &options.stop_config_search_at,
        );
        match result {
            Ok((s, dir)) => {
                if dir.is_some() {
                    (s, dir)
                } else {
                    // No config found — use fallback
                    (fallback, None)
                }
            }
            Err(e) => {
                if options.continue_on_error {
                    if !options.silent {
                        eprintln!("Warning: {:#}", e);
                    }
                    (fallback, None)
                } else {
                    return Err(e);
                }
            }
        }
    };

    // Apply --locale override
    if let Some(ref locale) = options.locale {
        settings.language = Some(locale.clone());
    }

    // Fill default dictionaries if config didn't specify any
    apply_default_dictionaries(&mut settings);

    run_check_inner(
        &effective_paths,
        &settings,
        config_dir.as_deref(),
        format,
        show_suggestions,
        unique,
        strict,
        options,
    )
}

/// Run check with pre-built settings (used by cargo-matchum).
pub fn run_check_with_settings(
    paths: &[PathBuf],
    settings: &CSpellSettings,
    config_dir: Option<&Path>,
    format: &str,
    show_suggestions: bool,
    unique: bool,
    strict: bool,
    options: CheckOptions,
) -> Result<CheckResult> {
    run_check_inner(
        paths,
        settings,
        config_dir,
        format,
        show_suggestions,
        unique,
        strict,
        options,
    )
}

/// Collect all spelling issues from files without formatting output.
/// Returns (file_path, file_content, issues) tuples.
/// Used by `review` command to get raw results.
pub fn collect_all_issues(
    paths: &[PathBuf],
    config_path: Option<&Path>,
    options: CheckOptions,
) -> Result<Vec<(PathBuf, String, Vec<ValidationIssue>)>> {
    let fallback = options.fallback_settings.clone().unwrap_or_default();
    let (mut settings, config_dir) = if options.no_default_configuration {
        (CSpellSettings::default(), None)
    } else {
        let config_search_start = paths.first().map(|p| p.as_path());
        let result = load_settings(
            config_path,
            config_search_start,
            options.config_search,
            &options.stop_config_search_at,
        );
        match result {
            Ok((s, dir)) => {
                if dir.is_some() {
                    (s, dir)
                } else {
                    (fallback, None)
                }
            }
            Err(e) => return Err(e),
        }
    };

    if let Some(ref locale) = options.locale {
        settings.language = Some(locale.clone());
    }

    apply_default_dictionaries(&mut settings);

    run_collect_issues(paths, &settings, config_dir.as_deref(), &options)
}

/// Core issue collection pipeline: build dicts, collect files, validate in parallel.
fn run_collect_issues(
    effective_paths: &[PathBuf],
    settings: &CSpellSettings,
    config_dir: Option<&Path>,
    options: &CheckOptions,
) -> Result<Vec<(PathBuf, String, Vec<ValidationIssue>)>> {
    let (named_dicts, extra_active) =
        build_dictionary_catalog(settings, config_dir, options.dict_base_dir.as_deref())?;
    let compiled_overrides = matchum_config::overrides::compile_overrides(settings);
    let base_validator_config =
        build_validator_config(settings, options.allow_compound_words, false);
    let mut files = collect_files(effective_paths, settings, options)?;

    if let Some(ref df) = options.diff_filter {
        files.retain(|f| df.contains_file(f));
    }

    let language_id = options.language_id.clone();

    let word_caches: std::sync::Mutex<
        std::collections::HashMap<String, matchum_core::validator::WordCache>,
    > = std::sync::Mutex::new(std::collections::HashMap::new());

    let results: Vec<(PathBuf, String, Vec<ValidationIssue>)> = files
        .par_iter()
        .filter_map(|file| {
            let overridden = if compiled_overrides.is_empty() {
                None
            } else {
                overrides::apply_compiled_overrides(settings, file, &compiled_overrides)
            };
            let needs_lang = language_id.is_some();
            let effective_owned = if overridden.is_some() || needs_lang {
                let mut s = overridden.unwrap_or_else(|| settings.clone());
                if let Some(ref lang) = language_id {
                    s.language = Some(lang.clone());
                }
                Some(s)
            } else {
                None
            };
            let effective_settings = effective_owned.as_ref().unwrap_or(settings);
            if effective_settings.enabled == Some(false) {
                return None;
            }

            let content = read_file_mmap(file)?;

            let precompiled = if effective_owned.is_none() {
                Some(&base_validator_config)
            } else {
                None
            };
            let mut validator = build_validator(
                effective_settings,
                &named_dicts,
                options,
                &extra_active,
                false,
                Some(file),
                precompiled,
            );
            if effective_owned.is_none() {
                let lang = language_id_from_path(file);
                let cache = {
                    let mut map = word_caches.lock().unwrap();
                    map.entry(lang)
                        .or_insert_with(Validator::new_word_cache)
                        .clone()
                };
                validator.set_word_cache(cache);
            }
            let issues = validator.validate_text(&content);

            if issues.is_empty() {
                None
            } else {
                Some((file.clone(), content, issues))
            }
        })
        .collect();

    Ok(results)
}

fn run_check_inner(
    effective_paths: &[PathBuf],
    settings: &CSpellSettings,
    config_dir: Option<&Path>,
    format: &str,
    show_suggestions: bool,
    unique: bool,
    strict: bool,
    options: CheckOptions,
) -> Result<CheckResult> {
    let wall_start = std::time::Instant::now();

    let (named_dicts, extra_active) =
        build_dictionary_catalog(settings, config_dir, options.dict_base_dir.as_deref())?;
    let dict_count = named_dicts.len();
    let compiled_overrides = matchum_config::overrides::compile_overrides(&settings);
    let base_validator_config =
        build_validator_config(&settings, options.allow_compound_words, show_suggestions);
    let mut files = collect_files(&effective_paths, &settings, &options)?;

    // Restrict to files present in the diff
    if let Some(ref df) = options.diff_filter {
        files.retain(|f| df.contains_file(f));
    }

    if files.is_empty() && !options.no_must_find_files {
        if !options.quiet && !options.silent {
            eprintln!("No files found to check.");
        }
    }

    let max_file_size = options
        .max_file_size
        .as_deref()
        .and_then(parse_size_to_bytes);

    // Load cache if enabled
    let cache = if options.cache && !options.cache_reset {
        SpellCache::load(&cache_path(&options))
    } else {
        SpellCache::default()
    };
    let use_cache = options.cache;
    let cache_strategy = options.cache_strategy.as_deref().unwrap_or("content");
    let cache = std::sync::Mutex::new(cache);

    let fail_fast = options.fail_fast;
    let fail_fast_flag = std::sync::atomic::AtomicBool::new(false);
    let need_context = options.show_context;
    let verbose = options.verbose;
    let silent = options.silent;
    let quiet = options.quiet;
    let validate_directives = options.validate_directives;
    let language_id = options.language_id.clone();

    let word_caches: std::sync::Mutex<
        std::collections::HashMap<String, matchum_core::validator::WordCache>,
    > = std::sync::Mutex::new(std::collections::HashMap::new());

    let results: Vec<(PathBuf, String, Vec<ValidationIssue>)> = files
        .par_iter()
        .filter_map(|file| {
            if fail_fast && fail_fast_flag.load(std::sync::atomic::Ordering::Relaxed) {
                return None;
            }

            if let Some(max_size) = max_file_size {
                let size = std::fs::metadata(file).ok()?.len();
                if size > max_size {
                    if verbose > 0 && !silent {
                        eprintln!("Skipping (too large): {}", file.display());
                    }
                    return None;
                }
            }

            // Cache check
            if use_cache {
                if let Ok(guard) = cache.lock() {
                    if let Some(cached) = guard.check(file, cache_strategy) {
                        if verbose > 0 && !silent {
                            eprintln!("Cache hit: {}", file.display());
                        }
                        if cached.is_empty() {
                            return None;
                        }
                        let content = if need_context {
                            read_file_mmap(file).unwrap_or_default()
                        } else {
                            String::new()
                        };
                        return Some((file.clone(), content, cached));
                    }
                }
            }

            if verbose > 0 && !silent {
                eprintln!("Checking: {}", file.display());
            }

            // Use pre-compiled overrides: avoids glob re-compilation and
            // skips CSpellSettings clone when no override matches.
            let overridden = if compiled_overrides.is_empty() {
                None
            } else {
                overrides::apply_compiled_overrides(&settings, file, &compiled_overrides)
            };
            let needs_lang = language_id.is_some();
            let effective_owned = if overridden.is_some() || needs_lang {
                let mut s = overridden.unwrap_or_else(|| settings.clone());
                if let Some(ref lang) = language_id {
                    s.language = Some(lang.clone());
                }
                Some(s)
            } else {
                None
            };
            let effective_settings = effective_owned.as_ref().unwrap_or(&settings);
            if effective_settings.enabled == Some(false) {
                return None;
            }

            let content = match read_file_mmap(file) {
                Some(c) => c,
                None => {
                    if !silent {
                        eprintln!("Warning: cannot read {}", file.display());
                    }
                    return None;
                }
            };

            // Reuse pre-compiled ValidatorConfig when no overrides changed
            // pattern/word-related fields (avoids regex recompilation per file).
            let precompiled = if effective_owned.is_none() {
                Some(&base_validator_config)
            } else {
                None
            };
            let mut validator = build_validator(
                effective_settings,
                &named_dicts,
                &options,
                &extra_active,
                show_suggestions,
                Some(file),
                precompiled,
            );
            if effective_owned.is_none() {
                let lang = language_id_from_path(file);
                let cache = {
                    let mut map = word_caches.lock().unwrap();
                    map.entry(lang)
                        .or_insert_with(Validator::new_word_cache)
                        .clone()
                };
                validator.set_word_cache(cache);
            }
            let mut issues = validator.validate_text(&content);

            // Validate directives if requested
            if validate_directives {
                let directive_issues = check_directives(&content);
                issues.extend(directive_issues);
                issues.sort_by(|a, b| a.offset.cmp(&b.offset));
            }

            if issues.is_empty() {
                if use_cache {
                    if let Ok(mut guard) = cache.lock() {
                        guard.update(file, &[], cache_strategy, Some(content.as_bytes()));
                    }
                }
                None
            } else {
                if fail_fast {
                    fail_fast_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                if use_cache {
                    if let Ok(mut guard) = cache.lock() {
                        guard.update(file, &issues, cache_strategy, Some(content.as_bytes()));
                    }
                }
                let kept_content = if need_context { content } else { String::new() };
                Some((file.clone(), kept_content, issues))
            }
        })
        .collect();

    // Save cache
    if use_cache {
        if let Ok(guard) = cache.lock() {
            guard.save(&cache_path(&options));
        }
    }

    let mut total_issues = 0;
    let mut unique_words = HashSet::new();
    let show_issues = !options.no_issues && format != "json";
    let base_dir = if options.no_relative {
        None
    } else {
        std::env::current_dir().ok()
    };

    for (file, content, issues) in &results {
        for issue in issues {
            // Skip issues not on added lines when diff filtering
            if let Some(ref df) = options.diff_filter {
                if !df.should_report(file, issue.line) {
                    continue;
                }
            }
            if unique && !unique_words.insert(issue.word.clone()) {
                continue;
            }
            total_issues += 1;
            if show_issues {
                let display_path = match &base_dir {
                    Some(base) => file.strip_prefix(base).unwrap_or(file).to_path_buf(),
                    None => std::fs::canonicalize(file).unwrap_or_else(|_| file.clone()),
                };
                if format == "words-only" {
                    println!("{}", issue.word);
                } else if format == "github" {
                    print_issue_github(&display_path, issue);
                } else {
                    let ctx = if need_context {
                        Some(content.as_str())
                    } else {
                        None
                    };
                    print_issue_text(&display_path, issue, show_suggestions, need_context, ctx);
                }
            }
        }
    }

    if format == "json" {
        print_json_output(&results, &options.diff_filter)?;
    }

    let files_checked = files.len();
    let files_with_issues = results.len();

    if !options.no_summary && !quiet && format != "json" {
        if total_issues > 0 {
            eprintln!(
                "\nFound {} issue{} in {} file{}",
                total_issues,
                if total_issues == 1 { "" } else { "s" },
                files_with_issues,
                if files_with_issues == 1 { "" } else { "s" },
            );
        } else if verbose > 0 && !silent {
            eprintln!("\nNo issues found. ({files_checked} file(s) checked)");
        }
    }

    if options.stats {
        let elapsed = wall_start.elapsed();
        eprintln!("\n--- Statistics ---");
        eprintln!("Files checked:      {}", files_checked);
        eprintln!("Files with issues:  {}", files_with_issues);
        eprintln!("Issues found:       {}", total_issues);
        eprintln!("Dictionaries:       {}", dict_count);
        eprintln!("Time:               {:.2}s", elapsed.as_secs_f64());
    }

    if strict && total_issues > 0 {
        std::process::exit(1);
    }

    Ok(CheckResult {
        files_checked,
        files_with_issues,
        total_issues,
    })
}

fn load_settings(
    config_path: Option<&Path>,
    start_path: Option<&Path>,
    config_search: bool,
    stop_config_search_at: &[PathBuf],
) -> Result<(CSpellSettings, Option<PathBuf>)> {
    if let Some(path) = config_path {
        let config_dir = path.parent().map(|p| p.to_path_buf());
        let settings = load_config_by_ext(path)?;
        return Ok((settings, config_dir));
    }

    if !config_search {
        return Ok((CSpellSettings::default(), None));
    }

    let search_dir = start_path
        .and_then(|p| {
            if p.is_dir() {
                Some(p.to_path_buf())
            } else {
                p.parent().map(|pp| pp.to_path_buf())
            }
        })
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    match resolver::find_config_prioritized_with_stop(&search_dir, stop_config_search_at) {
        resolver::ConfigFound::Matchum(path) => {
            let config_dir = path.parent().map(|p| p.to_path_buf());
            let settings =
                resolver::load_matchum_config(&path).context("failed to load matchum.toml")?;
            Ok((settings, config_dir))
        }
        resolver::ConfigFound::Cspell(path) => {
            eprintln!(
                "hint: using cspell config. Run `matchum init --from-cspell` to migrate to matchum.toml."
            );
            let config_dir = path.parent().map(|p| p.to_path_buf());
            let settings = resolver::load_config(&path).context("failed to load config file")?;
            Ok((settings, config_dir))
        }
        resolver::ConfigFound::None => Ok((CSpellSettings::default(), None)),
    }
}

/// Load config by file extension: `.toml` → matchum loader, otherwise → cspell loader.
fn load_config_by_ext(path: &Path) -> Result<CSpellSettings> {
    resolver::load_config_auto(path).context("failed to load config")
}

/// A dictionary loading job collected in the first (sequential) phase.
struct DictJob {
    name: String,
    path: PathBuf,
    extra_active: bool,
}

pub fn build_dictionary_catalog(
    settings: &CSpellSettings,
    config_dir: Option<&Path>,
    dict_base_dir: Option<&Path>,
) -> Result<(Vec<(String, Arc<dyn Dictionary>)>, HashSet<String>)> {
    let defined_names: HashSet<String> = settings
        .dictionary_definitions
        .iter()
        .map(|d| d.name.to_lowercase())
        .collect();

    // Phase 1: Collect all dictionary paths (sequential, lightweight I/O)
    let mut jobs: Vec<DictJob> = Vec::new();
    let mut collected_names: HashSet<String> = HashSet::new();

    // User-defined dictionaries
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
                let name = def.name.to_lowercase();
                collected_names.insert(name.clone());
                jobs.push(DictJob {
                    name,
                    path: resolved,
                    extra_active: false,
                });
            }
        }
    }

    // Auto-fetch @cspell/dict-* packages
    let npm_base = dict_base_dir.or(config_dir);
    if let Some(base) = npm_base {
        for dict_name in &settings.dictionaries {
            let lower = dict_name.to_lowercase();
            if defined_names.contains(&lower) || collected_names.contains(&lower) {
                continue;
            }
            let pkg_name = dict_name_to_package(&lower);
            let ext_path = format!("{}/cspell-ext.json", pkg_name);
            if let Some(ext_json) = resolve_npm_import(&ext_path, base) {
                if let Ok(content) = std::fs::read_to_string(&ext_json) {
                    if let Ok(ext_settings) = json5::from_str::<CSpellSettings>(&content) {
                        let ext_dir = ext_json.parent().unwrap_or(base);
                        let ext_active: HashSet<String> = ext_settings
                            .dictionaries
                            .iter()
                            .map(|d| d.to_lowercase())
                            .collect();
                        for ext_def in &ext_settings.dictionary_definitions {
                            if let Some(ref p) = ext_def.path {
                                let dict_path = ext_dir.join(p);
                                if dict_path.exists() {
                                    let name = ext_def.name.to_lowercase();
                                    let is_active = ext_active.contains(&name) || name == lower;
                                    collected_names.insert(name.clone());
                                    jobs.push(DictJob {
                                        name,
                                        path: dict_path,
                                        extra_active: is_active,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Default bundled dictionaries
    if let Some(base) = npm_base {
        for &(pkg, dict_name) in DEFAULT_BUNDLED_DICTS {
            if collected_names.contains(dict_name) {
                continue;
            }
            let ext_path = format!("{}/cspell-ext.json", pkg);
            if let Some(ext_json) = resolve_npm_import(&ext_path, base) {
                if let Ok(content) = std::fs::read_to_string(&ext_json) {
                    if let Ok(ext_settings) = json5::from_str::<CSpellSettings>(&content) {
                        let ext_dir = ext_json.parent().unwrap_or(base);
                        let ext_active: HashSet<String> = ext_settings
                            .dictionaries
                            .iter()
                            .map(|d| d.to_lowercase())
                            .collect();
                        for ext_def in &ext_settings.dictionary_definitions {
                            let name = ext_def.name.to_lowercase();
                            if collected_names.contains(&name) {
                                continue;
                            }
                            if let Some(ref p) = ext_def.path {
                                let dict_path = ext_dir.join(p);
                                if dict_path.exists() {
                                    let is_active = ext_active.contains(&name) || name == dict_name;
                                    collected_names.insert(name.clone());
                                    jobs.push(DictJob {
                                        name,
                                        path: dict_path,
                                        extra_active: is_active,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Phase 2: Load all dictionaries in parallel
    let results: Vec<_> = jobs
        .par_iter()
        .filter_map(|job| match loader::load_dictionary(&job.path) {
            Ok(dict) => Some((job, dict)),
            Err(e) => {
                eprintln!(
                    "Warning: failed to load dictionary {}: {}",
                    job.path.display(),
                    e
                );
                None
            }
        })
        .collect();

    // Phase 3: Assemble results (preserves job order from par_iter)
    let mut dicts: Vec<(String, Arc<dyn Dictionary>)> = Vec::with_capacity(results.len());
    let mut extra_active: HashSet<String> = HashSet::new();
    for (job, dict) in results {
        if job.extra_active {
            extra_active.insert(job.name.clone());
        }
        dicts.push((job.name.clone(), Arc::new(dict) as Arc<dyn Dictionary>));
    }

    Ok((dicts, extra_active))
}

/// Default bundled dictionary packages that cspell always loads via
/// @cspell/cspell-bundled-dicts. Each entry is (package_name, primary_dict_name).
const DEFAULT_BUNDLED_DICTS: &[(&str, &str)] = &[
    ("@cspell/dict-fullstack", "fullstack"),
    ("@cspell/dict-companies", "companies"),
    ("@cspell/dict-aws", "aws"),
    ("@cspell/dict-cryptocurrencies", "cryptocurrencies"),
    ("@cspell/dict-filetypes", "filetypes"),
    ("@cspell/dict-public-licenses", "public-licenses"),
];

/// Map a dictionary name (from `dictionaries` array) to an npm package name.
///
/// cspell uses camelCase dict names that map to kebab-case package names,
/// e.g. `softwareTerms` → `@cspell/dict-software-terms`
fn dict_name_to_package(dict_name: &str) -> String {
    // Known mappings for common cspell bundled dicts
    match dict_name {
        "softwareterms" => return "@cspell/dict-software-terms".into(),
        "en_us" => return "@cspell/dict-en_us".into(),
        "c" => return "@cspell/dict-cpp".into(),
        _ => {}
    }
    // General: convert camelCase to kebab-case
    let mut result = String::from("@cspell/dict-");
    for (i, ch) in dict_name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('-');
        }
        result.push(ch.to_lowercase().next().unwrap_or(ch));
    }
    result
}

/// Map a file extension to a cspell languageId.
fn language_id_from_path(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "rs" => "rust",
        "py" | "pyw" | "pyi" => "python",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => "cpp",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" | "mts" | "cts" => "typescript",
        "jsx" => "javascriptreact",
        "tsx" => "typescriptreact",
        "java" => "java",
        "go" => "go",
        "rb" => "ruby",
        "php" => "php",
        "cs" => "csharp",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "scala" => "scala",
        "sh" | "bash" | "zsh" => "shellscript",
        "ps1" | "psm1" => "powershell",
        "r" => "r",
        "lua" => "lua",
        "pl" | "pm" => "perl",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" => "scss",
        "less" => "less",
        "json" | "jsonc" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "xml" => "xml",
        "md" | "markdown" => "markdown",
        "sql" => "sql",
        "dockerfile" => "dockerfile",
        "makefile" => "makefile",
        _ => {
            // Check filename for special cases
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let name_lower = name.to_lowercase();
            if name_lower == "dockerfile" {
                "dockerfile"
            } else if name_lower == "makefile" || name_lower == "gnumakefile" {
                "makefile"
            } else {
                "plaintext"
            }
        }
    }
    .to_string()
}

/// Check if a list of languageId patterns matches a given languageId.
/// Supports `"*"` (all), and exact match.
fn language_id_matches(patterns: &[String], lang_id: &str) -> bool {
    patterns
        .iter()
        .any(|p| p == "*" || p.eq_ignore_ascii_case(lang_id))
}

/// Try to resolve an npm package import by walking up node_modules or auto-fetching.
fn resolve_npm_import(import: &str, base_dir: &Path) -> Option<PathBuf> {
    use matchum_config::npm_fetch;

    // Walk up looking for node_modules/
    let mut search_dir = Some(base_dir);
    while let Some(dir) = search_dir {
        let candidate = dir.join("node_modules").join(import);
        if candidate.exists() {
            return Some(candidate);
        }
        search_dir = dir.parent();
    }

    // Auto-download into cache directory (not project directory)
    let package_name = npm_fetch::extract_package_name(import);
    let sub_path = npm_fetch::extract_sub_path(import);
    let cache_dir = npm_fetch::default_cache_dir();
    if let Ok(pkg_dir) = npm_fetch::ensure_package(package_name, None, &cache_dir) {
        let resolved = match sub_path {
            Some(sub) => pkg_dir.join(sub),
            None => pkg_dir.join("cspell-ext.json"),
        };
        if resolved.exists() {
            return Some(resolved);
        }
    }

    None
}

fn build_validator(
    settings: &CSpellSettings,
    named_dicts: &[(String, Arc<dyn Dictionary>)],
    options: &CheckOptions,
    extra_active: &HashSet<String>,
    compute_suggestions: bool,
    file: Option<&Path>,
    precompiled_config: Option<&ValidatorConfig>,
) -> Validator {
    let requested: Option<HashSet<String>> =
        if settings.dictionaries.is_empty() && settings.language_settings.is_empty() {
            None
        } else {
            let mut set: HashSet<String> = settings
                .dictionaries
                .iter()
                .map(|d| d.to_lowercase())
                .collect();
            set.extend(extra_active.iter().cloned());
            // Apply languageSettings: activate dicts for matching file types
            if let Some(file_path) = file {
                let lang_id = language_id_from_path(file_path);
                for ls in &settings.language_settings {
                    if language_id_matches(&ls.language_id, &lang_id) {
                        for d in &ls.dictionaries {
                            set.insert(d.to_lowercase());
                        }
                    }
                }
            }
            Some(set)
        };

    let cli_enable: HashSet<String> = options
        .dictionary
        .iter()
        .map(|d| d.to_lowercase())
        .collect();
    let cli_disable: HashSet<String> = options
        .disable_dictionary
        .iter()
        .map(|d| d.to_lowercase())
        .collect();

    let mut entries: Vec<(String, Arc<dyn Dictionary>, bool)> = Vec::new();
    for (name, dict) in named_dicts {
        let mut active = requested
            .as_ref()
            .map(|set| set.contains(name))
            .unwrap_or(true);
        if cli_disable.contains(name) {
            active = false;
        }
        if cli_enable.contains(name) {
            active = true;
        }
        entries.push((name.clone(), Arc::clone(dict), active));
    }

    if !settings.words.is_empty() || !settings.user_words.is_empty() {
        let mut inline_dict = HashDictionary::new(false);
        for word in &settings.words {
            inline_dict.add_word(word);
        }
        for word in &settings.user_words {
            inline_dict.add_word(word);
        }
        entries.push(("__inline_words".into(), Arc::new(inline_dict), true));
    }

    let validator_config = match precompiled_config {
        Some(config) => config.clone(),
        None => build_validator_config(settings, options.allow_compound_words, compute_suggestions),
    };
    Validator::new_named(entries, validator_config)
}

fn build_validator_config(
    settings: &CSpellSettings,
    cli_allow_compound_words: Option<bool>,
    compute_suggestions: bool,
) -> ValidatorConfig {
    let (ignore_patterns, include_patterns) = resolve_patterns(settings);

    ValidatorConfig {
        min_word_length: settings.min_word_length.unwrap_or(4),
        case_sensitive: settings.case_sensitive.unwrap_or(false),
        ignore_patterns,
        include_patterns,
        flag_words: settings
            .flag_words
            .iter()
            .map(|w| compact_str::CompactString::from(w.to_lowercase()))
            .collect(),
        ignore_words: settings
            .ignore_words
            .iter()
            .map(|w| compact_str::CompactString::from(w.to_lowercase()))
            .collect(),
        allow_compound_words: cli_allow_compound_words
            .unwrap_or(settings.allow_compound_words.unwrap_or(false)),
        compound_words_mode: matchum_core::validator::CompoundWordsMode::None,
        compute_suggestions,
        max_duplicate_problems: settings.max_duplicate_problems.unwrap_or(5),
    }
}

fn resolve_patterns(settings: &CSpellSettings) -> (Vec<regex::Regex>, Vec<regex::Regex>) {
    let defs: HashMap<String, &PatternDefinition> = settings
        .patterns
        .iter()
        .map(|p| (p.name.to_lowercase(), p))
        .collect();

    let mut ignore = Vec::new();
    for p in &settings.ignore_reg_exp_list {
        resolve_pattern_token(p, &defs, &mut HashSet::new(), &mut ignore);
    }

    let mut include = Vec::new();
    for p in &settings.include_reg_exp_list {
        resolve_pattern_token(p, &defs, &mut HashSet::new(), &mut include);
    }

    (ignore, include)
}

fn resolve_pattern_token(
    token: &str,
    defs: &HashMap<String, &PatternDefinition>,
    visiting: &mut HashSet<String>,
    out: &mut Vec<regex::Regex>,
) {
    let key = token.trim().to_lowercase();
    if let Some(def) = defs.get(&key) {
        if !visiting.insert(key.clone()) {
            return;
        }
        match &def.pattern {
            StringOrList::Single(s) => resolve_pattern_token(s, defs, visiting, out),
            StringOrList::List(list) => {
                for s in list {
                    resolve_pattern_token(s, defs, visiting, out);
                }
            }
        }
        visiting.remove(&key);
        return;
    }

    if let Some(re) = parse_regex_pattern(token) {
        out.push(re);
    }
}

fn parse_regex_pattern(value: &str) -> Option<regex::Regex> {
    let s = value.trim();
    if s.starts_with('/') && s.len() > 1 {
        if let Some(last_slash) = s.rfind('/') {
            if last_slash > 0 {
                let body = &s[1..last_slash];
                let flags = &s[last_slash + 1..];
                let mut prefix = String::new();
                if flags.contains('i') {
                    prefix.push('i');
                }
                if flags.contains('m') {
                    prefix.push('m');
                }
                if flags.contains('s') {
                    prefix.push('s');
                }
                // 'u' — Rust regex is Unicode by default, ignore
                // 'g' — find_iter handles global matching, ignore
                // 'x' — verbose mode: strip unescaped whitespace and # comments
                let body = if flags.contains('x') {
                    strip_verbose_whitespace(body)
                } else {
                    body.to_string()
                };
                let pat = if prefix.is_empty() {
                    body
                } else {
                    format!("(?{}){}", prefix, body)
                };
                return regex::Regex::new(&pat).ok();
            }
        }
    }
    regex::Regex::new(s).ok()
}

/// Strip unescaped whitespace and `#` line comments for verbose (`x`) mode.
fn strip_verbose_whitespace(pattern: &str) -> String {
    let mut result = String::with_capacity(pattern.len());
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            result.push(ch);
            if let Some(next) = chars.next() {
                result.push(next);
            }
        } else if ch == '#' {
            for c in chars.by_ref() {
                if c == '\n' {
                    break;
                }
            }
        } else if ch.is_whitespace() {
            // Skip unescaped whitespace
        } else {
            result.push(ch);
        }
    }
    result
}

fn collect_files(
    paths: &[PathBuf],
    settings: &CSpellSettings,
    options: &CheckOptions,
) -> Result<Vec<PathBuf>> {
    let mut roots = paths.to_vec();
    roots.extend(read_file_list_paths(&options.file_list)?);
    for f in &options.file {
        if f.is_file() {
            roots.push(f.clone());
        }
    }

    // If no paths specified, try settings.files glob patterns
    if roots.is_empty() {
        if let Some(ref file_globs) = settings.files {
            for pattern in file_globs {
                if let Ok(glob) = globset::Glob::new(pattern) {
                    let matcher = glob.compile_matcher();
                    let walk_dir = std::env::current_dir().unwrap_or_default();
                    let walker = WalkBuilder::new(&walk_dir).hidden(false).build();
                    for entry in walker {
                        if let Ok(entry) = entry {
                            if entry.file_type().is_some_and(|ft| ft.is_file()) {
                                let path = entry.path();
                                let rel = path.strip_prefix(&walk_dir).unwrap_or(path);
                                if matcher.is_match(rel) {
                                    roots.push(entry.into_path());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let use_gitignore = options.use_gitignore.unwrap_or(
        settings
            .use_gitignore
            .unwrap_or(options.use_gitignore_default),
    );
    let show_hidden = options.dot;

    let mut files = Vec::new();

    for path in &roots {
        if path.is_file() {
            files.push(path.clone());
        } else {
            let mut builder = WalkBuilder::new(path);
            builder.hidden(!show_hidden).git_ignore(use_gitignore);

            // --gitignore-root limits .gitignore search depth
            if let Some(ref gi_root) = options.gitignore_root {
                builder.git_global(false);
                // Add the gitignore root as a custom ignore root
                let gi_path = gi_root.join(".gitignore");
                if gi_path.exists() {
                    let _ = builder.add_ignore(&gi_path);
                }
            }

            for entry in builder.build() {
                if let Ok(entry) = entry {
                    if entry.file_type().is_some_and(|ft| ft.is_file()) {
                        files.push(entry.into_path());
                    }
                }
            }
        }
    }

    if !settings.ignore_paths.is_empty() {
        let mut glob_builder = globset::GlobSetBuilder::new();
        for pattern in &settings.ignore_paths {
            if let Ok(glob) = globset::Glob::new(pattern) {
                glob_builder.add(glob);
            }
        }
        if let Ok(glob_set) = glob_builder.build() {
            files.retain(|f| !glob_set.is_match(f));
        }
    }

    if !options.exclude.is_empty() {
        let mut glob_builder = globset::GlobSetBuilder::new();
        for pattern in &options.exclude {
            if let Ok(glob) = globset::Glob::new(pattern) {
                glob_builder.add(glob);
            }
        }
        if let Ok(glob_set) = glob_builder.build() {
            files.retain(|f| !glob_set.is_match(f));
        }
    }

    // Filter out linguist-vendored / linguist-generated paths from .gitattributes.
    // Globs like `vendor/**` are relative, so we must strip the root prefix
    // from each walked path before matching.
    {
        let mut vendored_entries: Vec<(PathBuf, globset::GlobSet)> = Vec::new();
        for path in &roots {
            let root = if path.is_file() {
                path.parent().unwrap_or(path)
            } else {
                path.as_path()
            };
            let patterns = crate::gitattributes::vendored_globs(root);
            if patterns.is_empty() {
                continue;
            }
            let mut builder = globset::GlobSetBuilder::new();
            for pattern in &patterns {
                if let Ok(glob) = globset::Glob::new(pattern) {
                    builder.add(glob);
                }
            }
            if let Ok(glob_set) = builder.build() {
                vendored_entries.push((root.to_path_buf(), glob_set));
            }
        }
        if !vendored_entries.is_empty() {
            files.retain(|f| {
                !vendored_entries.iter().any(|(root, glob_set)| {
                    let rel = f.strip_prefix(root).unwrap_or(f);
                    glob_set.is_match(rel)
                })
            });
        }
    }

    // --type filter: only keep files matching the given type
    if let Some(ref ft) = options.file_type {
        let exts = type_to_extensions(ft);
        if !exts.is_empty() {
            files.retain(|f| {
                f.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| exts.contains(&e.to_lowercase().as_str()))
            });
        }
    }

    let mut seen = HashSet::new();
    files.retain(|f| seen.insert(f.clone()));

    Ok(files)
}

fn type_to_extensions(file_type: &str) -> Vec<&'static str> {
    match file_type.to_lowercase().as_str() {
        "rust" | "rs" => vec!["rs"],
        "python" | "py" => vec!["py", "pyw", "pyi"],
        "javascript" | "js" => vec!["js", "mjs", "cjs", "jsx"],
        "typescript" | "ts" => vec!["ts", "mts", "cts", "tsx"],
        "c" => vec!["c", "h"],
        "cpp" | "c++" => vec!["cpp", "cc", "cxx", "hpp", "hxx", "hh", "h"],
        "java" => vec!["java"],
        "go" => vec!["go"],
        "ruby" | "rb" => vec!["rb"],
        "php" => vec!["php"],
        "swift" => vec!["swift"],
        "kotlin" | "kt" => vec!["kt", "kts"],
        "scala" => vec!["scala"],
        "shell" | "sh" | "bash" => vec!["sh", "bash", "zsh"],
        "html" => vec!["html", "htm"],
        "css" => vec!["css", "scss", "less"],
        "json" => vec!["json", "jsonc"],
        "yaml" | "yml" => vec!["yaml", "yml"],
        "toml" => vec!["toml"],
        "markdown" | "md" => vec!["md", "markdown"],
        "sql" => vec!["sql"],
        "xml" => vec!["xml"],
        _ => vec![],
    }
}

fn read_file_list_paths(file_lists: &[String]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in file_lists {
        if entry == "stdin" {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            for line in buf.lines() {
                let path = line.trim();
                if !path.is_empty() {
                    files.push(PathBuf::from(path));
                }
            }
            continue;
        }

        let content = std::fs::read_to_string(entry)
            .with_context(|| format!("failed to read file list {}", entry))?;
        for line in content.lines() {
            let path = line.trim();
            if !path.is_empty() {
                files.push(PathBuf::from(path));
            }
        }
    }
    Ok(files)
}

fn parse_size_to_bytes(s: &str) -> Option<u64> {
    let t = s.trim();
    let mut split = t.len();
    for (i, ch) in t.char_indices() {
        if !ch.is_ascii_digit() && ch != '.' {
            split = i;
            break;
        }
    }
    let (num_part, unit_part) = t.split_at(split);
    let num: f64 = num_part.parse().ok()?;
    let unit = unit_part.trim().to_ascii_uppercase();
    let multiplier = match unit.as_str() {
        "" | "B" => 1.0,
        "KB" => 1024.0,
        "MB" => 1024.0 * 1024.0,
        "GB" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((num * multiplier) as u64)
}

fn print_issue_github(file: &Path, issue: &ValidationIssue) {
    let kind = if issue.is_forbidden {
        "Forbidden word"
    } else {
        "Unknown word"
    };
    println!(
        "::error file={},line={},col={}::{} ({})",
        file.display(),
        issue.line,
        issue.column,
        kind,
        issue.word,
    );
}

fn print_issue_text(
    file: &Path,
    issue: &ValidationIssue,
    show_suggestions: bool,
    show_context: bool,
    content: Option<&str>,
) {
    let kind = if issue.is_forbidden {
        "Forbidden word"
    } else {
        "Unknown word"
    };

    print!(
        "{}:{}:{} - {} ({})",
        file.display(),
        issue.line,
        issue.column,
        kind,
        issue.word
    );
    if !issue.is_forbidden && show_suggestions && !issue.suggestions.is_empty() {
        print!(" fix: ({})", issue.suggestions.join(", "));
    }
    println!();

    if show_context {
        if let Some(text) = content {
            if let Some(line_text) = text.lines().nth(issue.line.saturating_sub(1)) {
                eprintln!("    {line_text}");
            }
        }
    }
}

fn print_json_output(
    results: &[(PathBuf, String, Vec<ValidationIssue>)],
    diff_filter: &Option<Arc<DiffFilter>>,
) -> Result<()> {
    let output: Vec<serde_json::Value> = results
        .iter()
        .map(|(file, _content, issues)| {
            let issue_values: Vec<serde_json::Value> = issues
                .iter()
                .filter(|i| {
                    diff_filter
                        .as_ref()
                        .map_or(true, |df| df.should_report(file, i.line))
                })
                .map(|i| {
                    serde_json::json!({
                        "word": i.word,
                        "line": i.line,
                        "column": i.column,
                        "offset": i.offset,
                        "forbidden": i.is_forbidden,
                        "suggestions": i.suggestions,
                    })
                })
                .collect();
            serde_json::json!({
                "path": file.display().to_string(),
                "issues": issue_values,
            })
        })
        .collect();

    let json = serde_json::json!({
        "files": output,
        "summary": {
            "files_with_issues": results.len(),
            "total_issues": results.iter().map(|(_, _, i)| i.len()).sum::<usize>(),
        }
    });

    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}

/// Read a file using memory-mapping, falling back to `read_to_string` for
/// empty files or when mmap fails.
fn read_file_mmap(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let meta = file.metadata().ok()?;
    let len = meta.len();
    if len == 0 {
        return Some(String::new());
    }
    // SAFETY: We only read the mapped memory as UTF-8 bytes.
    // Concurrent file modification could cause garbled output but not
    // memory unsafety in practice (CLI tool, non-critical).
    let mmap = unsafe { memmap2::Mmap::map(&file) }.ok()?;
    let s = std::str::from_utf8(&mmap).ok()?;
    Some(s.to_string())
}

// ---------------------------------------------------------------------------
// Directive validation
// ---------------------------------------------------------------------------

/// Known directive prefixes (case-insensitive)
const DIRECTIVE_PREFIXES: &[&str] = &["cspell:", "spell-checker:", "spellchecker:"];

const KNOWN_DIRECTIVES: &[&str] = &[
    "enable",
    "disable",
    "disable-line",
    "disable-next",
    "disable-next-line",
    "word",
    "words",
    "ignore",
    "ignoreWord",
    "ignoreWords",
    "ignore-word",
    "ignore-words",
    "includeRegExp",
    "ignoreRegExp",
    "local",
    "locale",
    "language",
    "dictionaries",
    "dictionary",
    "forbid",
    "forbidWord",
    "forbid-word",
    "flag",
    "flagWord",
    "flag-word",
    "enableCompoundWords",
    "enableAllowCompoundWords",
    "disableCompoundWords",
    "disableAllowCompoundWords",
    "enableCaseSensitive",
    "disableCaseSensitive",
];

fn check_directives(content: &str) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let directive_re = regex::Regex::new(r"(?i)(?:cspell|spell-?checker)\s*:\s*(\S+)").unwrap();

    for (line_idx, line) in content.lines().enumerate() {
        for cap in directive_re.captures_iter(line) {
            let directive = &cap[1];
            let directive_lower = directive.to_lowercase();
            let is_known = KNOWN_DIRECTIVES
                .iter()
                .any(|d| d.to_lowercase() == directive_lower);
            if !is_known {
                let _ = DIRECTIVE_PREFIXES; // suppress unused warning
                let match_start = cap.get(1).unwrap().start();
                issues.push(ValidationIssue {
                    word: format!("cspell:{directive}"),
                    offset: 0,
                    line: line_idx + 1,
                    column: match_start + 1,
                    is_forbidden: false,
                    is_known_typo: false,
                    suggestions: vec![],
                });
            }
        }
    }
    issues
}

// ---------------------------------------------------------------------------
// Cache system
// ---------------------------------------------------------------------------

fn cache_path(options: &CheckOptions) -> PathBuf {
    options
        .cache_location
        .clone()
        .unwrap_or_else(|| PathBuf::from(".cspellcache"))
}

#[derive(Default)]
struct SpellCache {
    entries: HashMap<PathBuf, CacheEntry>,
}

struct CacheEntry {
    mtime_secs: i64,
    size: u64,
    hash: Option<String>,
    issues: Vec<ValidationIssue>,
}

impl SpellCache {
    fn load(path: &Path) -> Self {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Self::default(),
        };
        let parsed: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return Self::default(),
        };
        let mut entries = HashMap::new();
        if let Some(obj) = parsed.get("entries").and_then(|e| e.as_object()) {
            for (key, val) in obj {
                let mtime_secs = val.get("mtime").and_then(|v| v.as_i64()).unwrap_or(0);
                let size = val.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                let hash = val
                    .get("hash")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let issues = val
                    .get("issues")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| {
                                Some(ValidationIssue {
                                    word: v.get("word")?.as_str()?.to_string(),
                                    offset: v.get("offset")?.as_u64()? as usize,
                                    line: v.get("line")?.as_u64()? as usize,
                                    column: v.get("column")?.as_u64()? as usize,
                                    is_forbidden: v
                                        .get("forbidden")
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(false),
                                    is_known_typo: false,
                                    suggestions: vec![],
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                entries.insert(
                    PathBuf::from(key),
                    CacheEntry {
                        mtime_secs,
                        size,
                        hash,
                        issues,
                    },
                );
            }
        }
        Self { entries }
    }

    fn check(&self, path: &Path, strategy: &str) -> Option<Vec<ValidationIssue>> {
        let entry = self.entries.get(path)?;

        if strategy == "content" {
            // Content strategy: compare MD5 hash of file contents
            let cached_hash = entry.hash.as_deref()?;
            let content = std::fs::read(path).ok()?;
            let hash = format!("{:x}", Md5::digest(&content));
            if hash == cached_hash {
                Some(entry.issues.clone())
            } else {
                None
            }
        } else {
            // Metadata strategy: compare mtime + size
            let meta = std::fs::metadata(path).ok()?;
            let size = meta.len();
            let mtime = meta
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_secs() as i64;

            if entry.mtime_secs == mtime && entry.size == size {
                Some(entry.issues.clone())
            } else {
                None
            }
        }
    }

    fn update(
        &mut self,
        path: &Path,
        issues: &[ValidationIssue],
        strategy: &str,
        content: Option<&[u8]>,
    ) {
        if let Ok(meta) = std::fs::metadata(path) {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let hash = if strategy == "content" {
                let data = content.unwrap_or_else(|| {
                    // Caller should provide content for content strategy;
                    // fallback: read the file (less efficient but correct)
                    &[]
                });
                if data.is_empty() {
                    std::fs::read(path)
                        .ok()
                        .map(|d| format!("{:x}", Md5::digest(&d)))
                } else {
                    Some(format!("{:x}", Md5::digest(data)))
                }
            } else {
                None
            };
            self.entries.insert(
                path.to_path_buf(),
                CacheEntry {
                    mtime_secs: mtime,
                    size: meta.len(),
                    hash,
                    issues: issues.to_vec(),
                },
            );
        }
    }

    fn save(&self, path: &Path) {
        let mut entries = serde_json::Map::new();
        for (file_path, entry) in &self.entries {
            let issues: Vec<serde_json::Value> = entry
                .issues
                .iter()
                .map(|i| {
                    serde_json::json!({
                        "word": i.word,
                        "offset": i.offset,
                        "line": i.line,
                        "column": i.column,
                        "forbidden": i.is_forbidden,
                    })
                })
                .collect();
            let mut entry_json = serde_json::json!({
                "mtime": entry.mtime_secs,
                "size": entry.size,
                "issues": issues,
            });
            if let Some(ref hash) = entry.hash {
                entry_json["hash"] = serde_json::json!(hash);
            }
            entries.insert(file_path.display().to_string(), entry_json);
        }
        let json = serde_json::json!({
            "version": "1",
            "entries": entries,
        });
        if let Ok(content) = serde_json::to_string(&json) {
            let _ = std::fs::write(path, content);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_defaults_when_empty() {
        let mut settings = CSpellSettings::default();
        assert!(settings.dictionaries.is_empty());

        apply_default_dictionaries(&mut settings);

        assert_eq!(settings.dictionaries.len(), DEFAULT_DICTIONARIES.len());
        for &name in DEFAULT_DICTIONARIES {
            assert!(settings.dictionaries.iter().any(|d| d == name));
        }
    }

    #[test]
    fn preserve_explicit_dictionaries() {
        let mut settings = CSpellSettings {
            dictionaries: vec!["en_gb".into(), "custom".into()],
            ..Default::default()
        };

        apply_default_dictionaries(&mut settings);

        assert_eq!(settings.dictionaries, vec!["en_gb", "custom"]);
    }
}
