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

    // Inheritance — can be a single string or list of strings
    #[serde(deserialize_with = "deserialize_import", default)]
    pub import: Vec<String>,

    // Overrides
    pub overrides: Vec<OverrideSettings>,
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
