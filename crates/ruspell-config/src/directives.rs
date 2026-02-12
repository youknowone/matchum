use aho_corasick::{AhoCorasick, AhoCorasickKind};
use regex::Regex;
use std::sync::LazyLock;

/// An inline spell-checking directive parsed from a comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Directive {
    Disable,
    Enable,
    DisableLine,
    DisableNextLine,
    Ignore(Vec<String>),
    Words(Vec<String>),
    EnableCompoundWords,
    DisableCompoundWords,
    /// cspell:ignoreRegExp /pattern/
    IgnoreRegExp(String),
    /// cspell:language en-US
    Language(String),
    /// cspell:dictionaries dict1 dict2
    Dictionaries(Vec<String>),
    /// cspell:forbid word1 word2 / cspell:flag word1 word2
    ForbidWords(Vec<String>),
    /// cspell:includeRegExp /pattern/
    IncludeRegExp(String),
    /// cspell:enableCaseSensitive
    EnableCaseSensitive,
    /// cspell:disableCaseSensitive
    DisableCaseSensitive,
    /// Emacs-style: LocalWords: word1 word2
    LocalWords(Vec<String>),
}

// Matches: cSpell:, cspell::, spell-checker:, spellchecker:
// From cspell's InDocSettings.ts: /\b(?:spell-?checker|c?spell)::?(.*)/
static DIRECTIVE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:spell-?checker|c?spell)::?(.+)").unwrap());

/// Aho-Corasick DFA automaton for fast case-insensitive multi-pattern pre-filter.
/// All directive prefixes contain "spell" or "local". DFA mode for maximum
/// throughput on the tiny 2-pattern set.
pub static DIRECTIVE_PREFILTER: LazyLock<AhoCorasick> = LazyLock::new(|| {
    AhoCorasick::builder()
        .ascii_case_insensitive(true)
        .kind(Some(AhoCorasickKind::DFA))
        .build(["spell", "local"])
        .unwrap()
});

/// Parse a line of text for an inline cspell directive.
pub fn parse_directive(line: &str) -> Option<Directive> {
    // Quick rejection: skip regex for lines without directive keywords
    if !DIRECTIVE_PREFILTER.is_match(line) {
        return None;
    }
    parse_directive_inner(line)
}

/// Parse a directive without the AC prefilter check.
/// Use when the caller has already verified the line contains a directive keyword.
pub fn parse_directive_no_prefilter(line: &str) -> Option<Directive> {
    parse_directive_inner(line)
}

fn parse_directive_inner(line: &str) -> Option<Directive> {
    // Check for Emacs-style "LocalWords:" first
    if let Some(lw) = parse_local_words(line) {
        return Some(lw);
    }

    let caps = DIRECTIVE_RE.captures(line)?;
    let directive_text = caps.get(1)?.as_str().trim();

    // Match the directive keyword (order matters for prefix matching)
    let lower = directive_text.to_lowercase();

    if lower.starts_with("disable-next-line") {
        Some(Directive::DisableNextLine)
    } else if lower.starts_with("disable-next") {
        // "disable-next" is a synonym for "disable-next-line"
        Some(Directive::DisableNextLine)
    } else if lower.starts_with("disable-line") {
        Some(Directive::DisableLine)
    } else if lower.starts_with("disablecasesensitive")
        || lower.starts_with("disable-case-sensitive")
    {
        Some(Directive::DisableCaseSensitive)
    } else if lower.starts_with("disablecompoundwords")
        || lower.starts_with("disable-compound-words")
    {
        Some(Directive::DisableCompoundWords)
    } else if lower.starts_with("disable") {
        Some(Directive::Disable)
    } else if lower.starts_with("enablecasesensitive") || lower.starts_with("enable-case-sensitive")
    {
        Some(Directive::EnableCaseSensitive)
    } else if lower.starts_with("enablecompoundwords") || lower.starts_with("enable-compound-words")
    {
        Some(Directive::EnableCompoundWords)
    } else if lower.starts_with("enable") {
        Some(Directive::Enable)
    } else if lower.starts_with("includeregexp") || lower.starts_with("include-reg-exp") {
        let prefix_len = if lower.starts_with("includeregexp") {
            13
        } else {
            15
        };
        let rest = directive_text[prefix_len..].trim();
        Some(Directive::IncludeRegExp(rest.to_string()))
    } else if lower.starts_with("ignoreregexp") || lower.starts_with("ignore-reg-exp") {
        let prefix_len = if lower.starts_with("ignoreregexp") {
            12
        } else {
            14
        };
        let rest = directive_text[prefix_len..].trim();
        Some(Directive::IgnoreRegExp(rest.to_string()))
    } else if lower.starts_with("forbid") || lower.starts_with("flag") {
        let prefix_len = if lower.starts_with("forbid") { 6 } else { 4 };
        let mut rest = &directive_text[prefix_len..];
        let rest_lower = rest.to_lowercase();
        // Strip optional "-words", "-word", "words", "word" suffix
        if rest_lower.starts_with("-words") {
            rest = &rest[6..];
        } else if rest_lower.starts_with("-word") {
            rest = &rest[5..];
        } else if rest_lower.starts_with("words") {
            rest = &rest[5..];
        } else if rest_lower.starts_with("word") {
            rest = &rest[4..];
        }
        let words = parse_word_list(rest);
        Some(Directive::ForbidWords(words))
    } else if lower.starts_with("ignore") {
        let mut rest = &directive_text[6..]; // len("ignore") = 6
                                             // Strip optional "-words", "-word", "words", "word" suffix
        let rest_lower = rest.to_lowercase();
        if rest_lower.starts_with("-words") {
            rest = &rest[6..];
        } else if rest_lower.starts_with("-word") {
            rest = &rest[5..];
        } else if rest_lower.starts_with("words") {
            rest = &rest[5..];
        } else if rest_lower.starts_with("word") {
            rest = &rest[4..];
        }
        let words = parse_word_list(rest);
        Some(Directive::Ignore(words))
    } else if lower.starts_with("words") {
        let rest = &directive_text[5..]; // len("words") = 5
        let words = parse_word_list(rest);
        Some(Directive::Words(words))
    } else if lower.starts_with("word") && !lower.starts_with("words") {
        let rest = &directive_text[4..]; // len("word") = 4
        let words = parse_word_list(rest);
        Some(Directive::Words(words))
    } else if lower.starts_with("language") || lower.starts_with("locale") {
        let prefix_len = if lower.starts_with("language") { 8 } else { 6 };
        let rest = directive_text[prefix_len..].trim();
        Some(Directive::Language(rest.to_string()))
    } else if lower.starts_with("dictionaries") {
        let rest = &directive_text[12..]; // len("dictionaries") = 12
        let dicts = parse_word_list(rest);
        Some(Directive::Dictionaries(dicts))
    } else {
        None
    }
}

static LOCAL_WORDS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bLocalWords\s*:\s*(.+)").unwrap());

fn parse_local_words(line: &str) -> Option<Directive> {
    let caps = LOCAL_WORDS_RE.captures(line)?;
    let rest = caps.get(1)?.as_str().trim();
    let words = parse_word_list(rest);
    if words.is_empty() {
        None
    } else {
        Some(Directive::LocalWords(words))
    }
}

fn parse_word_list(text: &str) -> Vec<String> {
    text.split([',', ' ', ';'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disable() {
        assert_eq!(
            parse_directive("// cSpell:disable"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn test_enable() {
        assert_eq!(parse_directive("// cSpell:enable"), Some(Directive::Enable));
    }

    #[test]
    fn test_disable_line() {
        assert_eq!(
            parse_directive("const x = 1; // spell-checker:disable-line"),
            Some(Directive::DisableLine)
        );
    }

    #[test]
    fn test_disable_next_line() {
        assert_eq!(
            parse_directive("// cspell:disable-next-line"),
            Some(Directive::DisableNextLine)
        );
    }

    #[test]
    fn test_ignore_words() {
        assert_eq!(
            parse_directive("// cSpell:ignore myword anotherword"),
            Some(Directive::Ignore(vec![
                "myword".into(),
                "anotherword".into()
            ]))
        );
    }

    #[test]
    fn test_ignore_words_comma_separated() {
        assert_eq!(
            parse_directive("// cspell:ignore foo,bar,baz"),
            Some(Directive::Ignore(vec![
                "foo".into(),
                "bar".into(),
                "baz".into()
            ]))
        );
    }

    #[test]
    fn test_words_directive() {
        assert_eq!(
            parse_directive("// cspell:words customword"),
            Some(Directive::Words(vec!["customword".into()]))
        );
    }

    #[test]
    fn test_spellchecker_prefix() {
        assert_eq!(
            parse_directive("// spellchecker:disable"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn test_spell_checker_prefix() {
        assert_eq!(
            parse_directive("# spell-checker: disable"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn test_no_directive() {
        assert_eq!(parse_directive("// this is a normal comment"), None);
    }

    #[test]
    fn test_case_insensitive() {
        assert_eq!(
            parse_directive("// CSPELL:DISABLE"),
            Some(Directive::Disable)
        );
    }
}
