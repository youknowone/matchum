use crate::convert;
use crate::npm_fetch;
use crate::matchum_config::MatchumConfig;
use crate::settings::CSpellSettings;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Config file search locations, matching cspell's configLocations.ts.
/// JS/TS/TOML config formats are excluded (require runtime execution).
const CONFIG_NAMES: &[&str] = &[
    // Original locations
    ".cspell.json",
    "cspell.json",
    ".cSpell.json",
    "cSpell.json",
    // Original locations jsonc
    ".cspell.jsonc",
    "cspell.jsonc",
    // Alternate locations
    ".vscode/cspell.json",
    ".vscode/cSpell.json",
    ".vscode/.cspell.json",
    // Standard locations
    ".cspell.config.json",
    ".cspell.config.jsonc",
    ".cspell.config.yaml",
    ".cspell.config.yml",
    "cspell.config.json",
    "cspell.config.jsonc",
    "cspell.config.yaml",
    "cspell.config.yml",
    // YAML
    ".cspell.yaml",
    ".cspell.yml",
    "cspell.yaml",
    "cspell.yml",
    // .config/
    ".config/.cspell.json",
    ".config/cspell.json",
    ".config/.cSpell.json",
    ".config/cSpell.json",
    ".config/.cspell.jsonc",
    ".config/cspell.jsonc",
    ".config/cspell.config.json",
    ".config/cspell.config.jsonc",
    ".config/cspell.config.yaml",
    ".config/cspell.config.yml",
    ".config/.cspell.config.json",
    ".config/.cspell.config.jsonc",
    ".config/.cspell.config.yaml",
    ".config/.cspell.config.yml",
    ".config/cspell.yaml",
    ".config/cspell.yml",
];

/// Result of a prioritized config search.
#[derive(Debug)]
pub enum ConfigFound {
    /// Found a native `matchum.toml`.
    Matchum(PathBuf),
    /// Found a cspell config file (json/jsonc/yaml).
    Cspell(PathBuf),
    /// No config found.
    None,
}

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Json(String),
    #[error("TOML parse error: {0}")]
    Toml(String),
    #[error("config not found")]
    NotFound,
    #[error("circular import: {0}")]
    CircularImport(String),
}

/// Find the cspell config file by searching up the directory tree.
pub fn find_config(start_dir: &Path) -> Option<PathBuf> {
    find_config_with_stop(start_dir, &[])
}

/// Find cspell config file by searching up the directory tree, stopping at specific directories.
pub fn find_config_with_stop(start_dir: &Path, stop_search_at: &[PathBuf]) -> Option<PathBuf> {
    let mut dir = Some(start_dir);
    let stop_set: HashSet<PathBuf> = stop_search_at
        .iter()
        .map(|p| p.canonicalize().unwrap_or_else(|_| p.clone()))
        .collect();
    while let Some(d) = dir {
        let canonical = d.canonicalize().unwrap_or_else(|_| d.to_path_buf());
        if stop_set.contains(&canonical) {
            break;
        }
        for name in CONFIG_NAMES {
            let config_path = d.join(name);
            if config_path.exists() {
                return Some(config_path);
            }
        }
        dir = d.parent();
    }
    None
}

/// Matchum native config file names, checked before cspell configs.
const MATCHUM_CONFIG_NAMES: &[&str] = &["matchum.toml", ".matchum.toml"];

/// Find config with priority: `matchum.toml` first, then cspell configs.
pub fn find_config_prioritized(start_dir: &Path) -> ConfigFound {
    find_config_prioritized_with_stop(start_dir, &[])
}

/// Find config with priority, stopping at specific directories.
pub fn find_config_prioritized_with_stop(
    start_dir: &Path,
    stop_search_at: &[PathBuf],
) -> ConfigFound {
    let mut dir = Some(start_dir);
    let stop_set: HashSet<PathBuf> = stop_search_at
        .iter()
        .map(|p| p.canonicalize().unwrap_or_else(|_| p.clone()))
        .collect();
    while let Some(d) = dir {
        let canonical = d.canonicalize().unwrap_or_else(|_| d.to_path_buf());
        if stop_set.contains(&canonical) {
            break;
        }
        // Check matchum.toml first
        for name in MATCHUM_CONFIG_NAMES {
            let config_path = d.join(name);
            if config_path.exists() {
                return ConfigFound::Matchum(config_path);
            }
        }
        // Then check cspell configs
        for name in CONFIG_NAMES {
            let config_path = d.join(name);
            if config_path.exists() {
                return ConfigFound::Cspell(config_path);
            }
        }
        dir = d.parent();
    }
    ConfigFound::None
}

/// Load a `matchum.toml` and convert it to `CSpellSettings`.
pub fn load_matchum_config(path: &Path) -> Result<CSpellSettings, ResolveError> {
    let content = std::fs::read_to_string(path)?;
    let config: MatchumConfig =
        toml::from_str(&content).map_err(|e| ResolveError::Toml(e.to_string()))?;
    let config_dir = path.parent().unwrap_or(Path::new("."));
    Ok(convert::to_cspell_settings(&config, config_dir))
}

/// Load config by file extension: `.toml` → matchum loader, otherwise → cspell loader.
pub fn load_config_auto(path: &Path) -> Result<CSpellSettings, ResolveError> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext == "toml" {
        load_matchum_config(path)
    } else {
        load_config(path)
    }
}

/// Load a cspell config file and resolve imports.
pub fn load_config(path: &Path) -> Result<CSpellSettings, ResolveError> {
    let mut visited = HashSet::new();
    load_config_recursive(path, &mut visited)
}

fn load_config_recursive(
    path: &Path,
    visited: &mut HashSet<PathBuf>,
) -> Result<CSpellSettings, ResolveError> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !visited.insert(canonical.clone()) {
        return Err(ResolveError::CircularImport(
            canonical.display().to_string(),
        ));
    }

    let content = std::fs::read_to_string(path)?;
    let mut settings: CSpellSettings =
        json5::from_str(&content).map_err(|e| ResolveError::Json(e.to_string()))?;

    let base_dir = path.parent().unwrap_or(Path::new("."));

    // Resolve relative dictionary paths to absolute before merging
    for def in &mut settings.dictionary_definitions {
        if let Some(ref p) = def.path {
            let dict_path = Path::new(p);
            if !dict_path.is_absolute() {
                let abs = base_dir.join(dict_path);
                if abs.exists() {
                    def.path = Some(abs.to_string_lossy().into_owned());
                }
            }
        }
    }

    let mut result = settings.clone();

    // Resolve imports
    for import_path_str in &settings.import {
        if let Some(import_path) = resolve_import_path(import_path_str, base_dir) {
            match load_config_recursive(&import_path, visited) {
                Ok(imported) => {
                    result = merge_settings(imported, result);
                }
                Err(e) => {
                    eprintln!(
                        "Warning: failed to load import '{}': {}",
                        import_path_str, e
                    );
                }
            }
        }
    }

    Ok(result)
}

/// Resolve an import path. Supports:
/// - absolute paths
/// - relative paths (resolved against base_dir)
/// - npm package paths like `@cspell/dict-en_us/cspell-ext.json`
fn resolve_import_path(import: &str, base_dir: &Path) -> Option<PathBuf> {
    let path = Path::new(import);

    // Absolute path
    if path.is_absolute() {
        return if path.exists() {
            Some(path.to_path_buf())
        } else {
            None
        };
    }

    // Relative path (starts with . or ..)
    if import.starts_with('.') {
        let resolved = base_dir.join(path);
        return if resolved.exists() {
            Some(resolved)
        } else {
            None
        };
    }

    // npm package path — walk up looking for node_modules/ (only for scoped packages)
    if import.starts_with('@') {
        let mut search_dir = Some(base_dir);
        while let Some(dir) = search_dir {
            let candidate = dir.join("node_modules").join(import);
            if candidate.exists() {
                // If it's a directory (bare package import), look for cspell-ext.json
                if candidate.is_dir() {
                    let ext_json = candidate.join("cspell-ext.json");
                    if ext_json.exists() {
                        return Some(ext_json);
                    }
                } else {
                    return Some(candidate);
                }
            }
            search_dir = dir.parent();
        }
    }

    // Auto-download scoped npm packages (e.g., @cspell/dict-en_us/cspell-ext.json)
    if import.starts_with('@') {
        let package_name = npm_fetch::extract_package_name(import);
        let sub_path = npm_fetch::extract_sub_path(import);
        if let Ok(pkg_dir) = npm_fetch::ensure_package(package_name, None, base_dir) {
            let resolved = match sub_path {
                Some(sub) => pkg_dir.join(sub),
                // No sub-path: default to cspell-ext.json
                None => pkg_dir.join("cspell-ext.json"),
            };
            if resolved.exists() {
                return Some(resolved);
            }
        }
    }

    // Fallback: try as relative path
    let resolved = base_dir.join(path);
    if resolved.exists() {
        Some(resolved)
    } else {
        None
    }
}

/// Merge base settings with overlay. Overlay values take precedence for scalars;
/// arrays are concatenated.
pub fn merge_settings(base: CSpellSettings, overlay: CSpellSettings) -> CSpellSettings {
    CSpellSettings {
        version: overlay.version.or(base.version),
        language: overlay.language.or(base.language),
        enabled: overlay.enabled.or(base.enabled),

        words: concat_vecs(base.words, overlay.words),
        ignore_words: concat_vecs(base.ignore_words, overlay.ignore_words),
        flag_words: concat_vecs(base.flag_words, overlay.flag_words),
        user_words: concat_vecs(base.user_words, overlay.user_words),

        dictionaries: concat_vecs_unique(base.dictionaries, overlay.dictionaries),
        dictionary_definitions: concat_vecs(
            base.dictionary_definitions,
            overlay.dictionary_definitions,
        ),

        ignore_reg_exp_list: concat_vecs(base.ignore_reg_exp_list, overlay.ignore_reg_exp_list),
        include_reg_exp_list: concat_vecs(base.include_reg_exp_list, overlay.include_reg_exp_list),
        patterns: concat_vecs(base.patterns, overlay.patterns),

        files: overlay.files.or(base.files),
        ignore_paths: concat_vecs(base.ignore_paths, overlay.ignore_paths),
        use_gitignore: overlay.use_gitignore.or(base.use_gitignore),

        case_sensitive: overlay.case_sensitive.or(base.case_sensitive),
        allow_compound_words: overlay.allow_compound_words.or(base.allow_compound_words),
        min_word_length: overlay.min_word_length.or(base.min_word_length),
        max_duplicate_problems: overlay
            .max_duplicate_problems
            .or(base.max_duplicate_problems),
        glob_root: overlay.glob_root.or(base.glob_root),
        suggest_words: concat_vecs(base.suggest_words, overlay.suggest_words),
        no_suggest_dictionaries: concat_vecs_unique(
            base.no_suggest_dictionaries,
            overlay.no_suggest_dictionaries,
        ),

        import: Vec::new(), // imports already resolved
        overrides: concat_vecs(base.overrides, overlay.overrides),
        language_settings: concat_vecs(base.language_settings, overlay.language_settings),
    }
}

fn concat_vecs<T>(mut a: Vec<T>, b: Vec<T>) -> Vec<T> {
    a.extend(b);
    a
}

fn concat_vecs_unique(mut a: Vec<String>, b: Vec<String>) -> Vec<String> {
    let existing: HashSet<String> = a.iter().map(|s| s.to_lowercase()).collect();
    for item in b {
        if !existing.contains(&item.to_lowercase()) {
            a.push(item);
        }
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_config_not_found() {
        let result = find_config(Path::new("/nonexistent/path"));
        assert!(result.is_none());
    }

    #[test]
    fn test_load_simple_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("cspell.json");
        std::fs::write(
            &config_path,
            r#"{"words": ["foo", "bar"], "language": "en"}"#,
        )
        .unwrap();

        let settings = load_config(&config_path).unwrap();
        assert_eq!(settings.words, vec!["foo", "bar"]);
        assert_eq!(settings.language.as_deref(), Some("en"));
    }

    #[test]
    fn test_find_config_in_dir() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("cspell.json");
        std::fs::write(&config_path, "{}").unwrap();

        let found = find_config(dir.path());
        assert_eq!(found, Some(config_path));
    }

    #[test]
    fn test_prioritized_finds_matchum_toml_first() {
        let dir = tempfile::tempdir().unwrap();
        // Create both config types
        std::fs::write(dir.path().join("matchum.toml"), "language = \"en\"\n").unwrap();
        std::fs::write(dir.path().join("cspell.json"), "{}").unwrap();

        match find_config_prioritized(dir.path()) {
            ConfigFound::Matchum(p) => {
                assert_eq!(p.file_name().unwrap(), "matchum.toml");
            }
            other => panic!("expected Matchum, got {:?}", other),
        }
    }

    #[test]
    fn test_prioritized_falls_back_to_cspell() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("cspell.json"), "{}").unwrap();

        match find_config_prioritized(dir.path()) {
            ConfigFound::Cspell(p) => {
                assert_eq!(p.file_name().unwrap(), "cspell.json");
            }
            other => panic!("expected Cspell, got {:?}", other),
        }
    }

    #[test]
    fn test_prioritized_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            find_config_prioritized(dir.path()),
            ConfigFound::None
        ));
    }

    #[test]
    fn test_load_matchum_config() {
        let dir = tempfile::tempdir().unwrap();
        let words_dir = dir.path().join(".matchum");
        std::fs::create_dir_all(&words_dir).unwrap();
        std::fs::write(words_dir.join("words.txt"), "matchum\ncspell\n").unwrap();

        let config_path = dir.path().join("matchum.toml");
        std::fs::write(
            &config_path,
            r#"
language = "en"
words_file = ".matchum/words.txt"
dictionaries = ["en_us"]
"#,
        )
        .unwrap();

        let settings = load_matchum_config(&config_path).unwrap();
        assert_eq!(settings.language.as_deref(), Some("en"));
        assert_eq!(settings.words, vec!["matchum", "cspell"]);
        assert_eq!(settings.dictionaries, vec!["en_us"]);
    }

    #[test]
    fn test_merge_settings() {
        let base = CSpellSettings {
            words: vec!["base_word".into()],
            language: Some("en".into()),
            ..Default::default()
        };
        let overlay = CSpellSettings {
            words: vec!["overlay_word".into()],
            language: Some("fr".into()),
            ..Default::default()
        };

        let merged = merge_settings(base, overlay);
        assert_eq!(merged.words, vec!["base_word", "overlay_word"]);
        assert_eq!(merged.language.as_deref(), Some("fr"));
    }
}

#[cfg(test)]
mod jsonc_tests {
    #[test]
    fn test_jsonc_with_line_comment_before_object() {
        let input = r#"// leading comment
{
  "version": "0.2",
  "words": ["foo"]
}"#;
        let result: Result<crate::settings::CSpellSettings, _> = json5::from_str(input);
        match &result {
            Ok(s) => println!("OK: words={:?}", s.words),
            Err(e) => println!("ERROR: {e}"),
        }
        assert!(result.is_ok(), "json5 should handle leading comment");
    }
}
