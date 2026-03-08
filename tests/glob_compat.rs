//! Glob pattern edge-case tests for matchum's override system.
//!
//! Exercises `apply_overrides` with various glob patterns to verify
//! correct matching behaviour from `globset`.
//!
//! The unit tests inside `crates/matchum-config/src/overrides.rs` already
//! cover `**/*.ts`, `*.py`, language overrides, and enabled-flag overrides.
//! This file focuses on less common patterns: brace expansion, double-star
//! prefixes, special filenames, directory-scoped globs, etc.

use matchum_config::overrides::apply_overrides;
use matchum_config::settings::{CSpellSettings, OverrideSettings};
use std::path::Path;

// ── helpers ────────────────────────────────────────────────────────

/// Build a `CSpellSettings` with a single override that adds a marker word
/// when `pattern` matches.  The test calls `apply_overrides` and checks
/// whether the marker word appeared.
fn settings_with_override(pattern: &str) -> CSpellSettings {
    CSpellSettings {
        words: vec!["base".into()],
        overrides: vec![OverrideSettings {
            filename: pattern.into(),
            words: vec!["matched".into()],
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// Build a `CSpellSettings` with two overrides that add different marker
/// words so we can tell which one(s) fired.
fn settings_with_two_overrides(pattern_a: &str, pattern_b: &str) -> CSpellSettings {
    CSpellSettings {
        words: vec!["base".into()],
        overrides: vec![
            OverrideSettings {
                filename: pattern_a.into(),
                words: vec!["override_a".into()],
                ..Default::default()
            },
            OverrideSettings {
                filename: pattern_b.into(),
                words: vec!["override_b".into()],
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

fn did_match(settings: &CSpellSettings, file_path: &str) -> bool {
    let effective = apply_overrides(settings, Path::new(file_path));
    effective.words.contains(&"matched".to_string())
}

// ── 1. brace expansion ────────────────────────────────────────────

#[test]
fn glob_brace_expansion_matches_first_alternative() {
    let s = settings_with_override("*.{js,ts}");
    assert!(did_match(&s, "app.js"));
}

#[test]
fn glob_brace_expansion_matches_second_alternative() {
    let s = settings_with_override("*.{js,ts}");
    assert!(did_match(&s, "app.ts"));
}

#[test]
fn glob_brace_expansion_rejects_non_member() {
    let s = settings_with_override("*.{js,ts}");
    assert!(!did_match(&s, "app.rs"));
}

#[test]
fn glob_brace_expansion_with_path_prefix() {
    let s = settings_with_override("src/**/*.{jsx,tsx}");
    assert!(did_match(&s, "src/components/Button.tsx"));
    assert!(did_match(&s, "src/App.jsx"));
    assert!(!did_match(&s, "lib/App.jsx"));
}

// ── 2. double-star patterns ───────────────────────────────────────

#[test]
fn glob_double_star_matches_nested_file() {
    let s = settings_with_override("**/*.json");
    assert!(did_match(&s, "a/b/c/config.json"));
}

#[test]
fn glob_double_star_matches_root_level() {
    let s = settings_with_override("**/*.json");
    assert!(did_match(&s, "config.json"));
}

#[test]
fn glob_double_star_rejects_wrong_extension() {
    let s = settings_with_override("**/*.json");
    assert!(!did_match(&s, "config.yaml"));
}

// ── 3. filename-only (single star) matching ───────────────────────

#[test]
fn glob_star_extension_matches_bare_filename() {
    let s = settings_with_override("*.py");
    assert!(did_match(&s, "script.py"));
}

#[test]
fn glob_star_extension_matches_nested_via_filename_fallback() {
    // `*.py` should still match `src/utils/script.py` because
    // `matches_override` also tests against just the file name.
    let s = settings_with_override("*.py");
    assert!(did_match(&s, "src/utils/script.py"));
}

#[test]
fn glob_star_extension_rejects_different_ext() {
    let s = settings_with_override("*.py");
    assert!(!did_match(&s, "script.rb"));
}

// ── 4. directory-scoped patterns ──────────────────────────────────

#[test]
fn glob_directory_scope_matches_inside() {
    let s = settings_with_override("src/**");
    assert!(did_match(&s, "src/main.rs"));
    assert!(did_match(&s, "src/lib/utils.rs"));
}

#[test]
fn glob_directory_scope_rejects_outside() {
    let s = settings_with_override("src/**");
    assert!(!did_match(&s, "tests/main.rs"));
}

#[test]
fn glob_specific_directory_and_ext() {
    let s = settings_with_override("docs/**/*.md");
    assert!(did_match(&s, "docs/guide/intro.md"));
    assert!(!did_match(&s, "docs/guide/intro.txt"));
    assert!(!did_match(&s, "src/README.md"));
}

// ── 5. special / unusual filenames ────────────────────────────────

#[test]
fn glob_dotfile_match() {
    let s = settings_with_override("**/.env");
    assert!(did_match(&s, ".env"));
    assert!(did_match(&s, "config/.env"));
}

#[test]
fn glob_matches_file_named_constructor() {
    // Ensure no prototype-pollution style weirdness with special JS names.
    let s = settings_with_override("**/*.js");
    assert!(did_match(&s, "constructor.js"));
    assert!(did_match(&s, "__proto__.js"));
    assert!(did_match(&s, "toString.js"));
}

#[test]
fn glob_matches_file_with_spaces() {
    let s = settings_with_override("**/*.txt");
    assert!(did_match(&s, "path/to/my file.txt"));
}

#[test]
fn glob_matches_file_with_unicode() {
    let s = settings_with_override("**/*.rs");
    assert!(did_match(&s, "src/modulo_ñ.rs"));
}

// ── 6. multiple overlapping overrides ─────────────────────────────

#[test]
fn glob_multiple_overrides_both_match() {
    let s = settings_with_two_overrides("**/*.ts", "src/**");
    let effective = apply_overrides(&s, Path::new("src/app.ts"));
    // Both overrides should fire
    assert!(effective.words.contains(&"override_a".to_string()));
    assert!(effective.words.contains(&"override_b".to_string()));
}

#[test]
fn glob_multiple_overrides_only_first_matches() {
    let s = settings_with_two_overrides("**/*.ts", "src/**");
    let effective = apply_overrides(&s, Path::new("lib/app.ts"));
    assert!(effective.words.contains(&"override_a".to_string()));
    assert!(!effective.words.contains(&"override_b".to_string()));
}

#[test]
fn glob_multiple_overrides_only_second_matches() {
    let s = settings_with_two_overrides("**/*.ts", "src/**");
    let effective = apply_overrides(&s, Path::new("src/app.rs"));
    assert!(!effective.words.contains(&"override_a".to_string()));
    assert!(effective.words.contains(&"override_b".to_string()));
}

#[test]
fn glob_multiple_overrides_neither_matches() {
    let s = settings_with_two_overrides("**/*.ts", "src/**");
    let effective = apply_overrides(&s, Path::new("lib/app.rs"));
    assert!(!effective.words.contains(&"override_a".to_string()));
    assert!(!effective.words.contains(&"override_b".to_string()));
    // base word should always be present
    assert!(effective.words.contains(&"base".to_string()));
}

// ── 7. generated / multi-extension patterns ───────────────────────

#[test]
fn glob_multi_extension_generated_files() {
    let s = settings_with_override("**/*.generated.*");
    assert!(did_match(&s, "src/types.generated.ts"));
    assert!(did_match(&s, "models.generated.rs"));
    assert!(!did_match(&s, "src/types.ts"));
}

#[test]
fn glob_test_file_pattern() {
    let s = settings_with_override("**/*.test.{ts,js}");
    assert!(did_match(&s, "src/utils.test.ts"));
    assert!(did_match(&s, "utils.test.js"));
    assert!(!did_match(&s, "utils.spec.ts"));
    assert!(!did_match(&s, "utils.test.py"));
}

// ── 8. exact filename match ───────────────────────────────────────

#[test]
fn glob_exact_filename() {
    let s = settings_with_override("Makefile");
    assert!(did_match(&s, "Makefile"));
}

#[test]
fn glob_exact_filename_in_subdir() {
    // "Makefile" should match a file called Makefile in a subdirectory
    // via the filename-only fallback in matches_override.
    let s = settings_with_override("Makefile");
    assert!(did_match(&s, "subdir/Makefile"));
}

#[test]
fn glob_exact_filename_rejects_partial() {
    let s = settings_with_override("Makefile");
    assert!(!did_match(&s, "Makefile.bak"));
}

// ── 9. question mark wildcard ─────────────────────────────────────

#[test]
fn glob_question_mark_matches_single_char() {
    let s = settings_with_override("file?.txt");
    assert!(did_match(&s, "file1.txt"));
    assert!(did_match(&s, "fileA.txt"));
}

#[test]
fn glob_question_mark_rejects_multiple_chars() {
    let s = settings_with_override("file?.txt");
    assert!(!did_match(&s, "file12.txt"));
}

// ── 10. override settings merge correctness ───────────────────────

#[test]
fn glob_override_merges_dictionaries() {
    let s = CSpellSettings {
        dictionaries: vec!["base-dict".into()],
        overrides: vec![OverrideSettings {
            filename: "**/*.rs".into(),
            dictionaries: vec!["rust-dict".into()],
            ..Default::default()
        }],
        ..Default::default()
    };

    let effective = apply_overrides(&s, Path::new("src/main.rs"));
    assert!(effective.dictionaries.contains(&"base-dict".to_string()));
    assert!(effective.dictionaries.contains(&"rust-dict".to_string()));
}

#[test]
fn glob_override_merges_ignore_words() {
    let s = CSpellSettings {
        ignore_words: vec!["todo".into()],
        overrides: vec![OverrideSettings {
            filename: "**/*.md".into(),
            ignore_words: vec!["tbd".into()],
            ..Default::default()
        }],
        ..Default::default()
    };

    let effective = apply_overrides(&s, Path::new("README.md"));
    assert!(effective.ignore_words.contains(&"todo".to_string()));
    assert!(effective.ignore_words.contains(&"tbd".to_string()));
}

#[test]
fn glob_override_merges_flag_words() {
    let s = CSpellSettings {
        flag_words: vec!["hte".into()],
        overrides: vec![OverrideSettings {
            filename: "**/*.txt".into(),
            flag_words: vec!["teh".into()],
            ..Default::default()
        }],
        ..Default::default()
    };

    let effective = apply_overrides(&s, Path::new("notes.txt"));
    assert!(effective.flag_words.contains(&"hte".to_string()));
    assert!(effective.flag_words.contains(&"teh".to_string()));
}

#[test]
fn glob_override_sets_case_sensitive() {
    let s = CSpellSettings {
        case_sensitive: None,
        overrides: vec![OverrideSettings {
            filename: "**/*.rs".into(),
            case_sensitive: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };

    let effective = apply_overrides(&s, Path::new("src/lib.rs"));
    assert_eq!(effective.case_sensitive, Some(true));
}

#[test]
fn glob_override_no_match_preserves_base_settings() {
    let s = CSpellSettings {
        case_sensitive: Some(false),
        dictionaries: vec!["en_us".into()],
        overrides: vec![OverrideSettings {
            filename: "**/*.rs".into(),
            case_sensitive: Some(true),
            dictionaries: vec!["rust-dict".into()],
            ..Default::default()
        }],
        ..Default::default()
    };

    let effective = apply_overrides(&s, Path::new("script.py"));
    assert_eq!(effective.case_sensitive, Some(false));
    assert_eq!(effective.dictionaries, vec!["en_us".to_string()]);
}

// ── 11. ignore_reg_exp_list merge ─────────────────────────────────

#[test]
fn glob_override_merges_ignore_regexp() {
    let s = CSpellSettings {
        ignore_reg_exp_list: vec!["https?://.*".into()],
        overrides: vec![OverrideSettings {
            filename: "**/*.md".into(),
            ignore_reg_exp_list: vec!["\\[.*?\\]\\(.*?\\)".into()],
            ..Default::default()
        }],
        ..Default::default()
    };

    let effective = apply_overrides(&s, Path::new("docs/guide.md"));
    assert_eq!(effective.ignore_reg_exp_list.len(), 2);
}

// ── 12. compiled overrides path ───────────────────────────────────

#[test]
fn compiled_overrides_match_same_as_dynamic() {
    use matchum_config::overrides::{apply_compiled_overrides, compile_overrides};

    let s = CSpellSettings {
        words: vec!["base".into()],
        overrides: vec![
            OverrideSettings {
                filename: "**/*.ts".into(),
                words: vec!["typescript".into()],
                ..Default::default()
            },
            OverrideSettings {
                filename: "**/*.rs".into(),
                words: vec!["rustlang".into()],
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let compiled = compile_overrides(&s);
    let path = Path::new("src/main.ts");

    let dynamic = apply_overrides(&s, path);
    let compiled_result = apply_compiled_overrides(&s, path, &compiled).unwrap();

    assert_eq!(dynamic.words, compiled_result.words);
}

#[test]
fn compiled_overrides_returns_none_when_no_match() {
    use matchum_config::overrides::{apply_compiled_overrides, compile_overrides};

    let s = CSpellSettings {
        overrides: vec![OverrideSettings {
            filename: "**/*.ts".into(),
            words: vec!["typescript".into()],
            ..Default::default()
        }],
        ..Default::default()
    };

    let compiled = compile_overrides(&s);
    let result = apply_compiled_overrides(&s, Path::new("main.rs"), &compiled);
    assert!(result.is_none());
}
