use crate::issue::ValidationIssue;
use crate::splitter;
use regex::Regex;
use ruspell_config::directives::{self, Directive};
use ruspell_dict::dictionary::Dictionary;
use std::collections::HashSet;
use std::sync::Arc;

/// Configuration for the validation pipeline.
pub struct ValidatorConfig {
    pub min_word_length: usize,
    pub case_sensitive: bool,
    pub ignore_patterns: Vec<Regex>,
    pub include_patterns: Vec<Regex>,
    pub flag_words: HashSet<String>,
    pub ignore_words: HashSet<String>,
    pub allow_compound_words: bool,
    pub compute_suggestions: bool,
}

impl Default for ValidatorConfig {
    fn default() -> Self {
        Self {
            min_word_length: 4,
            case_sensitive: false,
            ignore_patterns: Vec::new(),
            include_patterns: Vec::new(),
            flag_words: HashSet::new(),
            ignore_words: HashSet::new(),
            allow_compound_words: false,
            compute_suggestions: true,
        }
    }
}

struct DictionaryEntry {
    name: Option<String>,
    dict: Arc<dyn Dictionary>,
    default_active: bool,
}

/// Built-in regex patterns for regions to skip (URLs, emails, hex, etc.).
/// Mirrors cspell's default `ignoreRegExpList` patterns.
fn builtin_skip_patterns() -> Vec<Regex> {
    [
        r"(?i)\b(?:https?|ftp|file)://[^\s]+",
        r"(?i)\b[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}\b",
        r"\b0x[0-9a-fA-F]+\b",
        r"\b[0-9a-fA-F]{40,}\b",
        r"\\[nrtbfv0\\]|\\x[0-9a-fA-F]{2}|\\u[0-9a-fA-F]{4}",
        r#"[A-Z]:\\[\w\\.]+"#,
    ]
    .iter()
    .filter_map(|p| Regex::new(p).ok())
    .collect()
}

/// Validates text against a set of dictionaries.
pub struct Validator {
    dictionaries: Vec<DictionaryEntry>,
    config: ValidatorConfig,
    skip_patterns: Vec<Regex>,
}

impl Validator {
    pub fn new(dictionaries: Vec<Box<dyn Dictionary>>, config: ValidatorConfig) -> Self {
        let dictionaries = dictionaries
            .into_iter()
            .map(|d| DictionaryEntry {
                name: None,
                dict: Arc::from(d),
                default_active: true,
            })
            .collect();
        Self::new_internal(dictionaries, config)
    }

    pub fn new_named(
        dictionaries: Vec<(String, Arc<dyn Dictionary>, bool)>,
        config: ValidatorConfig,
    ) -> Self {
        let dictionaries = dictionaries
            .into_iter()
            .map(|(name, dict, default_active)| DictionaryEntry {
                name: Some(name.to_lowercase()),
                dict,
                default_active,
            })
            .collect();
        Self::new_internal(dictionaries, config)
    }

    fn new_internal(dictionaries: Vec<DictionaryEntry>, config: ValidatorConfig) -> Self {
        let skip_patterns = builtin_skip_patterns();
        Self {
            dictionaries,
            config,
            skip_patterns,
        }
    }

    /// Validate text and return all spelling issues found.
    pub fn validate_text(&self, text: &str) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        let mut disabled = false;
        let mut disable_next_line = false;
        let mut inline_words: HashSet<String> = HashSet::new();
        let mut directive_ignore_patterns: Vec<Regex> = Vec::new();
        let mut directive_dictionaries: Option<HashSet<String>> = None;
        let mut allow_compound_words = self.config.allow_compound_words;

        let mut line_start_offset = 0;

        for (line_idx, line) in text.lines().enumerate() {
            let line_num = line_idx + 1;

            if let Some(directive) = directives::parse_directive(line) {
                match directive {
                    Directive::Disable => {
                        disabled = true;
                        line_start_offset += line.len() + 1;
                        continue;
                    }
                    Directive::Enable => {
                        disabled = false;
                        line_start_offset += line.len() + 1;
                        continue;
                    }
                    Directive::DisableNextLine => {
                        disable_next_line = true;
                        line_start_offset += line.len() + 1;
                        continue;
                    }
                    Directive::DisableLine => {
                        line_start_offset += line.len() + 1;
                        continue;
                    }
                    Directive::Ignore(words)
                    | Directive::Words(words)
                    | Directive::LocalWords(words) => {
                        for w in words {
                            inline_words.insert(w.to_lowercase());
                        }
                        line_start_offset += line.len() + 1;
                        continue;
                    }
                    Directive::IgnoreRegExp(pattern) => {
                        if let Some(re) = parse_regex_pattern(&pattern) {
                            directive_ignore_patterns.push(re);
                        }
                        line_start_offset += line.len() + 1;
                        continue;
                    }
                    Directive::Dictionaries(dicts) => {
                        let set: HashSet<String> =
                            dicts.into_iter().map(|d| d.to_lowercase()).collect();
                        directive_dictionaries = Some(set);
                        line_start_offset += line.len() + 1;
                        continue;
                    }
                    Directive::EnableCompoundWords => {
                        allow_compound_words = true;
                        line_start_offset += line.len() + 1;
                        continue;
                    }
                    Directive::DisableCompoundWords => {
                        allow_compound_words = false;
                        line_start_offset += line.len() + 1;
                        continue;
                    }
                    Directive::Language(_) => {
                        line_start_offset += line.len() + 1;
                        continue;
                    }
                }
            }

            if disabled {
                line_start_offset += line.len() + 1;
                continue;
            }

            if disable_next_line {
                disable_next_line = false;
                line_start_offset += line.len() + 1;
                continue;
            }

            let include_ranges = self.find_include_ranges(line);
            let skip_ranges = self.find_skip_ranges(line, &directive_ignore_patterns);

            let tokens = splitter::extract_words(line);

            for token in &tokens {
                if !include_ranges.is_empty()
                    && !include_ranges
                        .iter()
                        .any(|(start, end)| token.offset >= *start && token.offset < *end)
                {
                    continue;
                }

                if skip_ranges
                    .iter()
                    .any(|(start, end)| token.offset >= *start && token.offset < *end)
                {
                    continue;
                }

                let token_lower = token.text.to_lowercase();
                let token_is_flagged = self.config.flag_words.contains(&token_lower)
                    || self
                        .dictionaries
                        .iter()
                        .filter(|d| self.is_dict_active(d, directive_dictionaries.as_ref()))
                        .any(|d| d.dict.is_forbidden(&token.text));

                if !token_is_flagged
                    && (inline_words.contains(&token_lower)
                        || self.config.ignore_words.contains(&token_lower))
                {
                    continue;
                }

                let parts = splitter::split_camel_case(&token.text);
                let sub_words: Vec<splitter::Word> = if parts.len() <= 1 {
                    vec![token.clone()]
                } else {
                    let mut subs = Vec::new();
                    let mut offset_in_token = 0;
                    for part in &parts {
                        let part_start = token.text[offset_in_token..]
                            .find(part)
                            .map(|pos| offset_in_token + pos)
                            .unwrap_or(offset_in_token);
                        subs.push(splitter::Word {
                            text: part.to_string(),
                            offset: token.offset + part_start,
                        });
                        offset_in_token = part_start + part.len();
                    }
                    subs
                };

                for word in &sub_words {
                    let lower = word.text.to_lowercase();

                    let is_forbidden = self.config.flag_words.contains(&lower)
                        || self
                            .dictionaries
                            .iter()
                            .filter(|d| self.is_dict_active(d, directive_dictionaries.as_ref()))
                            .any(|d| d.dict.is_forbidden(&word.text));

                    if is_forbidden {
                        issues.push(ValidationIssue {
                            word: word.text.clone(),
                            offset: line_start_offset + word.offset,
                            line: line_num,
                            column: word.offset + 1,
                            is_forbidden: true,
                            suggestions: Vec::new(),
                        });
                        continue;
                    }

                    if word.text.len() < self.config.min_word_length {
                        continue;
                    }

                    if inline_words.contains(&lower) || self.config.ignore_words.contains(&lower) {
                        continue;
                    }

                    if self
                        .config
                        .ignore_patterns
                        .iter()
                        .any(|re| re.is_match(&word.text))
                        || directive_ignore_patterns
                            .iter()
                            .any(|re| re.is_match(&word.text))
                    {
                        continue;
                    }

                    if !self.is_word_valid(
                        &word.text,
                        directive_dictionaries.as_ref(),
                        allow_compound_words,
                    ) {
                        let suggestions = if self.config.compute_suggestions {
                            self.get_suggestions(&word.text, directive_dictionaries.as_ref())
                        } else {
                            Vec::new()
                        };
                        issues.push(ValidationIssue {
                            word: word.text.clone(),
                            offset: line_start_offset + word.offset,
                            line: line_num,
                            column: word.offset + 1,
                            is_forbidden: false,
                            suggestions,
                        });
                    }
                }
            }

            line_start_offset += line.len() + 1;
        }

        issues
    }

    fn find_skip_ranges(&self, line: &str, directive_ignore_patterns: &[Regex]) -> Vec<(usize, usize)> {
        let mut ranges = Vec::new();
        for pattern in &self.skip_patterns {
            for m in pattern.find_iter(line) {
                ranges.push((m.start(), m.end()));
            }
        }
        for pattern in &self.config.ignore_patterns {
            for m in pattern.find_iter(line) {
                ranges.push((m.start(), m.end()));
            }
        }
        for pattern in directive_ignore_patterns {
            for m in pattern.find_iter(line) {
                ranges.push((m.start(), m.end()));
            }
        }
        ranges
    }

    fn find_include_ranges(&self, line: &str) -> Vec<(usize, usize)> {
        let mut ranges = Vec::new();
        for pattern in &self.config.include_patterns {
            for m in pattern.find_iter(line) {
                ranges.push((m.start(), m.end()));
            }
        }
        ranges
    }

    fn is_dict_active(
        &self,
        entry: &DictionaryEntry,
        directive_dictionaries: Option<&HashSet<String>>,
    ) -> bool {
        match directive_dictionaries {
            Some(active) => match &entry.name {
                Some(name) => active.contains(name),
                None => true,
            },
            None => entry.default_active,
        }
    }

    fn is_word_valid(
        &self,
        word: &str,
        directive_dictionaries: Option<&HashSet<String>>,
        allow_compound_words: bool,
    ) -> bool {
        if self.has_in_active_dicts(word, directive_dictionaries) {
            return true;
        }

        if !self.config.case_sensitive {
            let lower = word.to_lowercase();
            if lower != word && self.has_in_active_dicts(&lower, directive_dictionaries) {
                return true;
            }
        }

        if !allow_compound_words {
            return false;
        }

        self.is_compound_valid(word, directive_dictionaries)
    }

    fn has_in_active_dicts(
        &self,
        word: &str,
        directive_dictionaries: Option<&HashSet<String>>,
    ) -> bool {
        self.dictionaries
            .iter()
            .filter(|d| self.is_dict_active(d, directive_dictionaries))
            .any(|d| d.dict.has(word))
    }

    fn is_compound_valid(
        &self,
        word: &str,
        directive_dictionaries: Option<&HashSet<String>>,
    ) -> bool {
        if word.len() < 2 {
            return false;
        }

        let mut boundaries: Vec<usize> = word.char_indices().map(|(i, _)| i).collect();
        boundaries.push(word.len());

        for split in boundaries.iter().copied().skip(1).take(boundaries.len().saturating_sub(2)) {
            let (left, right) = word.split_at(split);
            if left.is_empty() || right.is_empty() {
                continue;
            }
            if self.has_in_active_dicts(left, directive_dictionaries)
                && self.has_in_active_dicts(right, directive_dictionaries)
            {
                return true;
            }

            if !self.config.case_sensitive {
                let left_lower = left.to_lowercase();
                let right_lower = right.to_lowercase();
                if self.has_in_active_dicts(&left_lower, directive_dictionaries)
                    && self.has_in_active_dicts(&right_lower, directive_dictionaries)
                {
                    return true;
                }
            }
        }

        false
    }

    fn get_suggestions(&self, word: &str, directive_dictionaries: Option<&HashSet<String>>) -> Vec<String> {
        let mut all = Vec::new();
        for dict in self
            .dictionaries
            .iter()
            .filter(|d| self.is_dict_active(d, directive_dictionaries))
        {
            all.extend(dict.dict.suggest(word, 5));
        }
        all.dedup();
        all.truncate(5);
        all
    }
}

fn parse_regex_pattern(value: &str) -> Option<Regex> {
    let s = value.trim();
    if s.starts_with('/') && s.len() > 1 {
        if let Some(last_slash) = s.rfind('/') {
            if last_slash > 0 {
                let body = &s[1..last_slash];
                let flags = &s[last_slash + 1..];
                let mut prefix = String::new();
                if flags.contains('i') {
                    prefix.push('i');
                }
                if flags.contains('m') {
                    prefix.push('m');
                }
                if flags.contains('s') {
                    prefix.push('s');
                }
                let pat = if prefix.is_empty() {
                    body.to_string()
                } else {
                    format!("(?{}){}", prefix, body)
                };
                return Regex::new(&pat).ok();
            }
        }
    }
    Regex::new(s).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruspell_dict::hashdict::HashDictionary;

    fn make_dict(words: &[&str]) -> Box<dyn Dictionary> {
        let mut dict = HashDictionary::new(false);
        for w in words {
            dict.add_word(w);
        }
        Box::new(dict)
    }

    #[test]
    fn test_valid_text() {
        let dict = make_dict(&["hello", "world"]);
        let validator = Validator::new(vec![dict], ValidatorConfig::default());

        let issues = validator.validate_text("hello world");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_unknown_word() {
        let dict = make_dict(&["hello"]);
        let validator = Validator::new(vec![dict], ValidatorConfig::default());

        let issues = validator.validate_text("hello xyzzy");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].word, "xyzzy");
        assert_eq!(issues[0].line, 1);
        assert!(!issues[0].is_forbidden);
    }
}
