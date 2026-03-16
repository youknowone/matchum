use crate::glob_match::{
    global_match_path, is_global_pattern, normalized_match_path,
    resolve_match_root, root_relative_match_path,
};
use crate::settings::{CSpellSettings, OverrideSettings};
use globset::{Glob, GlobMatcher};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

struct CompiledGlob {
    matcher: GlobMatcher,
    root: Option<PathBuf>,
    is_global: bool,
}

/// Pre-compiled override with glob matcher.
pub struct CompiledOverride {
    positive_patterns: Vec<CompiledGlob>,
    negative_patterns: Vec<CompiledGlob>,
    settings: OverrideSettings,
}

/// Pre-compile all overrides' glob patterns once.
pub fn compile_overrides(settings: &CSpellSettings) -> Vec<CompiledOverride> {
    settings
        .overrides
        .iter()
        .filter_map(|ov| {
            let (positive_patterns, negative_patterns) = compile_override_patterns(&ov.filename);
            if positive_patterns.is_empty() && negative_patterns.is_empty() {
                None
            } else {
                Some(CompiledOverride {
                    positive_patterns,
                    negative_patterns,
                    settings: ov.clone(),
                })
            }
        })
        .collect()
}

/// Apply pre-compiled overrides. Returns None if no override matches,
/// avoiding the CSpellSettings clone entirely.
pub fn apply_compiled_overrides(
    settings: &CSpellSettings,
    file_path: &Path,
    compiled: &[CompiledOverride],
) -> Option<CSpellSettings> {
    let matching: Vec<&OverrideSettings> = compiled
        .iter()
        .filter(|co| matches_compiled_override(co, file_path))
        .map(|co| &co.settings)
        .collect();

    if matching.is_empty() {
        return None;
    }

    let mut result = settings.clone();
    result.overrides = Vec::new();
    for ov in matching {
        merge_override(&mut result, ov);
    }
    Some(result)
}

/// Apply matching overrides from a CSpellSettings to produce an effective
/// configuration for a specific file.
pub fn apply_overrides(settings: &CSpellSettings, file_path: &Path) -> CSpellSettings {
    let mut result = settings.clone();
    // Clear overrides in the result — they've been applied
    result.overrides = Vec::new();

    for ov in &settings.overrides {
        if matches_override_set(&ov.filename, file_path) {
            merge_override(&mut result, ov);
        }
    }

    result
}

fn matches_compiled_override(compiled: &CompiledOverride, file_path: &Path) -> bool {
    if compiled
        .negative_patterns
        .iter()
        .any(|pattern| matches_compiled(pattern, file_path))
    {
        return false;
    }
    compiled
        .positive_patterns
        .iter()
        .any(|pattern| matches_compiled(pattern, file_path))
}

fn matches_compiled(compiled: &CompiledGlob, file_path: &Path) -> bool {
    if compiled.is_global && matches_glob_pattern(compiled, &global_match_path(file_path)) {
        return true;
    }
    if let Some(candidate) = root_relative_match_path(file_path, compiled.root.as_deref()) {
        return matches_glob_pattern(compiled, candidate.as_ref());
    }
    if !file_path.is_absolute() {
        return matches_glob_pattern(compiled, file_path);
    }
    false
}

fn matches_glob_pattern(compiled: &CompiledGlob, file_path: &Path) -> bool {
    if compiled.matcher.is_match(file_path) {
        return true;
    }
    normalized_match_path(file_path)
        .is_some_and(|file_str| compiled.matcher.is_match(Path::new(file_str.as_ref())))
}

fn matches_override_set(patterns: &crate::settings::GlobPatternSet, file_path: &Path) -> bool {
    let (positive_patterns, negative_patterns) = compile_override_patterns(patterns);
    if negative_patterns
        .iter()
        .any(|pattern| matches_compiled(pattern, file_path))
    {
        return false;
    }
    positive_patterns
        .iter()
        .any(|pattern| matches_compiled(pattern, file_path))
}

fn compile_override_patterns(
    patterns: &crate::settings::GlobPatternSet,
) -> (Vec<CompiledGlob>, Vec<CompiledGlob>) {
    let mut positive_patterns = Vec::new();
    let mut negative_patterns = Vec::new();

    for glob in patterns.iter() {
        let root = glob.root.as_deref().map(resolve_match_root);
        let is_global = is_global_pattern(&glob.glob);
        for normalized in normalize_override_patterns(&glob.glob) {
            let Ok(compiled_glob) = Glob::new(normalized.pattern.as_ref()) else {
                continue;
            };
            let compiled = CompiledGlob {
                matcher: compiled_glob.compile_matcher(),
                root: root.clone(),
                is_global,
            };
            if normalized.is_negative {
                negative_patterns.push(compiled);
            } else {
                positive_patterns.push(compiled);
            }
        }
    }

    (positive_patterns, negative_patterns)
}

struct NormalizedOverridePattern<'a> {
    pattern: Cow<'a, str>,
    is_negative: bool,
}

fn normalize_override_patterns(pattern: &str) -> Vec<NormalizedOverridePattern<'_>> {
    let mut pattern = strip_double_negations(pattern);
    let is_negative = pattern.starts_with('!');
    if is_negative {
        pattern = &pattern[1..];
    }

    let normalized = normalize_override_pattern_nested(pattern);
    normalized
        .into_iter()
        .map(|pattern| NormalizedOverridePattern {
            pattern,
            is_negative,
        })
        .collect()
}

fn strip_double_negations(mut pattern: &str) -> &str {
    while let Some(stripped) = pattern.strip_prefix("!!") {
        pattern = stripped;
    }
    pattern
}

fn normalize_override_pattern_nested(pattern: &str) -> Vec<Cow<'_, str>> {
    if !pattern.contains('/') {
        if pattern == "**" {
            return vec![Cow::Borrowed("**")];
        }
        return vec![
            Cow::Owned(format!("**/{pattern}")),
            Cow::Owned(format!("**/{pattern}/**")),
        ];
    }

    let has_leading_slash = pattern.starts_with('/');
    let pattern = pattern.strip_prefix('/').unwrap_or(pattern);

    if pattern.ends_with('/') {
        if has_leading_slash || pattern[..pattern.len() - 1].contains('/') {
            return vec![Cow::Owned(format!("{pattern}**/*"))];
        }
        return vec![Cow::Owned(format!("**/{pattern}**/*"))];
    }

    if pattern.ends_with("**") {
        return vec![Cow::Borrowed(pattern)];
    }

    vec![Cow::Borrowed(pattern), Cow::Owned(format!("{pattern}/**"))]
}

/// Merge an override into the effective settings.
fn merge_override(settings: &mut CSpellSettings, ov: &OverrideSettings) {
    // Extend word lists
    settings.words.extend(ov.words.iter().cloned());
    settings
        .ignore_words
        .extend(ov.ignore_words.iter().cloned());
    settings.flag_words.extend(ov.flag_words.iter().cloned());
    settings
        .suggest_words
        .extend(ov.suggest_words.iter().cloned());
    settings
        .ignore_reg_exp_list
        .extend(ov.ignore_reg_exp_list.iter().cloned());
    settings
        .include_reg_exp_list
        .extend(ov.include_reg_exp_list.iter().cloned());
    settings.patterns.extend(ov.patterns.iter().cloned());

    // Extend dictionaries
    settings
        .dictionaries
        .extend(ov.dictionaries.iter().cloned());
    settings
        .dictionary_definitions
        .extend(ov.dictionary_definitions.iter().cloned());
    settings
        .no_suggest_dictionaries
        .extend(ov.no_suggest_dictionaries.iter().cloned());
    settings
        .language_settings
        .extend(ov.language_settings.iter().cloned());

    // Override scalar values if set
    if let Some(ref lang) = ov.language {
        settings.language = Some(lang.clone());
    }
    if let Some(ref language_id) = ov.language_id {
        settings.language_id = Some(language_id.clone());
    }
    if let Some(cs) = ov.case_sensitive {
        settings.case_sensitive = Some(cs);
    }
    if let Some(allow_compound_words) = ov.allow_compound_words {
        settings.allow_compound_words = Some(allow_compound_words);
    }
    if let Some(min_word_length) = ov.min_word_length {
        settings.min_word_length = Some(min_word_length);
    }
    if let Some(ignore_random_strings) = ov.ignore_random_strings {
        settings.ignore_random_strings = Some(ignore_random_strings);
    }
    if let Some(min_random_length) = ov.min_random_length {
        settings.min_random_length = Some(min_random_length);
    }
    if let Some(max_duplicate_problems) = ov.max_duplicate_problems {
        settings.max_duplicate_problems = Some(max_duplicate_problems);
    }
    if let Some(max_number_of_problems) = ov.max_number_of_problems {
        settings.max_number_of_problems = Some(max_number_of_problems);
    }
    if let Some(enabled) = ov.enabled {
        settings.enabled = Some(enabled);
    }
}

// cspell:disable
#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::GlobDef;
    use std::path::PathBuf;

    struct CwdGuard(PathBuf);

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }

    #[test]
    fn test_matches_glob_star() {
        let patterns = crate::settings::GlobPatternSet::from_glob_defs(vec![GlobDef {
            glob: "**/*.ts".into(),
            root: None,
            source: None,
        }]);
        assert!(matches_override_set(&patterns, Path::new("src/main.ts")));
        assert!(!matches_override_set(&patterns, Path::new("src/main.rs")));
    }

    #[test]
    fn test_matches_simple_extension() {
        let patterns = crate::settings::GlobPatternSet::from_glob_defs(vec![GlobDef {
            glob: "*.py".into(),
            root: None,
            source: None,
        }]);
        assert!(matches_override_set(&patterns, Path::new("script.py")));
        assert!(matches_override_set(
            &patterns,
            Path::new("nested/script.py")
        ));
        assert!(!matches_override_set(&patterns, Path::new("script.rs")));
    }

    #[test]
    fn test_matches_directory_override_descendants() {
        let patterns = crate::settings::GlobPatternSet::from_glob_defs(vec![GlobDef {
            glob: "temp/ktaranov/sqlserver-kit".into(),
            root: None,
            source: None,
        }]);
        assert!(matches_override_set(
            &patterns,
            Path::new("temp/ktaranov/sqlserver-kit/ADS/README.md")
        ));
        assert!(!matches_override_set(
            &patterns,
            Path::new("temp/ktaranov/other-repo/ADS/README.md")
        ));
    }

    #[test]
    fn test_negated_override_pattern_excludes_match() {
        let patterns = crate::settings::GlobPatternSet::from_glob_defs(vec![
            GlobDef {
                glob: "*.yaml".into(),
                root: None,
                source: None,
            },
            GlobDef {
                glob: "!test.yaml".into(),
                root: None,
                source: None,
            },
        ]);
        assert!(!matches_override_set(
            &patterns,
            Path::new(".github/workflows/test.yaml")
        ));
        assert!(matches_override_set(
            &patterns,
            Path::new(".github/workflows/build.yaml")
        ));
    }

    #[test]
    fn test_apply_override_words() {
        let settings = CSpellSettings {
            words: vec!["hello".into()],
            overrides: vec![OverrideSettings {
                filename: "**/*.ts".into(),
                words: vec!["typescript".into()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let effective = apply_overrides(&settings, Path::new("src/main.ts"));
        assert!(effective.words.contains(&"hello".to_string()));
        assert!(effective.words.contains(&"typescript".to_string()));
    }

    #[test]
    fn test_apply_override_no_match() {
        let settings = CSpellSettings {
            words: vec!["hello".into()],
            overrides: vec![OverrideSettings {
                filename: "**/*.ts".into(),
                words: vec!["typescript".into()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let effective = apply_overrides(&settings, Path::new("src/main.rs"));
        assert!(effective.words.contains(&"hello".to_string()));
        assert!(!effective.words.contains(&"typescript".to_string()));
    }

    #[test]
    fn test_apply_override_language() {
        let settings = CSpellSettings {
            language: Some("en".into()),
            overrides: vec![OverrideSettings {
                filename: "**/NL/*.txt".into(),
                language: Some("en,nl".into()),
                ..Default::default()
            }],
            ..Default::default()
        };

        let effective = apply_overrides(&settings, Path::new("docs/NL/readme.txt"));
        assert_eq!(effective.language.as_deref(), Some("en,nl"));
    }

    #[test]
    fn test_apply_override_language_id() {
        let settings = CSpellSettings {
            overrides: vec![OverrideSettings {
                filename: "**/*.jsxx".into(),
                language_id: Some("javascript".into()),
                ..Default::default()
            }],
            ..Default::default()
        };

        let effective = apply_overrides(&settings, Path::new("src/main.jsxx"));
        assert_eq!(effective.language_id.as_deref(), Some("javascript"));
    }

    #[test]
    fn test_root_anchored_override_matches_repo_root_only() {
        let settings = CSpellSettings {
            overrides: vec![OverrideSettings {
                filename: "/README.md".into(),
                words: vec!["rootonly".into()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let effective_root = apply_overrides(&settings, Path::new("README.md"));
        assert!(effective_root.words.contains(&"rootonly".to_string()));

        let effective_nested = apply_overrides(&settings, Path::new("docs/README.md"));
        assert!(!effective_nested.words.contains(&"rootonly".to_string()));
    }

    #[test]
    fn test_apply_override_enabled() {
        let settings = CSpellSettings {
            enabled: Some(true),
            overrides: vec![OverrideSettings {
                filename: "**/*.generated.*".into(),
                enabled: Some(false),
                ..Default::default()
            }],
            ..Default::default()
        };

        let effective = apply_overrides(&settings, Path::new("src/types.generated.ts"));
        assert_eq!(effective.enabled, Some(false));
    }

    #[test]
    fn test_apply_override_allow_compound_words() {
        let settings = CSpellSettings {
            allow_compound_words: Some(true),
            overrides: vec![OverrideSettings {
                filename: "**/*.py".into(),
                allow_compound_words: Some(false),
                ..Default::default()
            }],
            ..Default::default()
        };

        let effective = apply_overrides(&settings, Path::new("scripts/main.py"));
        assert_eq!(effective.allow_compound_words, Some(false));
    }

    #[test]
    fn test_apply_override_language_settings() {
        let settings = CSpellSettings {
            language_settings: vec![crate::settings::LanguageSetting {
                language_id: vec!["python".into()],
                words: vec!["bytearray".into()],
                ..Default::default()
            }],
            overrides: vec![OverrideSettings {
                filename: "temp/AdaDoom3/AdaDoom3/**/*.py".into(),
                language_settings: vec![crate::settings::LanguageSetting {
                    language_id: vec!["python".into()],
                    allow_compound_words: Some(false),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };

        let effective = apply_overrides(
            &settings,
            Path::new("temp/AdaDoom3/AdaDoom3/Tools/compile-idmap.py"),
        );

        assert_eq!(effective.language_settings.len(), 2);
        assert_eq!(
            effective.language_settings[1].allow_compound_words,
            Some(false)
        );
    }

    #[test]
    fn test_global_override_pattern_uses_absolute_file_path() {
        let repo = tempfile::tempdir().unwrap();
        let file = repo
            .path()
            .join("specification/keyvault/Security.KeyVault.Administration/README.md");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "renamings\n").unwrap();

        let settings = CSpellSettings {
            overrides: vec![OverrideSettings {
                filename: crate::settings::GlobPatternSet::from_glob_defs(vec![GlobDef {
                    glob: "**/specification/keyvault/Security.KeyVault.Administration/README.md"
                        .into(),
                    root: Some(
                        repo.path()
                            .join("specification/keyvault")
                            .to_string_lossy()
                            .into_owned(),
                    ),
                    source: None,
                }]),
                words: vec!["renamings".into()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let effective = apply_overrides(&settings, &file);
        assert!(effective.words.contains(&"renamings".to_string()));
    }

    #[test]
    fn test_global_override_pattern_matches_hidden_path_segments() {
        let root = Path::new(
            "/Users/al03219714/.matchum_cache/repos/azure-rest-api-specs/specification/keyvault",
        );
        let file = Path::new(
            "/Users/al03219714/.matchum_cache/repos/azure-rest-api-specs/specification/keyvault/Security.KeyVault.Administration/README.md",
        );
        let settings = CSpellSettings {
            overrides: vec![OverrideSettings {
                filename: crate::settings::GlobPatternSet::from_glob_defs(vec![GlobDef {
                    glob: "**/specification/keyvault/Security.KeyVault.Administration/README.md"
                        .into(),
                    root: Some(root.to_string_lossy().into_owned()),
                    source: None,
                }]),
                words: vec!["renamings".into()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let effective = apply_overrides(&settings, file);
        assert!(
            effective.words.contains(&"renamings".to_string()),
            "global override should match absolute paths even through hidden segments"
        );
    }

    #[test]
    fn test_rooted_override_matches_relative_file_from_repo_cwd() {
        let _guard = CwdGuard(std::env::current_dir().unwrap());
        let dir = tempfile::tempdir().unwrap();
        let repositories = dir.path().join("repositories");
        let repo = repositories
            .join("temp")
            .join("MartinThoma")
            .join("LaTeX-examples");
        std::fs::create_dir_all(repo.join("publications")).unwrap();
        std::env::set_current_dir(&repo).unwrap();
        let repositories = repositories
            .canonicalize()
            .unwrap_or_else(|_| repositories.clone());

        let settings = CSpellSettings {
            overrides: vec![OverrideSettings {
                filename: crate::settings::GlobPatternSet::from_glob_defs(vec![GlobDef {
                    glob: "temp/MartinThoma/LaTeX-examples/**/*.tex".into(),
                    root: Some(repositories.to_string_lossy().into_owned()),
                    source: Some(repositories.join("cspell-latex.json").display().to_string()),
                }]),
                words: vec!["override-hit".into()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let effective = apply_overrides(
            &settings,
            Path::new("publications/Seminar-Kognitive-Automobile.tex"),
        );

        assert!(effective.words.iter().any(|word| word == "override-hit"));
    }
}
