use super::{defaults, resolve};
use anyhow::Result;
use matchum_config::settings::CSpellSettings;
use matchum_config::settings::DictionaryDefinition;
use matchum_dict::dictionary::Dictionary;
use matchum_dict::hashdict::HashDictionary;
use matchum_dict::loader;
use matchum_dict::repmap::RepMap;
use md5::{Digest, Md5};
use rayon::prelude::*;
use std::borrow::Cow;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use unicode_normalization::UnicodeNormalization;

use crate::commands::check::DictionaryCatalog;
use matchum_dict::dictionary::FindResult;

/// A dictionary loading job collected in the first (sequential) phase.
struct DictJob {
    name: String,
    path: PathBuf,
    extra_active: bool,
    rep_map: Vec<(String, String)>,
    dictionary_type: Option<String>,
    ignore_forbidden_words: bool,
    use_compounds: bool,
}

struct ConfiguredDictionary {
    inner: Arc<dyn Dictionary>,
    ignore_forbidden_words: bool,
    use_compounds: bool,
}

impl ConfiguredDictionary {
    fn normalized_search_form<'a>(&self, word: &'a str) -> Cow<'a, str> {
        if word.is_ascii() {
            if word.bytes().all(|b| !b.is_ascii_uppercase()) {
                return Cow::Borrowed(word);
            }
            return Cow::Owned(word.to_ascii_lowercase());
        }

        let nfc: String = word.nfc().collect();
        Cow::Owned(nfc.to_lowercase())
    }

    fn normalize_find_result(&self, result: FindResult) -> FindResult {
        if self.ignore_forbidden_words && result.forbidden {
            return FindResult::default();
        }
        result
    }

    fn find_direct(&self, word: &str) -> FindResult {
        self.find_with_search_forms(word)
    }

    fn find_direct_with_compounds(&self, word: &str) -> FindResult {
        self.find_with_search_forms(word)
    }

    fn has_direct(&self, word: &str) -> bool {
        self.find_direct(word).found
    }

    fn has_direct_with_compounds(&self, word: &str) -> bool {
        self.find_direct_with_compounds(word).found
    }

    #[allow(clippy::needless_range_loop)]
    fn has_legacy_compound(&self, word: &str) -> bool {
        const MAX_DEPTH: usize = 6;
        const MIN_PART_LEN: usize = 3;

        let char_count = word.chars().count();
        if char_count < MIN_PART_LEN {
            return false;
        }

        let mut boundaries: Vec<usize> = word.char_indices().map(|(idx, _)| idx).collect();
        boundaries.push(word.len());

        let mut stack = vec![(0usize, 0usize)];
        let mut seen = HashSet::new();

        while let Some((start_idx, depth)) = stack.pop() {
            if depth > MAX_DEPTH || !seen.insert((start_idx, depth)) {
                continue;
            }
            let start = boundaries[start_idx];

            for next_idx in start_idx + 1..boundaries.len().saturating_sub(1) {
                let split = boundaries[next_idx];
                let left = &word[start..split];
                let right = &word[split..];
                if left.chars().count() < MIN_PART_LEN || right.chars().count() < MIN_PART_LEN {
                    continue;
                }
                if !self.has_direct(left) {
                    continue;
                }
                if self.has_direct(right) {
                    return true;
                }
                if depth < MAX_DEPTH {
                    stack.push((next_idx, depth + 1));
                }
            }
        }

        false
    }

    fn lookup_inner(&self, word: &str) -> FindResult {
        // In cspell, `useCompounds` controls legacy word-compound search.
        // Dictionary-native compounds remain available through the dictionary's
        // default lookup path even when `allowCompoundWords` is false.
        self.normalize_find_result(self.inner.find(word))
    }

    fn find_with_search_forms(&self, word: &str) -> FindResult {
        let direct = self.lookup_inner(word);
        if direct.found {
            return direct;
        }
        if self.inner.is_case_sensitive() {
            let normalized = self.normalized_search_form(word);
            let found = if self.use_compounds {
                self.inner
                    .has_pre_normalized_with_compounds(word, normalized.as_ref())
            } else {
                self.inner
                    .has_pre_normalized_without_compounds(word, normalized.as_ref())
            };
            if found {
                return self.normalize_find_result(FindResult {
                    found: true,
                    forbidden: self
                        .inner
                        .is_forbidden_pre_normalized(word, normalized.as_ref()),
                    no_suggest: self
                        .inner
                        .is_no_suggest_pre_normalized(word, normalized.as_ref()),
                });
            }
        }
        FindResult::default()
    }

    fn has_pre_normalized_search_forms(&self, word: &str, normalized: &str) -> bool {
        let found = self.inner.has_pre_normalized(word, normalized);
        if found
            && !(self.ignore_forbidden_words
                && self.inner.is_forbidden_pre_normalized(word, normalized))
        {
            return true;
        }

        if self.inner.is_case_sensitive() && normalized != word {
            let lower_found = self.inner.has_pre_normalized(normalized, normalized);
            if lower_found
                && !(self.ignore_forbidden_words
                    && self
                        .inner
                        .is_forbidden_pre_normalized(normalized, normalized))
            {
                return true;
            }
        }

        false
    }
}

impl Dictionary for ConfiguredDictionary {
    fn has(&self, word: &str) -> bool {
        self.has_direct_with_compounds(word)
            || (self.use_compounds && self.has_legacy_compound(word))
    }

    fn has_direct_only(&self, word: &str) -> bool {
        self.has_direct(word)
    }

    fn is_forbidden(&self, word: &str) -> bool {
        !self.ignore_forbidden_words && self.inner.is_forbidden(word)
    }

    fn suggest(&self, word: &str, limit: usize) -> Vec<String> {
        self.inner.suggest(word, limit)
    }

    fn find(&self, word: &str) -> FindResult {
        let direct = self.find_direct_with_compounds(word);
        if direct.found {
            return direct;
        }
        if self.use_compounds && self.has_legacy_compound(word) {
            return FindResult {
                found: true,
                forbidden: false,
                no_suggest: false,
            };
        }
        direct
    }

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn has_forbidden_words(&self) -> bool {
        !self.ignore_forbidden_words && self.inner.has_forbidden_words()
    }

    fn has_no_suggest_words(&self) -> bool {
        self.inner.has_no_suggest_words()
    }

    fn is_case_sensitive(&self) -> bool {
        self.inner.is_case_sensitive()
    }

    fn is_no_suggest(&self, word: &str) -> bool {
        self.inner.is_no_suggest(word)
    }

    fn is_no_suggest_pre_normalized(&self, word: &str, normalized: &str) -> bool {
        self.inner.is_no_suggest_pre_normalized(word, normalized)
    }

    fn has_pre_normalized(&self, word: &str, normalized: &str) -> bool {
        self.has_pre_normalized_search_forms(word, normalized)
    }

    fn has_pre_normalized_direct_only(&self, word: &str, normalized: &str) -> bool {
        self.has_pre_normalized_search_forms(word, normalized)
    }

    fn has_pre_normalized_with_compounds(&self, word: &str, _normalized: &str) -> bool {
        self.has(word)
    }

    fn has_expensive_forms(&self) -> bool {
        self.use_compounds || self.inner.has_expensive_forms()
    }

    fn is_forbidden_pre_normalized(&self, word: &str, _normalized: &str) -> bool {
        self.is_forbidden(word)
    }
}

fn parse_suggestion_entry(entry: &str) -> Option<(&str, Vec<&str>)> {
    let (word, suggestions) = entry.split_once("->")?;
    let word = word.trim();
    if word.is_empty() {
        return None;
    }
    let suggestions: Vec<&str> = suggestions
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if suggestions.is_empty() {
        return None;
    }
    Some((word, suggestions))
}

fn build_inline_dictionary(def: &DictionaryDefinition) -> Option<Arc<dyn Dictionary>> {
    if def.words.is_empty() && def.flag_words.is_empty() && def.suggest_words.is_empty() {
        return None;
    }

    let mut dict = HashDictionary::new(true);
    if let Some(rep_map) = build_rep_map(&def.rep_map) {
        dict.set_repmap(rep_map);
    }
    for word in &def.words {
        dict.add_word(word);
    }
    for word in &def.flag_words {
        if let Some((misspelling, suggestions)) = parse_suggestion_entry(word) {
            dict.add_forbidden(misspelling);
            dict.add_suggestions(misspelling, suggestions);
        } else {
            dict.add_forbidden(word);
        }
    }
    for word in &def.suggest_words {
        if let Some((misspelling, suggestions)) = parse_suggestion_entry(word) {
            dict.add_suggestions(misspelling, suggestions);
        }
    }

    Some(Arc::new(dict) as Arc<dyn Dictionary>)
}

fn build_rep_map_pairs(raw: &[Vec<String>]) -> Vec<(String, String)> {
    raw.iter()
        .filter_map(|entry| match entry.as_slice() {
            [from, to, ..] if !from.is_empty() => Some((from.clone(), to.clone())),
            _ => None,
        })
        .collect()
}

fn build_rep_map(raw: &[Vec<String>]) -> Option<RepMap> {
    let pairs = build_rep_map_pairs(raw);
    (!pairs.is_empty()).then(|| RepMap::new(pairs))
}

fn collect_dictionary_definition(
    def: &DictionaryDefinition,
    base_dir: Option<&Path>,
    is_active: bool,
    jobs: &mut Vec<DictJob>,
    collected_names: &mut HashSet<String>,
    inline_named_dicts: &mut Vec<(String, Arc<dyn Dictionary>)>,
    inline_extra_active: &mut HashSet<String>,
) {
    let name = def.name.to_lowercase();
    if collected_names.contains(&name) {
        return;
    }

    if let Some(ref path_str) = def.path {
        if path_str.starts_with("http://") || path_str.starts_with("https://") {
            if let Some(local) = fetch_remote_dict(path_str, &def.name) {
                collected_names.insert(name.clone());
                jobs.push(DictJob {
                    name,
                    path: local,
                    extra_active: is_active,
                    rep_map: build_rep_map_pairs(&def.rep_map),
                    dictionary_type: def.r#type.clone(),
                    ignore_forbidden_words: def.ignore_forbidden_words.unwrap_or(false),
                    use_compounds: def.use_compounds.unwrap_or(false),
                });
            }
            return;
        }
        let path = Path::new(path_str);
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(base) = base_dir {
            base.join(path)
        } else {
            path.to_path_buf()
        };
        if resolved.exists() {
            collected_names.insert(name.clone());
            jobs.push(DictJob {
                name,
                path: resolved,
                extra_active: is_active,
                rep_map: build_rep_map_pairs(&def.rep_map),
                dictionary_type: def.r#type.clone(),
                ignore_forbidden_words: def.ignore_forbidden_words.unwrap_or(false),
                use_compounds: def.use_compounds.unwrap_or(false),
            });
        }
        return;
    }

    if let Some(dict) = build_inline_dictionary(def) {
        collected_names.insert(name.clone());
        if is_active {
            inline_extra_active.insert(name.clone());
        }
        inline_named_dicts.push((name, dict));
    }
}

pub fn build_dictionary_catalog(
    settings: &CSpellSettings,
    config_dir: Option<&Path>,
    dict_base_dir: Option<&Path>,
    auto_resolve_cspell: bool,
) -> Result<DictionaryCatalog> {
    let defined_names: HashSet<String> = settings
        .dictionary_definitions
        .iter()
        .map(|d| d.name.to_lowercase())
        .collect();

    // Phase 1: Collect all dictionary paths (sequential, lightweight I/O)
    let mut jobs: Vec<DictJob> = Vec::new();
    let mut collected_names: HashSet<String> = HashSet::new();
    let mut inline_named_dicts: Vec<(String, Arc<dyn Dictionary>)> = Vec::new();
    let mut inline_extra_active: HashSet<String> = HashSet::new();
    let mut accumulated_lang_settings: Vec<matchum_config::settings::LanguageSetting> = Vec::new();
    let mut accumulated_overrides: Vec<matchum_config::settings::OverrideSettings> = Vec::new();

    // User-defined dictionaries
    for def in &settings.dictionary_definitions {
        collect_dictionary_definition(
            def,
            config_dir,
            false,
            &mut jobs,
            &mut collected_names,
            &mut inline_named_dicts,
            &mut inline_extra_active,
        );
    }

    // Auto-fetch @cspell/dict-* packages.
    // When auto_resolve_cspell is true (cspell.json mode), bare names like "en_us"
    // are mapped to npm packages via dict_name_to_package.
    // When false (matchum.toml mode), only names already starting with "@" are
    // resolved as npm packages — bare names require a matching dictionary_definition.
    let npm_base = dict_base_dir.or(config_dir);
    let resolve_start = std::time::Instant::now();
    if let Some(base) = npm_base {
        for dict_name in &settings.dictionaries {
            let lower = dict_name.to_lowercase();
            if defined_names.contains(&lower) || collected_names.contains(&lower) {
                continue;
            }
            let pkg_name = if dict_name.starts_with('@') {
                // Already a scoped package name — use as-is
                lower.clone()
            } else if auto_resolve_cspell {
                defaults::dict_name_to_package(&lower)
            } else {
                continue;
            };
            let ext_path = format!("{}/cspell-ext.json", pkg_name);
            if let Some(ext_json) = resolve::resolve_npm_import(&ext_path, base)
                && let Ok(ext_settings) = matchum_config::resolver::load_config(&ext_json)
            {
                let ext_dir = ext_json.parent().unwrap_or(base);
                let ext_active: HashSet<String> = ext_settings
                    .dictionaries
                    .iter()
                    .map(|d| d.to_lowercase())
                    .collect();
                for ext_def in &ext_settings.dictionary_definitions {
                    let is_active = ext_active.contains(&ext_def.name.to_lowercase())
                        || ext_def.name.eq_ignore_ascii_case(&lower);
                    collect_dictionary_definition(
                        ext_def,
                        Some(ext_dir),
                        is_active,
                        &mut jobs,
                        &mut collected_names,
                        &mut inline_named_dicts,
                        &mut inline_extra_active,
                    );
                }
            }
        }
    }

    // Default bundled dictionaries — also collect their languageSettings.
    // Dicts listed in the package's `dictionaries` field are globally active.
    // Dicts only in languageSettings are activated per file type.
    // Only in cspell mode: matchum.toml mode requires explicit definitions.
    if auto_resolve_cspell && let Some(base) = npm_base {
        let mut loaded_pkgs: HashSet<String> = HashSet::new();
        for &(pkg, _dict_name, _version) in defaults::DEFAULT_BUNDLED_DICTS {
            if loaded_pkgs.contains(pkg) {
                continue;
            }
            let ext_path = format!("{}/cspell-ext.json", pkg);
            if let Some(ext_json) = resolve::resolve_npm_import(&ext_path, base)
                && let Ok(ext_settings) = matchum_config::resolver::load_config(&ext_json)
            {
                loaded_pkgs.insert(pkg.to_string());
                let ext_dir = ext_json.parent().unwrap_or(base);
                let pkg_active: HashSet<String> = ext_settings
                    .dictionaries
                    .iter()
                    .map(|d| d.to_lowercase())
                    .collect();
                for ext_def in &ext_settings.dictionary_definitions {
                    let is_active = pkg_active.contains(&ext_def.name.to_lowercase());
                    collect_dictionary_definition(
                        ext_def,
                        Some(ext_dir),
                        is_active,
                        &mut jobs,
                        &mut collected_names,
                        &mut inline_named_dicts,
                        &mut inline_extra_active,
                    );
                }
                for mut ls in ext_settings.language_settings {
                    for p in &ext_settings.patterns {
                        if !ls.patterns.iter().any(|existing| existing.name == p.name) {
                            ls.patterns.push(p.clone());
                        }
                    }
                    accumulated_lang_settings.push(ls);
                }
                accumulated_overrides.extend(ext_settings.overrides);
            }
        }
    }

    // Add cspell's core default languageSettings.
    accumulated_lang_settings.extend(defaults::default_language_settings());

    let resolve_elapsed = resolve_start.elapsed();
    if resolve_elapsed.as_millis() > 500 {
        eprintln!(
            "Loading {} dictionaries... ({:.1}s resolving packages)",
            jobs.len(),
            resolve_elapsed.as_secs_f64()
        );
    }

    // Phase 2: Load all dictionaries in parallel
    let results: Vec<_> = jobs
        .par_iter()
        .filter_map(|job| {
            match loader::load_dictionary_with_options(&job.path, job.dictionary_type.as_deref()) {
                Ok(mut dict) => {
                    dict.set_case_sensitive(true);
                    if !job.rep_map.is_empty() {
                        dict.set_repmap(RepMap::new(job.rep_map.clone()));
                    }
                    Some((job, dict))
                }
                Err(e) => {
                    eprintln!(
                        "Warning: failed to load dictionary {}: {}",
                        job.path.display(),
                        e
                    );
                    None
                }
            }
        })
        .collect();

    // Phase 3: Assemble results
    let mut named_dicts: Vec<(String, Arc<dyn Dictionary>)> =
        Vec::with_capacity(results.len() + inline_named_dicts.len());
    let mut extra_active: HashSet<String> = inline_extra_active;
    named_dicts.extend(inline_named_dicts);
    for (job, dict) in results {
        if job.extra_active {
            extra_active.insert(job.name.clone());
        }
        let dict: Arc<dyn Dictionary> = Arc::new(ConfiguredDictionary {
            inner: Arc::new(dict),
            ignore_forbidden_words: job.ignore_forbidden_words,
            use_compounds: job.use_compounds,
        });
        named_dicts.push((job.name.clone(), dict));
    }

    Ok(DictionaryCatalog {
        named_dicts,
        extra_active,
        lang_settings: accumulated_lang_settings,
        overrides: accumulated_overrides,
    })
}

/// Fetch a remote dictionary from an HTTP(S) URL. Caches locally under ~/.matchum_cache/remote_dicts/.
fn fetch_remote_dict(url: &str, dict_name: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let cache_dir = PathBuf::from(home)
        .join(".matchum_cache")
        .join("remote_dicts");
    std::fs::create_dir_all(&cache_dir).ok()?;

    let mut hasher = Md5::new();
    hasher.update(url.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let ext = if url.ends_with(".trie") || url.ends_with(".trie.gz") {
        "trie"
    } else {
        "txt"
    };
    let cached = cache_dir.join(format!("{dict_name}-{hash}.{ext}"));
    if cached.exists() {
        return Some(cached);
    }

    eprintln!("Fetching remote dictionary: {url}");
    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .unversioned_rustls_crypto_provider(std::sync::Arc::new(
                        rustls::crypto::aws_lc_rs::default_provider(),
                    ))
                    .build(),
            )
            .build(),
    );
    let body = agent.get(url).call().ok()?.into_body().read_to_vec().ok()?;
    std::fs::write(&cached, &body).ok()?;
    Some(cached)
}

// cspell:disable
#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::check::{CheckOptions, build_validator};
    use crate::commands::cspell::resolve;
    use matchum_dict::dictionary::FindResult;
    use matchum_dict::hashdict::HashDictionary;

    struct ForbiddenHitDictionary;

    impl Dictionary for ForbiddenHitDictionary {
        fn has(&self, _word: &str) -> bool {
            true
        }

        fn has_direct_only(&self, _word: &str) -> bool {
            true
        }

        fn is_forbidden(&self, _word: &str) -> bool {
            true
        }

        fn suggest(&self, _word: &str, _limit: usize) -> Vec<String> {
            Vec::new()
        }

        fn find(&self, _word: &str) -> FindResult {
            FindResult {
                found: true,
                forbidden: true,
                no_suggest: false,
            }
        }

        fn len(&self) -> usize {
            1
        }

        fn has_forbidden_words(&self) -> bool {
            true
        }

        fn is_case_sensitive(&self) -> bool {
            true
        }

        fn has_pre_normalized_direct_only(&self, _word: &str, _normalized: &str) -> bool {
            true
        }

        fn is_forbidden_pre_normalized(&self, _word: &str, _normalized: &str) -> bool {
            true
        }
    }

    #[test]
    fn build_catalog_loads_inline_dictionary_definitions_from_imports() {
        let dir = tempfile::tempdir().unwrap();
        let pkg_dir = dir
            .path()
            .join("node_modules")
            .join("@cspell")
            .join("dict-test-inline");
        std::fs::create_dir_all(pkg_dir.join("dict")).unwrap();
        std::fs::write(
            pkg_dir.join("cspell-ext.json"),
            r#"{
                "import": ["./dict/dict-en.json"],
                "languageSettings": [
                    {
                        "languageId": "*",
                        "locale": "en",
                        "dictionaries": ["test-inline"]
                    }
                ]
            }"#,
        )
        .unwrap();
        std::fs::write(
            pkg_dir.join("dict/dict-en.json"),
            r#"{
                "dictionaryDefinitions": [
                    {
                        "name": "test-inline",
                        "suggestWords": ["hdinsight->hindsight"]
                    }
                ]
            }"#,
        )
        .unwrap();

        let settings = CSpellSettings {
            dictionaries: vec!["test-inline".into()],
            language: Some("en".into()),
            ..Default::default()
        };
        let catalog = build_dictionary_catalog(&settings, Some(dir.path()), Some(dir.path()), true)
            .expect("catalog should build");

        let file = dir.path().join("README.md");
        std::fs::write(&file, "hdinsight\n").unwrap();
        let validator = build_validator(
            &settings,
            &catalog.named_dicts,
            &CheckOptions::default(),
            &catalog.extra_active,
            true,
            Some(&file),
            None,
        );
        let issues = validator.validate_text("hdinsight\n");

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].word, "hdinsight");
        assert!(
            issues[0].suggestions.iter().any(|s| s == "hindsight"),
            "inline imported dictionary should provide suggestions: {:?}",
            issues[0].suggestions
        );
    }

    #[test]
    fn build_catalog_collects_bundled_language_settings() {
        let dir = tempfile::tempdir().unwrap();
        let pkg_dir = dir
            .path()
            .join("node_modules")
            .join("@cspell")
            .join("dict-scala");
        std::fs::create_dir_all(pkg_dir.join("dict")).unwrap();
        std::fs::write(
            pkg_dir.join("cspell-ext.json"),
            r#"{
                "dictionaryDefinitions": [
                    { "name": "scala", "path": "./dict/scala.txt" }
                ],
                "dictionaries": [],
                "languageSettings": [
                    {
                        "languageId": "scala",
                        "locale": "*",
                        "ignoreRegExpList": ["/^\\s*import\\s+\\w+/m", "/\\.\\w+/"],
                        "dictionaries": ["scala"]
                    }
                ]
            }"#,
        )
        .unwrap();
        std::fs::write(pkg_dir.join("dict/scala.txt"), "scalatra\n").unwrap();

        let settings = CSpellSettings {
            language: Some("en".into()),
            ..Default::default()
        };
        let catalog = build_dictionary_catalog(&settings, Some(dir.path()), Some(dir.path()), true)
            .expect("catalog should build");

        assert!(
            catalog.named_dicts.iter().any(|(name, _)| name == "scala"),
            "expected bundled scala dictionary to load"
        );
        assert!(
            catalog.lang_settings.iter().any(|ls| {
                ls.language_id.iter().any(|id| id == "scala")
                    && ls.ignore_reg_exp_list.iter().any(|pat| pat == r"/\.\w+/")
            }),
            "expected bundled scala language settings to be collected"
        );
    }

    #[test]
    fn configured_dictionary_masks_forbidden_direct_hits_when_ignore_forbidden_words_is_enabled() {
        let dict = ConfiguredDictionary {
            inner: Arc::new(ForbiddenHitDictionary),
            ignore_forbidden_words: true,
            use_compounds: false,
        };

        assert!(!dict.has_direct_only("Verteidigungs"));
        assert!(!dict.has_pre_normalized_direct_only("Verteidigungs", "verteidigungs"));
    }

    #[test]
    fn configured_dictionary_keeps_native_compounds_enabled_without_use_compounds() {
        let mut raw = HashDictionary::new(false);
        raw.add_compound_part_explicit("splash", true, true, true);
        raw.add_compound_part_explicit("screen", true, true, true);

        let wrapped = ConfiguredDictionary {
            inner: Arc::new(raw),
            ignore_forbidden_words: false,
            use_compounds: false,
        };

        assert!(
            wrapped.has("splashscreen"),
            "dictionary-native compounds should remain available by default"
        );
        assert!(
            wrapped.has_pre_normalized("splashscreen", "splashscreen"),
            "pre-normalized lookup should preserve dictionary-native compounds"
        );
        assert!(
            wrapped.find("splashscreen").found,
            "detailed lookup should preserve dictionary-native compounds"
        );
    }

    #[test]
    fn configured_dictionary_use_compounds_only_enables_legacy_compound_search() {
        let mut raw_disabled = HashDictionary::new(false);
        raw_disabled.add_word("error");
        raw_disabled.add_word("code");
        let mut raw_enabled = HashDictionary::new(false);
        raw_enabled.add_word("error");
        raw_enabled.add_word("code");

        let disabled = ConfiguredDictionary {
            inner: Arc::new(raw_disabled),
            ignore_forbidden_words: false,
            use_compounds: false,
        };
        let enabled = ConfiguredDictionary {
            inner: Arc::new(raw_enabled),
            ignore_forbidden_words: false,
            use_compounds: true,
        };

        assert!(
            !disabled.has("errorcode"),
            "legacy compounding should stay disabled unless useCompounds is set"
        );
        assert!(
            enabled.has("errorcode"),
            "useCompounds=true should enable legacy same-dictionary compounding"
        );
    }

    #[test]
    fn de_de_catalog_masks_forbidden_fragments_like_cspell() {
        let cache_dir = crate::commands::dict_cache_dir();
        let ext = resolve::resolve_npm_import("@cspell/dict-de-de/cspell-ext.json", &cache_dir)
            .expect("resolve dict-de-de (run `matchum cspell lint` once to populate cache)");

        let settings = matchum_config::resolver::load_config(&ext).expect("load de-de ext config");
        let catalog =
            build_dictionary_catalog(&settings, ext.parent(), None, true).expect("catalog");
        let dict = catalog
            .named_dicts
            .iter()
            .find(|(name, _)| name == "de-de")
            .map(|(_, dict)| dict)
            .expect("expected de-de dictionary");

        for word in [
            "kenn",
            "Aktualisierungs",
            "navigations",
            "Verteidigungs",
            "sicherheits",
            "Sicherheits",
            "Adress",
        ] {
            let find = dict.find(word);
            let forbidden = dict.is_forbidden(word);
            assert!(
                !dict.has(word),
                "expected wrapped de-de dictionary to reject {word}: find={find:?} forbidden={forbidden}"
            );
        }

        for word in [
            "Formelsammlung",
            "Grenzwerte",
            "Inhaltsverzeichnis",
            "Vorlesung",
            "Quelltext",
            "Grundmenge",
            "Vektorraum",
        ] {
            let find = dict.find(word);
            assert!(
                dict.has(word),
                "expected wrapped de-de dictionary to accept {word}: find={find:?}"
            );
        }
    }

    #[test]
    fn configured_en_us_dictionary_keeps_api_visible() {
        let cache_dir = crate::commands::dict_cache_dir();
        let ext = resolve::resolve_npm_import("@cspell/dict-en_us/cspell-ext.json", &cache_dir)
            .expect("resolve dict-en_us");
        let settings = matchum_config::resolver::load_config(&ext).expect("load en_us ext config");
        let def = settings
            .dictionary_definitions
            .iter()
            .find(|def| def.name.eq_ignore_ascii_case("en_us"))
            .expect("missing en_us dictionary definition");
        let path = ext
            .parent()
            .expect("ext parent")
            .join(def.path.as_deref().expect("en_us dictionary path"));

        let mut raw = loader::load_dictionary_with_options(&path, def.r#type.as_deref())
            .expect("load en_us trie");
        raw.set_case_sensitive(true);
        if !def.rep_map.is_empty() {
            raw.set_repmap(RepMap::new(build_rep_map_pairs(&def.rep_map)));
        }

        assert!(
            raw.has_pre_normalized_with_compounds("api", "api"),
            "raw en_us dictionary should accept api via ignore-case trie search: find={:?} forbidden={}",
            raw.find("api"),
            raw.is_forbidden("api")
        );

        let wrapped = ConfiguredDictionary {
            inner: Arc::new(raw),
            ignore_forbidden_words: def.ignore_forbidden_words.unwrap_or(false),
            use_compounds: def.use_compounds.unwrap_or(false),
        };

        assert!(
            wrapped.has("api"),
            "wrapped en_us dictionary should contain api: find={:?} forbidden={}",
            wrapped.find("api"),
            wrapped.is_forbidden("api")
        );
    }
}
