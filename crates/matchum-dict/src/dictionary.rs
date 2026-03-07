/// Result of looking up a word in a dictionary.
#[derive(Debug, Clone, Default)]
pub struct FindResult {
    pub found: bool,
    pub forbidden: bool,
    pub no_suggest: bool,
}

/// A spell-checking dictionary.
pub trait Dictionary: Send + Sync {
    /// Check if the word exists in the dictionary.
    fn has(&self, word: &str) -> bool;

    /// Check if the word is forbidden.
    fn is_forbidden(&self, word: &str) -> bool;

    /// Get spelling suggestions for a word.
    fn suggest(&self, word: &str, limit: usize) -> Vec<String>;

    /// Detailed lookup.
    fn find(&self, word: &str) -> FindResult;

    /// Number of words in the dictionary.
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether this dictionary has any forbidden words.
    /// Used to skip is_forbidden() calls entirely when no words are forbidden.
    fn has_forbidden_words(&self) -> bool {
        false
    }

    /// Whether this dictionary uses case-sensitive matching.
    /// When false, `has()` internally normalizes to lowercase.
    fn is_case_sensitive(&self) -> bool {
        false
    }

    /// Check if a word exists using a pre-normalized (lowercase) form.
    /// Avoids redundant normalization when the caller has already computed it.
    /// `word` is the original-case word (for ALL-CAPS ucfirst fallback),
    /// `normalized` is the lowercase/NFC form.
    fn has_pre_normalized(&self, word: &str, _normalized: &str) -> bool {
        self.has(word)
    }

    /// Check if a word is forbidden using a pre-normalized form.
    /// Avoids redundant normalization when the caller has already computed it.
    fn is_forbidden_pre_normalized(&self, _word: &str, _normalized: &str) -> bool {
        false
    }
}
