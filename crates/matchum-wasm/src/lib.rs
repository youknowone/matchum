use matchum_core::issue::ValidationIssue;
use matchum_core::validator::{Validator, ValidatorConfig};
use matchum_dict::dictionary::Dictionary;
use matchum_dict::hashdict::HashDictionary;
use matchum_dict::loader::{self, DictFormat};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct WasmIssue {
    word: String,
    line: usize,
    column: usize,
    offset: usize,
    length: usize,
    severity: &'static str,
    suggestions: Vec<String>,
}

impl From<&ValidationIssue> for WasmIssue {
    fn from(issue: &ValidationIssue) -> Self {
        Self {
            length: issue.word.len(),
            word: issue.word.clone(),
            line: issue.line,
            column: issue.column,
            offset: issue.offset,
            severity: if issue.is_forbidden {
                "error"
            } else {
                "warning"
            },
            suggestions: issue.suggestions.clone(),
        }
    }
}

#[derive(Deserialize)]
struct WasmSettings {
    #[serde(default = "default_min_word_length")]
    min_word_length: usize,
    #[serde(default)]
    case_sensitive: bool,
    #[serde(default)]
    allow_compound_words: bool,
    #[serde(default = "default_true")]
    compute_suggestions: bool,
    #[serde(default)]
    words: Vec<String>,
    #[serde(default)]
    user_words: Vec<String>,
    #[serde(default)]
    flag_words: Vec<String>,
    #[serde(default)]
    ignore_words: Vec<String>,
}

fn default_min_word_length() -> usize {
    4
}

fn default_true() -> bool {
    true
}

#[wasm_bindgen]
pub struct WasmChecker {
    named_dicts: Vec<(String, Arc<dyn Dictionary>)>,
    settings: WasmSettings,
}

#[wasm_bindgen]
impl WasmChecker {
    /// Create a new checker from a JSON settings string.
    #[wasm_bindgen(constructor)]
    pub fn new(settings_json: &str) -> Result<WasmChecker, JsError> {
        let settings: WasmSettings =
            serde_json::from_str(settings_json).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(WasmChecker {
            named_dicts: Vec::new(),
            settings,
        })
    }

    /// Add a dictionary from raw bytes.
    /// format: "txt", "txt.gz", "trie", "trie.gz"
    pub fn add_dictionary(&mut self, name: &str, data: &[u8], format: &str) -> Result<(), JsError> {
        let fmt = match format {
            "txt" => DictFormat::Txt,
            "txt.gz" => DictFormat::TxtGz,
            "trie" => DictFormat::TrieV3,
            "trie.gz" => DictFormat::TrieV3Gz,
            _ => return Err(JsError::new(&format!("unknown format: {format}"))),
        };
        let dict = loader::load_dictionary_from_bytes(data, fmt)
            .map_err(|e| JsError::new(&e.to_string()))?;
        self.named_dicts.push((name.to_lowercase(), Arc::new(dict)));
        Ok(())
    }

    /// Add a dictionary from a word list (for config `words`/`flagWords` arrays).
    pub fn add_word_list(&mut self, name: &str, words: Vec<String>) -> Result<(), JsError> {
        let mut dict = HashDictionary::new(false);
        for word in &words {
            dict.add_word(word);
        }
        self.named_dicts.push((name.to_lowercase(), Arc::new(dict)));
        Ok(())
    }

    /// Check text and return issues as a JSON array.
    pub fn check_text(&self, text: &str, language_id: &str) -> String {
        let _ = language_id; // reserved for future languageSettings support
        let validator = self.build_validator();
        let issues = validator.validate_text(text);
        let wasm_issues: Vec<WasmIssue> = issues.iter().map(WasmIssue::from).collect();
        serde_json::to_string(&wasm_issues).unwrap_or_else(|_| "[]".into())
    }

    /// Update settings (e.g., when config file changes).
    pub fn update_settings(&mut self, settings_json: &str) -> Result<(), JsError> {
        self.settings =
            serde_json::from_str(settings_json).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(())
    }

    /// Clear all loaded dictionaries.
    pub fn clear_dictionaries(&mut self) {
        self.named_dicts.clear();
    }
}

impl WasmChecker {
    fn build_validator(&self) -> Validator {
        let mut entries: Vec<(String, Arc<dyn Dictionary>, bool)> = self
            .named_dicts
            .iter()
            .map(|(name, dict)| (name.clone(), Arc::clone(dict), true))
            .collect();

        // Add inline words/user_words dictionary
        if !self.settings.words.is_empty() || !self.settings.user_words.is_empty() {
            let mut inline_dict = HashDictionary::new(false);
            for word in &self.settings.words {
                inline_dict.add_word(word);
            }
            for word in &self.settings.user_words {
                inline_dict.add_word(word);
            }
            entries.push(("__inline_words".into(), Arc::new(inline_dict), true));
        }

        let config = ValidatorConfig {
            min_word_length: self.settings.min_word_length,
            case_sensitive: self.settings.case_sensitive,
            allow_compound_words: self.settings.allow_compound_words,
            compute_suggestions: self.settings.compute_suggestions,
            flag_words: self
                .settings
                .flag_words
                .iter()
                .map(|w| compact_str::CompactString::from(w.to_lowercase()))
                .collect(),
            ignore_words: self
                .settings
                .ignore_words
                .iter()
                .map(|w| compact_str::CompactString::from(w.to_lowercase()))
                .collect(),
            ..Default::default()
        };

        Validator::new_named(entries, config)
    }
}
