use compact_str::CompactString;
use hashbrown::HashSet;

/// Result of looking up a word in a dictionary.
#[derive(Debug, Clone, Default)]
pub struct FindResult {
    pub found: bool,
    pub forbidden: bool,
    pub no_suggest: bool,
}

/// Borrowed references to a dictionary's internal word sets.
/// Used to build a merged flat index for O(1) lookup across all dictionaries.
pub struct WordSetsRef<'a> {
    pub words: &'a HashSet<CompactString>,
    pub exact_words: &'a HashSet<CompactString>,
    pub non_strict_words: &'a HashSet<CompactString>,
    pub folded_exact_words: &'a HashSet<CompactString>,
    pub case_sensitive: bool,
}

/// A spell-checking dictionary.
pub trait Dictionary: Send + Sync {
    /// Check if the word exists in the dictionary.
    fn has(&self, word: &str) -> bool;

    /// Check if the word exists using only direct dictionary entries.
    /// This skips repMap / compound fallback so callers can defer expensive checks.
    fn has_direct_only(&self, word: &str) -> bool {
        self.has(word)
    }

    /// Check if the word exists in the dictionary with dictionary-native
    /// compound lookup enabled.
    fn has_with_compounds(&self, word: &str) -> bool {
        self.has(word)
    }

    /// Check if the word is forbidden.
    fn is_forbidden(&self, word: &str) -> bool;

    /// Get spelling suggestions for a word.
    fn suggest(&self, word: &str, limit: usize) -> Vec<String>;

    /// Detailed lookup.
    fn find(&self, word: &str) -> FindResult;

    /// Detailed lookup without dictionary-native compound lookup.
    fn find_without_compounds(&self, word: &str) -> FindResult {
        self.find(word)
    }

    /// Detailed lookup with dictionary-native compound lookup enabled.
    fn find_with_compounds(&self, word: &str) -> FindResult {
        self.find(word)
    }

    /// Whether this dictionary has any `noSuggest` words.
    /// Used to skip expensive ignore checks entirely when absent.
    fn has_no_suggest_words(&self) -> bool {
        false
    }

    /// Check if the word is marked `noSuggest`.
    /// This is cheaper than `find()` because it does not need direct/compound lookup.
    fn is_no_suggest(&self, word: &str) -> bool {
        self.find(word).no_suggest
    }

    /// Check if a word is `noSuggest` using a pre-normalized form.
    /// Implementations may ignore `normalized` when they cannot use it.
    fn is_no_suggest_pre_normalized(&self, word: &str, _normalized: &str) -> bool {
        self.is_no_suggest(word)
    }

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

    /// Check if a word exists using a pre-normalized form without
    /// dictionary-native compound lookup.
    fn has_pre_normalized_without_compounds(&self, word: &str, normalized: &str) -> bool {
        self.has_pre_normalized(word, normalized)
    }

    /// Check if a word exists using a pre-normalized form and only direct dictionary entries.
    fn has_pre_normalized_direct_only(&self, word: &str, normalized: &str) -> bool {
        self.has_pre_normalized(word, normalized)
    }

    /// Check if a word exists using a pre-normalized form with dictionary-native
    /// compound lookup enabled.
    fn has_pre_normalized_with_compounds(&self, word: &str, normalized: &str) -> bool {
        let _ = normalized;
        self.has_with_compounds(word)
    }

    /// Whether `has()` can still succeed after `has_direct_only()` misses.
    fn has_expensive_forms(&self) -> bool {
        false
    }

    /// Check if a word is forbidden using a pre-normalized form.
    /// Avoids redundant normalization when the caller has already computed it.
    fn is_forbidden_pre_normalized(&self, _word: &str, _normalized: &str) -> bool {
        false
    }

    /// Export internal word sets for building a merged flat index.
    /// Returns None for dictionaries that don't support merging.
    fn export_word_sets(&self) -> Option<WordSetsRef<'_>> {
        None
    }
}
