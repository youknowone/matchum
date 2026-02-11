use anyhow::{Context, Result};
use ignore::WalkBuilder;
use md5::{Digest, Md5};
use rayon::prelude::*;
use ruspell_config::overrides;
use ruspell_config::resolver;
use ruspell_config::settings::{CSpellSettings, PatternDefinition, StringOrList};
use ruspell_core::issue::ValidationIssue;
use ruspell_core::validator::{Validator, ValidatorConfig};
use ruspell_dict::dictionary::Dictionary;
use ruspell_dict::hashdict::HashDictionary;
use ruspell_dict::loader;
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
            Ok(v) => v,
            Err(e) => {
                if options.continue_on_error {
                    if !options.silent {
                        eprintln!("Warning: {:#}", e);
                    }
                    (CSpellSettings::default(), None)
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

    let (named_dicts, extra_active) = build_dictionary_catalog(&settings, config_dir.as_deref())?;
    let files = collect_files(&effective_paths, &settings, &options)?;

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
    let cache_strategy = options
        .cache_strategy
        .as_deref()
        .unwrap_or("content");
    let cache = std::sync::Mutex::new(cache);

    let fail_fast = options.fail_fast;
    let fail_fast_flag = std::sync::atomic::AtomicBool::new(false);
    let need_context = options.show_context;
    let verbose = options.verbose;
    let silent = options.silent;
    let quiet = options.quiet;
    let validate_directives = options.validate_directives;
    let language_id = options.language_id.clone();

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
                            std::fs::read_to_string(file).unwrap_or_default()
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

            let mut effective_settings = overrides::apply_overrides(&settings, file);
            if effective_settings.enabled == Some(false) {
                return None;
            }

            // Apply --language-id: set language for override matching
            if let Some(ref lang) = language_id {
                effective_settings.language = Some(lang.clone());
            }

            let content = match std::fs::read_to_string(file) {
                Ok(c) => c,
                Err(e) => {
                    if !silent {
                        eprintln!("Warning: cannot read {}: {}", file.display(), e);
                    }
                    return None;
                }
            };

            let validator = build_validator(
                &effective_settings,
                &named_dicts,
                &options,
                &extra_active,
                show_suggestions,
            );
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
                let kept_content = if need_context {
                    content
                } else {
                    String::new()
                };
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
            if unique && !unique_words.insert(issue.word.to_lowercase()) {
                continue;
            }
            total_issues += 1;
            if show_issues {
                let display_path = match &base_dir {
                    Some(base) => file
                        .strip_prefix(base)
                        .unwrap_or(file)
                        .to_path_buf(),
                    None => std::fs::canonicalize(file).unwrap_or_else(|_| file.clone()),
                };
                if format == "words-only" {
                    println!("{}", issue.word);
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
        print_json_output(&results)?;
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
        let settings = resolver::load_config(path).context("failed to load config file")?;
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

    match resolver::find_config_with_stop(&search_dir, stop_config_search_at) {
        Some(path) => {
            let config_dir = path.parent().map(|p| p.to_path_buf());
            let settings = resolver::load_config(&path).context("failed to load config file")?;
            Ok((settings, config_dir))
        }
        None => Ok((CSpellSettings::default(), None)),
    }
}

fn build_dictionary_catalog(
    settings: &CSpellSettings,
    config_dir: Option<&Path>,
) -> Result<(Vec<(String, Arc<dyn Dictionary>)>, HashSet<String>)> {
    let mut dicts: Vec<(String, Arc<dyn Dictionary>)> = Vec::new();
    let mut extra_active: HashSet<String> = HashSet::new();
    let defined_names: HashSet<String> = settings
        .dictionary_definitions
        .iter()
        .map(|d| d.name.to_lowercase())
        .collect();

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
                    Ok(dict) => dicts.push((def.name.to_lowercase(), Arc::new(dict) as Arc<dyn Dictionary>)),
                    Err(e) => eprintln!("Warning: failed to load dictionary {}: {}", path_str, e),
                }
            }
        }
    }

    // Auto-fetch @cspell/dict-* packages for dictionaries without definitions
    if let Some(base) = config_dir {
        for dict_name in &settings.dictionaries {
            let lower = dict_name.to_lowercase();
            if defined_names.contains(&lower) {
                continue;
            }
            if dicts.iter().any(|(n, _)| n == &lower) {
                continue;
            }
            // Try to auto-fetch as @cspell/dict-{name}
            let pkg_name = dict_name_to_package(&lower);
            let ext_path = format!("{}/cspell-ext.json", pkg_name);
            if let Some(ext_json) = resolve_npm_import(&ext_path, base) {
                // Parse cspell-ext.json and load its dictionaryDefinitions
                if let Ok(content) = std::fs::read_to_string(&ext_json) {
                    if let Ok(ext_settings) = json5::from_str::<CSpellSettings>(&content) {
                        let ext_dir = ext_json.parent().unwrap_or(base);
                        for ext_def in &ext_settings.dictionary_definitions {
                            if let Some(ref p) = ext_def.path {
                                let dict_path = ext_dir.join(p);
                                if dict_path.exists() {
                                    match loader::load_dictionary(&dict_path) {
                                        Ok(dict) => {
                                            let name = ext_def.name.to_lowercase();
                                            extra_active.insert(name.clone());
                                            dicts.push((
                                                name,
                                                Arc::new(dict) as Arc<dyn Dictionary>,
                                            ));
                                        }
                                        Err(e) => eprintln!(
                                            "Warning: failed to load dictionary {}: {}",
                                            p, e
                                        ),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok((dicts, extra_active))
}

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

/// Try to resolve an npm package import by walking up node_modules or auto-fetching.
fn resolve_npm_import(import: &str, base_dir: &Path) -> Option<PathBuf> {
    use ruspell_config::npm_fetch;

    // Walk up looking for node_modules/
    let mut search_dir = Some(base_dir);
    while let Some(dir) = search_dir {
        let candidate = dir.join("node_modules").join(import);
        if candidate.exists() {
            return Some(candidate);
        }
        search_dir = dir.parent();
    }

    // Auto-download
    let package_name = npm_fetch::extract_package_name(import);
    let sub_path = npm_fetch::extract_sub_path(import);
    if let Ok(pkg_dir) = npm_fetch::ensure_package(package_name, None, base_dir) {
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
) -> Validator {
    let requested: Option<HashSet<String>> = if settings.dictionaries.is_empty() {
        None
    } else {
        let mut set: HashSet<String> = settings.dictionaries.iter().map(|d| d.to_lowercase()).collect();
        set.extend(extra_active.iter().cloned());
        Some(set)
    };

    let cli_enable: HashSet<String> = options.dictionary.iter().map(|d| d.to_lowercase()).collect();
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

    if !settings.words.is_empty() {
        let mut inline_dict = HashDictionary::new(false);
        for word in &settings.words {
            inline_dict.add_word(word);
        }
        entries.push(("__inline_words".into(), Arc::new(inline_dict), true));
    }

    let validator_config = build_validator_config(settings, options.allow_compound_words, compute_suggestions);
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
        flag_words: settings.flag_words.iter().map(|w| w.to_lowercase()).collect(),
        ignore_words: settings.ignore_words.iter().map(|w| w.to_lowercase()).collect(),
        allow_compound_words: cli_allow_compound_words
            .unwrap_or(settings.allow_compound_words.unwrap_or(false)),
        compute_suggestions,
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
                let pat = if prefix.is_empty() {
                    body.to_string()
                } else {
                    format!("(?{}){}", prefix, body)
                };
                return regex::Regex::new(&pat).ok();
            }
        }
    }
    regex::Regex::new(s).ok()
}

fn collect_files(paths: &[PathBuf], settings: &CSpellSettings, options: &CheckOptions) -> Result<Vec<PathBuf>> {
    let mut roots = paths.to_vec();
    roots.extend(read_file_list_paths(&options.file_list)?);
    for f in &options.file {
        if f.is_file() {
            roots.push(f.clone());
        }
    }

    let use_gitignore = options
        .use_gitignore
        .unwrap_or(settings.use_gitignore.unwrap_or(true));
    let show_hidden = options.dot;

    let mut files = Vec::new();

    for path in &roots {
        if path.is_file() {
            files.push(path.clone());
        } else {
            let mut builder = WalkBuilder::new(path);
            builder
                .hidden(!show_hidden)
                .git_ignore(use_gitignore);

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

    let mut seen = HashSet::new();
    files.retain(|f| seen.insert(f.clone()));

    Ok(files)
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
        "{}:{}:{} - {}: '{}'",
        file.display(),
        issue.line,
        issue.column,
        kind,
        issue.word
    );
    if !issue.is_forbidden && show_suggestions && !issue.suggestions.is_empty() {
        print!(" (suggestions: {})", issue.suggestions.join(", "));
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

fn print_json_output(results: &[(PathBuf, String, Vec<ValidationIssue>)]) -> Result<()> {
    let output: Vec<serde_json::Value> = results
        .iter()
        .map(|(file, _content, issues)| {
            let issue_values: Vec<serde_json::Value> = issues
                .iter()
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

// ---------------------------------------------------------------------------
// Directive validation
// ---------------------------------------------------------------------------

/// Known directive prefixes (case-insensitive)
const DIRECTIVE_PREFIXES: &[&str] = &[
    "cspell:", "spell-checker:", "spellchecker:",
];

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
    let directive_re = regex::Regex::new(
        r"(?i)(?:cspell|spell-?checker)\s*:\s*(\S+)"
    ).unwrap();

    for (line_idx, line) in content.lines().enumerate() {
        for cap in directive_re.captures_iter(line) {
            let directive = &cap[1];
            let directive_lower = directive.to_lowercase();
            let is_known = KNOWN_DIRECTIVES.iter().any(|d| d.to_lowercase() == directive_lower);
            if !is_known {
                let _ = DIRECTIVE_PREFIXES; // suppress unused warning
                let match_start = cap.get(1).unwrap().start();
                issues.push(ValidationIssue {
                    word: format!("cspell:{directive}"),
                    offset: 0,
                    line: line_idx + 1,
                    column: match_start + 1,
                    is_forbidden: false,
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
                let hash = val.get("hash").and_then(|v| v.as_str()).map(|s| s.to_string());
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
                                    is_forbidden: v.get("forbidden").and_then(|v| v.as_bool()).unwrap_or(false),
                                    suggestions: vec![],
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                entries.insert(PathBuf::from(key), CacheEntry { mtime_secs, size, hash, issues });
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

    fn update(&mut self, path: &Path, issues: &[ValidationIssue], strategy: &str, content: Option<&[u8]>) {
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
                    std::fs::read(path).ok().map(|d| format!("{:x}", Md5::digest(&d)))
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
