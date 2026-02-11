/// A spelling issue found during validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    /// The misspelled or forbidden word.
    pub word: String,
    /// Byte offset in the source text.
    pub offset: usize,
    /// 1-based line number.
    pub line: usize,
    /// 1-based column number (byte offset within the line + 1).
    pub column: usize,
    /// Whether this is a forbidden word (flagWord).
    pub is_forbidden: bool,
    /// Spelling suggestions.
    pub suggestions: Vec<String>,
}
