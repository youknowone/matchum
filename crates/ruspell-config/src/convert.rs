use crate::ruspell_config::{
    RuspellConfig, RuspellDictDef, RuspellLanguageSetting, RuspellOverride,
};
use crate::settings::{
    CSpellSettings, DictionaryDefinition, LanguageSetting, OverrideSettings, StringOrList,
};
use std::path::Path;

/// Load a word-list file: one word per line, `#` comments, blank lines skipped.
pub fn load_word_file(path: &Path) -> Vec<String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect()
}

/// Convert `RuspellConfig` to the internal `CSpellSettings`, loading word files.
pub fn to_cspell_settings(config: &RuspellConfig, config_dir: &Path) -> CSpellSettings {
    let mut s = CSpellSettings::default();

    s.language = config.language.clone();
    s.enabled = config.enabled;

    if let Some(ref p) = config.words_file {
        s.words = load_word_file(&config_dir.join(p));
    }
    if let Some(ref p) = config.flag_words_file {
        s.flag_words = load_word_file(&config_dir.join(p));
    }

    // Auto-discover .ruspell/dict/*.txt as dictionary definitions
    discover_dict_dir(&config_dir.join(".ruspell/dict"), &mut s);

    s.dictionaries.extend(config.dictionaries.iter().cloned());
    s.dictionary_definitions
        .extend(
            config
                .dictionary_definitions
                .iter()
                .map(|d| DictionaryDefinition {
                    name: d.name.clone(),
                    path: d.path.clone(),
                    add_words: d.add_words,
                    no_suggest: d.no_suggest,
                }),
        );

    s.ignore_reg_exp_list = config.ignore_patterns.clone();
    s.include_reg_exp_list = config.include_patterns.clone();
    s.ignore_paths = config.ignore_paths.clone();
    s.use_gitignore = config.use_gitignore;
    s.case_sensitive = config.case_sensitive;
    s.allow_compound_words = config.allow_compound_words;
    s.min_word_length = config.min_word_length;

    s.overrides = config
        .overrides
        .iter()
        .map(|ov| {
            let mut o = OverrideSettings {
                filename: ov.filename.clone(),
                dictionaries: ov.dictionaries.clone(),
                language: ov.language.clone(),
                case_sensitive: ov.case_sensitive,
                ignore_reg_exp_list: ov.ignore_patterns.clone(),
                enabled: ov.enabled,
                ..Default::default()
            };
            if let Some(ref p) = ov.words_file {
                o.words = load_word_file(&config_dir.join(p));
            }
            if let Some(ref p) = ov.flag_words_file {
                o.flag_words = load_word_file(&config_dir.join(p));
            }
            o
        })
        .collect();

    s.language_settings = config
        .language_settings
        .iter()
        .map(|ls| LanguageSetting {
            language_id: ls.language_id.clone(),
            locale: ls.locale.clone(),
            dictionaries: ls.dictionaries.clone(),
        })
        .collect();

    s
}

/// File to write during migration: (relative_path, content).
pub type WordFile = (String, String);

/// Convert `CSpellSettings` to `RuspellConfig`, extracting inline words to files.
///
/// Returns the config and a list of word files to create.
pub fn from_cspell_settings(settings: &CSpellSettings) -> (RuspellConfig, Vec<WordFile>) {
    let mut config = RuspellConfig::default();
    let mut files: Vec<WordFile> = Vec::new();

    config.language = settings.language.clone();
    config.enabled = settings.enabled;

    if !settings.words.is_empty() {
        let path = ".ruspell/words.txt".to_string();
        files.push((path.clone(), words_to_content(&settings.words)));
        config.words_file = Some(path);
    }
    if !settings.flag_words.is_empty() {
        let path = ".ruspell/flagged.txt".to_string();
        files.push((path.clone(), words_to_content(&settings.flag_words)));
        config.flag_words_file = Some(path);
    }

    config.dictionaries = settings.dictionaries.clone();
    config.dictionary_definitions = settings
        .dictionary_definitions
        .iter()
        .map(|d| RuspellDictDef {
            name: d.name.clone(),
            path: d.path.clone(),
            add_words: d.add_words,
            no_suggest: d.no_suggest,
        })
        .collect();

    config.ignore_patterns = settings.ignore_reg_exp_list.clone();
    config.include_patterns = settings.include_reg_exp_list.clone();
    config.ignore_paths = settings.ignore_paths.clone();
    config.use_gitignore = settings.use_gitignore;
    config.case_sensitive = settings.case_sensitive;
    config.allow_compound_words = settings.allow_compound_words;
    config.min_word_length = settings.min_word_length;

    // Convert patterns (StringOrList → Vec<String> not needed in ruspell.toml, skip named patterns)
    // Named patterns are regex-only in cspell; we flatten them into ignore_patterns.
    for pat in &settings.patterns {
        let regexes = match &pat.pattern {
            StringOrList::Single(s) => vec![s.clone()],
            StringOrList::List(v) => v.clone(),
        };
        for regex in regexes {
            if !config.ignore_patterns.contains(&regex) {
                config.ignore_patterns.push(regex);
            }
        }
    }

    for (i, ov) in settings.overrides.iter().enumerate() {
        let mut r_ov = RuspellOverride {
            filename: ov.filename.clone(),
            dictionaries: ov.dictionaries.clone(),
            language: ov.language.clone(),
            case_sensitive: ov.case_sensitive,
            ignore_patterns: ov.ignore_reg_exp_list.clone(),
            enabled: ov.enabled,
            ..Default::default()
        };
        if !ov.words.is_empty() {
            let path = override_word_path(&ov.filename, "words", i);
            files.push((path.clone(), words_to_content(&ov.words)));
            r_ov.words_file = Some(path);
        }
        if !ov.flag_words.is_empty() {
            let path = override_word_path(&ov.filename, "flagged", i);
            files.push((path.clone(), words_to_content(&ov.flag_words)));
            r_ov.flag_words_file = Some(path);
        }
        config.overrides.push(r_ov);
    }

    config.language_settings = settings
        .language_settings
        .iter()
        .map(|ls| RuspellLanguageSetting {
            language_id: ls.language_id.clone(),
            locale: ls.locale.clone(),
            dictionaries: ls.dictionaries.clone(),
        })
        .collect();

    (config, files)
}

fn words_to_content(words: &[String]) -> String {
    let mut content = String::new();
    for word in words {
        content.push_str(word);
        content.push('\n');
    }
    content
}

/// Generate a word file path for an override, deriving a short name from the glob pattern.
fn override_word_path(filename_glob: &str, kind: &str, index: usize) -> String {
    // Extract extension-like part: "**/*.rs" → "rs", "**/*.{ts,tsx}" → "ts-tsx"
    let slug = filename_glob
        .rsplit('/')
        .next()
        .unwrap_or(filename_glob)
        .trim_start_matches("*.")
        .replace('{', "")
        .replace('}', "")
        .replace(',', "-")
        .replace('*', "all");
    let slug = if slug.is_empty() {
        format!("{index}")
    } else {
        slug
    };
    format!(".ruspell/override-{slug}-{kind}.txt")
}

/// Scan a directory for `*.txt` files and register them as dictionary definitions.
fn discover_dict_dir(dict_dir: &Path, s: &mut CSpellSettings) {
    if !dict_dir.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dict_dir) else {
        return;
    };
    let mut paths: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    paths.sort_by_key(|e| e.file_name());
    for entry in paths {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        s.words.extend(load_word_file(&path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::PatternDefinition;

    #[test]
    fn roundtrip_simple() {
        let settings = CSpellSettings {
            language: Some("en".into()),
            words: vec!["ruspell".into(), "cspell".into()],
            dictionaries: vec!["en_us".into()],
            ignore_paths: vec!["target".into()],
            ..Default::default()
        };

        let (config, files) = from_cspell_settings(&settings);
        assert_eq!(config.language.as_deref(), Some("en"));
        assert_eq!(config.words_file.as_deref(), Some(".ruspell/words.txt"));
        assert!(config.flag_words_file.is_none());
        assert_eq!(config.dictionaries, vec!["en_us"]);

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, ".ruspell/words.txt");
        assert_eq!(files[0].1, "ruspell\ncspell\n");

        // Write files to temp dir and convert back
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".ruspell")).unwrap();
        for (path, content) in &files {
            std::fs::write(dir.path().join(path), content).unwrap();
        }

        let restored = to_cspell_settings(&config, dir.path());
        assert_eq!(restored.language, settings.language);
        assert_eq!(restored.words, settings.words);
        assert_eq!(restored.dictionaries, settings.dictionaries);
    }

    #[test]
    fn override_word_file_naming() {
        let settings = CSpellSettings {
            overrides: vec![OverrideSettings {
                filename: "**/*.rs".into(),
                words: vec!["tokio".into()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let (config, files) = from_cspell_settings(&settings);
        assert_eq!(config.overrides.len(), 1);
        assert_eq!(
            config.overrides[0].words_file.as_deref(),
            Some(".ruspell/override-rs-words.txt")
        );
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].1, "tokio\n");
    }

    #[test]
    fn load_word_file_with_comments() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("words.txt");
        std::fs::write(
            &path,
            "# this is a comment\nfoo\n\n  bar  \n# another comment\nbaz\n",
        )
        .unwrap();

        let words = load_word_file(&path);
        assert_eq!(words, vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn discover_dict_dir_loads_words_only() {
        let dir = tempfile::tempdir().unwrap();
        let dict_dir = dir.path().join(".ruspell/dict");
        std::fs::create_dir_all(&dict_dir).unwrap();
        std::fs::write(dict_dir.join("project.txt"), "tokio\nrayon\n").unwrap();
        std::fs::write(dict_dir.join("names.txt"), "alice\nbob\n").unwrap();
        // non-txt file should be ignored
        std::fs::write(dict_dir.join("readme.md"), "ignored").unwrap();

        let config = RuspellConfig::default();
        let settings = to_cspell_settings(&config, dir.path());

        // Words from both txt files should be loaded
        assert!(settings.words.contains(&"tokio".to_string()));
        assert!(settings.words.contains(&"rayon".to_string()));
        assert!(settings.words.contains(&"alice".to_string()));
        assert!(settings.words.contains(&"bob".to_string()));
        // Non-txt content should not appear
        assert!(!settings.words.contains(&"ignored".to_string()));
        // dictionaries list should remain empty (no auto-populate)
        assert!(settings.dictionaries.is_empty());
    }

    #[test]
    fn discover_dict_dir_missing_dir_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        // No .ruspell/dict directory exists

        let config = RuspellConfig::default();
        let settings = to_cspell_settings(&config, dir.path());

        assert!(settings.words.is_empty());
        assert!(settings.dictionaries.is_empty());
    }

    #[test]
    fn discover_does_not_override_config_dictionaries() {
        let dir = tempfile::tempdir().unwrap();
        let dict_dir = dir.path().join(".ruspell/dict");
        std::fs::create_dir_all(&dict_dir).unwrap();
        std::fs::write(dict_dir.join("extra.txt"), "extraword\n").unwrap();

        let config = RuspellConfig {
            dictionaries: vec!["en_us".into(), "rust".into()],
            ..Default::default()
        };
        let settings = to_cspell_settings(&config, dir.path());

        // Config dictionaries preserved
        assert_eq!(settings.dictionaries, vec!["en_us", "rust"]);
        // Auto-discovered words still loaded
        assert!(settings.words.contains(&"extraword".to_string()));
    }

    #[test]
    fn patterns_flattened() {
        let settings = CSpellSettings {
            patterns: vec![PatternDefinition {
                name: "urls".into(),
                pattern: StringOrList::List(vec!["/https?:\\/\\//".into(), "/ftp:\\/\\//".into()]),
            }],
            ..Default::default()
        };

        let (config, _) = from_cspell_settings(&settings);
        assert_eq!(config.ignore_patterns.len(), 2);
    }
}
