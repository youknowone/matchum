use serde::{Deserialize, Serialize};

/// Native ruspell configuration format (`ruspell.toml`).
///
/// Key difference from cspell.json: no inline word lists.
/// Words are stored in separate `.txt` files referenced by path.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct RuspellConfig {
    pub language: Option<String>,
    pub enabled: Option<bool>,

    /// Path to a file containing words to accept (one per line).
    pub words_file: Option<String>,
    /// Path to a file containing forbidden words (one per line).
    pub flag_words_file: Option<String>,

    /// Dictionary names to activate (e.g. "en_us", "rust").
    pub dictionaries: Vec<String>,
    /// Custom dictionary definitions.
    #[serde(rename = "dictionary")]
    pub dictionary_definitions: Vec<RuspellDictDef>,

    /// Regex patterns to ignore during checking.
    pub ignore_patterns: Vec<String>,
    /// Regex patterns to include for checking.
    pub include_patterns: Vec<String>,

    /// Glob patterns for files to skip.
    pub ignore_paths: Vec<String>,
    /// Whether to respect .gitignore.
    pub use_gitignore: Option<bool>,

    pub case_sensitive: Option<bool>,
    pub allow_compound_words: Option<bool>,
    pub min_word_length: Option<usize>,

    /// Per-filename overrides.
    #[serde(rename = "override")]
    pub overrides: Vec<RuspellOverride>,

    /// Per-language-type dictionary activation.
    #[serde(rename = "language_setting")]
    pub language_settings: Vec<RuspellLanguageSetting>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RuspellDictDef {
    pub name: String,
    pub path: Option<String>,
    #[serde(default)]
    pub add_words: bool,
    #[serde(default)]
    pub no_suggest: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct RuspellOverride {
    pub filename: String,
    pub words_file: Option<String>,
    pub flag_words_file: Option<String>,
    pub dictionaries: Vec<String>,
    pub language: Option<String>,
    pub case_sensitive: Option<bool>,
    pub ignore_patterns: Vec<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct RuspellLanguageSetting {
    pub language_id: Vec<String>,
    pub locale: Option<String>,
    pub dictionaries: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_toml() {
        let input = r#"language = "en""#;
        let config: RuspellConfig = toml::from_str(input).unwrap();
        assert_eq!(config.language.as_deref(), Some("en"));
    }

    #[test]
    fn parse_full_toml() {
        let input = r#"
language = "en"
words_file = ".ruspell/words.txt"
dictionaries = ["en_us", "rust"]
ignore_paths = ["target", "node_modules"]

[[dictionary]]
name = "project"
path = "./dicts/project.txt"

[[override]]
filename = "**/*.rs"
dictionaries = ["rust"]

[[language_setting]]
language_id = ["rust"]
dictionaries = ["rust"]
"#;
        let config: RuspellConfig = toml::from_str(input).unwrap();
        assert_eq!(config.dictionaries, vec!["en_us", "rust"]);
        assert_eq!(config.dictionary_definitions.len(), 1);
        assert_eq!(config.dictionary_definitions[0].name, "project");
        assert_eq!(config.overrides.len(), 1);
        assert_eq!(config.overrides[0].filename, "**/*.rs");
        assert_eq!(config.language_settings.len(), 1);
    }

    #[test]
    fn roundtrip_serialize() {
        let config = RuspellConfig {
            language: Some("en".into()),
            dictionaries: vec!["en_us".into()],
            ignore_paths: vec!["target".into()],
            ..Default::default()
        };
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: RuspellConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.language, config.language);
        assert_eq!(parsed.dictionaries, config.dictionaries);
    }
}
