use crate::npm_fetch;
use crate::settings::CSpellSettings;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const CONFIG_NAMES: &[&str] = &[
    "cspell.json",
    ".cspell.json",
    ".cspellrc.json",
    "cspell.config.json",
    "cspell.yaml",
    ".cspellrc.yaml",
    "cspell.config.yaml",
    ".cspellrc",
];

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Json(String),
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
pub fn find_config_with_stop(
    start_dir: &Path,
    stop_search_at: &[PathBuf],
) -> Option<PathBuf> {
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
                Ok(mut imported) => {
                    // Activate all dictionary definitions from imported packages
                    for def in &imported.dictionary_definitions {
                        if !imported.dictionaries.iter().any(|d| d.eq_ignore_ascii_case(&def.name)) {
                            imported.dictionaries.push(def.name.clone());
                        }
                    }
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
        return if path.exists() { Some(path.to_path_buf()) } else { None };
    }

    // Relative path (starts with . or ..)
    if import.starts_with('.') {
        let resolved = base_dir.join(path);
        return if resolved.exists() { Some(resolved) } else { None };
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
    if resolved.exists() { Some(resolved) } else { None }
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

        dictionaries: concat_vecs(base.dictionaries, overlay.dictionaries),
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

        import: Vec::new(), // imports already resolved
        overrides: concat_vecs(base.overrides, overlay.overrides),
    }
}

fn concat_vecs<T>(mut a: Vec<T>, b: Vec<T>) -> Vec<T> {
    a.extend(b);
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
