/// Character replacement mapping for language-specific substitutions.
/// Enables matching words with alternative spellings (e.g., ae->a, ss->ß).
pub struct RepMap {
    entries: Vec<(String, String)>,
}

impl RepMap {
    pub fn new(pairs: Vec<(String, String)>) -> Self {
        let mut entries = Vec::new();
        for (from, to) in pairs {
            for alt in from.split('|') {
                if !alt.is_empty() {
                    entries.push((alt.to_string(), to.clone()));
                }
            }
        }
        Self { entries }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Generate alternative spellings by applying single-depth replacements.
    /// For each entry (from, to), find all occurrences of `from` in the word
    /// and generate a variant with that occurrence replaced by `to`.
    /// This follows cspell's repMap / outerWordForms directionality.
    pub fn generate_alternatives(&self, word: &str) -> Vec<String> {
        let mut alternatives = Vec::new();
        for (from, to) in &self.entries {
            Self::replace_all_occurrences(word, from, to, &mut alternatives);
        }
        alternatives.sort();
        alternatives.dedup();
        alternatives
    }

    /// Check if any repMap alternative matches the given predicate.
    /// Short-circuits on the first match, avoiding allocation of all alternatives.
    pub fn any_alternative_matches(
        &self,
        word: &str,
        mut predicate: impl FnMut(&str) -> bool,
    ) -> bool {
        for (from, to) in &self.entries {
            if from.is_empty() {
                continue;
            }
            let mut start = 0;
            while let Some(pos) = word[start..].find(from.as_str()) {
                let abs_pos = start + pos;
                let mut alt = String::with_capacity(word.len() + to.len() - from.len());
                alt.push_str(&word[..abs_pos]);
                alt.push_str(to);
                alt.push_str(&word[abs_pos + from.len()..]);
                if predicate(&alt) {
                    return true;
                }
                start = abs_pos + from.len();
            }
        }
        false
    }

    fn replace_all_occurrences(word: &str, from: &str, to: &str, results: &mut Vec<String>) {
        if from.is_empty() {
            return;
        }
        let mut start = 0;
        while let Some(pos) = word[start..].find(from) {
            let abs_pos = start + pos;
            let mut result = String::with_capacity(word.len() + to.len() - from.len());
            result.push_str(&word[..abs_pos]);
            result.push_str(to);
            result.push_str(&word[abs_pos + from.len()..]);
            results.push(result);
            start = abs_pos + from.len();
        }
    }
}

// cspell:disable
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn german_ss_to_eszett() {
        let rm = RepMap::new(vec![("ss".into(), "\u{00df}".into())]);
        let alts = rm.generate_alternatives("strasse");
        assert!(alts.contains(&"stra\u{00df}e".to_string()));
    }

    #[test]
    fn french_accent() {
        let rm = RepMap::new(vec![("e".into(), "\u{00e9}".into())]);
        let alts = rm.generate_alternatives("cafe");
        assert!(alts.contains(&"caf\u{00e9}".to_string()));
    }

    #[test]
    fn reverse_direction() {
        let rm = RepMap::new(vec![("ss".into(), "\u{00df}".into())]);
        let alts = rm.generate_alternatives("stra\u{00df}e");
        assert!(!alts.contains(&"strasse".to_string()));
    }

    #[test]
    fn empty_repmap() {
        let rm = RepMap::new(vec![]);
        assert!(rm.generate_alternatives("hello").is_empty());
    }

    #[test]
    fn no_match() {
        let rm = RepMap::new(vec![("xx".into(), "yy".into())]);
        assert!(rm.generate_alternatives("hello").is_empty());
    }

    #[test]
    fn multiple_occurrences() {
        let rm = RepMap::new(vec![("s".into(), "z".into())]);
        let alts = rm.generate_alternatives("sass");
        // Should generate: "zass", "sazs", "sasz" (forward only)
        assert!(alts.len() >= 3);
    }

    #[test]
    fn alternation_entries_expand_like_cspell() {
        let rm = RepMap::new(vec![("'|`|\u{2019}".into(), "'".into())]);

        let grave = rm.generate_alternatives("doesn`t");
        assert!(grave.contains(&"doesn't".to_string()));

        let curly = rm.generate_alternatives("doesn\u{2019}t");
        assert!(curly.contains(&"doesn't".to_string()));
    }
}
