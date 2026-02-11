use crate::dictionary::{Dictionary, FindResult};
use crate::distance;
use hashbrown::HashSet;

/// A dictionary backed by hash sets for O(1) lookup.
pub struct HashDictionary {
    words: HashSet<String>,
    forbidden: HashSet<String>,
    no_suggest: HashSet<String>,
    case_sensitive: bool,
}

impl HashDictionary {
    pub fn new(case_sensitive: bool) -> Self {
        Self {
            words: HashSet::default(),
            forbidden: HashSet::default(),
            no_suggest: HashSet::default(),
            case_sensitive,
        }
    }

    pub fn add_word(&mut self, word: &str) {
        self.words.insert(self.normalize(word));
    }

    pub fn add_forbidden(&mut self, word: &str) {
        self.forbidden.insert(self.normalize(word));
    }

    pub fn add_no_suggest(&mut self, word: &str) {
        self.no_suggest.insert(self.normalize(word));
        // Still a valid word, just don't suggest it
        self.words.insert(self.normalize(word));
    }

    fn normalize(&self, word: &str) -> String {
        if self.case_sensitive {
            word.to_string()
        } else {
            word.to_lowercase()
        }
    }
}

impl Dictionary for HashDictionary {
    fn has(&self, word: &str) -> bool {
        self.words.contains(&self.normalize(word))
    }

    fn is_forbidden(&self, word: &str) -> bool {
        self.forbidden.contains(&self.normalize(word))
    }

    fn suggest(&self, word: &str, limit: usize) -> Vec<String> {
        let normalized = self.normalize(word);
        // Filter out no-suggest and forbidden words from candidates
        let candidates = self.words.iter().filter_map(|w| {
            if self.no_suggest.contains(w) || self.forbidden.contains(w) {
                None
            } else {
                Some(w.as_str())
            }
        });
        let max_edits = 2; // default max edit distance for suggestions
        distance::select_nearest_words(&normalized, candidates, max_edits, limit)
            .into_iter()
            .map(|(w, _)| w)
            .collect()
    }

    fn find(&self, word: &str) -> FindResult {
        let normalized = self.normalize(word);
        FindResult {
            found: self.words.contains(&normalized),
            forbidden: self.forbidden.contains(&normalized),
            no_suggest: self.no_suggest.contains(&normalized),
        }
    }

    fn len(&self) -> usize {
        self.words.len()
    }
}
