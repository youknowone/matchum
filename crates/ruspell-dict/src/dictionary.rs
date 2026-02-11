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
}
