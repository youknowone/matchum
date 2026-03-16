use aho_corasick::{AhoCorasick, AhoCorasickKind};
use regex::Regex;
use std::sync::LazyLock;

/// Valid directive names for typo detection.
const VALID_DIRECTIVES: &[&str] = &[
    "disable",
    "enable",
    "disable-line",
    "disable-next-line",
    "disable-next",
    "ignore",
    "ignoreWord",
    "ignoreWords",
    "ignore-word",
    "ignore-words",
    "word",
    "words",
    "forbid",
    "forbidWord",
    "forbid-word",
    "flag",
    "flagWord",
    "flag-word",
    "enableCompoundWords",
    "enableAllowCompoundWords",
    "disableCompoundWords",
    "disableAllowCompoundWords",
    "enableCaseSensitive",
    "disableCaseSensitive",
    "ignoreRegExp",
    "includeRegExp",
    "dictionary",
    "dictionaries",
    "language",
    "locale",
    "local",
];

/// Warning produced when a directive name appears to be a typo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectiveWarning {
    /// The misspelled directive name found in source.
    pub found: String,
    /// The closest valid directive name, if within edit distance threshold.
    pub suggestion: String,
}

/// Compute the Levenshtein edit distance between two strings.
fn levenshtein(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut prev: Vec<usize> = (0..=b_len).collect();
    let mut curr = vec![0; b_len + 1];

    for (i, ca) in a.bytes().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.bytes().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(curr[j] + 1);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b_len]
}

/// Check whether a directive name is a typo and suggest the closest valid directive.
///
/// Returns `Some(DirectiveWarning)` if `name` is not a valid directive but a valid
/// directive exists within Levenshtein distance <= 3.
/// Returns `None` if `name` is already valid or no close match exists.
pub fn check_directive_typo(name: &str) -> Option<DirectiveWarning> {
    let lower = name.to_lowercase();

    // If it already matches a valid directive (case-insensitive), no typo.
    for &valid in VALID_DIRECTIVES {
        if lower == valid.to_lowercase() {
            return None;
        }
    }

    let mut best: Option<(&str, usize)> = None;
    for &valid in VALID_DIRECTIVES {
        let dist = levenshtein(&lower, &valid.to_lowercase());
        if dist <= 3
            && (best.is_none() || dist < best.unwrap().1) {
                best = Some((valid, dist));
            }
    }

    best.map(|(suggestion, _)| DirectiveWarning {
        found: name.to_string(),
        suggestion: suggestion.to_string(),
    })
}

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

/// Extract the raw directive name from a line containing a cspell prefix.
///
/// Returns the first word after `cspell:` / `spell-checker:` (lowercased),
/// or `None` if the line doesn't contain a directive prefix.
pub fn extract_directive_name(line: &str) -> Option<String> {
    let caps = DIRECTIVE_RE.captures(line)?;
    let text = caps.get(1)?.as_str().trim();
    // The directive name is everything up to the first space or end of text,
    // but compound directives like "disable-next-line" are a single token.
    let name = text.split(|c: char| c.is_whitespace() || c == ':').next()?;
    if name.is_empty() {
        return None;
    }
    Some(name.to_lowercase())
}

/// Parse a line of text for an inline cspell directive.
pub fn parse_directive(line: &str) -> Option<Directive> {
    // Quick rejection: skip regex for lines without directive keywords
    if !DIRECTIVE_PREFILTER.is_match(line) {
        return None;
    }
    // Try cspell-style directives first, then Emacs-style LocalWords
    parse_directive_inner(line).or_else(|| parse_local_words(line))
}

/// Parse a directive without the AC prefilter check.
/// Use when the caller has already verified the line contains a directive keyword.
pub fn parse_directive_no_prefilter(line: &str) -> Option<Directive> {
    parse_directive_inner(line).or_else(|| parse_local_words(line))
}

/// Check that a keyword match is at a word boundary followed by non-hyphen.
/// Matches cspell's `\b(?!-)` semantics: the character after the keyword
/// must not be a word char (`[a-zA-Z0-9_]`) and must not be a hyphen.
fn keyword_boundary(text: &str, keyword_len: usize) -> bool {
    let rest = &text[keyword_len..];
    if rest.is_empty() {
        return true;
    }
    let next = rest.as_bytes()[0];
    !next.is_ascii_alphanumeric() && next != b'_' && next != b'-'
}

/// Compute how many bytes the optional `-?words?` suffix consumes.
/// Matches cspell's `(?:-?words?)?` pattern.
fn strip_words_suffix_len(s: &str) -> usize {
    if s.starts_with("-words") {
        6
    } else if s.starts_with("-word") {
        5
    } else if s.starts_with("words") {
        5
    } else if s.starts_with("word") {
        4
    } else {
        0
    }
}

/// Match `(?:enable|disable)(?:allow)?CompoundWords` (case-insensitive, on lowered text).
/// Returns keyword length if matched.
fn matches_compound_words(lower: &str) -> Option<usize> {
    let prefix_len = if lower.starts_with("enable") {
        6
    } else if lower.starts_with("disable") {
        7
    } else {
        return None;
    };
    let rest = &lower[prefix_len..];
    let after_allow = if rest.starts_with("allow") {
        &rest[5..]
    } else {
        rest
    };
    if after_allow.starts_with("compoundwords") {
        Some(lower.len() - after_allow.len() + 13)
    } else {
        None
    }
}

/// Match `(?:enable|disable)CaseSensitive` (case-insensitive, on lowered text).
/// Returns keyword length if matched.
fn matches_case_sensitive(lower: &str) -> Option<usize> {
    let prefix_len = if lower.starts_with("enable") {
        6
    } else if lower.starts_with("disable") {
        7
    } else {
        return None;
    };
    if lower[prefix_len..].starts_with("casesensitive") {
        Some(prefix_len + 13)
    } else {
        None
    }
}

/// Match `ignore_?Reg_?Exp` (case-insensitive, on lowered text).
/// Returns keyword length if matched.
fn matches_ignore_regexp(lower: &str) -> Option<usize> {
    if !lower.starts_with("ignore") {
        return None;
    }
    let mut pos = 6;
    if lower.as_bytes().get(pos) == Some(&b'_') {
        pos += 1;
    }
    if !lower[pos..].starts_with("reg") {
        return None;
    }
    pos += 3;
    if lower.as_bytes().get(pos) == Some(&b'_') {
        pos += 1;
    }
    if !lower[pos..].starts_with("exp") {
        return None;
    }
    Some(pos + 3)
}

/// Match `include_?Reg_?Exp` (case-insensitive, on lowered text).
/// Returns keyword length if matched.
fn matches_include_regexp(lower: &str) -> Option<usize> {
    if !lower.starts_with("include") {
        return None;
    }
    let mut pos = 7;
    if lower.as_bytes().get(pos) == Some(&b'_') {
        pos += 1;
    }
    if !lower[pos..].starts_with("reg") {
        return None;
    }
    pos += 3;
    if lower.as_bytes().get(pos) == Some(&b'_') {
        pos += 1;
    }
    if !lower[pos..].starts_with("exp") {
        return None;
    }
    Some(pos + 3)
}

/// Extract regex pattern from directive value text.
/// cspell first tries regex literal (`/pattern/flags`), then falls back to
/// first non-whitespace token (allowing escaped spaces).
fn extract_regex_pattern(text: &str) -> String {
    // Try regex literal: /pattern/flags (greedy, matches cspell's /\/.*\/[gimuy]*/)
    if text.starts_with('/') {
        let mut last_slash = 0;
        for (i, b) in text.bytes().enumerate().skip(1) {
            if b == b'/' {
                last_slash = i;
            }
        }
        if last_slash > 0 {
            let after = &text[last_slash + 1..];
            let flags_len = after
                .bytes()
                .take_while(|&b| matches!(b, b'g' | b'i' | b'm' | b'u' | b'y'))
                .count();
            return text[..last_slash + 1 + flags_len].to_string();
        }
    }
    // Fallback: first non-whitespace token (allowing escaped spaces)
    let bytes = text.as_bytes();
    let mut end = 0;
    while end < bytes.len() {
        if end + 1 < bytes.len() && bytes[end] == b'\\' && bytes[end + 1] == b' ' {
            end += 2;
        } else if bytes[end].is_ascii_whitespace() {
            break;
        } else {
            end += 1;
        }
    }
    text[..end].to_string()
}

/// Parse locale/language value: split on whitespace/comma, join with comma.
/// Matches cspell's `match.trim().split(/[\s,]+/).slice(1).join(',')`.
fn parse_locale_value(text: &str) -> String {
    text.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

fn parse_directive_inner(line: &str) -> Option<Directive> {
    let caps = DIRECTIVE_RE.captures(line)?;
    let directive_text = caps.get(1)?.as_str().trim();
    let lower = directive_text.to_lowercase();

    // Order follows cspell's settingParsers array for correct precedence.

    // 1. CompoundWords: /^(?:enable|disable)(?:allow)?CompoundWords\b(?!-)/i
    if let Some(kw_len) = matches_compound_words(&lower)
        && keyword_boundary(&lower, kw_len) {
            return Some(if lower.starts_with("enable") {
                Directive::EnableCompoundWords
            } else {
                Directive::DisableCompoundWords
            });
        }

    // 2. CaseSensitive: /^(?:enable|disable)CaseSensitive\b(?!-)/i
    if let Some(kw_len) = matches_case_sensitive(&lower)
        && keyword_boundary(&lower, kw_len) {
            return Some(if lower.starts_with("enable") {
                Directive::EnableCaseSensitive
            } else {
                Directive::DisableCaseSensitive
            });
        }

    // 3. Enable: /^enable\b(?!-)/i
    if lower.starts_with("enable") && keyword_boundary(&lower, 6) {
        return Some(Directive::Enable);
    }

    // 4. Disable: /^disable(-line|-next(-line)?)?\b(?!-)/i
    if lower.starts_with("disable") {
        if lower.starts_with("disable-next-line") && keyword_boundary(&lower, 17) {
            return Some(Directive::DisableNextLine);
        }
        if lower.starts_with("disable-next") && keyword_boundary(&lower, 12) {
            return Some(Directive::DisableNextLine);
        }
        if lower.starts_with("disable-line") && keyword_boundary(&lower, 12) {
            return Some(Directive::DisableLine);
        }
        if keyword_boundary(&lower, 7) {
            return Some(Directive::Disable);
        }
    }

    // 5. Words: /^words?\b(?!-)/i
    if lower.starts_with("words") && keyword_boundary(&lower, 5) {
        let rest = &directive_text[5..];
        return Some(Directive::Words(parse_word_list(rest)));
    }
    if lower.starts_with("word") && keyword_boundary(&lower, 4) {
        let rest = &directive_text[4..];
        return Some(Directive::Words(parse_word_list(rest)));
    }

    // 6. IgnoreWords: /^ignore(?:-?words?)?\b(?!-)/i
    if lower.starts_with("ignore") {
        let suffix_len = strip_words_suffix_len(&lower[6..]);
        let full_len = 6 + suffix_len;
        if keyword_boundary(&lower, full_len) {
            let rest = &directive_text[full_len..];
            return Some(Directive::Ignore(parse_word_list(rest)));
        }
    }

    // 7. FlagWords: /^(?:flag|forbid)(?:-?words?)?\b(?!-)/i
    if lower.starts_with("forbid") || lower.starts_with("flag") {
        let prefix_len = if lower.starts_with("forbid") { 6 } else { 4 };
        let suffix_len = strip_words_suffix_len(&lower[prefix_len..]);
        let full_len = prefix_len + suffix_len;
        if keyword_boundary(&lower, full_len) {
            let rest = &directive_text[full_len..];
            return Some(Directive::ForbidWords(parse_word_list(rest)));
        }
    }

    // 8. IgnoreRegExp: /^ignore_?Reg_?Exp\s+.+$/i
    if let Some(kw_len) = matches_ignore_regexp(&lower) {
        let rest = &directive_text[kw_len..];
        // cspell requires \s+ before the pattern
        if !rest.is_empty() && rest.as_bytes()[0].is_ascii_whitespace() {
            let trimmed = rest.trim_start();
            if !trimmed.is_empty() {
                return Some(Directive::IgnoreRegExp(extract_regex_pattern(trimmed)));
            }
        }
    }

    // 9. IncludeRegExp: /^include_?Reg_?Exp\s+.+$/i
    if let Some(kw_len) = matches_include_regexp(&lower) {
        let rest = &directive_text[kw_len..];
        if !rest.is_empty() && rest.as_bytes()[0].is_ascii_whitespace() {
            let trimmed = rest.trim_start();
            if !trimmed.is_empty() {
                return Some(Directive::IncludeRegExp(extract_regex_pattern(trimmed)));
            }
        }
    }

    // 10. Locale: /^locale?\b(?!-)/i  (matches both "locale" and "local")
    //     Language: /^language\s\b(?!-)/i
    if lower.starts_with("locale") && keyword_boundary(&lower, 6) {
        let rest = &directive_text[6..];
        return Some(Directive::Language(parse_locale_value(rest)));
    }
    if lower.starts_with("local") && keyword_boundary(&lower, 5) {
        let rest = &directive_text[5..];
        return Some(Directive::Language(parse_locale_value(rest)));
    }
    if lower.starts_with("language") && keyword_boundary(&lower, 8) {
        let rest = &directive_text[8..];
        return Some(Directive::Language(parse_locale_value(rest)));
    }

    // 11. Dictionaries: /^dictionar(?:y|ies)\b(?!-)/i
    if lower.starts_with("dictionaries") && keyword_boundary(&lower, 12) {
        let rest = &directive_text[12..];
        return Some(Directive::Dictionaries(parse_word_list(rest)));
    }
    if lower.starts_with("dictionary") && keyword_boundary(&lower, 10) {
        let rest = &directive_text[10..];
        return Some(Directive::Dictionaries(parse_word_list(rest)));
    }

    None
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

// cspell:disable
#[cfg(test)]
mod tests {
    use super::*;

    // --- Basic directives ---

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
    fn test_parse_directive_no_prefilter_local_words() {
        assert_eq!(
            parse_directive_no_prefilter("// LocalWords: Megistos Antonis Stamboulis"),
            Some(Directive::LocalWords(vec![
                "Megistos".into(),
                "Antonis".into(),
                "Stamboulis".into(),
            ]))
        );
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
    fn test_disable_next() {
        // "disable-next" is synonym for "disable-next-line"
        assert_eq!(
            parse_directive("// cspell:disable-next"),
            Some(Directive::DisableNextLine)
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

    // --- Ignore / Words ---

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
    fn test_ignorewords_variant() {
        assert_eq!(
            parse_directive("// cspell:ignoreWords foo bar"),
            Some(Directive::Ignore(vec!["foo".into(), "bar".into()]))
        );
    }

    #[test]
    fn test_ignore_word_singular() {
        assert_eq!(
            parse_directive("// cspell:ignoreWord foo"),
            Some(Directive::Ignore(vec!["foo".into()]))
        );
    }

    #[test]
    fn test_ignore_hyphen_words() {
        assert_eq!(
            parse_directive("// cspell:ignore-words foo bar"),
            Some(Directive::Ignore(vec!["foo".into(), "bar".into()]))
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
    fn test_word_singular() {
        assert_eq!(
            parse_directive("// cspell:word myword"),
            Some(Directive::Words(vec!["myword".into()]))
        );
    }

    // --- ForbidWords / flag ---

    #[test]
    fn test_forbid_words() {
        assert_eq!(
            parse_directive("// cspell:forbid badword"),
            Some(Directive::ForbidWords(vec!["badword".into()]))
        );
    }

    #[test]
    fn test_flag_words() {
        assert_eq!(
            parse_directive("// cspell:flag badword"),
            Some(Directive::ForbidWords(vec!["badword".into()]))
        );
    }

    #[test]
    fn test_forbidword_singular() {
        assert_eq!(
            parse_directive("// cspell:forbidWord badword"),
            Some(Directive::ForbidWords(vec!["badword".into()]))
        );
    }

    #[test]
    fn test_flag_hyphen_word() {
        assert_eq!(
            parse_directive("// cspell:flag-word badword"),
            Some(Directive::ForbidWords(vec!["badword".into()]))
        );
    }

    // --- CompoundWords ---

    #[test]
    fn test_enable_compound_words() {
        assert_eq!(
            parse_directive("// cspell:enableCompoundWords"),
            Some(Directive::EnableCompoundWords)
        );
    }

    #[test]
    fn test_disable_compound_words() {
        assert_eq!(
            parse_directive("// cspell:disableCompoundWords"),
            Some(Directive::DisableCompoundWords)
        );
    }

    #[test]
    fn test_enable_allow_compound_words() {
        // cspell: (?:allow)? makes "allow" optional
        assert_eq!(
            parse_directive("// cspell:enableAllowCompoundWords"),
            Some(Directive::EnableCompoundWords)
        );
    }

    #[test]
    fn test_disable_allow_compound_words() {
        assert_eq!(
            parse_directive("// cspell:disableAllowCompoundWords"),
            Some(Directive::DisableCompoundWords)
        );
    }

    // --- CaseSensitive ---

    #[test]
    fn test_enable_case_sensitive() {
        assert_eq!(
            parse_directive("// cspell:enableCaseSensitive"),
            Some(Directive::EnableCaseSensitive)
        );
    }

    #[test]
    fn test_disable_case_sensitive() {
        assert_eq!(
            parse_directive("// cspell:disableCaseSensitive"),
            Some(Directive::DisableCaseSensitive)
        );
    }

    // --- IgnoreRegExp / IncludeRegExp ---

    #[test]
    fn test_ignore_regexp() {
        assert_eq!(
            parse_directive("// cspell:ignoreRegExp /foo/gi"),
            Some(Directive::IgnoreRegExp("/foo/gi".into()))
        );
    }

    #[test]
    fn test_ignore_regexp_plain_pattern() {
        // Non-regex-literal: first token only
        assert_eq!(
            parse_directive("// cspell:ignoreRegExp foo extra"),
            Some(Directive::IgnoreRegExp("foo".into()))
        );
    }

    #[test]
    fn test_ignore_underscore_regexp() {
        // cspell: ignore_?Reg_?Exp
        assert_eq!(
            parse_directive("// cspell:ignore_RegExp /bar/"),
            Some(Directive::IgnoreRegExp("/bar/".into()))
        );
    }

    #[test]
    fn test_ignore_reg_underscore_exp() {
        assert_eq!(
            parse_directive("// cspell:ignore_Reg_Exp /baz/i"),
            Some(Directive::IgnoreRegExp("/baz/i".into()))
        );
    }

    #[test]
    fn test_ignore_regexp_no_pattern() {
        // No pattern after keyword → not a valid directive
        assert_eq!(parse_directive("// cspell:ignoreRegExp"), None);
    }

    #[test]
    fn test_include_regexp() {
        assert_eq!(
            parse_directive("// cspell:includeRegExp /test/"),
            Some(Directive::IncludeRegExp("/test/".into()))
        );
    }

    #[test]
    fn test_include_underscore_regexp() {
        assert_eq!(
            parse_directive("// cspell:include_RegExp /test/"),
            Some(Directive::IncludeRegExp("/test/".into()))
        );
    }

    // --- Locale / Language ---

    #[test]
    fn test_locale() {
        assert_eq!(
            parse_directive("// cspell:locale en-US"),
            Some(Directive::Language("en-US".into()))
        );
    }

    #[test]
    fn test_local_without_e() {
        // cspell: /^locale?\b/ matches "local" (without e)
        assert_eq!(
            parse_directive("// cspell:local en, nl"),
            Some(Directive::Language("en,nl".into()))
        );
    }

    #[test]
    fn test_language() {
        assert_eq!(
            parse_directive("// cspell:language en-US"),
            Some(Directive::Language("en-US".into()))
        );
    }

    #[test]
    fn test_locale_comma_normalization() {
        // Multiple locales separated by comma and space → joined with comma
        assert_eq!(
            parse_directive("// cspell:locale es-ES, en-US"),
            Some(Directive::Language("es-ES,en-US".into()))
        );
    }

    // --- Dictionaries ---

    #[test]
    fn test_dictionaries() {
        assert_eq!(
            parse_directive("// cspell:dictionaries cpp java"),
            Some(Directive::Dictionaries(vec!["cpp".into(), "java".into()]))
        );
    }

    #[test]
    fn test_dictionary_singular() {
        // cspell: dictionar(?:y|ies)
        assert_eq!(
            parse_directive("// cspell:dictionary cpp"),
            Some(Directive::Dictionaries(vec!["cpp".into()]))
        );
    }

    // --- Keyword boundary ---

    #[test]
    fn test_false_directive_in_comment() {
        // "cspell:dictionaries)," should NOT match because ) after keyword
        // is valid non-word non-hyphen char. Wait - ) IS valid boundary.
        // But the word list would just be ["),"]. Actually let's test the
        // real problematic case from before:
        // The original bug was `cspell:dictionaries),` being parsed.
        // With the new boundary: ')' is non-word, non-hyphen → passes.
        // But parse_word_list on ")," gives ["),"]. This is correct behavior
        // per cspell - the boundary check passes and the value is parsed.
        // The fix was the additive merge in the validator, not blocking this.
        assert!(parse_directive("// cspell:dictionaries),").is_some());
    }

    #[test]
    fn test_no_match_word_boundary() {
        // "disableFoo" should NOT match "disable" (F is word char)
        assert_eq!(parse_directive("// cspell:disableFoo"), None);
    }

    #[test]
    fn test_no_match_hyphen_boundary() {
        // "disable-foo" should NOT match "disable" (hyphen rejected by (?!-))
        assert_eq!(parse_directive("// cspell:disable-foo"), None);
    }

    // --- Levenshtein / Typo detection ---

    #[test]
    fn test_levenshtein_identical() {
        assert_eq!(levenshtein("abc", "abc"), 0);
    }

    #[test]
    fn test_levenshtein_empty() {
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", ""), 0);
    }

    #[test]
    fn test_levenshtein_substitution() {
        assert_eq!(levenshtein("cat", "car"), 1);
    }

    #[test]
    fn test_levenshtein_insertion_deletion() {
        assert_eq!(levenshtein("abc", "ab"), 1);
        assert_eq!(levenshtein("ab", "abc"), 1);
    }

    #[test]
    fn test_typo_wrods_suggests_words() {
        let w = check_directive_typo("wrods").unwrap();
        assert_eq!(w.suggestion, "words");
    }

    #[test]
    fn test_typo_igore_suggests_ignore() {
        let w = check_directive_typo("igore").unwrap();
        assert_eq!(w.suggestion, "ignore");
    }

    #[test]
    fn test_typo_disble_suggests_disable() {
        let w = check_directive_typo("disble").unwrap();
        assert_eq!(w.suggestion, "disable");
    }

    #[test]
    fn test_typo_enble_suggests_enable() {
        let w = check_directive_typo("enble").unwrap();
        assert_eq!(w.suggestion, "enable");
    }

    #[test]
    fn test_valid_directive_returns_none() {
        assert!(check_directive_typo("disable").is_none());
        assert!(check_directive_typo("enable").is_none());
        assert!(check_directive_typo("words").is_none());
        assert!(check_directive_typo("ignore").is_none());
        assert!(check_directive_typo("local").is_none());
        assert!(check_directive_typo("dictionary").is_none());
    }

    #[test]
    fn test_completely_unrelated_returns_none() {
        assert!(check_directive_typo("xyzzyplugh").is_none());
        assert!(check_directive_typo("somethingelse").is_none());
    }

    // --- Extract regex pattern ---

    #[test]
    fn test_extract_regex_literal() {
        assert_eq!(extract_regex_pattern("/foo/gi"), "/foo/gi");
    }

    #[test]
    fn test_extract_regex_literal_no_flags() {
        assert_eq!(extract_regex_pattern("/foo/"), "/foo/");
    }

    #[test]
    fn test_extract_plain_token() {
        assert_eq!(extract_regex_pattern("foo extra"), "foo");
    }

    #[test]
    fn test_extract_escaped_space() {
        assert_eq!(extract_regex_pattern(r"foo\ bar extra"), r"foo\ bar");
    }

    // --- Locale value parsing ---

    #[test]
    fn test_parse_locale_value() {
        assert_eq!(parse_locale_value(" en, nl"), "en,nl");
        assert_eq!(parse_locale_value(" en-US"), "en-US");
        assert_eq!(parse_locale_value(" es-ES,  en-US"), "es-ES,en-US");
    }
}
