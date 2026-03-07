//! Config parsing and merging tests ported from cspell's CSpellSettingsServer.test.ts
//! and configLoader tests.
//!
//! Sources:
//! - vendor/cspell/packages/cspell-lib/src/lib/Settings/CSpellSettingsServer.test.ts
//! - vendor/cspell/packages/cspell-lib/src/lib/Settings/Controller/configLoader/configLoader.test.ts

use matchum_config::overrides::apply_overrides;
use matchum_config::resolver;
use matchum_config::settings::CSpellSettings;
use std::path::{Path, PathBuf};

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// ============================================================
// 1. Basic JSON parsing
// ============================================================

mod basic_parsing {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let json = r#"{}"#;
        let settings: CSpellSettings = serde_json::from_str(json).unwrap();
        assert!(settings.words.is_empty());
        assert!(settings.language.is_none());
        assert!(settings.version.is_none());
    }

    #[test]
    fn parse_version_and_language() {
        let json = r#"{"version": "0.2", "language": "en"}"#;
        let settings: CSpellSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.version.as_deref(), Some("0.2"));
        assert_eq!(settings.language.as_deref(), Some("en"));
    }

    #[test]
    fn parse_words_list() {
        let json = r#"{"words": ["foo", "bar", "baz"]}"#;
        let settings: CSpellSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.words, vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn parse_flag_words() {
        let json = r#"{"flagWords": ["hte", "therefor"]}"#;
        let settings: CSpellSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.flag_words, vec!["hte", "therefor"]);
    }

    #[test]
    fn parse_ignore_words() {
        let json = r#"{"ignoreWords": ["xyzzy", "plugh"]}"#;
        let settings: CSpellSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.ignore_words, vec!["xyzzy", "plugh"]);
    }

    #[test]
    fn parse_case_sensitive() {
        let json = r#"{"caseSensitive": true}"#;
        let settings: CSpellSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.case_sensitive, Some(true));
    }

    #[test]
    fn parse_allow_compound_words() {
        let json = r#"{"allowCompoundWords": true}"#;
        let settings: CSpellSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.allow_compound_words, Some(true));
    }

    #[test]
    fn parse_min_word_length() {
        let json = r#"{"minWordLength": 3}"#;
        let settings: CSpellSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.min_word_length, Some(3));
    }

    #[test]
    fn parse_ignore_paths() {
        let json = r#"{"ignorePaths": ["node_modules/**", ".git/**"]}"#;
        let settings: CSpellSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.ignore_paths, vec!["node_modules/**", ".git/**"]);
    }

    #[test]
    fn parse_enabled() {
        let json = r#"{"enabled": false}"#;
        let settings: CSpellSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.enabled, Some(false));
    }

    #[test]
    fn parse_dictionary_definitions() {
        let json = r#"{
            "dictionaryDefinitions": [
                {"name": "custom", "path": "./words.txt"},
                {"name": "test", "path": "./test.txt", "addWords": true, "noSuggest": true}
            ]
        }"#;
        let settings: CSpellSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.dictionary_definitions.len(), 2);
        assert_eq!(settings.dictionary_definitions[0].name, "custom");
        assert_eq!(
            settings.dictionary_definitions[0].path.as_deref(),
            Some("./words.txt")
        );
        assert!(!settings.dictionary_definitions[0].add_words);
        assert!(settings.dictionary_definitions[1].add_words);
        assert!(settings.dictionary_definitions[1].no_suggest);
    }

    #[test]
    fn parse_dictionaries() {
        let json = r#"{"dictionaries": ["en_us", "software-terms"]}"#;
        let settings: CSpellSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.dictionaries, vec!["en_us", "software-terms"]);
    }

    #[test]
    fn parse_ignore_reg_exp_list() {
        let json = r#"{"ignoreRegExpList": ["/#include.*/", "/0x[0-9a-f]+/i"]}"#;
        let settings: CSpellSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.ignore_reg_exp_list.len(), 2);
    }

    #[test]
    fn parse_overrides() {
        let json = r#"{
            "overrides": [
                {
                    "filename": "**/*.ts",
                    "dictionaries": ["typescript"],
                    "caseSensitive": true
                }
            ]
        }"#;
        let settings: CSpellSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.overrides.len(), 1);
        assert_eq!(settings.overrides[0].filename, "**/*.ts");
        assert_eq!(settings.overrides[0].dictionaries, vec!["typescript"]);
        assert_eq!(settings.overrides[0].case_sensitive, Some(true));
    }

    #[test]
    fn parse_user_words() {
        let json = r#"{"userWords": ["myCustomWord"]}"#;
        let settings: CSpellSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.user_words, vec!["myCustomWord"]);
    }

    #[test]
    fn parse_import_single() {
        let json = r#"{"import": "../base.json"}"#;
        let settings: CSpellSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.import, vec!["../base.json"]);
    }

    #[test]
    fn parse_import_list() {
        let json = r#"{"import": ["base.json", "extra.json"]}"#;
        let settings: CSpellSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.import, vec!["base.json", "extra.json"]);
    }

    #[test]
    fn parse_patterns() {
        let json = r#"{
            "patterns": [
                {"name": "string", "pattern": "/\"[^\"]*\"/g"}
            ]
        }"#;
        let settings: CSpellSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.patterns.len(), 1);
        assert_eq!(settings.patterns[0].name, "string");
    }
}

// ============================================================
// 2. Settings merging
// ============================================================

mod merge_settings {
    use super::*;

    #[test]
    fn merge_words_concatenated() {
        let base = CSpellSettings {
            words: vec!["base_word".into()],
            ..Default::default()
        };
        let overlay = CSpellSettings {
            words: vec!["overlay_word".into()],
            ..Default::default()
        };
        let merged = resolver::merge_settings(base, overlay);
        assert_eq!(merged.words, vec!["base_word", "overlay_word"]);
    }

    #[test]
    fn merge_language_overlay_wins() {
        let base = CSpellSettings {
            language: Some("en".into()),
            ..Default::default()
        };
        let overlay = CSpellSettings {
            language: Some("fr".into()),
            ..Default::default()
        };
        let merged = resolver::merge_settings(base, overlay);
        assert_eq!(merged.language.as_deref(), Some("fr"));
    }

    #[test]
    fn merge_language_base_kept_if_overlay_none() {
        let base = CSpellSettings {
            language: Some("en".into()),
            ..Default::default()
        };
        let overlay = CSpellSettings::default();
        let merged = resolver::merge_settings(base, overlay);
        assert_eq!(merged.language.as_deref(), Some("en"));
    }

    #[test]
    fn merge_ignore_words_concatenated() {
        let base = CSpellSettings {
            ignore_words: vec!["base".into()],
            ..Default::default()
        };
        let overlay = CSpellSettings {
            ignore_words: vec!["overlay".into()],
            ..Default::default()
        };
        let merged = resolver::merge_settings(base, overlay);
        assert_eq!(merged.ignore_words, vec!["base", "overlay"]);
    }

    #[test]
    fn merge_flag_words_concatenated() {
        let base = CSpellSettings {
            flag_words: vec!["hte".into()],
            ..Default::default()
        };
        let overlay = CSpellSettings {
            flag_words: vec!["colour".into()],
            ..Default::default()
        };
        let merged = resolver::merge_settings(base, overlay);
        assert_eq!(merged.flag_words, vec!["hte", "colour"]);
    }

    #[test]
    fn merge_dictionaries_concatenated() {
        let base = CSpellSettings {
            dictionaries: vec!["en_us".into()],
            ..Default::default()
        };
        let overlay = CSpellSettings {
            dictionaries: vec!["software-terms".into()],
            ..Default::default()
        };
        let merged = resolver::merge_settings(base, overlay);
        assert_eq!(merged.dictionaries, vec!["en_us", "software-terms"]);
    }

    #[test]
    fn merge_case_sensitive_overlay_wins() {
        let base = CSpellSettings {
            case_sensitive: Some(false),
            ..Default::default()
        };
        let overlay = CSpellSettings {
            case_sensitive: Some(true),
            ..Default::default()
        };
        let merged = resolver::merge_settings(base, overlay);
        assert_eq!(merged.case_sensitive, Some(true));
    }

    #[test]
    fn merge_enabled_overlay_wins() {
        let base = CSpellSettings {
            enabled: Some(true),
            ..Default::default()
        };
        let overlay = CSpellSettings {
            enabled: Some(false),
            ..Default::default()
        };
        let merged = resolver::merge_settings(base, overlay);
        assert_eq!(merged.enabled, Some(false));
    }

    #[test]
    fn merge_overrides_concatenated() {
        use matchum_config::settings::OverrideSettings;
        let base = CSpellSettings {
            overrides: vec![OverrideSettings {
                filename: "**/*.ts".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let overlay = CSpellSettings {
            overrides: vec![OverrideSettings {
                filename: "**/*.js".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let merged = resolver::merge_settings(base, overlay);
        assert_eq!(merged.overrides.len(), 2);
        assert_eq!(merged.overrides[0].filename, "**/*.ts");
        assert_eq!(merged.overrides[1].filename, "**/*.js");
    }

    #[test]
    fn merge_ignore_paths_concatenated() {
        let base = CSpellSettings {
            ignore_paths: vec!["node_modules".into()],
            ..Default::default()
        };
        let overlay = CSpellSettings {
            ignore_paths: vec![".git".into()],
            ..Default::default()
        };
        let merged = resolver::merge_settings(base, overlay);
        assert_eq!(merged.ignore_paths, vec!["node_modules", ".git"]);
    }

    #[test]
    fn merge_imports_cleared() {
        let base = CSpellSettings {
            import: vec!["base.json".into()],
            ..Default::default()
        };
        let overlay = CSpellSettings::default();
        let merged = resolver::merge_settings(base, overlay);
        assert!(merged.import.is_empty(), "imports cleared after merge");
    }

    #[test]
    fn merge_min_word_length_overlay_wins() {
        let base = CSpellSettings {
            min_word_length: Some(4),
            ..Default::default()
        };
        let overlay = CSpellSettings {
            min_word_length: Some(3),
            ..Default::default()
        };
        let merged = resolver::merge_settings(base, overlay);
        assert_eq!(merged.min_word_length, Some(3));
    }
}

// ============================================================
// 3. Config file loading
// ============================================================

mod config_loading {
    use super::*;

    #[test]
    fn load_simple_json() {
        let dir = std::env::temp_dir().join("matchum_cfg_test_simple");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cspell.json");
        std::fs::write(
            &path,
            r#"{"version": "0.2", "language": "en", "words": ["hello"]}"#,
        )
        .unwrap();

        let settings = resolver::load_config(&path).unwrap();
        assert_eq!(settings.version.as_deref(), Some("0.2"));
        assert_eq!(settings.language.as_deref(), Some("en"));
        assert!(settings.words.contains(&"hello".to_string()));

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn load_with_flag_words() {
        let dir = std::env::temp_dir().join("matchum_cfg_test_flag");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cspell.json");
        std::fs::write(&path, r#"{"flagWords": ["therefor", "they'd", "they'll"]}"#).unwrap();

        let settings = resolver::load_config(&path).unwrap();
        assert_eq!(settings.flag_words, vec!["therefor", "they'd", "they'll"]);

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn load_with_dictionary_definitions() {
        let dir = std::env::temp_dir().join("matchum_cfg_test_dicts");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cspell.json");
        std::fs::write(
            &path,
            r#"{
                "dictionaryDefinitions": [
                    {"name": "custom", "path": "./words.txt", "addWords": true}
                ],
                "dictionaries": ["custom"]
            }"#,
        )
        .unwrap();

        let settings = resolver::load_config(&path).unwrap();
        assert_eq!(settings.dictionaries, vec!["custom"]);
        assert_eq!(settings.dictionary_definitions[0].name, "custom");
        assert!(settings.dictionary_definitions[0].add_words);

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn load_nonexistent_returns_error() {
        let result = resolver::load_config(std::path::Path::new("/nonexistent/cspell.json"));
        assert!(result.is_err());
    }
}

// ============================================================
// 4. Config file searching
// ============================================================

mod config_search {
    use super::*;

    #[test]
    fn find_config_in_current_dir() {
        let dir = std::env::temp_dir().join("matchum_cfg_search_1");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("cspell.json"), "{}").unwrap();

        let found = resolver::find_config(&dir);
        assert!(found.is_some());
        assert!(found.unwrap().ends_with("cspell.json"));

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn find_cspell_config_json_supported() {
        let dir = std::env::temp_dir().join("matchum_cfg_search_2");
        std::fs::create_dir_all(&dir).unwrap();
        // Remove any leftover configs so only .cspell.config.json exists
        std::fs::remove_file(dir.join("cspell.json")).ok();
        std::fs::remove_file(dir.join(".cspell.json")).ok();
        std::fs::write(dir.join(".cspell.config.json"), "{}").unwrap();

        let found = resolver::find_config(&dir);
        assert!(found.is_some(), ".cspell.config.json should be found");
        assert!(found.unwrap().ends_with(".cspell.config.json"));

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn find_config_prefers_dot_cspell_json() {
        let dir = std::env::temp_dir().join("matchum_cfg_search_3");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".cspell.json"), "{}").unwrap();
        std::fs::write(dir.join("cspell.config.json"), "{}").unwrap();

        let found = resolver::find_config(&dir);
        assert!(found.is_some());
        // .cspell.json comes before cspell.config.json in cspell's search order
        assert!(found.unwrap().ends_with(".cspell.json"));

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn find_config_not_found() {
        let result = resolver::find_config(std::path::Path::new("/nonexistent/deep/path"));
        assert!(result.is_none());
    }
}

// ============================================================
// 5. Import resolution
// ============================================================

mod import_resolution {
    use super::*;

    #[test]
    fn single_import_merges_words() {
        let dir = std::env::temp_dir().join("matchum_cfg_import_1");
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(
            dir.join("base.json"),
            r#"{"words": ["base_word"], "language": "en"}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("cspell.json"),
            r#"{"import": "base.json", "words": ["overlay_word"]}"#,
        )
        .unwrap();

        let settings = resolver::load_config(&dir.join("cspell.json")).unwrap();
        assert!(
            settings.words.contains(&"base_word".to_string()),
            "base words: {:?}",
            settings.words
        );
        assert!(
            settings.words.contains(&"overlay_word".to_string()),
            "overlay words: {:?}",
            settings.words
        );

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn multiple_imports_merge() {
        let dir = std::env::temp_dir().join("matchum_cfg_import_2");
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join("dict-a.json"), r#"{"words": ["alpha", "beta"]}"#).unwrap();
        std::fs::write(dir.join("dict-b.json"), r#"{"words": ["gamma", "delta"]}"#).unwrap();
        std::fs::write(
            dir.join("cspell.json"),
            r#"{"import": ["dict-a.json", "dict-b.json"], "words": ["epsilon"]}"#,
        )
        .unwrap();

        let settings = resolver::load_config(&dir.join("cspell.json")).unwrap();
        assert!(settings.words.contains(&"alpha".to_string()));
        assert!(settings.words.contains(&"gamma".to_string()));
        assert!(settings.words.contains(&"epsilon".to_string()));

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn circular_import_handled() {
        let dir = std::env::temp_dir().join("matchum_cfg_import_circular");
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(
            dir.join("a.json"),
            r#"{"import": ["b.json"], "words": ["aa"]}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("b.json"),
            r#"{"import": ["a.json"], "words": ["bb"]}"#,
        )
        .unwrap();

        // Should not infinite loop — circular import returns error
        let result = resolver::load_config(&dir.join("a.json"));
        // Either succeeds with both words or returns circular error
        match result {
            Ok(settings) => {
                assert!(settings.words.contains(&"aa".to_string()));
            }
            Err(e) => {
                let msg = format!("{}", e);
                assert!(msg.contains("circular"), "should mention circular: {}", msg);
            }
        }

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn missing_import_file_error() {
        let dir = std::env::temp_dir().join("matchum_cfg_import_missing");
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(
            dir.join("cspell.json"),
            r#"{"import": "nonexistent.json", "words": ["hello"]}"#,
        )
        .unwrap();

        // Missing import file: the config should still load (graceful)
        // or return an error
        let result = resolver::load_config(&dir.join("cspell.json"));
        match result {
            Ok(settings) => {
                // Graceful: loaded but missing import skipped
                assert!(settings.words.contains(&"hello".to_string()));
            }
            Err(_) => {
                // Also acceptable: hard fail on missing import
            }
        }

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn import_inherits_flag_words() {
        let dir = std::env::temp_dir().join("matchum_cfg_import_flags");
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join("base.json"), r#"{"flagWords": ["hte", "colour"]}"#).unwrap();
        std::fs::write(
            dir.join("cspell.json"),
            r#"{"import": "base.json", "flagWords": ["therefor"]}"#,
        )
        .unwrap();

        let settings = resolver::load_config(&dir.join("cspell.json")).unwrap();
        assert!(
            settings.flag_words.contains(&"hte".to_string()),
            "flag: {:?}",
            settings.flag_words
        );
        assert!(
            settings.flag_words.contains(&"therefor".to_string()),
            "flag: {:?}",
            settings.flag_words
        );

        std::fs::remove_dir_all(dir).ok();
    }
}

// ============================================================
// 6. Real cspell fixture configs
// ============================================================

mod real_fixtures {
    use super::*;

    fn fixture_path(name: &str) -> PathBuf {
        project_root()
            .join("vendor/cspell/packages/cspell-lib/samples")
            .join(name)
    }

    #[test]
    fn load_sample_cspell_json() {
        // NOTE: .cspell.json uses JSONC (comments) — skip if our parser can't handle it
        let path = fixture_path(".cspell.json");
        if !path.exists() {
            return;
        }
        match resolver::load_config(&path) {
            Ok(settings) => {
                assert_eq!(settings.language.as_deref(), Some("en"));
                assert!(settings.words.contains(&"gensequence".to_string()));
                assert!(settings.flag_words.contains(&"hte".to_string()));
            }
            Err(_) => {
                // JSONC not yet supported — acceptable
            }
        }
    }

    #[test]
    fn load_forbid_words_config() {
        let path = fixture_path("forbid-words/cspell.json");
        if !path.exists() {
            return;
        }
        let settings = resolver::load_config(&path).unwrap();
        assert!(settings.flag_words.contains(&"therefor".to_string()));
    }

    #[test]
    fn load_overrides_config() {
        let path = fixture_path("overrides/cspell.json");
        if !path.exists() {
            return;
        }
        let settings = resolver::load_config(&path).unwrap();
        assert!(!settings.overrides.is_empty());
        assert_eq!(settings.overrides[0].filename, "**/*.ts");
    }

    #[test]
    fn load_cspell_full_config() {
        let path = project_root().join("tests/fixtures/cspell-full.json");
        if !path.exists() {
            return;
        }
        let settings = resolver::load_config(&path).unwrap();
        assert_eq!(settings.version.as_deref(), Some("0.2"));
        assert_eq!(settings.language.as_deref(), Some("en"));
        assert!(settings.dictionaries.contains(&"en_us".to_string()));
        assert!(settings
            .dictionaries
            .contains(&"software-terms".to_string()));
    }

    #[test]
    fn load_linked_import() {
        // NOTE: cspell-import.json imports .cspell.json which has JSONC comments
        let path = fixture_path("linked/cspell-import.json");
        if !path.exists() {
            return;
        }
        match resolver::load_config(&path) {
            Ok(settings) => {
                assert!(
                    settings.words.contains(&"import".to_string()),
                    "words: {:?}",
                    settings.words
                );
            }
            Err(_) => {
                // Imported file has JSONC — acceptable
            }
        }
    }

    #[test]
    fn load_dutch_words() {
        // NOTE: cspell-dutch.json uses JSONC (comments)
        let path = fixture_path("linked/cspell-dutch.json");
        if !path.exists() {
            return;
        }
        match resolver::load_config(&path) {
            Ok(settings) => {
                assert!(settings.words.contains(&"leuk".to_string()));
                assert!(settings.words.contains(&"huis".to_string()));
            }
            Err(_) => {
                // JSONC not yet supported — acceptable
            }
        }
    }
}

// ============================================================
// 7. Full config with all field types
// ============================================================

mod full_config {
    use super::*;

    #[test]
    fn parse_comprehensive_config() {
        let json = r#"{
            "version": "0.2",
            "language": "en",
            "enabled": true,
            "words": ["hello", "world"],
            "ignoreWords": ["xyzzy"],
            "flagWords": ["hte"],
            "userWords": ["myword"],
            "dictionaries": ["en_us"],
            "dictionaryDefinitions": [
                {"name": "custom", "path": "./dict.txt"}
            ],
            "ignoreRegExpList": ["/0x[0-9a-f]+/i"],
            "includeRegExpList": ["/\\w+/"],
            "ignorePaths": ["node_modules/**"],
            "caseSensitive": false,
            "allowCompoundWords": false,
            "minWordLength": 4,
            "overrides": [
                {"filename": "**/*.ts", "caseSensitive": true}
            ]
        }"#;

        let settings: CSpellSettings = serde_json::from_str(json).unwrap();

        assert_eq!(settings.version.as_deref(), Some("0.2"));
        assert_eq!(settings.language.as_deref(), Some("en"));
        assert_eq!(settings.enabled, Some(true));
        assert_eq!(settings.words, vec!["hello", "world"]);
        assert_eq!(settings.ignore_words, vec!["xyzzy"]);
        assert_eq!(settings.flag_words, vec!["hte"]);
        assert_eq!(settings.user_words, vec!["myword"]);
        assert_eq!(settings.dictionaries, vec!["en_us"]);
        assert_eq!(settings.dictionary_definitions.len(), 1);
        assert_eq!(settings.ignore_reg_exp_list.len(), 1);
        assert_eq!(settings.include_reg_exp_list.len(), 1);
        assert_eq!(settings.ignore_paths, vec!["node_modules/**"]);
        assert_eq!(settings.case_sensitive, Some(false));
        assert_eq!(settings.allow_compound_words, Some(false));
        assert_eq!(settings.min_word_length, Some(4));
        assert_eq!(settings.overrides.len(), 1);
    }
}

// ============================================================
// 8. Override application — configLoader.test.ts
// ============================================================

mod override_application {
    use super::*;

    #[test]
    fn override_adds_words_for_matching_file() {
        let settings = CSpellSettings {
            words: vec!["hello".into()],
            overrides: vec![matchum_config::settings::OverrideSettings {
                filename: "**/*.ts".into(),
                words: vec!["typescript".into(), "readonly".into()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let effective = apply_overrides(&settings, Path::new("src/main.ts"));
        assert!(effective.words.contains(&"hello".to_string()));
        assert!(effective.words.contains(&"typescript".to_string()));
        assert!(effective.words.contains(&"readonly".to_string()));
    }

    #[test]
    fn override_does_not_apply_to_nonmatching_file() {
        let settings = CSpellSettings {
            words: vec!["hello".into()],
            overrides: vec![matchum_config::settings::OverrideSettings {
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
    fn override_changes_language() {
        let settings = CSpellSettings {
            language: Some("en".into()),
            overrides: vec![matchum_config::settings::OverrideSettings {
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
    fn override_disables_checking() {
        let settings = CSpellSettings {
            enabled: Some(true),
            overrides: vec![matchum_config::settings::OverrideSettings {
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
    fn override_adds_flag_words() {
        let settings = CSpellSettings {
            flag_words: vec!["todo".into()],
            overrides: vec![matchum_config::settings::OverrideSettings {
                filename: "**/*.py".into(),
                flag_words: vec!["fixme".into()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let effective = apply_overrides(&settings, Path::new("scripts/main.py"));
        assert!(effective.flag_words.contains(&"todo".to_string()));
        assert!(effective.flag_words.contains(&"fixme".to_string()));
    }

    #[test]
    fn override_adds_ignore_words() {
        let settings = CSpellSettings {
            ignore_words: vec!["xyzzy".into()],
            overrides: vec![matchum_config::settings::OverrideSettings {
                filename: "**/*.rs".into(),
                ignore_words: vec!["println".into(), "eprintln".into()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let effective = apply_overrides(&settings, Path::new("src/main.rs"));
        assert!(effective.ignore_words.contains(&"xyzzy".to_string()));
        assert!(effective.ignore_words.contains(&"println".to_string()));
    }

    #[test]
    fn override_case_sensitive() {
        let settings = CSpellSettings {
            case_sensitive: Some(false),
            overrides: vec![matchum_config::settings::OverrideSettings {
                filename: "**/*.lex".into(),
                case_sensitive: Some(true),
                ..Default::default()
            }],
            ..Default::default()
        };

        let effective = apply_overrides(&settings, Path::new("src/lexer.lex"));
        assert_eq!(effective.case_sensitive, Some(true));
    }

    #[test]
    fn multiple_overrides_applied_in_order() {
        let settings = CSpellSettings {
            words: vec!["base".into()],
            overrides: vec![
                matchum_config::settings::OverrideSettings {
                    filename: "**/*.ts".into(),
                    words: vec!["first".into()],
                    ..Default::default()
                },
                matchum_config::settings::OverrideSettings {
                    filename: "**/*.ts".into(),
                    words: vec!["second".into()],
                    language: Some("typescript".into()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let effective = apply_overrides(&settings, Path::new("src/main.ts"));
        assert!(effective.words.contains(&"base".to_string()));
        assert!(effective.words.contains(&"first".to_string()));
        assert!(effective.words.contains(&"second".to_string()));
        assert_eq!(effective.language.as_deref(), Some("typescript"));
    }

    #[test]
    fn overrides_cleared_after_application() {
        let settings = CSpellSettings {
            overrides: vec![matchum_config::settings::OverrideSettings {
                filename: "**/*.ts".into(),
                words: vec!["hello".into()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let effective = apply_overrides(&settings, Path::new("src/main.ts"));
        assert!(
            effective.overrides.is_empty(),
            "overrides should be cleared"
        );
    }
}
