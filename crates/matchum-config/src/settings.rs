use serde::de::Deserializer;
use serde::{Deserialize, Serialize};

/// Main cspell configuration, matching cspell.json schema.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CSpellSettings {
    pub version: Option<String>,
    pub language: Option<String>,
    pub enabled: Option<bool>,

    // Word lists
    pub words: Vec<String>,
    pub ignore_words: Vec<String>,
    pub flag_words: Vec<String>,
    pub user_words: Vec<String>,

    // Dictionaries
    pub dictionaries: Vec<String>,
    pub dictionary_definitions: Vec<DictionaryDefinition>,

    // Patterns
    pub ignore_reg_exp_list: Vec<String>,
    pub include_reg_exp_list: Vec<String>,
    pub patterns: Vec<PatternDefinition>,

    // File selection
    pub files: Option<Vec<String>>,
    pub ignore_paths: Vec<String>,
    pub use_gitignore: Option<bool>,

    // Behavior
    pub case_sensitive: Option<bool>,
    pub allow_compound_words: Option<bool>,
    pub min_word_length: Option<usize>,
    pub max_duplicate_problems: Option<usize>,
    pub max_number_of_problems: Option<usize>,
    pub glob_root: Option<String>,
    pub suggest_words: Vec<String>,
    pub no_suggest_dictionaries: Vec<String>,

    // Character replacement mapping (e.g., [["ss", "ß"], ["ae", "ä"]])
    pub rep_map: Vec<Vec<String>>,

    // Inheritance — can be a single string or list of strings
    #[serde(deserialize_with = "deserialize_import", default)]
    pub import: Vec<String>,

    // Overrides
    pub overrides: Vec<OverrideSettings>,

    // Language settings (per-file-type dictionary activation)
    pub language_settings: Vec<LanguageSetting>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryDefinition {
    pub name: String,
    pub path: Option<String>,
    #[serde(default)]
    pub add_words: bool,
    #[serde(default)]
    pub no_suggest: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternDefinition {
    pub name: String,
    pub pattern: StringOrList,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OverrideSettings {
    pub filename: String,
    pub words: Vec<String>,
    pub ignore_words: Vec<String>,
    pub flag_words: Vec<String>,
    pub dictionaries: Vec<String>,
    pub language: Option<String>,
    pub case_sensitive: Option<bool>,
    pub ignore_reg_exp_list: Vec<String>,
    pub enabled: Option<bool>,
}

impl Default for OverrideSettings {
    fn default() -> Self {
        Self {
            filename: String::new(),
            words: Vec::new(),
            ignore_words: Vec::new(),
            flag_words: Vec::new(),
            dictionaries: Vec::new(),
            language: None,
            case_sensitive: None,
            ignore_reg_exp_list: Vec::new(),
            enabled: None,
        }
    }
}

/// Per-language-type settings for dictionary activation.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LanguageSetting {
    /// Programming language ID filter. Can be a single string ("rust"),
    /// a comma-separated list ("c,cpp"), or a JSON array (["c","cpp","rust"]).
    #[serde(deserialize_with = "deserialize_language_id", default)]
    pub language_id: Vec<String>,
    /// Locale filter (e.g. "en", "en-GB", "*").
    pub locale: Option<String>,
    /// Dictionaries to activate for matching files.
    pub dictionaries: Vec<String>,
    /// Additional ignore patterns for matching files.
    pub ignore_reg_exp_list: Vec<String>,
    /// Additional pattern definitions scoped to this language setting.
    pub patterns: Vec<PatternDefinition>,
}

/// Either a single string or a list of strings.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StringOrList {
    Single(String),
    List(Vec<String>),
}

impl StringOrList {
    pub fn into_vec(self) -> Vec<String> {
        match self {
            StringOrList::Single(s) => vec![s],
            StringOrList::List(v) => v,
        }
    }
}

/// Deserialize `import` which can be a string, list of strings, or absent.
fn deserialize_import<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: StringOrList = Deserialize::deserialize(deserializer)?;
    Ok(value.into_vec())
}

/// Deserialize `languageId` which can be a string or list of strings.
/// A comma-separated string like `"c,cpp"` is split into individual IDs.
fn deserialize_language_id<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: StringOrList = Deserialize::deserialize(deserializer)?;
    let raw = value.into_vec();
    // Expand comma-separated entries
    Ok(raw
        .into_iter()
        .flat_map(|s| {
            s.split(',')
                .map(|p| p.trim().to_string())
                .collect::<Vec<_>>()
        })
        .collect())
}
