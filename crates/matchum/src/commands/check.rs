use crate::commands::cspell::config_search;
use crate::commands::cspell::defaults;
use crate::commands::cspell::patterns;
use crate::diff::DiffFilter;
use anyhow::Context as _;
use anyhow::Result;
use ignore::{WalkBuilder, WalkState};
use matchum_config::glob_match::{
    global_match_path, is_global_pattern, normalized_match_path, resolve_match_root,
    root_relative_match_path,
};
use matchum_config::overrides;
use matchum_config::resolver;
use matchum_config::settings::{
    CSpellSettings, GlobDef, GlobPatternSet, LanguageSetting, OverrideSettings, PatternDefinition,
    StringOrList,
};
use matchum_core::issue::ValidationIssue;
use matchum_core::validator::{
    CompoundWordsMode, CustomIgnorePatternMask, Validator, ValidatorConfig,
};
use matchum_dict::dictionary::Dictionary;
use matchum_dict::hashdict::HashDictionary;
use md5::{Digest, Md5};
use rayon::prelude::*;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::io::{self, Read};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// Pre-built dictionary catalog passed to the engine by callers.
/// Callers (matchum native, cspell compat) build this differently.
pub struct DictionaryCatalog {
    pub named_dicts: Vec<(String, Arc<dyn Dictionary>)>,
    pub extra_active: HashSet<String>,
    pub lang_settings: Vec<LanguageSetting>,
    pub overrides: Vec<OverrideSettings>,
}

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
    /// Override the validator's compound mode.
    /// Native mode leaves this unset; cspell-compat mode uses legacy same-dict compounds.
    pub compound_words_mode: Option<CompoundWordsMode>,
    /// When true, use cspell's validation pipeline ordering instead of the native fast path.
    pub cspell_compat_mode: bool,
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
    /// Only check files of the given type (e.g., "rust", "python").
    pub file_type: Option<String>,
    /// Show execution statistics.
    pub stats: bool,
    /// When set, only report issues on lines present in this diff filter.
    pub diff_filter: Option<Arc<DiffFilter>>,
    /// Config file path to auto-exclude from checking.
    pub config_file: Option<PathBuf>,
    /// When true, filter out linguist-generated/linguist-vendored files via .gitattributes.
    pub use_gitattributes: bool,
    /// When true, search for per-directory cspell configs that add local words.
    pub per_dir_config_search: bool,
    /// Working directory for path resolution.
    /// Not exposed as a CLI flag — used for programmatic and test overrides.
    /// `None` falls back to `std::env::current_dir()`.
    pub cwd: Option<PathBuf>,
}

impl CheckOptions {
    /// Resolved working directory. Falls back to `std::env::current_dir()`.
    /// The returned path is canonicalized to match `std::env::current_dir()` behavior
    /// (resolves symlinks, e.g. `/var` -> `/private/var` on macOS).
    pub fn cwd(&self) -> PathBuf {
        let raw = match &self.cwd {
            Some(path) => path.clone(),
            None => std::env::current_dir().unwrap_or_else(|e| {
                eprintln!("warning: failed to get current directory: {e}");
                PathBuf::new()
            }),
        };
        raw.canonicalize().unwrap_or(raw)
    }
}

const DEFAULT_DICTIONARIES: &[&str] = &[
    "en_us",
    "softwareTerms",
    "companies",
    "public-licenses",
    "filetypes",
];

/// Ensure default dictionaries are always present.
/// cspell merges default dictionaries with user-specified ones (never replaces).
/// A user can exclude a default dict with `"!dictName"` syntax in `dictionaries`.
pub fn apply_default_dictionaries(settings: &mut CSpellSettings) {
    // Collect negated names (e.g. "!companies" means "don't use companies")
    let negated: HashSet<String> = settings
        .dictionaries
        .iter()
        .filter(|d| d.starts_with('!'))
        .map(|d| d[1..].to_lowercase())
        .collect();

    for &name in DEFAULT_DICTIONARIES {
        if negated.contains(&name.to_lowercase()) {
            continue;
        }
        let name_lower = name.to_lowercase();
        let already = settings
            .dictionaries
            .iter()
            .any(|d| d.to_lowercase() == name_lower);
        if !already {
            settings.dictionaries.push(name.into());
        }
    }

    // Remove negation entries (cspell strips them after processing)
    settings.dictionaries.retain(|d| !d.starts_with('!'));
}

/// Ensure cspell predefined pattern definitions are always available.
/// These provide named pattern resolution for config tokens like `Base64` or `Email`.
pub fn apply_default_patterns(settings: &mut CSpellSettings) {
    for pattern in defaults::predefined_pattern_definitions() {
        let exists = settings
            .patterns
            .iter()
            .any(|existing| existing.name.eq_ignore_ascii_case(&pattern.name));
        if !exists {
            settings.patterns.push(pattern);
        }
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

#[derive(Clone)]
struct ConfigContext {
    settings: Arc<CSpellSettings>,
    compiled_overrides: Arc<Vec<overrides::CompiledOverride>>,
    base_validator_config: Arc<ValidatorConfig>,
    local_ignore_filter: Option<Arc<RootedIgnoreFilter>>,
    cache_key: String,
}

struct ValidatorTemplate {
    requested: Option<HashSet<String>>,
    inline_dict: Option<Arc<dyn Dictionary>>,
    validator_config: Arc<ValidatorConfig>,
}

fn merge_bundled_settings(
    settings: &CSpellSettings,
    bundled_lang_settings: &[LanguageSetting],
    bundled_overrides: &[OverrideSettings],
) -> CSpellSettings {
    let mut bundled = defaults::bundled_root_settings();
    bundled.language_settings = bundled_lang_settings.to_vec();
    bundled.overrides = bundled_overrides.to_vec();
    resolver::merge_settings(bundled, settings.clone())
}

fn build_config_context(
    settings: CSpellSettings,
    cache_key: String,
    options: &CheckOptions,
    show_suggestions: Option<bool>,
    local_ignore_filter: Option<Arc<RootedIgnoreFilter>>,
) -> Arc<ConfigContext> {
    let compiled_overrides = overrides::compile_overrides(&settings);
    let base_validator_config = build_validator_config(
        &settings,
        options.allow_compound_words,
        options.compound_words_mode,
        options.cspell_compat_mode,
        should_compute_suggestions(show_suggestions),
        None,
    );

    Arc::new(ConfigContext {
        settings: Arc::new(settings),
        compiled_overrides: Arc::new(compiled_overrides),
        base_validator_config: Arc::new(base_validator_config),
        local_ignore_filter,
        cache_key,
    })
}

fn build_root_config_context(
    settings: &CSpellSettings,
    config_dir: Option<&Path>,
    options: &CheckOptions,
    show_suggestions: Option<bool>,
) -> Arc<ConfigContext> {
    let base_dir = config_dir
        .map(|dir| dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()))
        .unwrap_or_else(|| options.cwd());

    build_config_context(
        settings.clone(),
        base_dir.to_string_lossy().into_owned(),
        options,
        show_suggestions,
        None,
    )
}

fn build_per_dir_config_contexts(
    files: &[PathBuf],
    root_config_file: Option<&Path>,
    base_settings: &CSpellSettings,
    options: &CheckOptions,
    show_suggestions: Option<bool>,
) -> HashMap<PathBuf, Arc<ConfigContext>> {
    let search_dirs = config_search::collect_search_dirs(files);
    if search_dirs.is_empty() {
        return HashMap::new();
    }

    let root_config_canonical =
        root_config_file.map(|path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf()));

    let mut config_cache: HashMap<PathBuf, Option<PathBuf>> = HashMap::new();
    let mut loaded_contexts: HashMap<PathBuf, Arc<ConfigContext>> = HashMap::new();
    let mut result: HashMap<PathBuf, Arc<ConfigContext>> = HashMap::new();

    for dir in search_dirs {
        let Some(config_path) = config_search::find_nearest_config(
            &dir,
            &options.stop_config_search_at,
            &mut config_cache,
        ) else {
            continue;
        };
        let config_canonical = config_path
            .canonicalize()
            .unwrap_or_else(|_| config_path.clone());
        if root_config_canonical
            .as_ref()
            .is_some_and(|root| *root == config_canonical)
        {
            continue;
        }

        let context = if let Some(existing) = loaded_contexts.get(&config_path) {
            existing.clone()
        } else {
            let Ok(local_settings) = resolver::load_config(&config_path) else {
                continue;
            };
            let local_ignore_filter = if local_settings.resolved_ignore_paths.is_empty() {
                None
            } else {
                build_rooted_ignore_filter(&local_settings.resolved_ignore_paths).map(Arc::new)
            };
            // cspell first resolves the base/global settings, then overlays the
            // nearest document config on top of them.
            let merged_settings = resolver::merge_settings(base_settings.clone(), local_settings);
            let cache_key = config_path
                .canonicalize()
                .unwrap_or_else(|_| config_path.clone())
                .to_string_lossy()
                .into_owned();
            let context = build_config_context(
                merged_settings,
                cache_key,
                options,
                show_suggestions,
                local_ignore_filter,
            );
            loaded_contexts.insert(config_path.clone(), context.clone());
            context
        };

        result.insert(dir, context);
    }

    result
}

/// Run check with pre-resolved settings and dictionary catalog.
/// This is the main public entry point — callers resolve config and build
/// the catalog themselves before calling this.
#[allow(clippy::too_many_arguments)]
pub fn run_check(
    paths: &[PathBuf],
    settings: &CSpellSettings,
    config_dir: Option<&Path>,
    catalog: DictionaryCatalog,
    format: &str,
    show_suggestions: Option<bool>,
    unique: bool,
    strict: bool,
    options: CheckOptions,
) -> Result<CheckResult> {
    // Handle --root: resolve paths relative to root
    let effective_paths = if let Some(ref root) = options.root {
        paths
            .iter()
            .map(|p| {
                if p.is_absolute() {
                    p.clone()
                } else {
                    root.join(p)
                }
            })
            .collect()
    } else {
        paths.to_vec()
    };

    run_check_inner(
        &effective_paths,
        settings,
        config_dir,
        catalog,
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
    settings: &CSpellSettings,
    config_dir: Option<&Path>,
    catalog: DictionaryCatalog,
    options: CheckOptions,
) -> Result<Vec<(PathBuf, String, Vec<ValidationIssue>)>> {
    run_collect_issues(paths, settings, config_dir, catalog, &options)
}

/// Core issue collection pipeline: collect files, validate in parallel.
fn run_collect_issues(
    effective_paths: &[PathBuf],
    settings: &CSpellSettings,
    config_dir: Option<&Path>,
    catalog: DictionaryCatalog,
    options: &CheckOptions,
) -> Result<Vec<(PathBuf, String, Vec<ValidationIssue>)>> {
    let DictionaryCatalog {
        named_dicts,
        extra_active,
        lang_settings: bundled_lang_settings,
        overrides: bundled_overrides,
    } = catalog;
    let merged_settings = if options.cspell_compat_mode {
        merge_bundled_settings(settings, &bundled_lang_settings, &bundled_overrides)
    } else {
        settings.clone()
    };
    let settings = &merged_settings;
    let mut files = collect_files(effective_paths, settings, options)?;

    if let Some(ref df) = options.diff_filter {
        files.retain(|f| df.contains_file(f));
    }

    let language_id = options.language_id.clone();
    let root_context = build_root_config_context(settings, config_dir, options, Some(false));
    let per_dir_contexts = if options.per_dir_config_search && options.config_search {
        build_per_dir_config_contexts(
            &files,
            options.config_file.as_deref(),
            settings,
            options,
            Some(false),
        )
    } else {
        HashMap::new()
    };

    let word_caches: std::sync::Mutex<
        std::collections::HashMap<String, matchum_core::validator::WordCache>,
    > = std::sync::Mutex::new(std::collections::HashMap::new());
    let validator_templates: std::sync::Mutex<
        std::collections::HashMap<String, Arc<ValidatorTemplate>>,
    > = std::sync::Mutex::new(std::collections::HashMap::new());

    let results: Vec<(PathBuf, String, Vec<ValidationIssue>)> = files
        .par_iter()
        .filter_map(|file| {
            let context = file
                .parent()
                .and_then(|dir| config_search::find_nearest_dir_value(dir, &per_dir_contexts))
                .cloned()
                .unwrap_or_else(|| root_context.clone());
            if context
                .local_ignore_filter
                .as_ref()
                .is_some_and(|filter| filter.is_ignored(file))
            {
                return None;
            }
            let context_settings = context.settings.as_ref();
            let overridden = if context.compiled_overrides.is_empty() {
                None
            } else {
                overrides::apply_compiled_overrides(
                    context_settings,
                    file,
                    context.compiled_overrides.as_ref(),
                )
            };
            let needs_lang = language_id.is_some();
            let effective_owned = if overridden.is_some() || needs_lang {
                let mut s = overridden.unwrap_or_else(|| context_settings.clone());
                if let Some(ref lang) = language_id {
                    s.language_id = Some(lang.clone());
                }
                Some(s)
            } else {
                None
            };
            let effective_settings = effective_owned.as_ref().unwrap_or(context_settings);
            if resolve_language_setting_scalars(effective_settings, Some(file)).enabled
                == Some(false)
            {
                return None;
            }

            let content = match read_file_mmap_classified(file)? {
                ReadFileMmap::Text(content) => content,
                ReadFileMmap::Binary => return None,
            };

            let lang_ids = effective_owned
                .is_none()
                .then(|| active_language_ids(effective_settings, Some(file)));
            let mut validator = if let Some(lang_ids) = lang_ids.as_ref() {
                let template_id = format!("{}::{}", context.cache_key, lang_ids.join(","));
                let template = {
                    let mut map = validator_templates.lock().unwrap();
                    map.entry(template_id)
                        .or_insert_with(|| {
                            Arc::new(prepare_validator_template(
                                effective_settings,
                                options,
                                &extra_active,
                                false,
                                Some(file),
                                Some(context.base_validator_config.as_ref()),
                            ))
                        })
                        .clone()
                };
                instantiate_validator(template.as_ref(), &named_dicts, options)
            } else {
                build_validator(
                    effective_settings,
                    &named_dicts,
                    options,
                    &extra_active,
                    false,
                    Some(file),
                    None,
                )
            };
            if should_use_shared_word_cache(options, effective_owned.is_none()) {
                let cache_id = format!(
                    "{}::{}",
                    context.cache_key,
                    lang_ids.as_ref().unwrap().join(",")
                );
                let cache = {
                    let mut map = word_caches.lock().unwrap();
                    map.entry(cache_id)
                        .or_insert_with(Validator::new_word_cache)
                        .clone()
                };
                validator.set_word_cache(cache);
            }
            let issues = validator.validate_text(&content);
            let issues = apply_issue_limits(issues, settings.max_number_of_problems);

            if issues.is_empty() {
                None
            } else {
                Some((file.clone(), content, issues))
            }
        })
        .collect();

    Ok(results)
}

#[allow(clippy::too_many_arguments)]
fn run_check_inner(
    effective_paths: &[PathBuf],
    settings: &CSpellSettings,
    config_dir: Option<&Path>,
    catalog: DictionaryCatalog,
    format: &str,
    show_suggestions: Option<bool>,
    unique: bool,
    strict: bool,
    options: CheckOptions,
) -> Result<CheckResult> {
    let wall_start = std::time::Instant::now();

    let DictionaryCatalog {
        named_dicts,
        extra_active,
        lang_settings: bundled_lang_settings,
        overrides: bundled_overrides,
    } = catalog;
    let merged_settings = if options.cspell_compat_mode {
        merge_bundled_settings(settings, &bundled_lang_settings, &bundled_overrides)
    } else {
        settings.clone()
    };
    let settings = &merged_settings;
    let dict_count = named_dicts.len();
    let mut files = collect_files(effective_paths, settings, &options)?;

    // Restrict to files present in the diff
    if let Some(ref df) = options.diff_filter {
        files.retain(|f| df.contains_file(f));
    }

    if files.is_empty() && !options.no_must_find_files && !options.quiet && !options.silent {
        eprintln!("No files found to check.");
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
    let cache_strategy = options.cache_strategy.as_deref().unwrap_or("metadata");
    let cache = std::sync::Mutex::new(cache);

    let fail_fast = options.fail_fast;
    let fail_fast_flag = std::sync::atomic::AtomicBool::new(false);
    let need_context = options.show_context;
    let verbose = options.verbose;
    let silent = options.silent;
    let quiet = options.quiet;
    let validate_directives = options.validate_directives;
    let language_id = options.language_id.clone();
    let root_context = build_root_config_context(settings, config_dir, &options, show_suggestions);
    let per_dir_contexts = if options.per_dir_config_search && options.config_search {
        build_per_dir_config_contexts(
            &files,
            options.config_file.as_deref(),
            settings,
            &options,
            show_suggestions,
        )
    } else {
        HashMap::new()
    };

    let word_caches: std::sync::Mutex<
        std::collections::HashMap<String, matchum_core::validator::WordCache>,
    > = std::sync::Mutex::new(std::collections::HashMap::new());
    let validator_templates: std::sync::Mutex<
        std::collections::HashMap<String, Arc<ValidatorTemplate>>,
    > = std::sync::Mutex::new(std::collections::HashMap::new());

    let show_progress =
        options.cspell_compat_mode && !options.no_progress && !options.quiet && !options.silent;
    let progress_total = files.len();
    let progress_counter = std::sync::atomic::AtomicUsize::new(0);
    let progress_cwd = if show_progress {
        Some(options.cwd())
    } else {
        None
    };

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

            // Cache check: only clean (no-issue) files are cached.
            // A cache hit means the file is unchanged and had no issues
            // last time, so we can skip it entirely.
            if use_cache
                && let Ok(guard) = cache.lock()
                && guard.check_clean(file, cache_strategy).is_some()
            {
                if verbose > 0 && !silent {
                    eprintln!("Cache hit (clean): {}", file.display());
                }
                return None;
            }

            if verbose > 0 && !silent {
                eprintln!("Checking: {}", file.display());
            }

            let file_start = if show_progress {
                Some(std::time::Instant::now())
            } else {
                None
            };

            let context = file
                .parent()
                .and_then(|dir| config_search::find_nearest_dir_value(dir, &per_dir_contexts))
                .cloned()
                .unwrap_or_else(|| root_context.clone());
            if context
                .local_ignore_filter
                .as_ref()
                .is_some_and(|filter| filter.is_ignored(file))
            {
                return None;
            }
            let context_settings = context.settings.as_ref();
            let overridden = if context.compiled_overrides.is_empty() {
                None
            } else {
                overrides::apply_compiled_overrides(
                    context_settings,
                    file,
                    context.compiled_overrides.as_ref(),
                )
            };
            let needs_lang = language_id.is_some();
            let effective_owned = if overridden.is_some() || needs_lang {
                let mut s = overridden.unwrap_or_else(|| context_settings.clone());
                if let Some(ref lang) = language_id {
                    s.language_id = Some(lang.clone());
                }
                Some(s)
            } else {
                None
            };
            let effective_settings = effective_owned.as_ref().unwrap_or(context_settings);
            if resolve_language_setting_scalars(effective_settings, Some(file)).enabled
                == Some(false)
            {
                return None;
            }

            let content = match read_file_mmap_classified(file) {
                Some(ReadFileMmap::Text(content)) => content,
                Some(ReadFileMmap::Binary) => return None,
                None => {
                    if !silent {
                        eprintln!("Warning: cannot read {}", file.display());
                    }
                    return None;
                }
            };

            let lang_ids = effective_owned
                .is_none()
                .then(|| active_language_ids(effective_settings, Some(file)));
            let mut validator = if let Some(lang_ids) = lang_ids.as_ref() {
                let template_id = format!("{}::{}", context.cache_key, lang_ids.join(","));
                let template = {
                    let mut map = validator_templates.lock().unwrap();
                    map.entry(template_id)
                        .or_insert_with(|| {
                            Arc::new(prepare_validator_template(
                                effective_settings,
                                &options,
                                &extra_active,
                                should_compute_suggestions(show_suggestions),
                                Some(file),
                                Some(context.base_validator_config.as_ref()),
                            ))
                        })
                        .clone()
                };
                instantiate_validator(template.as_ref(), &named_dicts, &options)
            } else {
                build_validator(
                    effective_settings,
                    &named_dicts,
                    &options,
                    &extra_active,
                    should_compute_suggestions(show_suggestions),
                    Some(file),
                    None,
                )
            };

            if should_use_shared_word_cache(&options, effective_owned.is_none()) {
                let cache_id = format!(
                    "{}::{}",
                    context.cache_key,
                    lang_ids.as_ref().unwrap().join(",")
                );
                let cache = {
                    let mut map = word_caches.lock().unwrap();
                    map.entry(cache_id)
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

            // Apply per-file total issue limit (cspell's maxNumberOfProblems, default 10000)
            issues = apply_issue_limits(issues, effective_settings.max_number_of_problems);

            if let Some(start) = file_start {
                let idx = progress_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                let elapsed = start.elapsed();
                let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
                let rel_path = progress_cwd
                    .as_ref()
                    .and_then(|cwd| file.strip_prefix(cwd).ok())
                    .unwrap_or(file);
                eprintln!("{idx}/{progress_total} {path} {elapsed_ms:.2}ms", path = rel_path.display());
            }

            if issues.is_empty() {
                if use_cache && let Ok(mut guard) = cache.lock() {
                    guard.update(file, &[], cache_strategy, Some(content.as_bytes()));
                }
                None
            } else {
                if fail_fast {
                    fail_fast_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                if use_cache && let Ok(mut guard) = cache.lock() {
                    guard.update(file, &issues, cache_strategy, Some(content.as_bytes()));
                }
                let kept_content = if need_context { content } else { String::new() };
                Some((file.clone(), kept_content, issues))
            }
        })
        .collect();

    // Save cache
    if use_cache && let Ok(guard) = cache.lock() {
        guard.save(&cache_path(&options));
    }

    let mut total_issues = 0;
    let mut unique_words = HashSet::new();
    let show_issues = !options.no_issues && format != "json";
    let base_dir = if options.no_relative {
        None
    } else {
        // Use the first absolute directory from input paths as the base for
        // relative display (cargo-matchum passes an absolute workspace root).
        // Falls back to current_dir() for the relative-path case.
        effective_paths
            .iter()
            .find(|p| p.is_dir() && p.is_absolute())
            .cloned()
            .or_else(|| Some(options.cwd()))
    };

    for (file, content, issues) in &results {
        // Pre-compute line start offsets for O(1) context line lookup.
        // Without this, `.lines().nth(N)` is O(N) per issue, which is
        // catastrophic for large files (298K-line multi.c × 9907 issues).
        let line_starts: Vec<usize> = if need_context && !content.is_empty() {
            let mut starts = vec![0usize];
            for (i, b) in content.as_bytes().iter().enumerate() {
                if *b == b'\n' {
                    starts.push(i + 1);
                }
            }
            starts
        } else {
            Vec::new()
        };

        for issue in issues {
            // Skip issues not on added lines when diff filtering
            if let Some(ref df) = options.diff_filter
                && !df.should_report(file, issue.line)
            {
                continue;
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
                    let ctx_line = if need_context && !line_starts.is_empty() {
                        let idx = issue.line.saturating_sub(1);
                        line_starts.get(idx).map(|&start| {
                            let end = line_starts
                                .get(idx + 1)
                                .map(|&e| if e > 0 { e - 1 } else { e })
                                .unwrap_or(content.len());
                            &content[start..end.min(content.len())]
                        })
                    } else {
                        None
                    };
                    print_issue_text_fast(
                        &display_path,
                        issue,
                        show_suggestions,
                        need_context,
                        ctx_line,
                    );
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
        if options.cspell_compat_mode {
            eprintln!(
                "CSpell: Files checked: {}, Issues found: {} in {} file{}.",
                files_checked,
                total_issues,
                files_with_issues,
                if files_with_issues == 1 { "" } else { "s" },
            );
        } else if total_issues > 0 {
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

/// Apply cspell's per-file issue limits:
/// - `maxNumberOfProblems` (default 10000): cap total issues per file
///
/// Note: `maxDuplicateProblems` is already applied inside `Validator::validate_text`
/// with case-sensitive counting (matching cspell behavior).
const DEFAULT_MAX_NUMBER_OF_PROBLEMS: usize = 10_000;

fn apply_issue_limits(
    issues: Vec<ValidationIssue>,
    max_total: Option<usize>,
) -> Vec<ValidationIssue> {
    let total_limit = max_total.unwrap_or(DEFAULT_MAX_NUMBER_OF_PROBLEMS);
    if issues.len() <= total_limit {
        return issues;
    }
    issues.into_iter().take(total_limit).collect()
}

fn normalize_language_id_list(language_id: &str) -> Vec<String> {
    language_id
        .replace(['|', ';'], ",")
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Match a single `languageId` against one language setting, mirroring
/// cspell's `doesLanguageSettingMatchLanguageId`.
fn language_id_matches(patterns: &[String], lang_id: &str) -> bool {
    if patterns.is_empty() {
        return true;
    }

    if patterns.iter().any(|p| p.eq_ignore_ascii_case(lang_id)) {
        return true;
    }

    if patterns.iter().any(|p| {
        p.strip_prefix('!')
            .is_some_and(|excluded| excluded.eq_ignore_ascii_case(lang_id))
    }) {
        return false;
    }

    patterns.iter().all(|p| p.starts_with('!'))
}

fn active_language_ids(settings: &CSpellSettings, file: Option<&Path>) -> Vec<String> {
    let explicit = settings
        .language_id
        .as_deref()
        .map(normalize_language_id_list);
    let detected = file.map(defaults::language_ids_from_path);

    let mut ids = Vec::new();
    let mut seen = HashSet::new();

    ids.push("*".to_string());
    seen.insert("*".to_string());

    for id in explicit.or(detected).unwrap_or_default() {
        if seen.insert(id.clone()) {
            ids.push(id);
        }
    }

    ids
}

#[derive(Debug, Clone, Copy, Default)]
struct LanguageSettingScalars {
    enabled: Option<bool>,
    case_sensitive: Option<bool>,
    allow_compound_words: Option<bool>,
}

struct ResolvedLanguageSettings<'a> {
    scalars: LanguageSettingScalars,
    dictionaries: Vec<&'a str>,
    words: Vec<&'a str>,
    ignore_words: Vec<&'a str>,
    flag_words: Vec<&'a str>,
    ignore_reg_exp_list: Vec<&'a str>,
    pattern_defs: HashMap<String, &'a PatternDefinition>,
}

fn language_setting_matches(patterns: &[String], active_language_ids: &[String]) -> bool {
    if patterns.is_empty() {
        return true;
    }

    active_language_ids
        .iter()
        .any(|lang_id| language_id_matches(patterns, lang_id))
}

fn resolve_language_setting_scalars(
    settings: &CSpellSettings,
    file: Option<&Path>,
) -> LanguageSettingScalars {
    resolve_language_settings(settings, file).scalars
}

fn resolve_language_settings<'a>(
    settings: &'a CSpellSettings,
    file: Option<&Path>,
) -> ResolvedLanguageSettings<'a> {
    let mut resolved = ResolvedLanguageSettings {
        scalars: LanguageSettingScalars {
            enabled: settings.enabled,
            case_sensitive: settings.case_sensitive,
            allow_compound_words: settings.allow_compound_words,
        },
        dictionaries: Vec::new(),
        words: Vec::new(),
        ignore_words: Vec::new(),
        flag_words: Vec::new(),
        ignore_reg_exp_list: Vec::new(),
        pattern_defs: settings
            .patterns
            .iter()
            .map(|p| (p.name.to_lowercase(), p))
            .collect(),
    };

    let Some(active_language_ids) =
        file.map(|file_path| active_language_ids(settings, Some(file_path)))
    else {
        return resolved;
    };

    let active_locale = settings.language.as_deref().unwrap_or("en");
    for ls in &settings.language_settings {
        if !language_setting_matches(&ls.language_id, &active_language_ids)
            || !locale_matches(ls.locale.as_deref(), active_locale)
        {
            continue;
        }

        if let Some(enabled) = ls.enabled {
            resolved.scalars.enabled = Some(enabled);
        }
        if let Some(case_sensitive) = ls.case_sensitive {
            resolved.scalars.case_sensitive = Some(case_sensitive);
        }
        if let Some(allow_compound_words) = ls.allow_compound_words {
            resolved.scalars.allow_compound_words = Some(allow_compound_words);
        }

        resolved
            .dictionaries
            .extend(ls.dictionaries.iter().map(String::as_str));
        resolved.words.extend(ls.words.iter().map(String::as_str));
        resolved
            .ignore_words
            .extend(ls.ignore_words.iter().map(String::as_str));
        resolved
            .flag_words
            .extend(ls.flag_words.iter().map(String::as_str));
        resolved
            .ignore_reg_exp_list
            .extend(ls.ignore_reg_exp_list.iter().map(String::as_str));
        for pattern in &ls.patterns {
            resolved
                .pattern_defs
                .insert(pattern.name.to_lowercase(), pattern);
        }
    }

    resolved
}

/// Check if a languageSetting's locale filter matches the active locale.
/// If the locale field is None or "*", it matches everything.
/// Both `ls_locale` and `active_locale` may be comma-separated lists;
/// a match occurs if any part of `ls_locale` matches any part of `active_locale`.
fn locale_matches(ls_locale: Option<&str>, active_locale: &str) -> bool {
    match ls_locale {
        None => true,
        Some(loc) => {
            let active_parts: Vec<&str> = active_locale.split(',').map(|s| s.trim()).collect();
            loc.split(',').any(|part| {
                let part = part.trim();
                part == "*" || active_parts.iter().any(|ap| part.eq_ignore_ascii_case(ap))
            })
        }
    }
}

#[inline]
fn should_compute_suggestions(show_suggestions: Option<bool>) -> bool {
    show_suggestions == Some(true)
}

fn should_use_shared_word_cache(_options: &CheckOptions, using_precompiled_settings: bool) -> bool {
    using_precompiled_settings
}
pub fn build_validator(
    settings: &CSpellSettings,
    named_dicts: &[(String, Arc<dyn Dictionary>)],
    options: &CheckOptions,
    extra_active: &HashSet<String>,
    compute_suggestions: bool,
    file: Option<&Path>,
    precompiled_config: Option<&ValidatorConfig>,
) -> Validator {
    let template = prepare_validator_template(
        settings,
        options,
        extra_active,
        compute_suggestions,
        file,
        precompiled_config,
    );
    instantiate_validator(&template, named_dicts, options)
}

fn prepare_validator_template(
    settings: &CSpellSettings,
    options: &CheckOptions,
    extra_active: &HashSet<String>,
    compute_suggestions: bool,
    file: Option<&Path>,
    precompiled_config: Option<&ValidatorConfig>,
) -> ValidatorTemplate {
    let resolved_lang_settings = resolve_language_settings(settings, file);
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
            for dict in &resolved_lang_settings.dictionaries {
                set.insert(dict.to_lowercase());
            }
            Some(set)
        };

    // Collect language-specific words/ignore_words/flag_words
    let lang_words = resolved_lang_settings.words;
    let lang_ignore_words = resolved_lang_settings.ignore_words;
    let lang_flag_words = resolved_lang_settings.flag_words;

    let inline_dict = if !settings.words.is_empty()
        || !settings.user_words.is_empty()
        || !lang_words.is_empty()
    {
        let mut inline_dict = HashDictionary::new(false);
        for word in &settings.words {
            inline_dict.add_word(word);
        }
        for word in &settings.user_words {
            inline_dict.add_word(word);
        }
        for word in &lang_words {
            inline_dict.add_word(word);
        }
        Some(Arc::new(inline_dict) as Arc<dyn Dictionary>)
    } else {
        None
    };

    let mut validator_config = match precompiled_config {
        Some(config) => config.clone(),
        None => build_validator_config(
            settings,
            options.allow_compound_words,
            options.compound_words_mode,
            options.cspell_compat_mode,
            compute_suggestions,
            file,
        ),
    };

    // Apply language-specific ignore_words and flag_words
    for w in &lang_ignore_words {
        validator_config
            .ignore_words
            .insert(compact_str::CompactString::from(w.to_lowercase()));
    }
    for w in &lang_flag_words {
        validator_config
            .flag_words
            .insert(compact_str::CompactString::from(w.to_lowercase()));
    }

    // Even with precompiled config, apply language-specific ignore patterns
    if precompiled_config.is_some() {
        for pattern in &resolved_lang_settings.ignore_reg_exp_list {
            resolve_pattern_token(
                pattern,
                &resolved_lang_settings.pattern_defs,
                &mut HashSet::new(),
                &mut validator_config.ignore_patterns,
                &mut validator_config.ignore_patterns_fancy,
                &mut validator_config.custom_ignore_patterns,
            );
        }
    }

    ValidatorTemplate {
        requested,
        inline_dict,
        validator_config: Arc::new(validator_config),
    }
}

fn instantiate_validator(
    template: &ValidatorTemplate,
    named_dicts: &[(String, Arc<dyn Dictionary>)],
    options: &CheckOptions,
) -> Validator {
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
        let mut active = template
            .requested
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

    if let Some(inline_dict) = &template.inline_dict {
        entries.push(("__inline_words".into(), Arc::clone(inline_dict), true));
    }

    Validator::new_named(entries, (*template.validator_config).clone())
}

pub fn build_validator_config(
    settings: &CSpellSettings,
    cli_allow_compound_words: Option<bool>,
    compound_words_mode: Option<CompoundWordsMode>,
    cspell_compat_mode: bool,
    compute_suggestions: bool,
    file: Option<&Path>,
) -> ValidatorConfig {
    let (
        mut ignore_patterns,
        mut ignore_patterns_fancy,
        include_patterns,
        include_patterns_fancy,
        mut custom_ignore_patterns,
    ) = resolve_patterns(settings);
    let resolved_lang_settings = resolve_language_settings(settings, file);
    let scalar_overrides = resolved_lang_settings.scalars;
    for pattern in &resolved_lang_settings.ignore_reg_exp_list {
        resolve_pattern_token(
            pattern,
            &resolved_lang_settings.pattern_defs,
            &mut HashSet::new(),
            &mut ignore_patterns,
            &mut ignore_patterns_fancy,
            &mut custom_ignore_patterns,
        );
    }

    let mut flag_words: HashSet<compact_str::CompactString> = settings
        .flag_words
        .iter()
        .map(|w| compact_str::CompactString::from(w.to_lowercase()))
        .collect();
    let mut ignore_words: HashSet<compact_str::CompactString> = settings
        .ignore_words
        .iter()
        .map(|w| compact_str::CompactString::from(w.to_lowercase()))
        .collect();

    for word in &resolved_lang_settings.ignore_words {
        ignore_words.insert(compact_str::CompactString::from(word.to_lowercase()));
    }
    for word in &resolved_lang_settings.flag_words {
        flag_words.insert(compact_str::CompactString::from(word.to_lowercase()));
    }

    ValidatorConfig {
        min_word_length: settings.min_word_length.unwrap_or(4),
        case_sensitive: scalar_overrides.case_sensitive.unwrap_or(false),
        ignore_patterns,
        ignore_patterns_fancy,
        include_patterns,
        include_patterns_fancy,
        custom_ignore_patterns,
        flag_words,
        ignore_words,
        allow_compound_words: cli_allow_compound_words
            .unwrap_or(scalar_overrides.allow_compound_words.unwrap_or(false)),
        compound_words_mode: compound_words_mode.unwrap_or(CompoundWordsMode::None),
        cspell_compat_mode,
        ignore_random_strings: settings.ignore_random_strings.unwrap_or(cspell_compat_mode),
        min_random_length: settings.min_random_length.unwrap_or(40),
        compute_suggestions,
        max_duplicate_problems: settings.max_duplicate_problems.unwrap_or(5),
    }
}

fn resolve_patterns(
    settings: &CSpellSettings,
) -> (
    Vec<regex::Regex>,
    Vec<fancy_regex::Regex>,
    Vec<regex::Regex>,
    Vec<fancy_regex::Regex>,
    CustomIgnorePatternMask,
) {
    let defs: HashMap<String, &PatternDefinition> = settings
        .patterns
        .iter()
        .map(|p| (p.name.to_lowercase(), p))
        .collect();

    let mut ignore = Vec::new();
    let mut ignore_fancy = Vec::new();
    let mut custom_ignore_patterns = CustomIgnorePatternMask::default();
    for p in &settings.ignore_reg_exp_list {
        resolve_pattern_token(
            p,
            &defs,
            &mut HashSet::new(),
            &mut ignore,
            &mut ignore_fancy,
            &mut custom_ignore_patterns,
        );
    }

    let mut include = Vec::new();
    let mut include_fancy = Vec::new();
    let mut include_custom_patterns = CustomIgnorePatternMask::default();
    for p in &settings.include_reg_exp_list {
        resolve_pattern_token(
            p,
            &defs,
            &mut HashSet::new(),
            &mut include,
            &mut include_fancy,
            &mut include_custom_patterns,
        );
    }

    (
        ignore,
        ignore_fancy,
        include,
        include_fancy,
        custom_ignore_patterns,
    )
}

fn resolve_pattern_token(
    token: &str,
    defs: &HashMap<String, &PatternDefinition>,
    visiting: &mut HashSet<String>,
    out: &mut Vec<regex::Regex>,
    fancy_out: &mut Vec<fancy_regex::Regex>,
    custom_out: &mut CustomIgnorePatternMask,
) {
    let key = token.trim().to_lowercase();
    if let Some(def) = defs.get(&key) {
        if !visiting.insert(key.clone()) {
            return;
        }
        match &def.pattern {
            StringOrList::Single(s) => {
                resolve_pattern_token(s, defs, visiting, out, fancy_out, custom_out)
            }
            StringOrList::List(list) => {
                for s in list {
                    resolve_pattern_token(s, defs, visiting, out, fancy_out, custom_out);
                }
            }
        }
        visiting.remove(&key);
        return;
    }

    if let Some(re) = patterns::parse_cspell_regex_pattern(token) {
        match re {
            patterns::CompiledRegex::Rust(re) => out.push(re),
            patterns::CompiledRegex::Fancy(re) => fancy_out.push(re),
        }
        return;
    }

    custom_out.extend(patterns::classify_custom_ignore_pattern(token));
}

fn is_glob_pattern(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[') || s.contains('{')
}

fn looks_like_utf16_prefix(bytes: &[u8]) -> bool {
    bytes.len() >= 2
        && matches!(
            [bytes[0], bytes[1]],
            [0xFE, 0xFF] | [0xFF, 0xFE] | [0, 1..=u8::MAX] | [1..=u8::MAX, 0]
        )
}

fn is_unknown_binary_content(bytes: &[u8]) -> bool {
    let prefix = &bytes[..bytes.len().min(1024)];
    prefix.contains(&0) && !looks_like_utf16_prefix(prefix)
}

fn is_generated_file(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();

    matches!(
        filename.as_str(),
        "package-lock.json"
            | "cargo.lock"
            | "berksfile.lock"
            | "composer.lock"
            | ".ds_store"
            | ".cspellcache"
            | ".eslintcache"
            | "id_rsa"
            | "id_rsa.pub"
    ) || matches!(
        ext.as_str(),
        "bin"
            | "cur"
            | "dll"
            | "eot"
            | "exe"
            | "gz"
            | "lib"
            | "o"
            | "obj"
            | "phar"
            | "wasm"
            | "zip"
            | "bmp"
            | "exr"
            | "gif"
            | "heic"
            | "ico"
            | "jpeg"
            | "jpg"
            | "pbm"
            | "pgm"
            | "png"
            | "ppm"
            | "ras"
            | "sgi"
            | "tiff"
            | "webp"
            | "xbm"
            | "avi"
            | "flv"
            | "mkv"
            | "mov"
            | "mp4"
            | "mpeg"
            | "mpg"
            | "wmv"
            | "ttf"
            | "woff"
            | "woff2"
            | "jar"
            | "mdb"
            | "spv"
            | "trie"
            | "webm"
            | "whl"
            | "map"
            | "lock"
            | "pdf"
            | "pem"
            | "pub"
            | "log"
    )
}

enum ReadFileMmap {
    Text(String),
    Binary,
}

struct IgnoreFilter {
    include: globset::GlobSet,
    exclude: Option<globset::GlobSet>,
}

impl IgnoreFilter {
    fn is_ignored(&self, path: &Path) -> bool {
        if !self.include.is_match(path) {
            return false;
        }
        if let Some(ref excl) = self.exclude {
            return !excl.is_match(path);
        }
        true
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct RootedGlobKey {
    root: Option<PathBuf>,
    is_global: bool,
}

struct RootedGlobMatcher {
    matcher: globset::GlobSet,
    basename_matcher: Option<globset::GlobSet>,
    root: Option<PathBuf>,
    is_global: bool,
}

struct RootedIgnoreFilter {
    include: Vec<RootedGlobMatcher>,
    exclude: Vec<RootedGlobMatcher>,
}

impl RootedIgnoreFilter {
    fn is_ignored(&self, path: &Path) -> bool {
        let mut candidates = RootedPathCandidates::new(path);
        if !self
            .include
            .iter()
            .any(|glob| matches_rooted_glob(glob, &mut candidates))
        {
            return false;
        }
        !self
            .exclude
            .iter()
            .any(|glob| matches_rooted_glob(glob, &mut candidates))
    }
}

struct RootedPathCandidates<'a> {
    file_path: &'a Path,
    global_path: Option<PathBuf>,
}

impl<'a> RootedPathCandidates<'a> {
    fn new(file_path: &'a Path) -> Self {
        Self {
            file_path,
            global_path: None,
        }
    }

    fn file_path(&self) -> &'a Path {
        self.file_path
    }

    fn global_path(&mut self) -> &Path {
        self.global_path
            .get_or_insert_with(|| global_match_path(self.file_path))
            .as_path()
    }
}

struct GitIgnoreCache {
    root: PathBuf,
    filters: HashMap<PathBuf, Option<Arc<GitIgnoreFilter>>>,
    hierarchies: HashMap<PathBuf, Arc<[Arc<GitIgnoreFilter>]>>,
    last_dir: Option<PathBuf>,
    last_hierarchy: Arc<[Arc<GitIgnoreFilter>]>,
}

impl GitIgnoreCache {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            filters: HashMap::new(),
            hierarchies: HashMap::new(),
            last_dir: None,
            last_hierarchy: Arc::from([]),
        }
    }

    fn is_ignored(&mut self, path: &Path) -> bool {
        let dir = path.parent().unwrap_or(path);
        if self.last_dir.as_deref() != Some(dir) {
            let hierarchy = self.hierarchy(dir);
            self.last_dir = Some(dir.to_path_buf());
            self.last_hierarchy = Arc::clone(&hierarchy);
        }

        self.last_hierarchy
            .iter()
            .any(|filter| filter.is_ignored(path))
    }

    fn hierarchy(&mut self, dir: &Path) -> Arc<[Arc<GitIgnoreFilter>]> {
        if let Some(cached) = self.hierarchies.get(dir) {
            return Arc::clone(cached);
        }

        let mut chain = if self.should_stop_at(dir) {
            Vec::new()
        } else if let Some(parent) = dir.parent() {
            self.hierarchy(parent).iter().cloned().collect()
        } else {
            Vec::new()
        };

        if let Some(filter) = self.filter_for(dir) {
            chain.push(filter);
        }

        let chain: Arc<[Arc<GitIgnoreFilter>]> = Arc::from(chain);
        self.hierarchies
            .insert(dir.to_path_buf(), Arc::clone(&chain));
        chain
    }

    fn should_stop_at(&self, dir: &Path) -> bool {
        if dir.starts_with(&self.root) {
            dir == self.root
        } else {
            dir.parent().is_none()
        }
    }

    fn filter_for(&mut self, dir: &Path) -> Option<Arc<GitIgnoreFilter>> {
        if let Some(cached) = self.filters.get(dir) {
            return cached.clone();
        }

        let filter = load_gitignore_filter(dir).map(Arc::new);
        self.filters.insert(dir.to_path_buf(), filter.clone());
        filter
    }
}

struct GitIgnoreFilter {
    root: PathBuf,
    filter: IgnoreFilter,
}

impl GitIgnoreFilter {
    fn is_ignored(&self, path: &Path) -> bool {
        let Ok(rel) = path.strip_prefix(&self.root) else {
            return false;
        };
        self.filter.is_ignored(rel)
    }
}

fn load_gitignore_filter(dir: &Path) -> Option<GitIgnoreFilter> {
    let gitignore = dir.join(".gitignore");
    let content = std::fs::read_to_string(&gitignore).ok()?;
    let patterns = content
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    let filter = build_ignore_filter(&patterns)?;
    Some(GitIgnoreFilter {
        root: dir.to_path_buf(),
        filter,
    })
}

/// Build an ignore filter for ignorePaths / --exclude patterns.
/// Each pattern is expanded via `normalize_pattern_nested` (cspell-compatible).
/// Supports `!(...)` extglob negation and `!` prefix re-inclusion patterns.
fn build_ignore_filter(patterns: &[String]) -> Option<IgnoreFilter> {
    let mut include_builder = globset::GlobSetBuilder::new();
    let mut exclude_builder = globset::GlobSetBuilder::new();
    let mut has_include = false;
    let mut has_exclude = false;

    for pattern in patterns {
        for ep in patterns::expand_extglobs(pattern) {
            for normalized in patterns::normalize_pattern_nested(&ep) {
                if let Some(stripped) = normalized.strip_prefix('!') {
                    if let Ok(glob) = globset::GlobBuilder::new(stripped)
                        .literal_separator(true)
                        .build()
                    {
                        exclude_builder.add(glob);
                        has_exclude = true;
                    }
                } else if let Ok(glob) = globset::GlobBuilder::new(&normalized)
                    .literal_separator(true)
                    .build()
                {
                    include_builder.add(glob);
                    has_include = true;
                }
            }
        }
    }

    if !has_include {
        return None;
    }

    let include = include_builder.build().ok()?;
    let exclude = if has_exclude {
        exclude_builder.build().ok()
    } else {
        None
    };

    Some(IgnoreFilter { include, exclude })
}

fn build_rooted_ignore_filter(patterns: &GlobPatternSet) -> Option<RootedIgnoreFilter> {
    let mut include = HashMap::<RootedGlobKey, RootedGlobMatcherBuilder>::new();
    let mut exclude = HashMap::<RootedGlobKey, RootedGlobMatcherBuilder>::new();

    for pattern in patterns.iter() {
        for expanded in patterns::expand_extglobs(&pattern.glob) {
            for normalized in patterns::normalize_pattern_nested(&expanded) {
                let glob_def = GlobDef {
                    glob: normalized,
                    root: pattern.root.clone(),
                    source: pattern.source.clone(),
                };
                let is_negative = glob_def.glob.starts_with('!');
                let Some((key, glob_pattern, basename_fallback)) = compile_rooted_glob(&glob_def)
                else {
                    continue;
                };
                let groups = if is_negative {
                    &mut exclude
                } else {
                    &mut include
                };
                groups
                    .entry(key)
                    .or_default()
                    .add(glob_pattern, basename_fallback);
            }
        }
    }

    let include = build_rooted_glob_matchers(include);
    if include.is_empty() {
        return None;
    }

    Some(RootedIgnoreFilter {
        include,
        exclude: build_rooted_glob_matchers(exclude),
    })
}

struct RootedGlobMatcherBuilder {
    matcher: globset::GlobSetBuilder,
    basename_matcher: globset::GlobSetBuilder,
    has_matcher: bool,
    has_basename_matcher: bool,
}

impl Default for RootedGlobMatcherBuilder {
    fn default() -> Self {
        Self {
            matcher: globset::GlobSetBuilder::new(),
            basename_matcher: globset::GlobSetBuilder::new(),
            has_matcher: false,
            has_basename_matcher: false,
        }
    }
}

impl RootedGlobMatcherBuilder {
    fn add(&mut self, glob_pattern: &str, basename_fallback: bool) {
        let Ok(glob) = globset::GlobBuilder::new(glob_pattern)
            .literal_separator(true)
            .build()
        else {
            return;
        };
        self.matcher.add(glob);
        self.has_matcher = true;

        if basename_fallback {
            let Ok(glob) = globset::GlobBuilder::new(glob_pattern)
                .literal_separator(true)
                .build()
            else {
                return;
            };
            self.basename_matcher.add(glob);
            self.has_basename_matcher = true;
        }
    }

    fn build(self, key: RootedGlobKey) -> Option<RootedGlobMatcher> {
        if !self.has_matcher {
            return None;
        }
        let matcher = self.matcher.build().ok()?;
        let basename_matcher = if self.has_basename_matcher {
            self.basename_matcher.build().ok()
        } else {
            None
        };

        Some(RootedGlobMatcher {
            matcher,
            basename_matcher,
            root: key.root,
            is_global: key.is_global,
        })
    }
}

fn build_rooted_glob_matchers(
    groups: HashMap<RootedGlobKey, RootedGlobMatcherBuilder>,
) -> Vec<RootedGlobMatcher> {
    groups
        .into_iter()
        .filter_map(|(key, builder)| builder.build(key))
        .collect()
}

fn compile_rooted_glob(pattern: &GlobDef) -> Option<(RootedGlobKey, &str, bool)> {
    let (glob_pattern, basename_fallback) = normalize_rooted_pattern(&pattern.glob);
    Some((
        RootedGlobKey {
            root: pattern.root.as_deref().map(resolve_match_root),
            is_global: is_global_pattern(&pattern.glob),
        },
        glob_pattern,
        basename_fallback,
    ))
}

fn normalize_rooted_pattern(pattern: &str) -> (&str, bool) {
    let pattern = pattern.strip_prefix('!').unwrap_or(pattern);
    if let Some(stripped) = pattern.strip_prefix('/') {
        (stripped, false)
    } else {
        (pattern, true)
    }
}

fn matches_rooted_glob(
    glob: &RootedGlobMatcher,
    candidates: &mut RootedPathCandidates<'_>,
) -> bool {
    if glob.is_global && matches_rooted_candidate(glob, candidates.global_path()) {
        return true;
    }
    if let Some(candidate) = root_relative_match_path(candidates.file_path(), glob.root.as_deref())
    {
        return matches_rooted_candidate(glob, &candidate);
    }
    if !candidates.file_path().is_absolute() {
        return matches_rooted_candidate(glob, candidates.file_path());
    }
    false
}

fn matches_rooted_candidate(glob: &RootedGlobMatcher, candidate: &Path) -> bool {
    if glob.matcher.is_match(candidate) {
        return true;
    }
    if let Some(basename_matcher) = &glob.basename_matcher
        && let Some(name) = candidate.file_name()
        && basename_matcher.is_match(Path::new(name))
    {
        return true;
    }
    normalized_match_path(candidate)
        .is_some_and(|normalized| glob.matcher.is_match(Path::new(normalized.as_ref())))
}

fn lexical_absolute_path(path: &Path, cwd: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };

    let mut normalized = PathBuf::new();
    let absolute = joined.is_absolute();

    for component in joined.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() && !absolute {
                    normalized.push(component.as_os_str());
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }

    normalized
}

fn configure_walk_builder(
    builder: &mut WalkBuilder,
    show_hidden: bool,
    cspell_compat_mode: bool,
    use_gitignore: bool,
) {
    builder.hidden(!show_hidden);
    if cspell_compat_mode {
        // cspell disables .ignore files but respects .gitignore during walk
        // for performance (avoids traversing target/, node_modules/, etc.)
        builder
            .ignore(false)
            .git_ignore(use_gitignore)
            .git_global(use_gitignore)
            .git_exclude(use_gitignore);
    } else {
        builder.git_ignore(use_gitignore);
    }
}

#[derive(Debug)]
enum PendingPath {
    KnownFile(PathBuf),
    Unknown(PathBuf),
}

impl PendingPath {
    fn path(&self) -> &Path {
        match self {
            PendingPath::KnownFile(path) | PendingPath::Unknown(path) => path,
        }
    }
}

/// Parallel directory walk: uses `ignore::WalkParallel` to traverse the
/// directory tree across multiple threads (capped at 12, like ruff).
/// Only files matching `glob_set` are collected.  Because very few files
/// typically match (e.g. 2 800 out of 273 000 in azure-rest-api-specs),
/// contention on the shared `Mutex` is negligible.
fn collect_walk_matches(
    walk_dir: &Path,
    show_hidden: bool,
    cspell_compat_mode: bool,
    use_gitignore: bool,
    glob_set: &globset::GlobSet,
    out: &mut Vec<PendingPath>,
) {
    let mut builder = WalkBuilder::new(walk_dir);
    configure_walk_builder(&mut builder, show_hidden, cspell_compat_mode, use_gitignore);
    builder.threads(
        std::thread::available_parallelism()
            .map_or(1, std::num::NonZeroUsize::get)
            .min(12),
    );

    let collected: std::sync::Arc<std::sync::Mutex<Vec<PendingPath>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let walker = builder.build_parallel();

    walker.run(|| {
        let collected = std::sync::Arc::clone(&collected);
        let walk_dir = walk_dir.to_path_buf();
        let glob_set = glob_set.clone();
        Box::new(move |entry| {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => return WalkState::Continue,
            };
            if entry.file_type().is_some_and(|ft| ft.is_file()) {
                let path = entry.path();
                let rel = path.strip_prefix(&walk_dir).unwrap_or(path);
                if glob_set.is_match(rel) {
                    collected
                        .lock()
                        .unwrap()
                        .push(PendingPath::KnownFile(entry.into_path()));
                }
            }
            WalkState::Continue
        })
    });

    out.extend(
        std::sync::Arc::try_unwrap(collected)
            .unwrap()
            .into_inner()
            .unwrap(),
    );
}

fn collect_files(
    paths: &[PathBuf],
    settings: &CSpellSettings,
    options: &CheckOptions,
) -> Result<Vec<PathBuf>> {
    let cwd = options.cwd();
    let mut roots: Vec<PendingPath> = Vec::new();
    let mut glob_patterns: Vec<String> = Vec::new();

    // Separate literal paths from glob patterns
    for path in paths {
        let s = path.to_string_lossy();
        if is_glob_pattern(&s) {
            glob_patterns.push(s.into_owned());
        } else {
            roots.push(PendingPath::Unknown(path.clone()));
        }
    }

    roots.extend(
        read_file_list_paths(&options.file_list)?
            .into_iter()
            .map(PendingPath::Unknown),
    );
    for f in &options.file {
        if f.is_file() {
            roots.push(PendingPath::KnownFile(f.clone()));
        }
    }

    let use_gitignore = options.use_gitignore.unwrap_or(
        settings
            .use_gitignore
            .unwrap_or(options.use_gitignore_default),
    );
    let show_hidden = options.dot;

    // Expand glob patterns from CLI positional arguments.
    // Use literal_separator so `*` does not match `/` (matching cspell/tinyGlob behavior).
    if !glob_patterns.is_empty() {
        let mut glob_builder = globset::GlobSetBuilder::new();
        for pattern in &glob_patterns {
            if let Ok(glob) = globset::GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
            {
                glob_builder.add(glob);
            }
        }
        if let Ok(glob_set) = glob_builder.build() {
            collect_walk_matches(
                &cwd,
                show_hidden,
                options.cspell_compat_mode,
                use_gitignore,
                &glob_set,
                &mut roots,
            );
        }
    }

    // If no paths specified, try settings.files glob patterns.
    // cspell uses include-mode glob normalization here:
    // patterns are matched relative to the root without implicit `**/` prefixing.
    if roots.is_empty()
        && let Some(ref file_globs) = settings.files
    {
        let mut glob_builder = globset::GlobSetBuilder::new();
        for pattern in file_globs {
            if let Ok(glob) = globset::GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
            {
                glob_builder.add(glob);
            }
        }
        if let Ok(glob_set) = glob_builder.build() {
            collect_walk_matches(
                &cwd,
                show_hidden,
                options.cspell_compat_mode,
                use_gitignore,
                &glob_set,
                &mut roots,
            );
        }
    }

    let mut files = Vec::new();

    for pending in &roots {
        let path = pending.path();
        if matches!(pending, PendingPath::KnownFile(_)) || path.is_file() {
            files.push(path.to_path_buf());
        } else if is_glob_pattern(&path.to_string_lossy()) {
            // Expand glob patterns in path arguments (e.g. "**/*.rs", "src/**/*.{ts,js}")
            let path_str = path.to_string_lossy();
            if let Some((base_dir, pattern)) = split_glob_base(&path_str) {
                let base = if base_dir.is_empty() {
                    cwd.clone()
                } else {
                    let candidate = PathBuf::from(&base_dir);
                    if candidate.is_absolute() {
                        candidate
                    } else {
                        cwd.join(candidate)
                    }
                };
                if base.is_dir()
                    && let Ok(glob) = globset::Glob::new(&pattern)
                {
                    let matcher = glob.compile_matcher();
                    let mut builder = WalkBuilder::new(&base);
                    configure_walk_builder(
                        &mut builder,
                        show_hidden,
                        options.cspell_compat_mode,
                        use_gitignore,
                    );
                    for entry in builder.build() {
                        if let Ok(entry) = entry
                            && entry.file_type().is_some_and(|ft| ft.is_file())
                        {
                            let entry_path = entry.path();
                            let rel = entry_path.strip_prefix(&base).unwrap_or(entry_path);
                            if matcher.is_match(rel) {
                                files.push(entry.into_path());
                            }
                        }
                    }
                }
            }
        } else {
            // Resolve relative paths (e.g., ".") to absolute so that
            // entry.into_path() yields absolute paths. This ensures
            // strip_prefix(cwd) works correctly for --exclude / ignorePaths.
            let abs_path = if path.is_relative() {
                cwd.join(path)
            } else {
                path.to_path_buf()
            };
            let mut builder = WalkBuilder::new(&abs_path);
            configure_walk_builder(
                &mut builder,
                show_hidden,
                options.cspell_compat_mode,
                use_gitignore,
            );

            // --gitignore-root limits .gitignore search depth
            if !options.cspell_compat_mode
                && let Some(ref gi_root) = options.gitignore_root
            {
                builder.git_global(false);
                // Add the gitignore root as a custom ignore root
                let gi_path = gi_root.join(".gitignore");
                if gi_path.exists() {
                    let _ = builder.add_ignore(&gi_path);
                }
            }

            // Parallel directory walk — collect all files across threads.
            builder.threads(
                std::thread::available_parallelism()
                    .map_or(1, std::num::NonZeroUsize::get)
                    .min(12),
            );
            let collected: std::sync::Arc<std::sync::Mutex<Vec<PathBuf>>> =
                std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let walker = builder.build_parallel();

            walker.run(|| {
                let collected = std::sync::Arc::clone(&collected);
                Box::new(move |entry| {
                    if let Ok(entry) = entry
                        && entry.file_type().is_some_and(|ft| ft.is_file())
                    {
                        collected.lock().unwrap().push(entry.into_path());
                    }
                    WalkState::Continue
                })
            });

            files.extend(
                std::sync::Arc::try_unwrap(collected)
                    .unwrap()
                    .into_inner()
                    .unwrap(),
            );
        }
    }

    // ignorePaths and --exclude patterns are matched against relative paths,
    // with patterns without '/' treated as basename-anywhere matches (prefixed with **/).

    // Exclude the config file itself (cspell auto-excludes its own config)
    if let Some(ref config_file) = options.config_file {
        let config_abs = lexical_absolute_path(config_file, &cwd);
        files.retain(|f| {
            if f == &config_abs {
                return false;
            }
            if f.is_absolute() {
                return true;
            }
            lexical_absolute_path(f, &cwd) != config_abs
        });
    }

    if options.cspell_compat_mode && use_gitignore {
        let gitignore_root = options
            .gitignore_root
            .as_deref()
            .map(|root| {
                if root.is_absolute() {
                    root.to_path_buf()
                } else {
                    cwd.join(root)
                }
            })
            .unwrap_or_else(|| cwd.clone());
        let mut gitignore = GitIgnoreCache::new(gitignore_root);
        files.retain(|f| !gitignore.is_ignored(f));
    }

    if !settings.resolved_ignore_paths.is_empty() {
        if let Some(filter) = build_rooted_ignore_filter(&settings.resolved_ignore_paths) {
            files.retain(|f| !filter.is_ignored(f));
        }
    } else if !settings.ignore_paths.is_empty() {
        let filter = build_ignore_filter(&settings.ignore_paths);
        if let Some(filter) = filter {
            files.retain(|f| {
                let rel = f.strip_prefix(&cwd).unwrap_or(f);
                !filter.is_ignored(rel)
            });
        }
    }

    if !options.exclude.is_empty() {
        let filter = build_ignore_filter(&options.exclude);
        if let Some(filter) = filter {
            files.retain(|f| {
                let rel = f.strip_prefix(&cwd).unwrap_or(f);
                !filter.is_ignored(rel)
            });
        }
    }

    // Note: settings.files is only used for file *discovery* (when no CLI paths are given).
    // When CLI paths are provided, files are checked as-is without filtering by settings.files.
    // This matches cspell behavior: explicit CLI paths override the `files` config field.
    // The discovery path is handled above (lines that check `roots.is_empty()`).

    // Filter out linguist-vendored / linguist-generated paths from .gitattributes.
    // Only in native mode — cspell doesn't respect .gitattributes.
    if options.use_gitattributes {
        let mut vendored_entries: Vec<(PathBuf, globset::GlobSet)> = Vec::new();
        for pending in &roots {
            let root = match pending {
                PendingPath::KnownFile(path) => path.parent().unwrap_or(path),
                PendingPath::Unknown(path) if path.is_file() => path.parent().unwrap_or(path),
                PendingPath::Unknown(path) => path.as_path(),
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

    // Filter out generated/binary files (cspell skips these via @cspell/filetypes)
    files.retain(|f| !is_generated_file(f));

    let mut seen = HashSet::new();
    files.retain(|f| seen.insert(f.clone()));

    // cspell's glob pipeline returns a deterministic, sorted file list.
    // This matters for `--unique`, where the first file that reports a word wins.
    if options.cspell_compat_mode {
        files.sort_unstable_by(|a, b| cspell_path_cmp(a, b));
    }

    Ok(files)
}

fn cspell_path_cmp(a: &Path, b: &Path) -> Ordering {
    #[cfg(unix)]
    {
        let a_bytes = a.as_os_str().as_bytes();
        let b_bytes = b.as_os_str().as_bytes();
        if a_bytes.is_ascii() && b_bytes.is_ascii() {
            return cspell_path_ascii_cmp(a_bytes, b_bytes);
        }
    }

    let a = a.to_string_lossy();
    let b = b.to_string_lossy();
    cspell_path_str_cmp(a.as_ref(), b.as_ref())
}

fn cspell_path_str_cmp(a: &str, b: &str) -> Ordering {
    if a.is_ascii() && b.is_ascii() {
        return cspell_path_ascii_cmp(a.as_bytes(), b.as_bytes());
    }

    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();
    match a_lower.cmp(&b_lower) {
        Ordering::Equal => cspell_path_case_tiebreak(a, b),
        ord => ord,
    }
}

fn cspell_path_ascii_cmp(a: &[u8], b: &[u8]) -> Ordering {
    for (&a_byte, &b_byte) in a.iter().zip(b.iter()) {
        match cspell_path_primary_weight(a_byte).cmp(&cspell_path_primary_weight(b_byte)) {
            Ordering::Equal => continue,
            ord => return ord,
        }
    }

    match a.len().cmp(&b.len()) {
        Ordering::Equal => cspell_path_ascii_case_tiebreak(a, b),
        ord => ord,
    }
}

fn cspell_path_ascii_case_tiebreak(a: &[u8], b: &[u8]) -> Ordering {
    for (&a_byte, &b_byte) in a.iter().zip(b.iter()) {
        if a_byte == b_byte {
            continue;
        }

        match (
            a_byte.is_ascii_lowercase(),
            b_byte.is_ascii_lowercase(),
            a_byte.is_ascii_uppercase(),
            b_byte.is_ascii_uppercase(),
        ) {
            (true, false, _, true) => return Ordering::Less,
            (false, true, true, _) => return Ordering::Greater,
            _ => return a_byte.cmp(&b_byte),
        }
    }

    Ordering::Equal
}

fn cspell_path_primary_weight(byte: u8) -> u16 {
    match byte {
        b'_' => 1,
        b'-' => 2,
        b'.' => 3,
        b'/' | b'\\' => 4,
        _ => 0x10 + byte.to_ascii_lowercase() as u16,
    }
}

fn cspell_path_case_tiebreak(a: &str, b: &str) -> Ordering {
    for (a_char, b_char) in a.chars().zip(b.chars()) {
        if a_char == b_char {
            continue;
        }

        match (
            a_char.is_lowercase(),
            b_char.is_lowercase(),
            a_char.is_uppercase(),
            b_char.is_uppercase(),
        ) {
            (true, false, _, true) => return Ordering::Less,
            (false, true, true, _) => return Ordering::Greater,
            _ => return a_char.cmp(&b_char),
        }
    }

    Ordering::Equal
}

/// Split a path with glob characters into (base_directory, glob_pattern).
///
/// E.g. `/home/user/repo/**/*.rs` → (`/home/user/repo`, `**/*.rs`)
///      `src/**/*.{ts,js}` → (`src`, `**/*.{ts,js}`)
///      `**/*.*` → (``, `**/*.*`)
fn split_glob_base(path: &str) -> Option<(String, String)> {
    // Split on '/' (works on all platforms for glob paths)
    let components: Vec<&str> = path.split('/').collect();

    for (i, component) in components.iter().enumerate() {
        if component.contains('*')
            || component.contains('?')
            || component.contains('[')
            || component.contains('{')
        {
            let base = components[..i].join("/");
            let pattern = components[i..].join("/");
            return Some((base, pattern));
        }
    }
    None
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

#[allow(dead_code)]
fn print_issue_text(
    file: &Path,
    issue: &ValidationIssue,
    show_suggestions: Option<bool>,
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
    if !issue.is_forbidden
        && should_render_suggestions(show_context, show_suggestions)
        && !issue.suggestions.is_empty()
    {
        print!(" fix: ({})", issue.suggestions.join(", "));
    }
    println!();

    if show_context
        && let Some(text) = content
        && let Some(line_text) = text.lines().nth(issue.line.saturating_sub(1))
    {
        eprintln!("    {line_text}");
    }
}

/// Like `print_issue_text` but accepts a pre-resolved context line slice,
/// avoiding the O(N) `.lines().nth()` scan per issue.
fn print_issue_text_fast(
    file: &Path,
    issue: &ValidationIssue,
    show_suggestions: Option<bool>,
    show_context: bool,
    ctx_line: Option<&str>,
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
    if !issue.is_forbidden
        && should_render_suggestions(show_context, show_suggestions)
        && !issue.suggestions.is_empty()
    {
        print!(" fix: ({})", issue.suggestions.join(", "));
    }
    println!();

    if show_context && let Some(line_text) = ctx_line {
        eprintln!("    {line_text}");
    }
}

fn should_render_suggestions(show_context: bool, show_suggestions: Option<bool>) -> bool {
    if show_context {
        show_suggestions == Some(true)
    } else {
        show_suggestions != Some(false)
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
                        .is_none_or(|df| df.should_report(file, i.line))
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

/// Read a file using memory-mapping and decode it like cspell's `decodeToString`.
///
/// Order:
/// - UTF-8 fast path
/// - UTF-16 BE/LE detection via BOM or NUL-byte heuristic
/// - lossy UTF-8 decode for invalid byte sequences
fn read_file_mmap_classified(path: &Path) -> Option<ReadFileMmap> {
    let file = std::fs::File::open(path).ok()?;
    let meta = file.metadata().ok()?;
    let len = meta.len();
    if len == 0 {
        return Some(ReadFileMmap::Text(String::new()));
    }
    // SAFETY: We only read the mapped memory as UTF-8 bytes.
    // Concurrent file modification could cause garbled output but not
    // memory unsafety in practice (CLI tool, non-critical).
    let mmap = unsafe { memmap2::Mmap::map(&file) }.ok()?;
    let bytes = &mmap[..];

    if !is_generated_file(path) && is_unknown_binary_content(bytes) {
        return Some(ReadFileMmap::Binary);
    }

    // Match cspell-io decodeToString: detect UTF-16 via BOM or NUL-byte pattern.
    if bytes.len() >= 2 {
        let bom = u16::from_be_bytes([bytes[0], bytes[1]]);

        if bom == 0xFEFF || (bytes[0] == 0 && bytes[1] != 0) {
            let start = usize::from(bom == 0xFEFF) * 2;
            let u16_iter = bytes[start..]
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]));
            let decoded: String = char::decode_utf16(u16_iter)
                .map(|r| r.unwrap_or('\u{FFFD}'))
                .collect();
            return Some(ReadFileMmap::Text(decoded));
        }

        if bom == 0xFFFE || (bytes[0] != 0 && bytes[1] == 0) {
            let start = usize::from(bom == 0xFFFE) * 2;
            let u16_iter = bytes[start..]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]));
            let decoded: String = char::decode_utf16(u16_iter)
                .map(|r| r.unwrap_or('\u{FFFD}'))
                .collect();
            return Some(ReadFileMmap::Text(decoded));
        }
    }

    // UTF-8 fast path after UTF-16 detection.
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Some(ReadFileMmap::Text(s.to_string()));
    }

    // Match TextDecoder('utf8'): replace invalid byte sequences with U+FFFD.
    Some(ReadFileMmap::Text(
        String::from_utf8_lossy(bytes).into_owned(),
    ))
}

#[allow(dead_code)]
fn read_file_mmap(path: &Path) -> Option<String> {
    match read_file_mmap_classified(path)? {
        ReadFileMmap::Text(text) => Some(text),
        ReadFileMmap::Binary => None,
    }
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
    /// `None` = file was clean (no issues).
    /// `Some(issues)` = file had issues (legacy format; new writes store clean-only).
    issues: Option<Vec<ValidationIssue>>,
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
                // New format: "clean":true means file had no issues (no "issues" key).
                // Legacy format: "issues":[] array is present.
                let issues = if val.get("clean").and_then(|v| v.as_bool()) == Some(true) {
                    None
                } else {
                    val.get("issues").and_then(|v| v.as_array()).map(|arr| {
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
                };
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

    /// Returns `Some(true)` if the file is cached as clean (no issues),
    /// `None` if not cached or stale.  Does NOT return cached issues —
    /// files with issues are always re-checked (like ruff).
    fn check_clean(&self, path: &Path, strategy: &str) -> Option<bool> {
        let entry = self.entries.get(path)?;

        // Only clean files (no issues) are eligible for cache hits.
        // Files that had issues must always be re-checked.
        if entry.issues.is_some() {
            return None;
        }

        let fresh = if strategy == "content" {
            let cached_hash = entry.hash.as_deref()?;
            let content = std::fs::read(path).ok()?;
            let hash = format!("{:x}", Md5::digest(&content));
            hash == cached_hash
        } else {
            let meta = std::fs::metadata(path).ok()?;
            let size = meta.len();
            let mtime = meta
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_secs() as i64;
            entry.mtime_secs == mtime && entry.size == size
        };

        if fresh { Some(true) } else { None }
    }

    fn update(
        &mut self,
        path: &Path,
        issues: &[ValidationIssue],
        strategy: &str,
        content: Option<&[u8]>,
    ) {
        // Only cache clean files (no issues).  Files with issues are
        // always re-checked to ensure corrections are detected.
        if !issues.is_empty() {
            return;
        }
        if let Ok(meta) = std::fs::metadata(path) {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let hash = if strategy == "content" {
                let data = content.unwrap_or(&[]);
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
                    issues: None,
                },
            );
        }
    }

    fn save(&self, path: &Path) {
        let mut entries = serde_json::Map::new();
        for (file_path, entry) in &self.entries {
            // Only clean entries are persisted.
            if entry.issues.is_some() {
                continue;
            }
            let mut entry_json = serde_json::json!({
                "mtime": entry.mtime_secs,
                "size": entry.size,
                "clean": true,
            });
            if let Some(ref hash) = entry.hash {
                entry_json["hash"] = serde_json::json!(hash);
            }
            entries.insert(file_path.display().to_string(), entry_json);
        }
        let json = serde_json::json!({
            "version": "2",
            "entries": entries,
        });
        if let Ok(content) = serde_json::to_string(&json) {
            let _ = std::fs::write(path, content);
        }
    }
}

// cspell:disable
#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

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
    fn merge_defaults_with_explicit_dictionaries() {
        let mut settings = CSpellSettings {
            dictionaries: vec!["en_gb".into(), "custom".into()],
            ..Default::default()
        };

        apply_default_dictionaries(&mut settings);

        // User dictionaries are preserved
        assert!(settings.dictionaries.contains(&"en_gb".to_string()));
        assert!(settings.dictionaries.contains(&"custom".to_string()));
        // Default dictionaries are also added
        for &name in DEFAULT_DICTIONARIES {
            assert!(
                settings.dictionaries.iter().any(|d| d == name),
                "default dict '{}' should be present",
                name
            );
        }
    }

    #[test]
    fn negated_dictionary_removes_default() {
        let mut settings = CSpellSettings {
            dictionaries: vec!["!companies".into(), "custom".into()],
            ..Default::default()
        };

        apply_default_dictionaries(&mut settings);

        // "companies" should be excluded
        assert!(!settings.dictionaries.iter().any(|d| d == "companies"));
        // "custom" should remain
        assert!(settings.dictionaries.contains(&"custom".to_string()));
        // Other defaults should be present
        assert!(settings.dictionaries.iter().any(|d| d == "en_us"));
        // Negation entries should be stripped
        assert!(!settings.dictionaries.iter().any(|d| d.starts_with('!')));
    }

    /// cspell.json uses bare dictionary names like "en_us" which auto-resolve
    /// to npm packages via `dict_name_to_package`.
    #[test]
    fn cspell_bare_dict_name_maps_to_npm_package() {
        assert_eq!(
            defaults::dict_name_to_package("en_us"),
            "@cspell/dict-en_us"
        );
        assert_eq!(
            defaults::dict_name_to_package("softwareterms"),
            "@cspell/dict-software-terms"
        );
        assert_eq!(defaults::dict_name_to_package("c"), "@cspell/dict-cpp");
        assert_eq!(defaults::dict_name_to_package("rust"), "@cspell/dict-rust");
        assert_eq!(
            defaults::dict_name_to_package("typescript"),
            "@cspell/dict-typescript"
        );
    }

    /// matchum.toml bare dictionary names must NOT auto-resolve via npm.
    /// Only names starting with `@cspell/` should trigger npm resolution.
    #[test]
    fn matchum_toml_bare_dict_does_not_auto_resolve_cspell() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("matchum.toml");
        std::fs::write(
            &config_path,
            r#"
language = "en"
dictionaries = ["en_us"]
"#,
        )
        .unwrap();

        let settings = resolver::load_matchum_config(&config_path).unwrap();
        assert_eq!(settings.dictionaries, vec!["en_us"]);

        // build_dictionary_catalog with is_cspell=false should NOT
        // try to map "en_us" → "@cspell/dict-en_us" and fetch from npm.
        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            Some(dir.path()),
            None,
            false,
        )
        .unwrap();

        // "en_us" has no dictionary_definition and auto-resolve is off,
        // so it must not appear in the loaded dictionaries.
        assert!(
            !catalog.named_dicts.iter().any(|(name, _)| name == "en_us"),
            "bare 'en_us' in matchum.toml must not auto-resolve via npm"
        );
    }

    #[test]
    fn locale_matches_none_matches_everything() {
        assert!(super::locale_matches(None, "en"));
        assert!(super::locale_matches(None, "de"));
    }

    #[test]
    fn locale_matches_wildcard() {
        assert!(super::locale_matches(Some("*"), "en"));
        assert!(super::locale_matches(Some("*"), "de"));
    }

    #[test]
    fn locale_matches_exact() {
        assert!(super::locale_matches(Some("en"), "en"));
        assert!(!super::locale_matches(Some("de"), "en"));
    }

    #[test]
    fn locale_matches_case_insensitive() {
        assert!(super::locale_matches(Some("en-GB"), "en-gb"));
        assert!(super::locale_matches(Some("EN"), "en"));
    }

    #[test]
    fn locale_matches_comma_separated() {
        assert!(super::locale_matches(Some("lorem,lorem-ipsum"), "lorem"));
        assert!(!super::locale_matches(Some("lorem,lorem-ipsum"), "en"));
    }

    #[test]
    fn locale_matches_comma_with_spaces() {
        assert!(super::locale_matches(
            Some("lorem, lorem-ipsum"),
            "lorem-ipsum"
        ));
    }

    #[test]
    fn language_id_matches_basic() {
        let pats = vec!["javascript".to_string(), "typescript".to_string()];
        assert!(super::language_id_matches(&pats, "javascript"));
        assert!(super::language_id_matches(&pats, "typescript"));
        assert!(!super::language_id_matches(&pats, "rust"));
    }

    #[test]
    fn language_id_matches_wildcard() {
        let pats = vec!["*".to_string()];
        assert!(super::language_id_matches(&pats, "*"));
        assert!(!super::language_id_matches(&pats, "anything"));
    }

    #[test]
    fn language_id_matches_exclusion_only() {
        let pats = vec!["!python".to_string()];
        assert!(super::language_id_matches(&pats, "*"));
        assert!(super::language_id_matches(&pats, "javascript"));
        assert!(!super::language_id_matches(&pats, "python"));
    }

    #[test]
    fn active_language_ids_unknown_extension_stays_wildcard_only() {
        let settings = CSpellSettings::default();
        let ids =
            super::active_language_ids(&settings, Some(Path::new("assets/materials/model.ogex")));
        assert_eq!(ids, vec!["*"]);
    }

    #[test]
    fn active_language_ids_respects_explicit_setting() {
        let settings = CSpellSettings {
            language_id: Some("javascript,html".into()),
            ..Default::default()
        };
        let ids =
            super::active_language_ids(&settings, Some(Path::new("assets/materials/model.ogex")));
        assert_eq!(ids, vec!["*", "javascript", "html"]);
    }

    #[test]
    fn java_member_function_ignore_pattern_suppresses_getenv_like_cspell() {
        let settings = CSpellSettings {
            language: Some("en".into()),
            patterns: vec![PatternDefinition {
                name: "java-member-function".into(),
                pattern: StringOrList::Single(r"/(\.\w+)+(?=\()/g".into()),
            }],
            language_settings: vec![LanguageSetting {
                language_id: vec!["java".into()],
                locale: Some("*".into()),
                ignore_reg_exp_list: vec!["java-member-function".into()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let validator = build_validator(
            &settings,
            &[],
            &CheckOptions::default(),
            &HashSet::new(),
            false,
            Some(Path::new("JettyLauncher.java")),
            None,
        );
        let issues = validator.validate_text(".getenv(");

        assert!(
            issues.is_empty(),
            "java member-function ignore pattern should suppress getenv, got {issues:?}"
        );
    }

    #[test]
    fn bundled_java_language_settings_ignore_getenv_like_cspell() {
        let mut settings = CSpellSettings::default();
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            None,
            Some(&crate::commands::dict_cache_dir()),
            true,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);

        let validator = build_validator(
            &merged_settings,
            &catalog.named_dicts,
            &CheckOptions::default(),
            &catalog.extra_active,
            false,
            Some(Path::new("JettyLauncher.java")),
            None,
        );
        let issues = validator.validate_text("System.getenv(\"HOME\");\n");

        assert!(
            issues.iter().all(|issue| issue.word != "getenv"),
            "bundled java language settings should suppress getenv, got {issues:?}"
        );
    }

    #[test]
    fn bundled_latex_language_settings_enable_custom_ignore_patterns() {
        let mut settings = CSpellSettings::default();
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            None,
            Some(&crate::commands::dict_cache_dir()),
            true,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);

        let validator_config = build_validator_config(
            &merged_settings,
            None,
            Some(CompoundWordsMode::SeparateWords),
            true,
            false,
            Some(Path::new("sample.tex")),
        );

        assert!(
            validator_config
                .custom_ignore_patterns
                .has_latex_macro_function_names(),
            "expected latex macro function ignore mask"
        );
        assert!(
            validator_config
                .custom_ignore_patterns
                .has_latex_macros_multiline(),
            "expected latex multiline macro ignore mask"
        );
        assert!(
            validator_config.custom_ignore_patterns.has_latex_math(),
            "expected latex math ignore mask"
        );
    }

    #[test]
    fn bundled_ada_language_settings_enable_custom_ignore_patterns() {
        let mut settings = CSpellSettings::default();
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            None,
            Some(&crate::commands::dict_cache_dir()),
            true,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);

        let validator_config = build_validator_config(
            &merged_settings,
            None,
            Some(CompoundWordsMode::SeparateWords),
            true,
            false,
            Some(Path::new("sample.adb")),
        );

        assert!(
            validator_config.custom_ignore_patterns.has_ada_word_break(),
            "expected ada word-break ignore mask"
        );
    }

    #[test]
    fn bundled_ada_language_settings_split_apostrophe_tokens_on_precompiled_path() {
        let mut settings = CSpellSettings::default();
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            None,
            Some(&crate::commands::dict_cache_dir()),
            true,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);
        let options = CheckOptions {
            cspell_compat_mode: true,
            ..CheckOptions::default()
        };
        let context = build_root_config_context(&merged_settings, None, &options, Some(false));

        let validator = build_validator(
            &merged_settings,
            &catalog.named_dicts,
            &options,
            &catalog.extra_active,
            false,
            Some(Path::new("sample.adb")),
            Some(context.base_validator_config.as_ref()),
        );
        let issues = validator.validate_text("for I in Gamepads'Range loop\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(
            words.contains(&"Gamepads"),
            "expected left token issue, got {words:?}"
        );
        assert!(
            !words.iter().any(|word| word.contains("'Range")),
            "did not expect whole apostrophe token, got {words:?}"
        );
    }

    #[test]
    fn merge_bundled_settings_applies_bundled_root_settings() {
        let merged = merge_bundled_settings(&CSpellSettings::default(), &[], &[]);

        assert_eq!(merged.language.as_deref(), Some("en"));
        assert_eq!(merged.allow_compound_words, Some(false));
        assert_eq!(merged.max_number_of_problems, Some(10_000));
        assert!(
            merged.ignore_paths.is_empty(),
            "default bundled settings should not inherit package-root ignorePaths"
        );
        assert!(
            merged.ignore_words.is_empty(),
            "default bundled settings should not inherit package-root ignoreWords"
        );
        assert!(
            merged.words.is_empty(),
            "default bundled settings should not inherit package-root words"
        );
    }

    #[test]
    fn bundled_latex_language_settings_skip_macros_like_cspell() {
        let mut settings = CSpellSettings::default();
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            None,
            Some(&crate::commands::dict_cache_dir()),
            true,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);

        let validator = build_validator(
            &merged_settings,
            &catalog.named_dicts,
            &CheckOptions {
                cspell_compat_mode: true,
                compound_words_mode: Some(CompoundWordsMode::SeparateWords),
                ..CheckOptions::default()
            },
            &catalog.extra_active,
            false,
            Some(Path::new("sample.tex")),
            None,
        );
        let issues = validator.validate_text("\\mathbb{R} \\section{Surjektiv} $Cauchyfolge$\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(
            !words.contains(&"mathbb"),
            "latex macro name should be ignored: {words:?}"
        );
        assert!(
            !words.contains(&"R"),
            "latex macro body should be ignored: {words:?}"
        );
        assert!(
            words.contains(&"Surjektiv"),
            "section text should remain visible: {words:?}"
        );
        assert!(
            !words.contains(&"Cauchyfolge"),
            "math content should be ignored: {words:?}"
        );
    }

    #[test]
    fn bundled_markdown_language_settings_accept_unplugin_from_npm_dictionary() {
        let mut settings = CSpellSettings::default();
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            None,
            Some(&crate::commands::dict_cache_dir()),
            true,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);
        let npm_dict = catalog
            .named_dicts
            .iter()
            .find(|(name, _)| name == "npm")
            .expect("expected npm dictionary to be loaded");
        assert!(
            npm_dict.1.has("unplugin"),
            "expected npm dictionary to contain unplugin"
        );
        let resolved_lang_settings =
            resolve_language_settings(&merged_settings, Some(Path::new("README.md")));
        assert!(
            resolved_lang_settings.dictionaries.contains(&"npm"),
            "expected markdown language settings to activate npm"
        );

        let validator = build_validator(
            &merged_settings,
            &catalog.named_dicts,
            &CheckOptions {
                cspell_compat_mode: true,
                compound_words_mode: Some(CompoundWordsMode::SeparateWords),
                ..CheckOptions::default()
            },
            &catalog.extra_active,
            false,
            Some(Path::new("README.md")),
            None,
        );
        let issues = validator.validate_text(
            "- [unplugin-auto-import](https://github.com/antfu/unplugin-auto-import)\n",
        );
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(
            !words.contains(&"unplugin"),
            "expected markdown/npm defaults to accept unplugin, got {words:?}"
        );
    }

    #[test]
    fn bundled_defaults_accept_livedata_from_coding_compound_terms() {
        let mut settings = CSpellSettings::default();
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            None,
            Some(&crate::commands::dict_cache_dir()),
            true,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);

        let coding_compound = catalog
            .named_dicts
            .iter()
            .find(|(name, _)| name == "coding-compound-terms")
            .expect("expected coding-compound-terms dictionary to load");
        assert!(
            coding_compound.1.has("livedata"),
            "expected coding-compound-terms to contain livedata via native compounds"
        );

        let validator = build_validator(
            &merged_settings,
            &catalog.named_dicts,
            &CheckOptions {
                cspell_compat_mode: true,
                compound_words_mode: Some(CompoundWordsMode::SeparateWords),
                ..CheckOptions::default()
            },
            &catalog.extra_active,
            false,
            Some(Path::new("sample.txt")),
            None,
        );
        let issues = validator.validate_text("livedata\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(
            !words.contains(&"livedata"),
            "expected bundled defaults to accept livedata, got {words:?}"
        );
    }

    #[test]
    fn bundled_cpp_defaults_accept_apientry() {
        let mut settings = CSpellSettings::default();
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            None,
            Some(&crate::commands::dict_cache_dir()),
            true,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);

        let resolved_lang_settings =
            resolve_language_settings(&merged_settings, Some(Path::new("sample.cpp")));
        assert!(
            resolved_lang_settings.dictionaries.contains(&"cpp"),
            "expected cpp language settings to activate cpp dictionary"
        );

        let validator = build_validator(
            &merged_settings,
            &catalog.named_dicts,
            &CheckOptions {
                cspell_compat_mode: true,
                compound_words_mode: Some(CompoundWordsMode::SeparateWords),
                ..CheckOptions::default()
            },
            &catalog.extra_active,
            false,
            Some(Path::new("sample.cpp")),
            None,
        );
        let issues = validator.validate_text("APIENTRY\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(
            !words.contains(&"APIENTRY"),
            "expected bundled cpp defaults to accept APIENTRY, got {words:?}"
        );
    }

    #[test]
    fn vitest_runtime_config_accepts_unplugin_in_markdown_links() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let repo = workspace_root
            .join("vendor/cspell/integration-tests/repositories/temp/vitest-dev/vitest");

        let resolved =
            crate::commands::setup::resolve_config(None, Some(repo.as_path()), true, &[]).unwrap();

        let mut settings = resolved.settings;
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            resolved.config_dir.as_deref(),
            Some(&crate::commands::dict_cache_dir()),
            true,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);
        let resolved_lang_settings =
            resolve_language_settings(&merged_settings, Some(Path::new("docs/guide/index.md")));
        assert!(
            resolved_lang_settings.dictionaries.contains(&"npm"),
            "expected runtime markdown settings to activate npm"
        );
        let npm_dict = catalog
            .named_dicts
            .iter()
            .find(|(name, _)| name == "npm")
            .expect("expected npm dictionary to be loaded");
        assert!(
            npm_dict.1.has("unplugin"),
            "expected runtime npm dictionary to contain unplugin"
        );

        let validator = build_validator(
            &merged_settings,
            &catalog.named_dicts,
            &CheckOptions {
                cspell_compat_mode: true,
                config_search: true,
                per_dir_config_search: true,
                compound_words_mode: Some(CompoundWordsMode::SeparateWords),
                ..CheckOptions::default()
            },
            &catalog.extra_active,
            false,
            Some(Path::new("docs/guide/index.md")),
            None,
        );
        let issues = validator.validate_text(
            "- [unplugin-auto-import](https://github.com/antfu/unplugin-auto-import)\n",
        );
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(
            !words.contains(&"unplugin"),
            "expected runtime config to accept unplugin, got {words:?}"
        );
    }

    #[test]
    fn collect_all_issues_vitest_runtime_config_skips_unplugin() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let repo = workspace_root
            .join("vendor/cspell/integration-tests/repositories/temp/vitest-dev/vitest");
        let file = repo.join("docs/guide/index.md");

        let resolved =
            crate::commands::setup::resolve_config(None, Some(repo.as_path()), true, &[]).unwrap();

        let mut settings = resolved.settings;
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            resolved.config_dir.as_deref(),
            Some(&crate::commands::dict_cache_dir()),
            true,
        )
        .expect("catalog");

        let results = collect_all_issues(
            std::slice::from_ref(&file),
            &settings,
            resolved.config_dir.as_deref(),
            catalog,
            CheckOptions {
                cspell_compat_mode: true,
                config_search: true,
                per_dir_config_search: true,
                no_must_find_files: true,
                compound_words_mode: Some(CompoundWordsMode::SeparateWords),
                cwd: Some(repo.clone()),
                ..CheckOptions::default()
            },
        )
        .expect("collect issues");

        let words: Vec<&str> = results
            .iter()
            .flat_map(|(_, _, issues)| issues.iter().map(|issue| issue.word.as_str()))
            .collect();

        assert!(
            !words.contains(&"unplugin"),
            "expected collect_all_issues to skip unplugin, got {words:?}"
        );
    }

    #[test]
    fn vitest_runtime_js_reports_weba_like_cspell() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let repo = workspace_root
            .join("vendor/cspell/integration-tests/repositories/temp/vitest-dev/vitest");

        let resolved =
            crate::commands::setup::resolve_config(None, Some(repo.as_path()), true, &[]).unwrap();

        let mut settings = resolved.settings;
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            resolved.config_dir.as_deref(),
            Some(&crate::commands::dict_cache_dir()),
            true,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);

        let validator = build_validator(
            &merged_settings,
            &catalog.named_dicts,
            &CheckOptions {
                cspell_compat_mode: true,
                compound_words_mode: Some(CompoundWordsMode::SeparateWords),
                ..CheckOptions::default()
            },
            &catalog.extra_active,
            false,
            Some(Path::new("sample.js")),
            None,
        );
        let issues = validator.validate_text(r#""audio/webm":["weba"]"#);
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(
            words.contains(&"weba"),
            "expected js excerpt to report weba, got {words:?}"
        );
    }

    #[test]
    fn vitest_runtime_css_reports_percent_encoded_svg_tokens_like_cspell() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let repo = workspace_root
            .join("vendor/cspell/integration-tests/repositories/temp/vitest-dev/vitest");

        let resolved =
            crate::commands::setup::resolve_config(None, Some(repo.as_path()), true, &[]).unwrap();

        let mut settings = resolved.settings;
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            resolved.config_dir.as_deref(),
            Some(&crate::commands::dict_cache_dir()),
            true,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);

        let validator = build_validator(
            &merged_settings,
            &catalog.named_dicts,
            &CheckOptions {
                cspell_compat_mode: true,
                compound_words_mode: Some(CompoundWordsMode::SeparateWords),
                ..CheckOptions::default()
            },
            &catalog.extra_active,
            false,
            Some(Path::new("sample.css")),
            None,
        );
        let issues = validator.validate_text(
            r#"url("data:image/svg+xml;utf8,%3Csvg viewBox='0 0 32 32'%3E%3Cpath d='M0 0'/%3E%3Ccircle cx='1' cy='1'/%3E")"#,
        );
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(
            words.contains(&"Csvg"),
            "expected css excerpt to report Csvg, got {words:?}"
        );
        assert!(
            words.contains(&"Cpath"),
            "expected css excerpt to report Cpath, got {words:?}"
        );
        assert!(
            words.contains(&"Ccircle"),
            "expected css excerpt to report Ccircle, got {words:?}"
        );
    }

    #[test]
    fn gitbucket_runtime_config_keeps_java_member_function_ignore_pattern() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let repo = workspace_root
            .join("vendor/cspell/integration-tests/repositories/temp/gitbucket/gitbucket");

        let resolved =
            crate::commands::setup::resolve_config(None, Some(repo.as_path()), true, &[]).unwrap();

        let mut settings = resolved.settings;
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            resolved.config_dir.as_deref(),
            Some(&crate::commands::dict_cache_dir()),
            true,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);

        let validator = build_validator(
            &merged_settings,
            &catalog.named_dicts,
            &CheckOptions {
                config_search: true,
                per_dir_config_search: true,
                ..CheckOptions::default()
            },
            &catalog.extra_active,
            false,
            Some(Path::new("src/main/java/JettyLauncher.java")),
            None,
        );
        let issues = validator.validate_text("System.getenv(\"HOME\");\n");

        assert!(
            issues.iter().all(|issue| issue.word != "getenv"),
            "gitbucket runtime config should suppress getenv, got {issues:?}"
        );
    }

    #[test]
    fn scala_language_settings_ignore_dotted_segments_in_sbt_files() {
        let settings = CSpellSettings {
            language_settings: vec![LanguageSetting {
                language_id: vec!["scala".into()],
                locale: Some("*".into()),
                ignore_reg_exp_list: vec![r"/^\s*import\s+\w+/m".into(), r"/\.\w+/".into()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let cfg = build_validator_config(
            &settings,
            None,
            Some(CompoundWordsMode::SeparateWords),
            true,
            false,
            Some(Path::new("build.sbt")),
        );

        assert!(
            cfg.ignore_patterns
                .iter()
                .any(|re| re.is_match(".jsuereth")),
            "expected scala ignore pattern to match dotted segments"
        );
        assert!(
            cfg.ignore_patterns
                .iter()
                .any(|re| re.is_match("import com")),
            "expected scala ignore pattern to match import prefixes"
        );
    }

    #[test]
    fn expand_extglobs_negation() {
        let result = super::patterns::expand_extglobs("src/translations/!(en).ts");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "src/translations/*.ts");
        assert_eq!(result[1], "!src/translations/en.ts");
    }

    #[test]
    fn expand_extglobs_multiple_alternatives() {
        let result = super::patterns::expand_extglobs("!(en|fr).ts");
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "*.ts");
        assert_eq!(result[1], "!en.ts");
        assert_eq!(result[2], "!fr.ts");
    }

    #[test]
    fn expand_extglobs_no_negation() {
        let result = super::patterns::expand_extglobs("**/*.ts");
        assert_eq!(result, vec!["**/*.ts"]);
    }

    #[test]
    fn is_generated_file_lock_files() {
        assert!(is_generated_file(Path::new("package-lock.json")));
        assert!(is_generated_file(Path::new("Cargo.lock")));
        assert!(is_generated_file(Path::new("composer.lock")));
        assert!(is_generated_file(Path::new("yarn.lock")));
        assert!(is_generated_file(Path::new("berksfile.lock")));
    }

    #[test]
    fn is_generated_file_binary_extensions() {
        assert!(is_generated_file(Path::new("image.png")));
        assert!(is_generated_file(Path::new("font.woff2")));
        assert!(is_generated_file(Path::new("archive.zip")));
        assert!(is_generated_file(Path::new("module.wasm")));
        assert!(is_generated_file(Path::new("video.mp4")));
        assert!(is_generated_file(Path::new("output.pdf")));
        assert!(is_generated_file(Path::new("output.log")));
    }

    #[test]
    fn is_generated_file_source_not_filtered() {
        assert!(!is_generated_file(Path::new("src/main.rs")));
        assert!(!is_generated_file(Path::new("README.md")));
        assert!(!is_generated_file(Path::new("package.json")));
        assert!(!is_generated_file(Path::new("index.ts")));
        assert!(!is_generated_file(Path::new("style.css")));
    }

    #[test]
    fn is_generated_file_cache_files() {
        assert!(is_generated_file(Path::new(".DS_Store")));
        assert!(is_generated_file(Path::new(".cspellcache")));
        assert!(is_generated_file(Path::new(".eslintcache")));
    }

    #[test]
    fn is_generated_file_map_files() {
        assert!(is_generated_file(Path::new("bundle.js.map")));
        assert!(is_generated_file(Path::new("styles.css.map")));
    }

    #[test]
    fn resolve_patterns_preserves_cspell_base64_named_pattern() {
        let mut settings = CSpellSettings {
            ignore_reg_exp_list: vec!["Base64".into()],
            ..Default::default()
        };
        apply_default_patterns(&mut settings);

        let (ignore, ignore_fancy, include, include_fancy, custom) = resolve_patterns(&settings);
        assert!(
            ignore.is_empty(),
            "Base64 should be handled by a custom scanner"
        );
        assert!(ignore_fancy.is_empty());
        assert!(include.is_empty());
        assert!(include_fancy.is_empty());
        assert!(custom.has_base64());
    }

    #[test]
    fn resolve_patterns_preserves_cspell_hash_strings_named_pattern() {
        let mut settings = CSpellSettings {
            ignore_reg_exp_list: vec!["HashStrings".into()],
            ..Default::default()
        };
        apply_default_patterns(&mut settings);

        let (ignore, ignore_fancy, include, include_fancy, custom) = resolve_patterns(&settings);
        assert!(
            ignore.is_empty(),
            "HashStrings should be handled by a custom scanner"
        );
        assert!(ignore_fancy.is_empty());
        assert!(include.is_empty());
        assert!(include_fancy.is_empty());
        assert!(custom.has_hash_strings());
    }

    #[test]
    fn build_validator_config_does_not_duplicate_builtin_hash_strings_from_bundled_defaults() {
        let mut settings = CSpellSettings::default();
        apply_default_patterns(&mut settings);

        let merged = merge_bundled_settings(&settings, &[], &[]);
        let config = build_validator_config(
            &merged,
            None,
            Some(CompoundWordsMode::SeparateWords),
            true,
            false,
            None,
        );

        assert!(
            !config.custom_ignore_patterns.has_hash_strings(),
            "default cspell exclude patterns are handled by validator builtins"
        );
    }

    #[test]
    fn collect_all_issues_ignores_wire_webapp_data_url_hash_strings() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../vendor/cspell/integration-tests/repositories/temp/wireapp/wire-webapp");
        let file = repo.join("apps/webapp/src/script/util/messageRenderer.test.ts");

        let resolved =
            crate::commands::setup::resolve_config(None, Some(repo.as_path()), true, &[])
                .expect("resolved config");
        let mut settings = resolved.settings;
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            resolved.config_dir.as_deref(),
            Some(&crate::commands::dict_cache_dir()),
            true,
        )
        .expect("catalog");

        let results = collect_all_issues(
            std::slice::from_ref(&file),
            &settings,
            resolved.config_dir.as_deref(),
            catalog,
            CheckOptions {
                cspell_compat_mode: true,
                config_search: true,
                per_dir_config_search: true,
                dot: false,
                use_gitignore: Some(true),
                gitignore_root: Some(repo.clone()),
                ..CheckOptions::default()
            },
        )
        .expect("issues");

        let words: Vec<String> = results
            .iter()
            .flat_map(|(_, _, issues)| issues.iter().map(|issue| issue.word.clone()))
            .collect();

        assert!(
            !words.iter().any(|word| word == "AQABAIAAAAAAAP"),
            "data URL payload should be ignored, got {words:?}"
        );
        assert!(
            !words
                .iter()
                .any(|word| word == "BAEAAAAALAAAAAABAAEAAAIBRAA"),
            "data URL payload should be ignored, got {words:?}"
        );
    }

    #[test]
    fn collect_all_issues_applies_bundled_ada_word_break_pattern() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../vendor/cspell/integration-tests/repositories/temp/AdaDoom3/AdaDoom3");
        let file = repo.join("Engine/Systems/Win32/neo-engine-system.adb");

        let resolved =
            crate::commands::setup::resolve_config(None, Some(repo.as_path()), true, &[])
                .expect("resolved config");
        let mut settings = resolved.settings;
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            resolved.config_dir.as_deref(),
            Some(&crate::commands::dict_cache_dir()),
            true,
        )
        .expect("catalog");

        let results = collect_all_issues(
            std::slice::from_ref(&file),
            &settings,
            resolved.config_dir.as_deref(),
            catalog,
            CheckOptions {
                cspell_compat_mode: true,
                config_search: true,
                per_dir_config_search: true,
                dot: false,
                use_gitignore: Some(true),
                gitignore_root: Some(repo.clone()),
                ..CheckOptions::default()
            },
        )
        .expect("issues");

        let words: Vec<String> = results
            .iter()
            .flat_map(|(_, _, issues)| issues.iter().map(|issue| issue.word.clone()))
            .collect();

        assert!(
            words.iter().any(|word| word == "Gamepads"),
            "expected ada word-break issue, got {words:?}"
        );
        assert!(
            !words.iter().any(|word| word == "Gamepads'Range"),
            "did not expect whole apostrophe token, got {words:?}"
        );
        assert!(
            words.iter().any(|word| word == "RAWINPUTDEVICELIST"),
            "expected left-side ada token, got {words:?}"
        );
        assert!(
            !words.iter().any(|word| word == "RAWINPUTDEVICELIST'Object"),
            "did not expect whole apostrophe token, got {words:?}"
        );
    }

    #[test]
    fn markdown_link_footer_pattern_matches_ascii_labels_only() {
        let footer = defaults::default_language_settings()
            .into_iter()
            .flat_map(|ls| ls.patterns.into_iter())
            .find(|p| p.name == "MARKDOWN-link-footer")
            .expect("missing markdown link footer pattern");

        let pattern = match footer.pattern {
            StringOrList::Single(s) => s,
            StringOrList::List(_) => panic!("expected single pattern"),
        };
        let re = Regex::new(&pattern).expect("invalid markdown footer regex");
        let ascii = "[Lee Byron]: https://github.com/leebyron";
        let unicode = "[António Nuno Monteiro]: https://github.com/anmonteiro";

        assert_eq!(
            re.find(ascii).map(|m| m.as_str()),
            Some("[Lee Byron]: https://github.com/leebyron")
        );
        assert!(re.find(unicode).is_none());
    }

    #[test]
    fn cspell_compat_markdown_does_not_implicitly_skip_fenced_code() {
        let settings = CSpellSettings::default();
        let options = CheckOptions {
            cspell_compat_mode: true,
            compound_words_mode: Some(CompoundWordsMode::SeparateWords),
            ..CheckOptions::default()
        };
        let validator = build_validator(
            &settings,
            &[],
            &options,
            &HashSet::new(),
            false,
            Some(Path::new("docs/readme.md")),
            None,
        );

        let issues =
            validator.validate_text("Outside csplit\n\n```\nPS> $Matches.user\njsmith\n```\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(
            words.contains(&"csplit"),
            "expected prose issue, got {words:?}"
        );
        assert!(
            words.contains(&"jsmith"),
            "default cspell markdown path should still check fenced code, got {words:?}"
        );
    }

    #[test]
    fn powershell_markdown_link_patterns_ignore_yaml_link_targets_like_cspell() {
        let settings = CSpellSettings {
            patterns: vec![
                PatternDefinition {
                    name: "markdown-link-inline".into(),
                    pattern: StringOrList::Single(r"/(?<=\])\([^\)]+\)/".into()),
                },
                PatternDefinition {
                    name: "markdown-link-definition".into(),
                    pattern: StringOrList::Single(
                        r"/(?<=\]:\s)(\s*((https?:)?|\/|\.{1,2}))(\/\S+)/".into(),
                    ),
                },
            ],
            ignore_reg_exp_list: vec![
                "markdown-link-inline".into(),
                "markdown-link-definition".into(),
            ],
            ..Default::default()
        };
        let validator = build_validator(
            &settings,
            &[],
            &CheckOptions {
                cspell_compat_mode: true,
                ..CheckOptions::default()
            },
            &HashSet::new(),
            false,
            Some(Path::new("faq.yml")),
            None,
        );

        let issues = validator.validate_text(
            "![PowerShell setup](./media/microsoft-update-faq/ps-msupdate-msi.png)\n\
[about_PSSessions](/powershell/module/microsoft.powershell.core/about/about_pssessions)\n",
        );

        assert!(
            issues.iter().all(|issue| issue.word != "msupdate"),
            "image link target should be ignored: {issues:?}"
        );
        assert!(
            issues.iter().all(|issue| issue.word != "pssessions"),
            "markdown link target should be ignored: {issues:?}"
        );
    }

    #[test]
    fn powershell_docs_markdown_code_blocks_still_report_somevalue_like_cspell() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../vendor/cspell/integration-tests/repositories/temp/MicrosoftDocs/PowerShell-Docs",
        );
        let config = repo.join(".vscode/cspell/psdocs/cspell.yaml");
        let file = repo.join("reference/7.6/CimCmdlets/Set-CimInstance.md");

        let mut settings = resolver::load_config(&config).expect("load powershell docs config");
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            Some(&repo),
            Some(&crate::commands::dict_cache_dir()),
            true,
        )
        .expect("build catalog");

        let crate::commands::check::DictionaryCatalog {
            named_dicts,
            extra_active,
            lang_settings,
            overrides,
        } = catalog;
        let merged_settings = merge_bundled_settings(&settings, &lang_settings, &overrides);
        let options = CheckOptions {
            compound_words_mode: Some(CompoundWordsMode::SeparateWords),
            cspell_compat_mode: true,
            ..CheckOptions::default()
        };
        let validator_config = build_validator_config(
            &merged_settings,
            options.allow_compound_words,
            options.compound_words_mode,
            options.cspell_compat_mode,
            false,
            Some(&file),
        );
        let validator = build_validator(
            &merged_settings,
            &named_dicts,
            &options,
            &extra_active,
            false,
            Some(&file),
            None,
        );

        let content = std::fs::read_to_string(&file).expect("read powershell docs file");
        let issue_lines: Vec<usize> = validator
            .validate_text(&content)
            .into_iter()
            .filter(|issue| issue.word == "somevalue")
            .map(|issue| issue.line)
            .collect();
        let expected_issue_lines = vec![105, 110, 158, 172];

        assert_eq!(
            issue_lines, expected_issue_lines,
            "powershell docs somevalue issues diverged"
        );

        for line_no in [105usize, 172usize] {
            let line = content
                .lines()
                .nth(line_no - 1)
                .expect("missing target line in powershell docs file");
            let col = line
                .find("somevalue")
                .expect("missing somevalue on target line");
            let abs_offset = content
                .lines()
                .take(line_no - 1)
                .map(|line| line.len() + 1)
                .sum::<usize>()
                + col;

            let covering_patterns: Vec<String> = validator_config
                .ignore_patterns
                .iter()
                .filter_map(|re| {
                    re.find_iter(&content)
                        .any(|m| m.start() <= abs_offset && abs_offset < m.end())
                        .then(|| re.as_str().to_string())
                })
                .collect();

            assert!(
                covering_patterns.is_empty(),
                "line {line_no} somevalue should not be ignored, got {covering_patterns:?}"
            );
        }
    }

    #[test]
    fn precompiled_powershell_docs_markdown_keeps_code_block_somevalue_issues() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../vendor/cspell/integration-tests/repositories/temp/MicrosoftDocs/PowerShell-Docs",
        );
        let config = repo.join(".vscode/cspell/psdocs/cspell.yaml");
        let file = repo.join("reference/7.6/CimCmdlets/Set-CimInstance.md");

        let mut settings = resolver::load_config(&config).expect("load powershell docs config");
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            Some(&repo),
            Some(&crate::commands::dict_cache_dir()),
            true,
        )
        .expect("build catalog");

        let crate::commands::check::DictionaryCatalog {
            named_dicts,
            extra_active,
            lang_settings,
            overrides,
        } = catalog;
        let merged_settings = merge_bundled_settings(&settings, &lang_settings, &overrides);
        let options = CheckOptions {
            compound_words_mode: Some(CompoundWordsMode::SeparateWords),
            cspell_compat_mode: true,
            ..CheckOptions::default()
        };
        let context =
            build_root_config_context(&merged_settings, Some(&repo), &options, Some(false));
        let validator = build_validator(
            &merged_settings,
            &named_dicts,
            &options,
            &extra_active,
            false,
            Some(&file),
            Some(context.base_validator_config.as_ref()),
        );

        let content = std::fs::read_to_string(&file).expect("read powershell docs file");
        let issue_lines: Vec<usize> = validator
            .validate_text(&content)
            .into_iter()
            .filter(|issue| issue.word == "somevalue")
            .map(|issue| issue.line)
            .collect();

        assert_eq!(issue_lines, vec![105, 110, 158, 172]);
    }

    #[test]
    fn precompiled_invoke_webrequest_reports_inline_jdoe_png_like_cspell() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../vendor/cspell/integration-tests/repositories/temp/MicrosoftDocs/PowerShell-Docs",
        );
        let config = repo.join(".vscode/cspell/psdocs/cspell.yaml");
        let file = repo.join("reference/7.6/Microsoft.PowerShell.Utility/Invoke-WebRequest.md");

        let mut settings = resolver::load_config(&config).expect("load powershell docs config");
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            Some(&repo),
            Some(&crate::commands::dict_cache_dir()),
            true,
        )
        .expect("build catalog");

        let crate::commands::check::DictionaryCatalog {
            named_dicts,
            extra_active,
            lang_settings,
            overrides,
        } = catalog;
        let merged_settings = merge_bundled_settings(&settings, &lang_settings, &overrides);
        let options = CheckOptions {
            compound_words_mode: Some(CompoundWordsMode::SeparateWords),
            cspell_compat_mode: true,
            ..CheckOptions::default()
        };
        let validator_config = build_validator_config(
            &merged_settings,
            options.allow_compound_words,
            options.compound_words_mode,
            options.cspell_compat_mode,
            false,
            Some(&file),
        );
        let context =
            build_root_config_context(&merged_settings, Some(&repo), &options, Some(false));
        let validator = build_validator(
            &merged_settings,
            &named_dicts,
            &options,
            &extra_active,
            false,
            Some(&file),
            Some(context.base_validator_config.as_ref()),
        );

        let content = std::fs::read_to_string(&file).expect("read invoke webrequest file");
        let issue_lines: Vec<usize> = validator
            .validate_text(&content)
            .into_iter()
            .filter(|issue| issue.word == "jdoe")
            .map(|issue| issue.line)
            .collect();

        let line = content
            .lines()
            .nth(256 - 1)
            .expect("missing jdoe.png line in invoke webrequest file");
        let col = line.find("jdoe").expect("missing jdoe on target line");
        let abs_offset = content
            .lines()
            .take(256 - 1)
            .map(|line| line.len() + 1)
            .sum::<usize>()
            + col;
        let covering_patterns: Vec<String> = validator_config
            .ignore_patterns
            .iter()
            .filter_map(|re| {
                re.find_iter(&content)
                    .any(|m| m.start() <= abs_offset && abs_offset < m.end())
                    .then(|| re.as_str().to_string())
            })
            .collect();
        assert!(
            covering_patterns.is_empty(),
            "line 256 jdoe should not be ignored, got {covering_patterns:?}"
        );

        assert_eq!(issue_lines, vec![151, 256]);
    }

    #[test]
    fn rustpython_python_config_reports_stylized_subwords_like_cspell() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../vendor/cspell/integration-tests/repositories/temp/RustPython/RustPython");
        let config = repo.join(".cspell.json");
        let file = repo.join("extra_tests/snippets/builtin_str_encode.py");

        let mut settings = resolver::load_config(&config).expect("load rustpython config");
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            Some(&repo),
            Some(&crate::commands::dict_cache_dir()),
            true,
        )
        .expect("build catalog");

        let crate::commands::check::DictionaryCatalog {
            named_dicts,
            extra_active,
            lang_settings,
            overrides,
        } = catalog;
        let merged_settings = merge_bundled_settings(&settings, &lang_settings, &overrides);
        let options = CheckOptions {
            compound_words_mode: Some(CompoundWordsMode::SeparateWords),
            cspell_compat_mode: true,
            ..CheckOptions::default()
        };

        let validator = build_validator(
            &merged_settings,
            &named_dicts,
            &options,
            &extra_active,
            false,
            Some(&file),
            None,
        );

        let content = std::fs::read_to_string(&file).expect("read rustpython snippet");
        let issues = validator.validate_text(&content);
        let words: Vec<&str> = issues
            .iter()
            .filter(|issue| matches!(issue.line, 21 | 22))
            .map(|issue| issue.word.as_str())
            .collect();

        assert!(
            words.contains(&"𝕐𝕥"),
            "expected 𝕐𝕥 in issues, got {words:?}"
        );
        assert!(
            words.contains(&"ק𝔂t"),
            "expected ק𝔂t in issues, got {words:?}"
        );

        let context =
            build_root_config_context(&merged_settings, Some(&repo), &options, Some(false));
        let mut precompiled_validator = build_validator(
            &merged_settings,
            &named_dicts,
            &options,
            &extra_active,
            false,
            Some(&file),
            Some(context.base_validator_config.as_ref()),
        );
        precompiled_validator.set_word_cache(Validator::new_word_cache());

        let cached_issues = precompiled_validator.validate_text(&content);
        let cached_words: Vec<&str> = cached_issues
            .iter()
            .filter(|issue| matches!(issue.line, 21 | 22))
            .map(|issue| issue.word.as_str())
            .collect();

        assert_eq!(cached_words, words, "precompiled path diverged");
    }

    #[test]
    fn collect_issues_applies_nested_local_config() {
        let dir = tempfile::tempdir().unwrap();
        let docs_dir = dir.path().join("docs");
        std::fs::create_dir_all(&docs_dir).unwrap();

        let file = docs_dir.join("README.md");
        std::fs::write(&file, "azsdk\n").unwrap();
        std::fs::write(
            docs_dir.join("cspell.yaml"),
            "version: '0.2'\nwords:\n  - azsdk\n",
        )
        .unwrap();

        let catalog = DictionaryCatalog {
            named_dicts: Vec::new(),
            extra_active: HashSet::new(),
            lang_settings: Vec::new(),
            overrides: Vec::new(),
        };

        let results = collect_all_issues(
            std::slice::from_ref(&file),
            &CSpellSettings::default(),
            Some(dir.path()),
            catalog,
            CheckOptions {
                config_search: true,
                per_dir_config_search: true,
                no_must_find_files: true,
                ..CheckOptions::default()
            },
        )
        .unwrap();

        assert!(
            results.is_empty(),
            "nested cspell.yaml words should suppress issues, got: {results:?}"
        );
    }

    #[test]
    fn collect_issues_applies_nested_local_config_override_with_repo_relative_glob() {
        let dir = tempfile::tempdir().unwrap();
        let keyvault_dir = dir.path().join("specification/keyvault");
        let file_dir = keyvault_dir.join("Security.KeyVault.Administration");
        std::fs::create_dir_all(&file_dir).unwrap();

        let file = file_dir.join("README.md");
        std::fs::write(&file, "renamings\n").unwrap();
        std::fs::write(
            keyvault_dir.join("cspell.yaml"),
            concat!(
                "version: '0.2'\n",
                "overrides:\n",
                "  - filename: '**/specification/keyvault/Security.KeyVault.Administration/README.md'\n",
                "    words:\n",
                "      - renamings\n",
            ),
        )
        .unwrap();

        let catalog = DictionaryCatalog {
            named_dicts: Vec::new(),
            extra_active: HashSet::new(),
            lang_settings: Vec::new(),
            overrides: Vec::new(),
        };

        let results = collect_all_issues(
            std::slice::from_ref(&file),
            &CSpellSettings::default(),
            Some(dir.path()),
            catalog,
            CheckOptions {
                config_search: true,
                per_dir_config_search: true,
                no_must_find_files: true,
                ..CheckOptions::default()
            },
        )
        .unwrap();

        assert!(
            results.is_empty(),
            "nested override from local cspell.yaml should suppress issues, got: {results:?}"
        );
    }

    #[test]
    fn collect_issues_applies_nested_local_config_override_from_glob_expanded_paths() {
        let dir = tempfile::tempdir().unwrap();
        let root_config = dir.path().join("cspell.json");
        let keyvault_dir = dir.path().join("specification/keyvault");
        let file_dir = keyvault_dir.join("Security.KeyVault.Administration");
        std::fs::create_dir_all(&file_dir).unwrap();

        let file = file_dir.join("README.md");
        std::fs::write(&file, "renamings\n").unwrap();
        std::fs::write(&root_config, "{ \"version\": \"0.2\" }\n").unwrap();
        std::fs::write(
            keyvault_dir.join("cspell.yaml"),
            concat!(
                "version: '0.2'\n",
                "import:\n",
                "  - ../../cspell.json\n",
                "overrides:\n",
                "  - filename: '**/specification/keyvault/Security.KeyVault.Administration/README.md'\n",
                "    words:\n",
                "      - renamings\n",
            ),
        )
        .unwrap();

        let settings = resolver::load_config(&root_config).unwrap();
        let catalog = DictionaryCatalog {
            named_dicts: Vec::new(),
            extra_active: HashSet::new(),
            lang_settings: Vec::new(),
            overrides: Vec::new(),
        };

        let results = collect_all_issues(
            &[PathBuf::from(
                "**/Security.KeyVault.Administration/README.md",
            )],
            &settings,
            Some(dir.path()),
            catalog,
            CheckOptions {
                config_search: true,
                per_dir_config_search: true,
                no_must_find_files: true,
                cwd: Some(dir.path().to_path_buf()),
                ..CheckOptions::default()
            },
        )
        .unwrap();

        assert!(
            results.is_empty(),
            "glob-expanded file paths should still use nested local overrides, got: {results:?}"
        );
    }

    #[test]
    fn azure_rest_api_specs_markdown_yaml_keeps_allow_compound_words() {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let repo = workspace_root
            .join("vendor/cspell/integration-tests/repositories/temp/Azure/azure-rest-api-specs");
        if !repo.exists() {
            panic!(
                "expected azure-rest-api-specs fixture at {}",
                repo.display()
            );
        }

        let resolved =
            crate::commands::setup::resolve_config(None, Some(repo.as_path()), true, &[])
                .expect("resolve config");

        let mut settings = resolved.settings;
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            resolved.config_dir.as_deref(),
            Some(&crate::commands::dict_cache_dir()),
            resolved.is_cspell,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);

        let (_, en_us) = catalog
            .named_dicts
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("en_us"))
            .expect("missing en_us dictionary");
        assert!(en_us.has("multi"), "catalog en_us should contain multi");
        assert!(en_us.has("api"), "catalog en_us should contain api");

        let file = repo.join("documentation/code-gen/configure-python-sdk.md");
        let options = CheckOptions {
            cspell_compat_mode: true,
            compound_words_mode: Some(CompoundWordsMode::SeparateWords),
            ..CheckOptions::default()
        };

        let validator = build_validator(
            &merged_settings,
            &catalog.named_dicts,
            &options,
            &catalog.extra_active,
            false,
            Some(&file),
            None,
        );
        let content = std::fs::read_to_string(&file).expect("read azure markdown");
        let issues = validator.validate_text(&content);
        let words: Vec<&str> = issues
            .iter()
            .filter(|issue| matches!(issue.line, 143 | 147 | 150))
            .map(|issue| issue.word.as_str())
            .collect();

        assert!(
            !words.contains(&"multiapi") && !words.contains(&"multiapiscript"),
            "runtime validator should not flag allowCompoundWords examples, got {words:?}"
        );

        let context =
            build_root_config_context(&merged_settings, Some(&repo), &options, Some(false));
        let precompiled_validator = build_validator(
            &merged_settings,
            &catalog.named_dicts,
            &options,
            &catalog.extra_active,
            false,
            Some(&file),
            Some(context.base_validator_config.as_ref()),
        );
        let cached_issues = precompiled_validator.validate_text(&content);
        let cached_words: Vec<&str> = cached_issues
            .iter()
            .filter(|issue| matches!(issue.line, 143 | 147 | 150))
            .map(|issue| issue.word.as_str())
            .collect();

        assert_eq!(
            cached_words, words,
            "precompiled validator diverged for azure markdown yaml block"
        );
    }

    #[test]
    fn azure_documentation_local_config_keeps_allow_compound_words() {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let repo = workspace_root
            .join("vendor/cspell/integration-tests/repositories/temp/Azure/azure-rest-api-specs");
        if !repo.exists() {
            panic!(
                "expected azure-rest-api-specs fixture at {}",
                repo.display()
            );
        }

        let resolved =
            crate::commands::setup::resolve_config(None, Some(repo.as_path()), true, &[])
                .expect("resolve config");

        let mut settings = resolved.settings;
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            resolved.config_dir.as_deref(),
            Some(&crate::commands::dict_cache_dir()),
            resolved.is_cspell,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);

        let file = repo.join("documentation/code-gen/configure-python-sdk.md");
        let options = CheckOptions {
            cspell_compat_mode: true,
            config_search: true,
            per_dir_config_search: true,
            compound_words_mode: Some(CompoundWordsMode::SeparateWords),
            config_file: resolved.config_file.clone(),
            ..CheckOptions::default()
        };

        let root_context = build_root_config_context(
            &merged_settings,
            resolved.config_dir.as_deref(),
            &options,
            Some(false),
        );
        let per_dir_contexts = build_per_dir_config_contexts(
            std::slice::from_ref(&file),
            options.config_file.as_deref(),
            &merged_settings,
            &options,
            Some(false),
        );
        let context = file
            .parent()
            .and_then(|dir| config_search::find_nearest_dir_value(dir, &per_dir_contexts))
            .cloned()
            .unwrap_or_else(|| root_context.clone());

        assert!(
            context.cache_key.ends_with("documentation/cspell.yaml"),
            "expected documentation local config, got {}",
            context.cache_key
        );
        assert_eq!(context.settings.allow_compound_words, Some(true));
        assert!(
            context.base_validator_config.allow_compound_words,
            "local context validator should keep allowCompoundWords enabled"
        );
        assert_eq!(
            context.base_validator_config.compound_words_mode,
            CompoundWordsMode::SeparateWords
        );

        let validator = build_validator(
            context.settings.as_ref(),
            &catalog.named_dicts,
            &options,
            &catalog.extra_active,
            false,
            Some(&file),
            Some(context.base_validator_config.as_ref()),
        );
        let content = std::fs::read_to_string(&file).expect("read azure markdown");
        let issues = validator.validate_text(&content);
        let words: Vec<&str> = issues
            .iter()
            .filter(|issue| matches!(issue.line, 143 | 147 | 150))
            .map(|issue| issue.word.as_str())
            .collect();

        assert!(
            !words.contains(&"multiapi") && !words.contains(&"multiapiscript"),
            "documentation local config path should not flag allowCompoundWords examples, got {words:?}"
        );
    }

    #[test]
    fn flutter_kotlin_file_does_not_activate_fullstack_and_reports_splashscreen() {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let repo = workspace_root
            .join("vendor/cspell/integration-tests/repositories/temp/flutter/samples");
        if !repo.exists() {
            panic!("expected flutter fixture at {}", repo.display());
        }

        let resolved =
            crate::commands::setup::resolve_config(None, Some(repo.as_path()), true, &[])
                .expect("resolve config");

        let mut settings = resolved.settings;
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            resolved.config_dir.as_deref(),
            Some(&crate::commands::dict_cache_dir()),
            resolved.is_cspell,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);

        let file = repo.join(
            "android_splash_screen/android/app/src/main/kotlin/com/example/splash_screen_sample/MainActivity.kt",
        );
        let options = CheckOptions {
            cspell_compat_mode: true,
            config_search: true,
            per_dir_config_search: true,
            compound_words_mode: Some(CompoundWordsMode::SeparateWords),
            config_file: resolved.config_file.clone(),
            ..CheckOptions::default()
        };

        let root_context = build_root_config_context(
            &merged_settings,
            resolved.config_dir.as_deref(),
            &options,
            Some(false),
        );
        let per_dir_contexts = build_per_dir_config_contexts(
            std::slice::from_ref(&file),
            options.config_file.as_deref(),
            &merged_settings,
            &options,
            Some(false),
        );
        let context = file
            .parent()
            .and_then(|dir| config_search::find_nearest_dir_value(dir, &per_dir_contexts))
            .cloned()
            .unwrap_or_else(|| root_context.clone());
        let template = prepare_validator_template(
            context.settings.as_ref(),
            &options,
            &catalog.extra_active,
            false,
            Some(&file),
            Some(context.base_validator_config.as_ref()),
        );

        let requested = template
            .requested
            .as_ref()
            .expect("expected active dictionaries for flutter kotlin file");
        assert!(
            requested.contains("kotlin"),
            "expected kotlin dictionary to be active, got {requested:?}"
        );
        assert!(
            !requested.contains("fullstack"),
            "fullstack should not be active for kotlin, got {requested:?}"
        );

        let validator = instantiate_validator(&template, &catalog.named_dicts, &options);
        let active_hits: Vec<String> = catalog
            .named_dicts
            .iter()
            .filter(|(name, _)| requested.contains(name))
            .filter_map(|(name, dict)| {
                let direct = dict.has_pre_normalized_direct_only("splashscreen", "splashscreen");
                let full = dict.has_pre_normalized("splashscreen", "splashscreen");
                (direct || full).then(|| format!("{name}: direct={direct} full={full}"))
            })
            .collect();
        let content = std::fs::read_to_string(&file).expect("read flutter kotlin");
        let issues = validator.validate_text(&content);
        let words: Vec<&str> = issues
            .iter()
            .filter(|issue| matches!(issue.line, 32 | 33 | 98 | 111 | 121 | 206 | 207))
            .map(|issue| issue.word.as_str())
            .collect();

        assert!(
            words.contains(&"splashscreen") && words.contains(&"SPLASHSCREEN"),
            "expected flutter kotlin config path to report splashscreen words, got {words:?}; active_hits={active_hits:?}; requested={requested:?}"
        );
    }

    fn flutter_runtime_validator_for(
        relative_file: &str,
    ) -> (
        PathBuf,
        Arc<ConfigContext>,
        ValidatorTemplate,
        crate::commands::check::DictionaryCatalog,
        CheckOptions,
    ) {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let repo = workspace_root
            .join("vendor/cspell/integration-tests/repositories/temp/flutter/samples");
        if !repo.exists() {
            panic!("expected flutter fixture at {}", repo.display());
        }

        let resolved =
            crate::commands::setup::resolve_config(None, Some(repo.as_path()), true, &[])
                .expect("resolve config");

        let mut settings = resolved.settings;
        settings.language = Some("en,en-GB,lorem".into());
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            resolved.config_dir.as_deref(),
            Some(&crate::commands::dict_cache_dir()),
            resolved.is_cspell,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);

        let file = repo.join(relative_file);
        let options = CheckOptions {
            cspell_compat_mode: true,
            config_search: true,
            per_dir_config_search: true,
            compound_words_mode: Some(CompoundWordsMode::SeparateWords),
            config_file: resolved.config_file.clone(),
            cwd: Some(repo.clone()),
            ..CheckOptions::default()
        };

        let root_context = build_root_config_context(
            &merged_settings,
            resolved.config_dir.as_deref(),
            &options,
            Some(false),
        );
        let per_dir_contexts = build_per_dir_config_contexts(
            std::slice::from_ref(&file),
            options.config_file.as_deref(),
            &merged_settings,
            &options,
            Some(false),
        );
        let context = file
            .parent()
            .and_then(|dir| config_search::find_nearest_dir_value(dir, &per_dir_contexts))
            .cloned()
            .unwrap_or_else(|| root_context.clone());
        let template = prepare_validator_template(
            context.settings.as_ref(),
            &options,
            &catalog.extra_active,
            false,
            Some(&file),
            Some(context.base_validator_config.as_ref()),
        );

        (file, context, template, catalog, options)
    }

    #[test]
    fn flutter_runtime_accepts_livedata_in_gradle() {
        let (file, _context, template, catalog, options) =
            flutter_runtime_validator_for("add_to_app/android_view/android_view/app/build.gradle");
        let requested = template
            .requested
            .as_ref()
            .expect("expected active dictionaries for flutter gradle file");
        let validator = instantiate_validator(&template, &catalog.named_dicts, &options);
        let content = std::fs::read_to_string(&file).expect("read flutter gradle");
        let issues = validator.validate_text(&content);
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(
            !words.contains(&"livedata"),
            "expected flutter runtime config to accept livedata, got requested={requested:?} words={words:?}"
        );
    }

    #[test]
    fn flutter_runtime_accepts_apientry_in_cpp() {
        let (file, _context, template, catalog, options) =
            flutter_runtime_validator_for("animations/windows/runner/main.cpp");
        let requested = template
            .requested
            .as_ref()
            .expect("expected active dictionaries for flutter cpp file");
        let validator = instantiate_validator(&template, &catalog.named_dicts, &options);
        let content = std::fs::read_to_string(&file).expect("read flutter cpp");
        let issues = validator.validate_text(&content);
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(
            !words.contains(&"APIENTRY"),
            "expected flutter runtime config to accept APIENTRY, got requested={requested:?} words={words:?}"
        );
    }

    #[test]
    fn collect_issues_applies_nested_local_override_with_duplicate_root_import_chain() {
        let dir = tempfile::tempdir().unwrap();
        let root_json = dir.path().join("cspell.json");
        let root_yaml = dir.path().join("cspell.yaml");
        let keyvault_dir = dir.path().join("specification/keyvault");
        let file_dir = keyvault_dir.join("Security.KeyVault.Administration");
        std::fs::create_dir_all(&file_dir).unwrap();

        let file = file_dir.join("README.md");
        std::fs::write(&file, "renamings\n").unwrap();
        std::fs::write(
            &root_json,
            "{ \"version\": \"0.2\", \"import\": [\"cspell.yaml\"] }\n",
        )
        .unwrap();
        std::fs::write(
            &root_yaml,
            concat!(
                "version: '0.2'\n",
                "language: en\n",
                "overrides:\n",
                "  - filename: '/README.md'\n",
                "    words:\n",
                "      - azsdk\n",
            ),
        )
        .unwrap();
        std::fs::write(
            keyvault_dir.join("cspell.yaml"),
            concat!(
                "version: '0.2'\n",
                "import:\n",
                "  - ../../cspell.yaml\n",
                "overrides:\n",
                "  - filename: '**/specification/keyvault/Security.KeyVault.Administration/README.md'\n",
                "    words:\n",
                "      - renamings\n",
            ),
        )
        .unwrap();

        let settings = resolver::load_config(&root_json).unwrap();
        let catalog = DictionaryCatalog {
            named_dicts: Vec::new(),
            extra_active: HashSet::new(),
            lang_settings: Vec::new(),
            overrides: Vec::new(),
        };

        let results = collect_all_issues(
            &[PathBuf::from(
                "**/Security.KeyVault.Administration/README.md",
            )],
            &settings,
            Some(dir.path()),
            catalog,
            CheckOptions {
                config_search: true,
                per_dir_config_search: true,
                no_must_find_files: true,
                cwd: Some(dir.path().to_path_buf()),
                ..CheckOptions::default()
            },
        )
        .unwrap();

        assert!(
            results.is_empty(),
            "duplicate root imports should not break nested overrides, got: {results:?}"
        );
    }

    #[test]
    fn read_file_mmap_decodes_invalid_utf8_lossily_like_cspell() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("invalid.bin");
        let bytes = [0xDA, 0xC4, 0xC4, b'\n'];
        std::fs::write(&file, bytes).unwrap();

        let text = read_file_mmap(&file).unwrap();

        assert_eq!(text, String::from_utf8_lossy(&bytes));
        assert_ne!(text, "\u{00DA}\u{00C4}\u{00C4}\n");
    }

    #[test]
    fn read_file_mmap_detects_utf16le_without_bom_like_cspell() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("utf16le.txt");
        std::fs::write(&file, [b'h', 0, b'i', 0, b'\n', 0]).unwrap();

        let text = read_file_mmap(&file).unwrap();

        assert_eq!(text, "hi\n");
    }

    #[test]
    fn read_file_mmap_skips_unknown_binary_with_nul_prefix_like_cspell() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("texture.ktx");
        std::fs::write(&file, [0xAB, 0x4B, 0, 0x54, 0x58, 0x20]).unwrap();

        let text = read_file_mmap(&file);

        assert!(text.is_none());
    }

    #[test]
    fn read_file_mmap_keeps_unknown_utf16le_content() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("document.ogex");
        std::fs::write(&file, [b'h', 0, b'i', 0, b'\n', 0]).unwrap();

        let text = read_file_mmap(&file).unwrap();

        assert_eq!(text, "hi\n");
    }

    #[test]
    fn collect_files_sorts_in_cspell_compat_mode() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("b.md"), "b\n").unwrap();
        std::fs::write(dir.path().join("a.md"), "a\n").unwrap();

        let files = collect_files(
            &[PathBuf::from(".")],
            &CSpellSettings::default(),
            &CheckOptions {
                cspell_compat_mode: true,
                use_gitignore: Some(false),
                no_must_find_files: true,
                cwd: Some(dir.path().to_path_buf()),
                ..CheckOptions::default()
            },
        )
        .unwrap();

        let names: Vec<_> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        assert_eq!(names, vec!["a.md", "b.md"]);
    }

    #[test]
    fn collect_files_excludes_config_file_via_lexical_relative_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("cspell.json"), "{ \"version\": \"0.2\" }\n").unwrap();
        std::fs::write(dir.path().join("notes.md"), "hello\n").unwrap();

        let files = collect_files(
            &[PathBuf::from(".")],
            &CSpellSettings::default(),
            &CheckOptions {
                cspell_compat_mode: true,
                use_gitignore: Some(false),
                config_file: Some(PathBuf::from("configs/../cspell.json")),
                no_must_find_files: true,
                cwd: Some(dir.path().to_path_buf()),
                ..CheckOptions::default()
            },
        )
        .unwrap();

        let names: Vec<_> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        assert_eq!(names, vec!["notes.md"]);
    }

    #[test]
    fn collect_files_cspell_compat_gitignore_honors_same_file_negation() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "*.md\n!keep.md\n").unwrap();
        std::fs::write(dir.path().join("keep.md"), "keep\n").unwrap();
        std::fs::write(dir.path().join("drop.md"), "drop\n").unwrap();

        let files = collect_files(
            &[PathBuf::from(".")],
            &CSpellSettings::default(),
            &CheckOptions {
                cspell_compat_mode: true,
                use_gitignore: Some(true),
                gitignore_root: Some(PathBuf::from(".")),
                no_must_find_files: true,
                cwd: Some(dir.path().to_path_buf()),
                ..CheckOptions::default()
            },
        )
        .unwrap();

        let names: Vec<_> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        assert_eq!(names, vec!["keep.md"]);
    }

    #[test]
    fn collect_files_cspell_compat_gitignore_child_negation_does_not_override_parent_ignore() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("docs");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(dir.path().join(".gitignore"), "*.md\n").unwrap();
        std::fs::write(child.join(".gitignore"), "!keep.md\n").unwrap();
        std::fs::write(child.join("keep.md"), "keep\n").unwrap();
        std::fs::write(child.join("keep.txt"), "keep\n").unwrap();

        let files = collect_files(
            &[PathBuf::from(".")],
            &CSpellSettings::default(),
            &CheckOptions {
                cspell_compat_mode: true,
                use_gitignore: Some(true),
                gitignore_root: Some(PathBuf::from(".")),
                no_must_find_files: true,
                cwd: Some(dir.path().to_path_buf()),
                ..CheckOptions::default()
            },
        )
        .unwrap();

        let root = dir.path().canonicalize().unwrap();
        let rels: Vec<_> = files
            .iter()
            .map(|p| {
                p.canonicalize()
                    .unwrap_or_else(|_| p.to_path_buf())
                    .strip_prefix(&root)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();

        assert_eq!(rels, vec!["docs/keep.txt"]);
    }

    #[test]
    fn collect_files_respects_ignore_paths_relative_to_parent_config_root() {
        let dir = tempfile::tempdir().unwrap();
        let repositories = dir.path().join("repositories");
        let repo = repositories.join("temp/gitbucket/gitbucket");
        let assets = repo.join("src/main/webapp/assets/common/js");
        let src = repo.join("src/main/scala");
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::create_dir_all(&src).unwrap();

        std::fs::write(
            repositories.join("cspell.yaml"),
            concat!(
                "version: '0.2'\n",
                "ignorePaths:\n",
                "  - '**/gitbucket/**/webapp/assets/**'\n",
            ),
        )
        .unwrap();
        std::fs::write(assets.join("gitbucket.js"), "jsoniq\n").unwrap();
        std::fs::write(src.join("App.scala"), "hello\n").unwrap();

        let settings = resolver::load_config(&repositories.join("cspell.yaml")).unwrap();
        assert!(
            !settings.resolved_ignore_paths.is_empty(),
            "expected resolved ignore paths to be hydrated from config root"
        );

        let files = collect_files(
            &[PathBuf::from("**")],
            &settings,
            &CheckOptions {
                cspell_compat_mode: true,
                use_gitignore: Some(false),
                no_must_find_files: true,
                cwd: Some(repo.clone()),
                ..CheckOptions::default()
            },
        )
        .unwrap();

        let rels: Vec<_> = files
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();

        assert!(
            rels.iter()
                .any(|p| p.ends_with("/src/main/scala/App.scala")),
            "expected normal source file to remain, got {rels:?}"
        );
        assert!(
            rels.iter().all(|p| !p.contains("/src/main/webapp/assets/")),
            "expected assets path to be excluded by parent config root, got {rels:?}"
        );
    }

    #[test]
    fn collect_files_respects_ignore_paths_loaded_via_imported_root_config() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let eng = repo.join("eng/common");
        let docs = repo.join("profiles");
        std::fs::create_dir_all(&eng).unwrap();
        std::fs::create_dir_all(&docs).unwrap();

        std::fs::write(
            repo.join("cspell.json"),
            "{ \"version\": \"0.2\", \"import\": [\"cspell.yaml\"] }\n",
        )
        .unwrap();
        std::fs::write(
            repo.join("cspell.yaml"),
            concat!("version: '0.2'\n", "ignorePaths:\n", "  - eng/**\n",),
        )
        .unwrap();
        std::fs::write(eng.join("README.md"), "azsdk\n").unwrap();
        std::fs::write(docs.join("README.md"), "valid\n").unwrap();

        let settings = resolver::load_config(&repo.join("cspell.json")).unwrap();
        assert!(
            !settings.resolved_ignore_paths.is_empty(),
            "expected imported ignore paths to be hydrated"
        );

        let files = collect_files(
            &[PathBuf::from("**/*.{md,ts,js}")],
            &settings,
            &CheckOptions {
                cspell_compat_mode: true,
                use_gitignore: Some(false),
                no_must_find_files: true,
                cwd: Some(repo.to_path_buf()),
                ..CheckOptions::default()
            },
        )
        .unwrap();

        let repo = repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf());
        let rels: Vec<_> = files
            .iter()
            .map(|p| {
                p.canonicalize()
                    .unwrap_or_else(|_| p.to_path_buf())
                    .strip_prefix(&repo)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();

        assert!(
            rels.iter().any(|p| p == "profiles/README.md"),
            "expected non-ignored file to remain, got {rels:?}"
        );
        assert!(
            rels.iter().all(|p| !p.starts_with("eng/")),
            "expected eng/** from imported config to be excluded, got {rels:?}"
        );
    }

    #[test]
    fn collect_files_excludes_eng_paths_for_real_azure_rest_api_specs_fixture() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let repo = workspace_root
            .join("vendor/cspell/integration-tests/repositories/temp/Azure/azure-rest-api-specs");
        let config = repo.join("cspell.json");
        if !config.exists() {
            panic!(
                "expected azure-rest-api-specs fixture at {}",
                config.display()
            );
        }

        let settings = resolver::load_config(&config).unwrap();

        let files = collect_files(
            &[PathBuf::from("**/*.{md,ts,js}")],
            &settings,
            &CheckOptions {
                cspell_compat_mode: true,
                use_gitignore: Some(true),
                gitignore_root: Some(PathBuf::from(".")),
                no_must_find_files: true,
                cwd: Some(repo.clone()),
                ..CheckOptions::default()
            },
        )
        .unwrap();

        let repo = repo.canonicalize().unwrap_or(repo);
        let rels: Vec<_> = files
            .iter()
            .map(|p| {
                p.canonicalize()
                    .unwrap_or_else(|_| p.to_path_buf())
                    .strip_prefix(&repo)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();

        assert!(
            rels.iter().all(|p| !p.starts_with("eng/")),
            "expected root ignorePaths to exclude eng/**, got first matches: {:?}",
            rels.iter()
                .filter(|p| p.starts_with("eng/"))
                .take(20)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn collect_all_issues_skips_eng_glob_for_real_azure_rest_api_specs_fixture() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let repo = workspace_root
            .join("vendor/cspell/integration-tests/repositories/temp/Azure/azure-rest-api-specs");
        let config = repo.join("cspell.json");
        if !config.exists() {
            panic!(
                "expected azure-rest-api-specs fixture at {}",
                config.display()
            );
        }

        let mut settings = resolver::load_config(&config).unwrap();
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let results = collect_all_issues(
            &[PathBuf::from("eng/**/*.{md,ts,js}")],
            &settings,
            Some(&repo),
            DictionaryCatalog {
                named_dicts: Vec::new(),
                extra_active: HashSet::new(),
                lang_settings: Vec::new(),
                overrides: Vec::new(),
            },
            CheckOptions {
                cspell_compat_mode: true,
                config_search: true,
                per_dir_config_search: true,
                use_gitignore: Some(true),
                gitignore_root: Some(PathBuf::from(".")),
                no_must_find_files: true,
                cwd: Some(repo.clone()),
                ..CheckOptions::default()
            },
        )
        .unwrap();

        assert!(
            results.is_empty(),
            "expected eng/**/* to be ignored by root ignorePaths, got files: {:?}",
            results
                .iter()
                .map(|(path, _, _)| path.to_string_lossy().to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn collect_all_issues_skips_eng_glob_for_real_azure_fixture_via_setup_resolve_config() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let repo = workspace_root
            .join("vendor/cspell/integration-tests/repositories/temp/Azure/azure-rest-api-specs");
        let config = repo.join("cspell.json");
        if !config.exists() {
            panic!(
                "expected azure-rest-api-specs fixture at {}",
                config.display()
            );
        }

        let resolved = crate::commands::setup::resolve_config(
            Some(config.as_path()),
            Some(repo.as_path()),
            true,
            &[],
        )
        .unwrap();

        let mut settings = resolved.settings;
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let results = collect_all_issues(
            &[PathBuf::from("eng/**/*.{md,ts,js}")],
            &settings,
            resolved.config_dir.as_deref(),
            DictionaryCatalog {
                named_dicts: Vec::new(),
                extra_active: HashSet::new(),
                lang_settings: Vec::new(),
                overrides: Vec::new(),
            },
            CheckOptions {
                cspell_compat_mode: true,
                config_search: true,
                per_dir_config_search: true,
                use_gitignore: Some(true),
                gitignore_root: Some(PathBuf::from(".")),
                config_file: resolved.config_file,
                no_must_find_files: true,
                cwd: Some(repo.clone()),
                ..CheckOptions::default()
            },
        )
        .unwrap();

        assert!(
            results.is_empty(),
            "expected eng/**/* to be ignored via setup::resolve_config, got files: {:?}",
            results
                .iter()
                .map(|(path, _, _)| path.to_string_lossy().to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn collect_all_issues_uses_vendor_common_local_ignore_paths_outside_explicit_config_dir() {
        let dir = tempfile::tempdir().unwrap();
        let repositories = dir.path().join("repositories");
        let repo = repositories.join("temp/microsoft/TypeScript-Website");
        let repo_config_dir = dir
            .path()
            .join("config/repositories/microsoft/TypeScript-Website");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&repo_config_dir).unwrap();

        std::fs::write(
            repositories.join("cspell.yaml"),
            "version: '0.2'\nignorePaths:\n  - pnpm-lock.yaml\n",
        )
        .unwrap();
        let repo_config = repo_config_dir.join("cspell.json");
        std::fs::write(&repo_config, "{ \"version\": \"0.2\" }\n").unwrap();
        std::fs::write(repo.join("pnpm-lock.yaml"), "estree\n").unwrap();

        let mut settings = resolver::load_config(&repo_config).unwrap();
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let results = collect_all_issues(
            &[PathBuf::from("pnpm-lock.yaml")],
            &settings,
            Some(&repo_config_dir),
            DictionaryCatalog {
                named_dicts: Vec::new(),
                extra_active: HashSet::new(),
                lang_settings: Vec::new(),
                overrides: Vec::new(),
            },
            CheckOptions {
                cspell_compat_mode: true,
                config_search: true,
                per_dir_config_search: true,
                config_file: Some(repo_config),
                no_must_find_files: true,
                cwd: Some(repo.clone()),
                ..CheckOptions::default()
            },
        )
        .unwrap();

        assert!(
            results.is_empty(),
            "expected repositories/cspell.yaml local ignorePaths to skip pnpm-lock.yaml, got: {results:?}"
        );
    }

    #[test]
    fn cspell_compat_sort_matches_collator_case_insensitive_order() {
        let mut files = vec![
            PathBuf::from("ATTIC/src/modules/README.md"),
            PathBuf::from("ATTIC/src/modules/adventure/infoint/control.c"),
            PathBuf::from("ATTIC/src/modules/gallups/gallups.c"),
            PathBuf::from("ATTIC/src/modules/gallups-old/gallups.c"),
        ];

        files.sort_unstable_by(|a, b| cspell_path_cmp(a, b));

        let names: Vec<_> = files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        assert_eq!(
            names,
            vec![
                "ATTIC/src/modules/adventure/infoint/control.c",
                "ATTIC/src/modules/gallups-old/gallups.c",
                "ATTIC/src/modules/gallups/gallups.c",
                "ATTIC/src/modules/README.md",
            ]
        );
    }

    #[test]
    fn cspell_path_cmp_prefers_lowercase_when_only_case_differs() {
        assert_eq!(cspell_path_str_cmp("a", "A"), std::cmp::Ordering::Less);
        assert_eq!(
            cspell_path_str_cmp("README.md", "readme.md"),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn cspell_path_cmp_matches_punctuation_priority_used_by_collator() {
        let mut files = vec![
            PathBuf::from("ATTIC/src/modules/emailclubs/bbsmail/bbsmail.c"),
            PathBuf::from("ATTIC/src/modules/emailclubs/bbsmail/bbsmail.h"),
            PathBuf::from("ATTIC/src/modules/emailclubs/bbsmail/bbsmail_run.c"),
            PathBuf::from("ATTIC/src/system/daemons/rpc.metabbs/register.c"),
            PathBuf::from("ATTIC/src/system/daemons/rpc.metabbs/register_non_megistos.c"),
        ];

        files.sort_unstable_by(|a, b| cspell_path_cmp(a, b));

        let names: Vec<_> = files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        assert_eq!(
            names,
            vec![
                "ATTIC/src/modules/emailclubs/bbsmail/bbsmail_run.c",
                "ATTIC/src/modules/emailclubs/bbsmail/bbsmail.c",
                "ATTIC/src/modules/emailclubs/bbsmail/bbsmail.h",
                "ATTIC/src/system/daemons/rpc.metabbs/register_non_megistos.c",
                "ATTIC/src/system/daemons/rpc.metabbs/register.c",
            ]
        );
    }

    #[test]
    fn should_compute_suggestions_only_when_explicitly_requested() {
        assert!(!should_compute_suggestions(None));
        assert!(!should_compute_suggestions(Some(false)));
        assert!(should_compute_suggestions(Some(true)));
    }

    fn make_issue(word: &str, offset: usize, line: usize) -> ValidationIssue {
        ValidationIssue {
            word: word.into(),
            offset,
            line,
            column: 1,
            suggestions: vec![],
            is_forbidden: false,
            is_known_typo: false,
        }
    }

    #[test]
    fn total_issue_limit_under() {
        let issues = vec![
            make_issue("foo", 0, 1),
            make_issue("bar", 10, 2),
            make_issue("baz", 20, 3),
        ];

        // Under the default limit, all pass through
        let filtered = apply_issue_limits(issues, None);
        assert_eq!(filtered.len(), 3);
    }

    #[test]
    fn total_issue_limit() {
        let mut issues = Vec::new();
        for i in 0..20 {
            issues.push(make_issue(&format!("word{}", i), i * 10, i + 1));
        }

        // Limit total issues to 10
        let filtered = apply_issue_limits(issues, Some(10));
        assert_eq!(filtered.len(), 10);
    }

    #[test]
    fn collect_files_keeps_real_licia_explicit_changelog() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let repo =
            workspace_root.join("vendor/cspell/integration-tests/repositories/temp/liriliri/licia");
        let config = repo.join("cspell.json");
        if !config.exists() {
            panic!("expected licia fixture at {}", config.display());
        }

        let resolved = crate::commands::setup::resolve_config(
            Some(config.as_path()),
            Some(repo.as_path()),
            true,
            &[],
        )
        .unwrap();

        let mut settings = resolved.settings;
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            resolved.config_dir.as_deref(),
            Some(&crate::commands::dict_cache_dir()),
            resolved.is_cspell,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);
        let files = collect_files(
            &[PathBuf::from("CHANGELOG.md")],
            &merged_settings,
            &CheckOptions {
                cspell_compat_mode: true,
                config_search: true,
                per_dir_config_search: true,
                config_file: resolved.config_file.clone(),
                no_must_find_files: true,
                cwd: Some(repo.clone()),
                ..CheckOptions::default()
            },
        )
        .expect("collect files");

        let rels: Vec<String> = files
            .iter()
            .map(|f| {
                f.strip_prefix(&repo)
                    .unwrap_or(f)
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert!(
            rels.iter().any(|rel| rel == "CHANGELOG.md"),
            "expected explicit CHANGELOG.md to survive bundled defaults, got {rels:?}"
        );
    }

    #[test]
    fn exclude_glob_single_star_does_not_cross_directories() {
        let filter = build_ignore_filter(&["pkg/*/test/**".to_string()]).expect("filter");

        assert!(filter.is_ignored(Path::new("pkg/foo/test/bar.dart")));
        assert!(!filter.is_ignored(Path::new(
            "pkg/compiler/tool/kernel_visitor/test/test_classes.dart"
        )));
    }

    #[test]
    fn collect_all_issues_reports_real_licia_changelog_unenumerable() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let repo =
            workspace_root.join("vendor/cspell/integration-tests/repositories/temp/liriliri/licia");
        let config = repo.join("cspell.json");
        if !config.exists() {
            panic!("expected licia fixture at {}", config.display());
        }

        let resolved = crate::commands::setup::resolve_config(
            Some(config.as_path()),
            Some(repo.as_path()),
            true,
            &[],
        )
        .unwrap();

        let mut settings = resolved.settings;
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            resolved.config_dir.as_deref(),
            Some(&crate::commands::dict_cache_dir()),
            resolved.is_cspell,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);
        let results = collect_all_issues(
            &[PathBuf::from("CHANGELOG.md")],
            &merged_settings,
            resolved.config_dir.as_deref(),
            DictionaryCatalog {
                named_dicts: catalog.named_dicts,
                extra_active: catalog.extra_active,
                lang_settings: Vec::new(),
                overrides: Vec::new(),
            },
            CheckOptions {
                cspell_compat_mode: true,
                config_search: true,
                per_dir_config_search: true,
                config_file: resolved.config_file.clone(),
                no_must_find_files: true,
                cwd: Some(repo.clone()),
                ..CheckOptions::default()
            },
        )
        .expect("collect issues");

        let changelog_issues = results
            .into_iter()
            .find(|(path, _, _)| path.ends_with("CHANGELOG.md"))
            .map(|(_, _, issues)| issues)
            .unwrap_or_default();
        assert!(
            changelog_issues
                .iter()
                .any(|issue| issue.word == "unenumerable" && issue.line == 344),
            "expected CHANGELOG.md:344 'unenumerable', got {changelog_issues:?}"
        );
    }

    #[test]
    fn collect_all_issues_reports_real_dart_sdk_unoverridden() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let repo =
            workspace_root.join("vendor/cspell/integration-tests/repositories/temp/dart-lang/sdk");
        if !repo.exists() {
            panic!("expected dart-sdk fixture at {}", repo.display());
        }

        let resolved =
            crate::commands::setup::resolve_config(None, Some(repo.as_path()), true, &[])
                .expect("resolve config");

        let mut settings = resolved.settings;
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            resolved.config_dir.as_deref(),
            Some(&crate::commands::dict_cache_dir()),
            resolved.is_cspell,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);
        let results = collect_all_issues(
            &[PathBuf::from(
                "pkg/compiler/tool/kernel_visitor/test/test_classes.dart",
            )],
            &merged_settings,
            resolved.config_dir.as_deref(),
            DictionaryCatalog {
                named_dicts: catalog.named_dicts,
                extra_active: catalog.extra_active,
                lang_settings: Vec::new(),
                overrides: Vec::new(),
            },
            CheckOptions {
                cspell_compat_mode: true,
                config_search: true,
                per_dir_config_search: true,
                config_file: resolved.config_file.clone(),
                no_must_find_files: true,
                cwd: Some(repo.clone()),
                ..CheckOptions::default()
            },
        )
        .expect("collect issues");

        let dart_issues = results
            .into_iter()
            .find(|(path, _, _)| {
                path.to_string_lossy()
                    .replace('\\', "/")
                    .ends_with("pkg/compiler/tool/kernel_visitor/test/test_classes.dart")
            })
            .map(|(_, _, issues)| issues)
            .unwrap_or_default()
            .into_iter()
            .filter(|issue| issue.line == 58)
            .collect::<Vec<_>>();
        assert!(
            dart_issues.iter().any(|issue| issue.word == "unoverridden"),
            "expected test_classes.dart:58 'unoverridden', got {dart_issues:?}"
        );
    }

    #[test]
    fn latex_examples_runtime_override_activates_de_locale_and_latex_dictionary() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let repo = workspace_root
            .join("vendor/cspell/integration-tests/repositories/temp/MartinThoma/LaTeX-examples");
        if !repo.exists() {
            panic!("expected latex-examples fixture at {}", repo.display());
        }

        let resolved =
            crate::commands::setup::resolve_config(None, Some(repo.as_path()), true, &[])
                .expect("resolve config");

        let mut settings = resolved.settings;
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            resolved.config_dir.as_deref(),
            Some(&crate::commands::dict_cache_dir()),
            resolved.is_cspell,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);

        let file = repo.join("documents/Analysis I/Analysis-I.tex");
        let compiled_overrides = overrides::compile_overrides(&merged_settings);
        let effective_settings =
            overrides::apply_compiled_overrides(&merged_settings, &file, &compiled_overrides)
                .expect("expected latex override to match fixture file");

        assert_eq!(
            effective_settings.language.as_deref(),
            Some("en,de,lorem-ipsum"),
            "expected runtime override language to apply"
        );
        assert!(
            effective_settings
                .dictionaries
                .iter()
                .any(|dict| dict.eq_ignore_ascii_case("latex")),
            "expected runtime override to activate latex dictionary"
        );

        let resolved_lang_settings =
            resolve_language_settings(&effective_settings, Some(file.as_path()));
        assert!(
            resolved_lang_settings
                .dictionaries
                .iter()
                .any(|dict| dict.eq_ignore_ascii_case("de-de")),
            "expected de-de language setting to activate under overridden locale"
        );

        let validator = build_validator(
            &effective_settings,
            &catalog.named_dicts,
            &CheckOptions {
                cspell_compat_mode: true,
                config_search: true,
                per_dir_config_search: true,
                compound_words_mode: Some(CompoundWordsMode::SeparateWords),
                ..CheckOptions::default()
            },
            &catalog.extra_active,
            false,
            Some(file.as_path()),
            None,
        );
        let issues = validator.validate_text("Formelsammlung\n");
        assert!(
            !issues.iter().any(|issue| issue.word == "Formelsammlung"),
            "expected Formelsammlung to be accepted, got {issues:?}"
        );
    }

    #[test]
    fn collect_files_respects_real_prettier_files_whitelist() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let repo = workspace_root
            .join("vendor/cspell/integration-tests/repositories/temp/prettier/prettier");
        let config = repo.join("cspell.json");
        if !config.exists() {
            panic!("expected prettier fixture at {}", config.display());
        }

        let resolved =
            crate::commands::setup::resolve_config(None, Some(repo.as_path()), true, &[])
                .expect("resolve config");

        let mut settings = resolved.settings;
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            resolved.config_dir.as_deref(),
            Some(&crate::commands::dict_cache_dir()),
            resolved.is_cspell,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);
        let files = collect_files(
            &[],
            &merged_settings,
            &CheckOptions {
                cspell_compat_mode: true,
                config_search: true,
                per_dir_config_search: true,
                config_file: resolved.config_file.clone(),
                use_gitignore: Some(true),
                no_must_find_files: true,
                cwd: Some(repo.clone()),
                ..CheckOptions::default()
            },
        )
        .expect("collect files");

        let rels: Vec<String> = files
            .iter()
            .map(|f| {
                f.strip_prefix(&repo)
                    .unwrap_or(f)
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();

        assert!(
            rels.iter()
                .any(|rel| rel == "tests/format/angular/angular/format.test.js"),
            "expected whitelisted format.test.js to be discovered, got {rels:?}"
        );
        assert!(
            !rels
                .iter()
                .any(|rel| rel == "tests/format/angular/angular/attributes.component.html"),
            "did not expect non-whitelisted angular fixture HTML to be discovered, got {rels:?}"
        );
    }

    #[test]
    fn collect_all_issues_ignores_real_prettier_prettier_ignore_fence() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let repo = workspace_root
            .join("vendor/cspell/integration-tests/repositories/temp/prettier/prettier");
        if !repo.exists() {
            panic!("expected prettier fixture at {}", repo.display());
        }

        let resolved =
            crate::commands::setup::resolve_config(None, Some(repo.as_path()), true, &[])
                .expect("resolve config");

        let mut settings = resolved.settings;
        apply_default_dictionaries(&mut settings);
        apply_default_patterns(&mut settings);

        let catalog = crate::commands::cspell::catalog::build_dictionary_catalog(
            &settings,
            resolved.config_dir.as_deref(),
            Some(&crate::commands::dict_cache_dir()),
            resolved.is_cspell,
        )
        .expect("catalog");
        let merged_settings =
            merge_bundled_settings(&settings, &catalog.lang_settings, &catalog.overrides);
        let results = collect_all_issues(
            &[PathBuf::from("website/blog/2018-02-26-1.11.0.md")],
            &merged_settings,
            resolved.config_dir.as_deref(),
            DictionaryCatalog {
                named_dicts: catalog.named_dicts,
                extra_active: catalog.extra_active,
                lang_settings: Vec::new(),
                overrides: Vec::new(),
            },
            CheckOptions {
                cspell_compat_mode: true,
                config_search: true,
                per_dir_config_search: true,
                config_file: resolved.config_file.clone(),
                use_gitignore: Some(true),
                no_must_find_files: true,
                cwd: Some(repo.clone()),
                ..CheckOptions::default()
            },
        )
        .expect("collect issues");

        let blog_issues = results
            .into_iter()
            .find(|(path, _, _)| {
                path.to_string_lossy()
                    .replace('\\', "/")
                    .ends_with("website/blog/2018-02-26-1.11.0.md")
            })
            .map(|(_, _, issues)| issues)
            .unwrap_or_default();

        assert!(
            !blog_issues
                .iter()
                .any(|issue| issue.word == "println" && issue.line == 993),
            "expected prettier-ignore fenced block to suppress println at line 993, got {blog_issues:?}"
        );
    }

    #[test]
    fn shared_word_cache_enabled_for_precompiled_settings() {
        let compat_options = CheckOptions {
            cspell_compat_mode: true,
            ..CheckOptions::default()
        };
        assert!(
            should_use_shared_word_cache(&compat_options, true),
            "precompiled settings allow shared word cache"
        );
        assert!(
            !should_use_shared_word_cache(&compat_options, false),
            "non-precompiled settings disable shared word cache"
        );
    }
}
