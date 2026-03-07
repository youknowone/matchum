use crate::settings::{CSpellSettings, OverrideSettings};
use globset::{Glob, GlobMatcher};
use std::path::Path;

/// Pre-compiled override with glob matcher.
pub struct CompiledOverride {
    matcher: GlobMatcher,
    settings: OverrideSettings,
}

/// Pre-compile all overrides' glob patterns once.
pub fn compile_overrides(settings: &CSpellSettings) -> Vec<CompiledOverride> {
    settings
        .overrides
        .iter()
        .filter_map(|ov| {
            Glob::new(&ov.filename).ok().map(|g| CompiledOverride {
                matcher: g.compile_matcher(),
                settings: ov.clone(),
            })
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
        .filter(|co| matches_compiled(&co.matcher, file_path))
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
        if matches_override(&ov.filename, file_path) {
            merge_override(&mut result, ov);
        }
    }

    result
}

fn matches_compiled(matcher: &GlobMatcher, file_path: &Path) -> bool {
    if matcher.is_match(file_path) {
        return true;
    }
    if let Some(name) = file_path.file_name() {
        if matcher.is_match(Path::new(name)) {
            return true;
        }
    }
    let file_str = file_path.to_string_lossy();
    matcher.is_match(file_str.as_ref())
}

/// Check if a file path matches an override's filename glob pattern.
fn matches_override(pattern: &str, file_path: &Path) -> bool {
    let file_str = file_path.to_string_lossy();

    // Try matching as a glob pattern
    if let Ok(glob) = Glob::new(pattern) {
        let matcher: GlobMatcher = glob.compile_matcher();
        if matcher.is_match(file_path) {
            return true;
        }
        // Also try matching against just the filename
        if let Some(name) = file_path.file_name() {
            if matcher.is_match(Path::new(name)) {
                return true;
            }
        }
        // Try matching against string representation
        if matcher.is_match(file_str.as_ref()) {
            return true;
        }
    }

    false
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
        .ignore_reg_exp_list
        .extend(ov.ignore_reg_exp_list.iter().cloned());

    // Extend dictionaries
    settings
        .dictionaries
        .extend(ov.dictionaries.iter().cloned());

    // Override scalar values if set
    if let Some(ref lang) = ov.language {
        settings.language = Some(lang.clone());
    }
    if let Some(cs) = ov.case_sensitive {
        settings.case_sensitive = Some(cs);
    }
    if let Some(enabled) = ov.enabled {
        settings.enabled = Some(enabled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_glob_star() {
        assert!(matches_override("**/*.ts", Path::new("src/main.ts")));
        assert!(!matches_override("**/*.ts", Path::new("src/main.rs")));
    }

    #[test]
    fn test_matches_simple_extension() {
        assert!(matches_override("*.py", Path::new("script.py")));
        assert!(!matches_override("*.py", Path::new("script.rs")));
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
}
