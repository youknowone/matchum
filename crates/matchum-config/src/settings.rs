use serde::Serializer;
use serde::de::Deserializer;
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Main cspell configuration, matching cspell.json schema.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CSpellSettings {
    pub version: Option<String>,
    pub language: Option<String>,
    #[serde(deserialize_with = "deserialize_optional_language_id", default)]
    pub language_id: Option<String>,
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

    #[serde(skip)]
    pub resolved_files: Option<GlobPatternSet>,
    #[serde(skip)]
    pub resolved_ignore_paths: GlobPatternSet,

    // Behavior
    pub case_sensitive: Option<bool>,
    pub allow_compound_words: Option<bool>,
    pub min_word_length: Option<usize>,
    pub ignore_random_strings: Option<bool>,
    pub min_random_length: Option<usize>,
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GlobDef {
    pub glob: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlobPatternSet(Vec<GlobDef>);

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum GlobPatternItem {
    String(String),
    Def(GlobDef),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum GlobPatternSetDe {
    String(String),
    Def(GlobDef),
    List(Vec<GlobPatternItem>),
}

impl GlobPatternSet {
    pub fn from_glob_defs(patterns: Vec<GlobDef>) -> Self {
        Self(patterns)
    }

    pub fn iter(&self) -> std::slice::Iter<'_, GlobDef> {
        self.0.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, GlobDef> {
        self.0.iter_mut()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn first_glob(&self) -> Option<&str> {
        self.0.first().map(|g| g.glob.as_str())
    }
}

impl From<String> for GlobPatternSet {
    fn from(value: String) -> Self {
        Self(vec![GlobDef {
            glob: value,
            root: None,
            source: None,
        }])
    }
}

impl From<&str> for GlobPatternSet {
    fn from(value: &str) -> Self {
        value.to_string().into()
    }
}

impl fmt::Display for GlobPatternSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.first_glob() {
            Some(glob) => write!(f, "{glob}"),
            None => Ok(()),
        }
    }
}

impl PartialEq<&str> for GlobPatternSet {
    fn eq(&self, other: &&str) -> bool {
        matches!(self.first_glob(), Some(glob) if glob == *other)
    }
}

impl PartialEq<GlobPatternSet> for &str {
    fn eq(&self, other: &GlobPatternSet) -> bool {
        other == self
    }
}

impl Serialize for GlobPatternSet {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.0.as_slice() {
            [] => serializer.serialize_seq(Some(0))?.end(),
            [single] if single.root.is_none() && single.source.is_none() => {
                serializer.serialize_str(&single.glob)
            }
            [single] => single.serialize(serializer),
            many => many.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for GlobPatternSet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let parsed = GlobPatternSetDe::deserialize(deserializer)?;
        let patterns = match parsed {
            GlobPatternSetDe::String(glob) => vec![GlobDef {
                glob,
                root: None,
                source: None,
            }],
            GlobPatternSetDe::Def(def) => vec![def],
            GlobPatternSetDe::List(items) => items
                .into_iter()
                .map(|item| match item {
                    GlobPatternItem::String(glob) => GlobDef {
                        glob,
                        root: None,
                        source: None,
                    },
                    GlobPatternItem::Def(def) => def,
                })
                .collect(),
        };
        Ok(Self(patterns))
    }
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
    pub r#type: Option<String>,
    pub use_compounds: Option<bool>,
    pub ignore_forbidden_words: Option<bool>,
    pub support_non_strict_searches: Option<bool>,
    #[serde(default)]
    pub words: Vec<String>,
    #[serde(default)]
    pub flag_words: Vec<String>,
    #[serde(default)]
    pub suggest_words: Vec<String>,
    #[serde(default)]
    pub rep_map: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternDefinition {
    pub name: String,
    pub pattern: StringOrList,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
#[derive(Default)]
pub struct OverrideSettings {
    pub filename: GlobPatternSet,
    pub words: Vec<String>,
    pub ignore_words: Vec<String>,
    pub flag_words: Vec<String>,
    pub dictionaries: Vec<String>,
    pub dictionary_definitions: Vec<DictionaryDefinition>,
    pub language: Option<String>,
    /// Can be a single string ("html") or an array (["html", "css", "typescript"]).
    #[serde(deserialize_with = "deserialize_optional_language_id", default)]
    pub language_id: Option<String>,
    pub case_sensitive: Option<bool>,
    pub allow_compound_words: Option<bool>,
    pub min_word_length: Option<usize>,
    pub ignore_random_strings: Option<bool>,
    pub min_random_length: Option<usize>,
    pub max_duplicate_problems: Option<usize>,
    pub max_number_of_problems: Option<usize>,
    pub ignore_reg_exp_list: Vec<String>,
    pub include_reg_exp_list: Vec<String>,
    pub patterns: Vec<PatternDefinition>,
    pub suggest_words: Vec<String>,
    pub no_suggest_dictionaries: Vec<String>,
    pub language_settings: Vec<LanguageSetting>,
    pub enabled: Option<bool>,
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
    /// Words to add to the dictionary for matching files.
    pub words: Vec<String>,
    /// Words to ignore for matching files.
    pub ignore_words: Vec<String>,
    /// Words to flag as errors for matching files.
    pub flag_words: Vec<String>,
    /// Enable / disable checking for matching files.
    pub enabled: Option<bool>,
    /// Case sensitivity override for matching files.
    pub case_sensitive: Option<bool>,
    /// Allow compound words override for matching files.
    pub allow_compound_words: Option<bool>,
    /// Additional ignore patterns for matching files.
    pub ignore_reg_exp_list: Vec<String>,
    /// Additional pattern definitions scoped to this language setting.
    pub patterns: Vec<PatternDefinition>,
    /// Dictionary definitions scoped to this language setting.
    pub dictionary_definitions: Vec<DictionaryDefinition>,
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

/// Deserialize `languageId` in OverrideSettings: can be a string, an array, or absent.
/// Arrays are joined with a comma into a single string (cspell format).
fn deserialize_optional_language_id<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<StringOrList> = Deserialize::deserialize(deserializer)?;
    Ok(value.map(|v| {
        let parts = v.into_vec();
        parts.join(",")
    }))
}
