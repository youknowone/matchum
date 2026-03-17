use crate::issue::ValidationIssue;
use crate::random;
use crate::splitter;
use aho_corasick::{AhoCorasick, AhoCorasickKind};
use compact_str::CompactString;
use fancy_regex::Regex as FancyRegex;
use matchum_config::directives::{self, Directive};
use matchum_dict::dictionary::Dictionary;
use regex::Regex;
use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::{Arc, LazyLock};

pub type WordCache = Arc<papaya::HashMap<CompactString, bool>>;

const LOCAL_WORD_CACHE_SIZE: usize = 1000;

#[derive(Default)]
struct LocalWordCache {
    count: usize,
    current: hashbrown::HashMap<CompactString, bool>,
    previous: hashbrown::HashMap<CompactString, bool>,
}

impl LocalWordCache {
    fn get(&mut self, word: &str) -> Option<bool> {
        if let Some(&found) = self.current.get(word) {
            return Some(found);
        }
        let found = self.previous.get(word).copied()?;
        self.insert(word, found);
        Some(found)
    }

    fn insert(&mut self, word: &str, found: bool) {
        if self.current.contains_key(word) {
            return;
        }
        if self.count >= LOCAL_WORD_CACHE_SIZE {
            std::mem::swap(&mut self.current, &mut self.previous);
            self.current.clear();
            self.count = 0;
        }
        self.count += 1;
        self.current.insert(CompactString::from(word), found);
    }

    fn clear(&mut self) {
        self.count = 0;
        self.current.clear();
        self.previous.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompoundWordsMode {
    #[default]
    None,
    SeparateWords,
    JoinWords,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CustomIgnorePatternMask(u8);

impl CustomIgnorePatternMask {
    const BASE64: u8 = 1 << 0;
    const LATEX_MACRO_FUNCTION_NAMES: u8 = 1 << 1;
    const LATEX_MACROS_MULTILINE: u8 = 1 << 2;
    const LATEX_MATH: u8 = 1 << 3;
    const HASH_STRINGS: u8 = 1 << 4;
    const ADA_WORD_BREAK: u8 = 1 << 5;

    pub fn enable_base64(&mut self) {
        self.0 |= Self::BASE64;
    }

    pub fn has_base64(self) -> bool {
        self.0 & Self::BASE64 != 0
    }

    pub fn enable_latex_macro_function_names(&mut self) {
        self.0 |= Self::LATEX_MACRO_FUNCTION_NAMES;
    }

    pub fn has_latex_macro_function_names(self) -> bool {
        self.0 & Self::LATEX_MACRO_FUNCTION_NAMES != 0
    }

    pub fn enable_latex_macros_multiline(&mut self) {
        self.0 |= Self::LATEX_MACROS_MULTILINE;
    }

    pub fn has_latex_macros_multiline(self) -> bool {
        self.0 & Self::LATEX_MACROS_MULTILINE != 0
    }

    pub fn enable_latex_math(&mut self) {
        self.0 |= Self::LATEX_MATH;
    }

    pub fn has_latex_math(self) -> bool {
        self.0 & Self::LATEX_MATH != 0
    }

    pub fn enable_hash_strings(&mut self) {
        self.0 |= Self::HASH_STRINGS;
    }

    pub fn has_hash_strings(self) -> bool {
        self.0 & Self::HASH_STRINGS != 0
    }

    pub fn enable_ada_word_break(&mut self) {
        self.0 |= Self::ADA_WORD_BREAK;
    }

    pub fn has_ada_word_break(self) -> bool {
        self.0 & Self::ADA_WORD_BREAK != 0
    }

    pub fn extend(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

/// Configuration for the validation pipeline.
#[derive(Clone)]
pub struct ValidatorConfig {
    pub min_word_length: usize,
    pub case_sensitive: bool,
    pub ignore_patterns: Vec<Regex>,
    pub ignore_patterns_fancy: Vec<FancyRegex>,
    pub include_patterns: Vec<Regex>,
    pub include_patterns_fancy: Vec<FancyRegex>,
    pub custom_ignore_patterns: CustomIgnorePatternMask,
    pub flag_words: HashSet<CompactString>,
    pub ignore_words: HashSet<CompactString>,
    pub allow_compound_words: bool,
    pub compound_words_mode: CompoundWordsMode,
    pub cspell_compat_mode: bool,
    pub ignore_random_strings: bool,
    pub min_random_length: usize,
    pub compute_suggestions: bool,
    /// Maximum number of times a duplicate word is reported per document.
    /// Matches cspell's `maxDuplicateProblems` (default: 5).
    pub max_duplicate_problems: usize,
}

impl Default for ValidatorConfig {
    fn default() -> Self {
        Self {
            min_word_length: 4,
            case_sensitive: false,
            ignore_patterns: Vec::new(),
            ignore_patterns_fancy: Vec::new(),
            include_patterns: Vec::new(),
            include_patterns_fancy: Vec::new(),
            custom_ignore_patterns: CustomIgnorePatternMask::default(),
            flag_words: HashSet::new(),
            ignore_words: HashSet::new(),
            allow_compound_words: false,
            compound_words_mode: CompoundWordsMode::None,
            cspell_compat_mode: false,
            ignore_random_strings: true,
            min_random_length: 40,
            compute_suggestions: true,
            max_duplicate_problems: 5,
        }
    }
}

#[derive(Debug, Clone)]
struct CompatIssue {
    word: String,
    offset: usize,
    is_forbidden: bool,
}

#[derive(Debug, Clone)]
struct KnownCompatIssues {
    possible_word_abs_offset: usize,
    issues: Vec<CompatIssue>,
}

#[derive(Debug, Clone, Default)]
struct KnownCspellWordInfo {
    is_found: Option<bool>,
    is_flagged: Option<bool>,
    is_ignored: Option<bool>,
    fin: bool,
}

#[derive(Debug, Clone, Copy)]
struct CheckedCspellWord {
    is_flagged: bool,
    is_found: Option<bool>,
}

struct DictionaryEntry {
    name: Option<String>,
    dict: Arc<dyn Dictionary>,
    default_active: bool,
    len: usize,
    case_sensitive: bool,
    has_forbidden: bool,
    has_no_suggest: bool,
    has_expensive_forms: bool,
}

impl DictionaryEntry {
    fn new(name: Option<String>, dict: Arc<dyn Dictionary>, default_active: bool) -> Self {
        let len = dict.len();
        let case_sensitive = dict.is_case_sensitive();
        let has_forbidden = dict.has_forbidden_words();
        let has_no_suggest = dict.has_no_suggest_words();
        let has_expensive_forms = dict.has_expensive_forms();
        Self {
            name,
            dict,
            default_active,
            len,
            case_sensitive,
            has_forbidden,
            has_no_suggest,
            has_expensive_forms,
        }
    }
}

use crate::js_string_len;

/// Raw pattern strings for builtin skip patterns.
/// Matches cspell's `definedDefaultRegExpExcludeList` from DefaultSettings.ts.
/// SpellCheckerDisable/SpellCheckerIgnoreInDocSetting are handled by the directive system.
/// Patterns that require JS-only features (backreferences, lookaheads, lookbehinds) are
/// simplified to best-effort Rust regex approximations.
const BUILTIN_SKIP_PATTERN_STRS: &[&str] = &[
    // Urls
    r#"(?i)(?:https?|ftp)://[^\s"]+"#,
    // Email (ASCII `\w` / `\b` semantics to match JS RegExp behavior)
    r"(?i)(?-u:\b)[-A-Za-z0-9_.+]+@[A-Za-z0-9_]+(?:\.[A-Za-z0-9_]+){1,4}(?-u:\b)",
    // RsaCert (simplified: no backreference)
    r"-{5}BEGIN\s+[A-Za-z0-9_\s]+-{5}[A-Za-z0-9_=+\-/\\\s]+?-{5}END\s+[A-Za-z0-9_\s]+-{5}",
    // SshRsa (simplified: no negative lookahead)
    r"(?i)ssh-rsa\s+[A-Za-z0-9/+]{28,}={0,3}",
    // Base64MultiLine (simplified: no lookbehind/lookahead)
    r"(?m)[A-Za-z0-9/+]{40,}\n(?:\s*[A-Za-z0-9/+]{40,}\n)*(?:\s*[A-Za-z0-9/+]+=*)?",
    // Base64SingleLine (simplified: no lookbehind/lookahead)
    r"[A-Za-z0-9/+]{40,}={0,3}",
    // CommitHash (simplified: no negative lookahead)
    r"(?i)(?-u:\b)(?:0x)?[0-9a-f]{7,}(?-u:\b)",
    // CommitHashLink
    r"(?i)\[[0-9a-f]{7,}\]",
    // CStyleHexValue
    r"(?i)(?-u:\b)0x[0-9a-f_]+n?(?-u:\b)",
    // CSSHexValue
    r"(?i)#[0-9a-f]{3,8}(?-u:\b)",
    // SHA
    r"(?i)(?-u:\b)sha[0-9]+-[A-Za-z0-9+/]{25,}={0,3}",
    // HashStrings (simplified: no lookahead)
    r"(?i)(?:(?-u:\b)(?:sha[0-9]+|md5|base64|crypt|bcrypt|scrypt|security-token|assertion)[-,:$=]|#code[/])[-A-Za-z0-9_/+%.]{25,}={0,3}",
    // UnicodeRef
    r"(?i)(?-u:\b)U\+[0-9a-f]{4,5}(?:-[0-9a-f]{4,5})?",
    // UUID
    r"(?i)(?-u:\b)[0-9a-fx]{8}-[0-9a-fx]{4}-[0-9a-fx]{4}-[0-9a-fx]{4}-[0-9a-fx]{12}(?-u:\b)",
    // BinaryLiteral
    r"(?i)(?-u:\b)0b[01_]+(?-u:\b)",
    // OctalLiteral
    r"(?i)(?-u:\b)0o[0-7_]+(?-u:\b)",
    // ScientificNotation
    r"(?-u:\b)[0-9]+\.?[0-9]*[eE][+-]?[0-9]+(?-u:\b)",
];

/// Compiled once via LazyLock and shared across all Validator instances.
static BUILTIN_SKIP_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    BUILTIN_SKIP_PATTERN_STRS
        .iter()
        .filter_map(|p| Regex::new(p).ok())
        .collect()
});

/// Aho-Corasick prefilter for builtin skip patterns.
struct SkipPrefilter {
    ac: AhoCorasick,
    /// For each AC pattern index, which regex indices it enables.
    anchor_to_regex: Vec<&'static [usize]>,
}

static BUILTIN_SKIP_PREFILTER: LazyLock<SkipPrefilter> = LazyLock::new(|| {
    const ANCHORS: &[(&str, &[usize])] = &[
        ("://", &[0]),             // URL
        ("@", &[1]),               // Email
        ("-----", &[2]),           // RsaCert
        ("ssh-rsa", &[3]),         // SshRsa
        ("[", &[7]),               // CommitHashLink
        ("0x", &[8]),              // CStyleHexValue
        ("#", &[9]),               // CSSHexValue
        ("sha", &[10, 11]),        // SHA + HashStrings
        ("md5", &[11]),            // HashStrings
        ("base64", &[11]),         // HashStrings
        ("crypt", &[11]),          // HashStrings (covers bcrypt/scrypt)
        ("security-token", &[11]), // HashStrings
        ("assertion", &[11]),      // HashStrings
        ("#code/", &[11]),         // HashStrings
        ("u+", &[12]),             // UnicodeRef
        ("0b", &[14]),             // BinaryLiteral
        ("0o", &[15]),             // OctalLiteral
    ];

    let patterns: Vec<&str> = ANCHORS.iter().map(|(s, _)| *s).collect();
    let ac = AhoCorasick::builder()
        .ascii_case_insensitive(true)
        .kind(Some(AhoCorasickKind::DFA))
        .build(&patterns)
        .unwrap();

    SkipPrefilter {
        ac,
        anchor_to_regex: ANCHORS.iter().map(|(_, v)| *v).collect(),
    }
});

/// Hex digit lookup table for fast byte-level validation.
static HEX_DIGIT: [bool; 256] = {
    let mut table = [false; 256];
    let mut i = 0u16;
    while i < 256 {
        table[i as usize] = (i as u8).is_ascii_hexdigit();
        i += 1;
    }
    table
};

/// Scan for CommitHash matches: `(?i)\b(?![a-f]+\b)(?:0x)?[0-9a-f]{7,}\b`
/// Hand-written byte scanner replacing regex for this always-check pattern.
/// The `(?![a-f]+\b)` negative lookahead requires at least one digit [0-9]
/// to avoid matching all-letter hex words like "CAFEDEAD" or "abdabababc". // cspell:ignore CAFEDEAD abdabababc
fn scan_commit_hashes(text: &[u8], ranges: &mut Vec<(usize, usize)>) {
    let len = text.len();
    let mut i = 0;
    while i < len {
        // Skip non-hex bytes quickly
        if !HEX_DIGIT[text[i] as usize] {
            // Check for "0x" prefix
            if text[i] == b'0' && i + 1 < len && (text[i + 1] == b'x' || text[i + 1] == b'X') {
                let start = i;
                i += 2;
                // Count hex digits after 0x
                let hex_start = i;
                while i < len && HEX_DIGIT[text[i] as usize] {
                    i += 1;
                }
                let hex_len = i - hex_start;
                // Word boundary check at end
                if hex_len >= 7 && (i >= len || !is_word_byte(text[i])) {
                    // Word boundary check at start
                    if start == 0 || !is_word_byte(text[start - 1]) {
                        ranges.push((start, i));
                    }
                }
                continue;
            }
            i += 1;
            continue;
        }
        // Found a hex digit — scan the full run
        let start = i;
        let mut has_digit = false;
        while i < len && HEX_DIGIT[text[i] as usize] {
            if text[i].is_ascii_digit() {
                has_digit = true;
            }
            i += 1;
        }
        let hex_len = i - start;
        // Must be 7+ hex chars AND contain at least one digit (not all-letters)
        if hex_len >= 7 && has_digit {
            // Word boundary: preceding byte must not be a word char
            let left_ok = start == 0 || !is_word_byte(text[start - 1]);
            // Word boundary: following byte must not be a word char
            let right_ok = i >= len || !is_word_byte(text[i]);
            if left_ok && right_ok {
                ranges.push((start, i));
            }
        }
    }
}

/// Scan for UUID matches: `(?i)\b[0-9a-fx]{8}-[0-9a-fx]{4}-[0-9a-fx]{4}-[0-9a-fx]{4}-[0-9a-fx]{12}\b`
fn scan_uuids(text: &[u8], ranges: &mut Vec<(usize, usize)>) {
    // UUID is exactly 36 bytes: 8-4-4-4-12
    if text.len() < 36 {
        return;
    }
    // Use memchr to find '-' candidates, then validate the full UUID structure
    let mut i = 8; // First dash is at offset 8
    while i < text.len() {
        if text[i] != b'-' {
            i += 1;
            continue;
        }
        // Potential UUID: check if this could be the first dash (offset 8)
        let start = i - 8;
        // Need at least 36 bytes from start
        if start + 36 > text.len() {
            break;
        }
        // Validate structure: 8 hex - 4 hex - 4 hex - 4 hex - 12 hex
        if text[start + 8] == b'-'
            && text[start + 13] == b'-'
            && text[start + 18] == b'-'
            && text[start + 23] == b'-'
            && is_hex_or_x_run(&text[start..start + 8])
            && is_hex_or_x_run(&text[start + 9..start + 13])
            && is_hex_or_x_run(&text[start + 14..start + 18])
            && is_hex_or_x_run(&text[start + 19..start + 23])
            && is_hex_or_x_run(&text[start + 24..start + 36])
        {
            // Word boundaries
            let left_ok = start == 0 || !is_word_byte(text[start - 1]);
            let end = start + 36;
            let right_ok = end >= text.len() || !is_word_byte(text[end]);
            if left_ok && right_ok {
                ranges.push((start, end));
                i = end;
                continue;
            }
        }
        i += 1;
    }
}

/// Scan for scientific notation: `\b\d+\.?\d*[eE][+-]?\d+\b`
fn scan_scientific_notation(text: &[u8], ranges: &mut Vec<(usize, usize)>) {
    let len = text.len();
    let mut i = 0;
    while i < len {
        if text[i] != b'e' && text[i] != b'E' {
            i += 1;
            continue;
        }
        // Found 'e'/'E' — check digits before and after
        let e_pos = i;
        // Scan backwards for digits (and optional dot)
        if e_pos == 0 || !text[e_pos - 1].is_ascii_digit() {
            i += 1;
            continue;
        }
        let mut back = e_pos - 1;
        while back > 0 && text[back - 1].is_ascii_digit() {
            back -= 1;
        }
        // Optional dot + digits before the digits we just found
        if back > 0 && text[back - 1] == b'.' {
            back -= 1;
            while back > 0 && text[back - 1].is_ascii_digit() {
                back -= 1;
            }
        }
        // Must start with a digit
        if !text[back].is_ascii_digit() {
            i += 1;
            continue;
        }
        // Word boundary at start
        if back > 0 && is_word_byte(text[back - 1]) {
            i += 1;
            continue;
        }
        // Scan forward: optional +/-, then digits
        let mut fwd = e_pos + 1;
        if fwd < len && (text[fwd] == b'+' || text[fwd] == b'-') {
            fwd += 1;
        }
        if fwd >= len || !text[fwd].is_ascii_digit() {
            i += 1;
            continue;
        }
        while fwd < len && text[fwd].is_ascii_digit() {
            fwd += 1;
        }
        // Word boundary at end
        if fwd < len && is_word_byte(text[fwd]) {
            i += 1;
            continue;
        }
        ranges.push((back, fwd));
        i = fwd;
    }
}

/// Find the next newline position (exclusive end-of-line + 1) starting from `from`.
/// Returns `text.len() + 1` as a sentinel when no newline is found, so that any
/// byte offset in the last line compares less than the sentinel.
#[inline]
fn memchr_newline(bytes: &[u8], from: usize) -> usize {
    let slice = &bytes[from..];
    match slice.iter().position(|&b| b == b'\n') {
        Some(pos) => from + pos + 1,
        None => bytes.len() + 1,
    }
}

struct TextLine<'a> {
    text: &'a str,
    offset: usize,
}

struct TextLineIter<'a> {
    text: &'a str,
    bytes: &'a [u8],
    next_offset: usize,
    finished: bool,
}

impl<'a> Iterator for TextLineIter<'a> {
    type Item = TextLine<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        let start = self.next_offset;
        let end = memchr_newline(self.bytes, start);
        if end > self.bytes.len() {
            self.finished = true;
            let raw = &self.text[start..];
            return Some(TextLine {
                text: trim_line_ending(raw),
                offset: start,
            });
        }

        self.next_offset = end;
        let raw = &self.text[start..end];
        Some(TextLine {
            text: trim_line_ending(raw),
            offset: start,
        })
    }
}

#[inline]
fn trim_line_ending(raw: &str) -> &str {
    let raw = raw.strip_suffix('\n').unwrap_or(raw);
    raw.strip_suffix('\r').unwrap_or(raw)
}

#[inline]
fn text_lines(text: &str) -> TextLineIter<'_> {
    TextLineIter {
        text,
        bytes: text.as_bytes(),
        next_offset: 0,
        finished: false,
    }
}

#[inline]
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[inline]
fn is_hex_or_x_run(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .all(|&b| HEX_DIGIT[b as usize] || b == b'x' || b == b'X')
}

/// Base64 character lookup table: `[A-Za-z0-9/+]`
static BASE64_CHAR: [bool; 256] = {
    let mut table = [false; 256];
    let mut i = 0u16;
    while i < 256 {
        table[i as usize] = matches!(
            i as u8,
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'+'
        );
        i += 1;
    }
    table
};

/// Scan for Base64 single-line: `[A-Za-z0-9/+]{40,}={0,3}`
///
/// Heuristics matching cspell's `regExBase64SingleLine`:
///   1. At least one digit or `+/` in the first 80 chars
///   2. An Upper-lower-Upper transition (e.g. `AbC`)
///   3. A digit bracketed by letters (e.g. `A9A`, `a9a`, `A9a`, `a9A`, `9X9`)
///   4. At least 3 consecutive same-case letters
fn scan_base64_single_line(text: &[u8], ranges: &mut Vec<(usize, usize)>) {
    let len = text.len();
    let mut i = 0;
    while i < len {
        if !BASE64_CHAR[text[i] as usize] {
            i += 1;
            continue;
        }
        let start = i;
        if start > 0 && (BASE64_CHAR[text[start - 1] as usize] || text[start - 1] == b'_') {
            i += 1;
            continue;
        }
        while i < len && BASE64_CHAR[text[i] as usize] {
            i += 1;
        }
        let b64_len = i - start;
        if b64_len >= 40 {
            let run = &text[start..i];
            if is_base64_like(run) {
                // Count trailing '='
                let mut eq = 0;
                while eq < 3 && i < len && text[i] == b'=' {
                    i += 1;
                    eq += 1;
                }
                ranges.push((start, i));
            }
        }
    }
}

/// Check if a run of base64 characters looks like actual base64 data.
/// Implements cspell's `regExBase64SingleLine` lookahead heuristics.
fn is_base64_like(run: &[u8]) -> bool {
    // Limit lookahead to first 80 chars (matching cspell's {0,80} quantifiers)
    let check_len = run.len().min(80);
    let check = &run[..check_len];

    // 1. Must contain at least one digit or '+/'
    let has_digit_or_plus = check
        .iter()
        .any(|&b| b.is_ascii_digit() || b == b'+' || b == b'/');
    if !has_digit_or_plus {
        return false;
    }

    // 2. Must have Upper,lower,Upper transition
    let mut has_ulu = false;
    for w in check.windows(3) {
        if w[0].is_ascii_uppercase() && w[1].is_ascii_lowercase() && w[2].is_ascii_uppercase() {
            has_ulu = true;
            break;
        }
    }
    if !has_ulu {
        return false;
    }

    // 3. Must have digit between letters pattern:
    //    [A-Z][0-9][A-Z] | [a-z][0-9][a-z] | [A-Z][0-9][a-z] | [a-z][0-9][A-Z] | [0-9][A-Za-z][0-9]
    let mut has_digit_between = false;
    for w in check.windows(3) {
        let (a, b, c) = (w[0], w[1], w[2]);
        if b.is_ascii_digit() && a.is_ascii_alphabetic() && c.is_ascii_alphabetic() {
            has_digit_between = true;
            break;
        }
        if a.is_ascii_digit() && b.is_ascii_alphabetic() && c.is_ascii_digit() {
            has_digit_between = true;
            break;
        }
    }
    if !has_digit_between {
        return false;
    }

    // 4. Must have 3+ consecutive same-case letters (aaa or AAA)
    let mut has_three_same_case = false;
    for w in check.windows(3) {
        if (w[0].is_ascii_lowercase() && w[1].is_ascii_lowercase() && w[2].is_ascii_lowercase())
            || (w[0].is_ascii_uppercase() && w[1].is_ascii_uppercase() && w[2].is_ascii_uppercase())
        {
            has_three_same_case = true;
            break;
        }
    }
    if !has_three_same_case {
        return false;
    }

    true
}

#[inline]
fn is_hash_string_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'/' | b'+' | b'%' | b'.')
}

#[inline]
fn is_hash_string_boundary(byte: u8) -> bool {
    !(byte.is_ascii_alphanumeric() || byte == b'_')
}

fn consume_hash_string_payload(text: &[u8], start: usize) -> Option<usize> {
    let len = text.len();
    let mut idx = start;
    while idx < len && is_hash_string_char(text[idx]) {
        idx += 1;
    }
    if idx - start < 25 {
        return None;
    }
    let mut pad_count = 0usize;
    while idx < len && text[idx] == b'=' && pad_count < 3 {
        idx += 1;
        pad_count += 1;
    }
    Some(idx)
}

fn match_hash_string_prefix(text: &[u8], start: usize) -> Option<usize> {
    let len = text.len();

    if text[start..].starts_with(b"#code/") {
        return Some(start + 6);
    }

    if start > 0 && !is_hash_string_boundary(text[start - 1]) {
        return None;
    }

    for prefix in [
        b"md5" as &[u8],
        b"base64",
        b"crypt",
        b"bcrypt",
        b"scrypt",
        b"security-token",
        b"assertion",
    ] {
        let end = start + prefix.len();
        if end < len
            && text[start..].starts_with(prefix)
            && matches!(text[end], b'-' | b',' | b':' | b'$' | b'=')
        {
            return Some(end + 1);
        }
    }

    if text[start..].starts_with(b"sha") {
        let mut idx = start + 3;
        let digits_start = idx;
        while idx < len && text[idx].is_ascii_digit() {
            idx += 1;
        }
        if idx > digits_start && idx < len && matches!(text[idx], b'-' | b',' | b':' | b'$' | b'=')
        {
            return Some(idx + 1);
        }
    }

    None
}

/// Scan for cspell's `HashStrings` pattern.
///
/// Rust regex does not support the lookaround / backreference combination used
/// by cspell's source regex, so we mirror the byte-level structure directly.
fn scan_hash_strings(text: &[u8], ranges: &mut Vec<(usize, usize)>) {
    let len = text.len();
    let mut i = 0usize;

    while i < len {
        let Some(payload_start) = match_hash_string_prefix(text, i) else {
            i += 1;
            continue;
        };

        let Some(mut end) = consume_hash_string_payload(text, payload_start) else {
            i += 1;
            continue;
        };

        loop {
            let saved = end;
            let mut idx = end;
            let mut quote = None;

            if idx < len && matches!(text[idx], b'\'' | b'"') {
                quote = Some(text[idx]);
                idx += 1;
            }
            while idx < len && text[idx].is_ascii_whitespace() {
                idx += 1;
            }
            if idx < len && text[idx] == b'+' {
                idx += 1;
            }
            while idx < len && text[idx].is_ascii_whitespace() {
                idx += 1;
            }
            if let Some(q) = quote
                && idx < len
                && text[idx] == q
            {
                idx += 1;
            }

            let Some(next_end) = consume_hash_string_payload(text, idx) else {
                end = saved;
                break;
            };
            end = next_end;
        }

        if end < len && (is_hash_string_char(text[end]) || text[end] == b'=') {
            i += 1;
            continue;
        }

        ranges.push((i, end));
        i = end.max(i + 1);
    }
}

/// Scan for cspell's `regExBase64`:
/// `(?<![A-Za-z0-9/+])(?:[A-Za-z0-9/+]{40,})(?:\\s^\\s*[A-Za-z0-9/+]{40,})*(?:\\s^\\s*[A-Za-z0-9/+]+=*)?(?![A-Za-z0-9/+=])`
///
/// This is broader than `Base64SingleLine` and intentionally matches long
/// slash-separated path segments when the `Base64` pattern is enabled.
fn scan_base64(text: &[u8], ranges: &mut Vec<(usize, usize)>) {
    let len = text.len();
    let mut i = 0;
    while i < len {
        if !BASE64_CHAR[text[i] as usize] {
            i += 1;
            continue;
        }

        let start = i;
        if start > 0 && BASE64_CHAR[text[start - 1] as usize] {
            i += 1;
            continue;
        }

        let mut end = start;
        while end < len && BASE64_CHAR[text[end] as usize] {
            end += 1;
        }

        if end - start < 40 {
            i = end.max(start + 1);
            continue;
        }

        let mut final_end = end;
        let mut cursor = end;

        loop {
            let saved = cursor;
            if cursor >= len || !text[cursor].is_ascii_whitespace() {
                break;
            }
            cursor += 1;
            if text[cursor - 1] != b'\n' {
                cursor = saved;
                break;
            }
            while cursor < len && matches!(text[cursor], b' ' | b'\t' | b'\r' | 0x0b | 0x0c) {
                cursor += 1;
            }
            let run_start = cursor;
            while cursor < len && BASE64_CHAR[text[cursor] as usize] {
                cursor += 1;
            }
            if cursor - run_start < 40 {
                cursor = saved;
                break;
            }
            final_end = cursor;
        }

        if cursor < len && text[cursor].is_ascii_whitespace() {
            cursor += 1;
            if text[cursor - 1] == b'\n' {
                while cursor < len && matches!(text[cursor], b' ' | b'\t' | b'\r' | 0x0b | 0x0c) {
                    cursor += 1;
                }
                let run_start = cursor;
                while cursor < len && BASE64_CHAR[text[cursor] as usize] {
                    cursor += 1;
                }
                if cursor > run_start {
                    while cursor < len && text[cursor] == b'=' {
                        cursor += 1;
                    }
                    final_end = cursor;
                }
            }
        }

        if final_end < len {
            let next = text[final_end];
            if BASE64_CHAR[next as usize] || next == b'=' {
                i = start + 1;
                continue;
            }
        }

        ranges.push((start, final_end));
        i = final_end.max(start + 1);
    }
}

fn consume_base64_quoted_run(
    text: &[u8],
    cursor: &mut usize,
    min_len: usize,
    allow_padding: bool,
) -> Option<usize> {
    let len = text.len();
    let mut idx = *cursor;

    if idx < len && matches!(text[idx], b'"' | b'\'') {
        idx += 1;
    }

    let run_start = idx;
    while idx < len && BASE64_CHAR[text[idx] as usize] {
        idx += 1;
    }
    if idx - run_start < min_len {
        return None;
    }

    if allow_padding {
        let mut pad_count = 0usize;
        while idx < len && text[idx] == b'=' && pad_count < 3 {
            idx += 1;
            pad_count += 1;
        }
    }

    if idx < len && matches!(text[idx], b'"' | b'\'') {
        idx += 1;
    }

    *cursor = idx;
    Some(idx)
}

fn consume_base64_multiline_gap(text: &[u8], cursor: &mut usize) -> bool {
    let len = text.len();
    let mut idx = *cursor;

    match text.get(idx).copied() {
        Some(b'\n') => idx += 1,
        Some(b'\r') => {
            idx += 1;
            if idx < len && text[idx] == b'\n' {
                idx += 1;
            }
        }
        _ => return false,
    }

    while idx < len && matches!(text[idx], b' ' | b'\t' | b'\r' | 0x0b | 0x0c) {
        idx += 1;
    }

    *cursor = idx;
    true
}

fn consume_base64_padding_and_optional_quote(text: &[u8], cursor: &mut usize) -> bool {
    let len = text.len();
    let mut idx = *cursor;
    let mut pad_count = 0usize;

    while idx < len && text[idx] == b'=' && pad_count < 3 {
        idx += 1;
        pad_count += 1;
    }

    if pad_count == 0 {
        return false;
    }

    if idx < len && matches!(text[idx], b'"' | b'\'') {
        idx += 1;
    }

    *cursor = idx;
    true
}

/// Scan for cspell's `regExBase64MultiLine`:
/// `(?<![A-Za-z0-9/+])["']?(?:[A-Za-z0-9/+]{40,})["']?(?:\s^\s*["']?[A-Za-z0-9/+]{40,}["']?)+(?:\s^\s*["']?[A-Za-z0-9/+]+={0,3}["']?)?(?![A-Za-z0-9/+=])`
///
/// Unlike `regExBase64SingleLine`, cspell does not apply content heuristics
/// here; the match is purely structural and intentionally accepts quoted,
/// multi-line password fixtures.
fn scan_base64_multiline(text: &[u8], ranges: &mut Vec<(usize, usize)>) {
    let len = text.len();
    let mut i = 0;
    while i < len {
        let start = i;
        let starts_with_quote = matches!(text[start], b'"' | b'\'');
        let base_start = if starts_with_quote { start + 1 } else { start };

        if base_start >= len || !BASE64_CHAR[text[base_start] as usize] {
            i += 1;
            continue;
        }

        if start > 0 && BASE64_CHAR[text[start - 1] as usize] {
            i += 1;
            continue;
        }

        let mut cursor = start;
        let Some(mut end) = consume_base64_quoted_run(text, &mut cursor, 40, false) else {
            i += 1;
            continue;
        };

        let mut continuation_count = 0usize;
        loop {
            let saved = cursor;
            if !consume_base64_multiline_gap(text, &mut cursor) {
                cursor = saved;
                break;
            }
            let Some(next_end) = consume_base64_quoted_run(text, &mut cursor, 40, false) else {
                cursor = saved;
                break;
            };
            continuation_count += 1;
            end = next_end;
        }

        if continuation_count == 0 {
            i = start + 1;
            continue;
        }

        if consume_base64_padding_and_optional_quote(text, &mut cursor) {
            end = cursor;
        } else if consume_base64_multiline_gap(text, &mut cursor)
            && let Some(next_end) = consume_base64_quoted_run(text, &mut cursor, 1, true)
        {
            end = next_end;
        }

        if end < len {
            let next = text[end];
            if BASE64_CHAR[next as usize] || next == b'=' {
                i = start + 1;
                continue;
            }
        }

        ranges.push((start, end));
        i = end.max(start + 1);
    }
}

#[inline]
fn is_escaped_by_backslash(bytes: &[u8], idx: usize) -> bool {
    let mut backslashes = 0usize;
    let mut cursor = idx;
    while cursor > 0 && bytes[cursor - 1] == b'\\' {
        backslashes += 1;
        cursor -= 1;
    }
    backslashes % 2 == 1
}

#[inline]
fn is_in_latex_comment(bytes: &[u8], idx: usize) -> bool {
    let mut line_start = idx;
    while line_start > 0 && bytes[line_start - 1] != b'\n' {
        line_start -= 1;
    }

    let mut cursor = line_start;
    while cursor < idx {
        if bytes[cursor] == b'%' && !is_escaped_by_backslash(bytes, cursor) {
            return true;
        }
        cursor += 1;
    }

    false
}

#[inline]
fn is_latex_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[inline]
fn is_latex_text_macro(name: &[u8]) -> bool {
    name.starts_with(b"title")
        || name.starts_with(b"color")
        || name.starts_with(b"section")
        || name.starts_with(b"subsection")
        || name.starts_with(b"footnote")
        || name.starts_with(b"chapter")
        || name.starts_with(b"part")
        || name.starts_with(b"caption")
        || name.starts_with(b"emph")
        || name.starts_with(b"enquote")
        || name.starts_with(b"text")
        || name == b"in"
}

fn scan_latex_macro_function_names(text: &[u8], ranges: &mut Vec<(usize, usize)>) {
    let len = text.len();
    let mut i = 0usize;
    while i < len {
        if text[i] != b'\\' || (i > 0 && text[i - 1] == b'\\') {
            i += 1;
            continue;
        }

        let mut name_start = i + 1;
        while name_start + 1 < len && text[name_start] == b'\\' && text[name_start + 1] == b'\\' {
            name_start += 2;
        }

        let mut end = name_start;
        while end < len && is_latex_word_byte(text[end]) {
            end += 1;
        }

        if end > name_start {
            ranges.push((i, end));
            i = end;
            continue;
        }

        i += 1;
    }
}

fn scan_latex_macros_multiline(text: &[u8], ranges: &mut Vec<(usize, usize)>) {
    let len = text.len();
    let mut i = 0usize;
    while i < len {
        if text[i] != b'\\' || (i > 0 && text[i - 1] == b'\\') {
            i += 1;
            continue;
        }

        let mut name_start = i + 1;
        while name_start + 1 < len && text[name_start] == b'\\' && text[name_start + 1] == b'\\' {
            name_start += 2;
        }

        let mut name_end = name_start;
        while name_end < len && is_latex_word_byte(text[name_end]) {
            name_end += 1;
        }

        if name_end == name_start || is_latex_text_macro(&text[name_start..name_end]) {
            i += 1;
            continue;
        }

        let mut end = name_end;
        loop {
            let Some(&open) = text.get(end) else {
                break;
            };
            let close = match open {
                b'[' => b']',
                b'{' => b'}',
                _ => break,
            };

            let mut cursor = end + 1;
            while cursor < len && text[cursor] != close {
                cursor += 1;
            }
            end = cursor.min(len.saturating_sub(1)) + usize::from(cursor < len);
            if cursor >= len {
                end = len;
                break;
            }
        }

        ranges.push((i, end));
        i = end.max(i + 1);
    }
}

fn scan_latex_math(text: &[u8], ranges: &mut Vec<(usize, usize)>) {
    let len = text.len();
    let mut i = 0usize;

    while i < len {
        if text[i] != b'$' || is_escaped_by_backslash(text, i) || is_in_latex_comment(text, i) {
            i += 1;
            continue;
        }

        let start = i;
        let mut opener_end = i + 1;
        while opener_end < len && text[opener_end] == b'$' {
            opener_end += 1;
        }

        let mut cursor = opener_end;
        let mut matched_end = None;
        while cursor < len {
            if text[cursor] == b'$'
                && cursor > opener_end
                && !is_escaped_by_backslash(text, cursor)
                && !is_in_latex_comment(text, cursor)
            {
                let mut end = cursor + 1;
                while end < len && text[end] == b'$' {
                    end += 1;
                }
                matched_end = Some(end);
                break;
            }
            cursor += 1;
        }

        if let Some(end) = matched_end {
            ranges.push((start, end));
            i = end;
            continue;
        }

        i = opener_end;
    }
}

/// Scan for Ada word-break apostrophes.
///
/// Mirrors `@cspell/dict-ada/cspell-ext.json`:
/// `/((?<=\\w)['](?=\\w)(?!((?<=n')t|ve|d|ll|m|s|re)\\b))/g`
///
/// cspell removes only the apostrophe span from the validation ranges, which
/// causes `Gamepads'Range` to be checked as `Gamepads` + `Range` while keeping // cspell:ignore Gamepads
/// English contractions intact.
fn scan_ada_word_breaks(text: &[u8], ranges: &mut Vec<(usize, usize)>) {
    for i in 1..text.len().saturating_sub(1) {
        if text[i] != b'\'' || !is_word_byte(text[i - 1]) || !is_word_byte(text[i + 1]) {
            continue;
        }

        if matches_english_apostrophe_suffix(text, i) {
            continue;
        }

        ranges.push((i, i + 1));
    }
}

#[inline]
fn matches_english_apostrophe_suffix(text: &[u8], apostrophe_offset: usize) -> bool {
    let suffix = &text[apostrophe_offset + 1..];

    if text[apostrophe_offset - 1] == b'n'
        && suffix.starts_with(b"t")
        && (suffix.len() == 1 || !is_word_byte(suffix[1]))
    {
        return true;
    }

    const SUFFIXES: [&[u8]; 6] = [b"ve", b"d", b"ll", b"m", b"s", b"re"];
    SUFFIXES.iter().any(|candidate| {
        suffix.starts_with(candidate)
            && (suffix.len() == candidate.len() || !is_word_byte(suffix[candidate.len()]))
    })
}

/// Validates text against a set of dictionaries.
pub struct Validator {
    dictionaries: Vec<DictionaryEntry>,
    config: ValidatorConfig,
    /// Pre-computed flag: whether any dictionary has forbidden words.
    /// When false, we can skip all is_forbidden() calls entirely.
    any_dict_has_forbidden: bool,
    /// Pre-computed flag: whether any dictionary has `noSuggest` words.
    /// When false, we can skip all ignore checks entirely.
    any_dict_has_no_suggest: bool,
    /// Pre-computed flag: true when all dictionaries are case-insensitive.
    /// When true, skip the validator's own lowercase fallback in is_word_valid
    /// because each dict's has() already normalizes to lowercase internally.
    all_dicts_case_insensitive: bool,
    /// Pre-computed flag: whether any dictionary has repMap / compound fallback
    /// that should be deferred until direct lookup misses.
    any_dict_has_expensive_forms: bool,
    /// Shared word validation cache across files with the same dictionary set.
    /// Key: word text, Value: is_word_valid result.
    word_cache: Option<WordCache>,
    /// Per-validator L1 cache mirroring cspell's small cached dictionary.
    /// This avoids repeated shared-cache and dictionary walks within a single run.
    local_word_cache: RefCell<LocalWordCache>,
    /// Pre-computed indices of default-active dictionaries (avoids per-call filtering).
    default_active_indices: Vec<usize>,
    /// Default-active dictionaries that contain forbidden words.
    default_active_forbidden_indices: Vec<usize>,
    /// Default-active dictionaries that contain `noSuggest` words.
    default_active_no_suggest_indices: Vec<usize>,
    /// Default-active dictionaries that can still match after direct-only lookup misses.
    default_active_expensive_indices: Vec<usize>,
}

impl Validator {
    pub fn new(dictionaries: Vec<Box<dyn Dictionary>>, config: ValidatorConfig) -> Self {
        let dictionaries = dictionaries
            .into_iter()
            .map(|d| DictionaryEntry::new(None, Arc::from(d), true))
            .collect();
        Self::new_internal(dictionaries, config)
    }

    pub fn new_named(
        dictionaries: Vec<(String, Arc<dyn Dictionary>, bool)>,
        config: ValidatorConfig,
    ) -> Self {
        let dictionaries = dictionaries
            .into_iter()
            .map(|(name, dict, default_active)| {
                DictionaryEntry::new(Some(name.to_lowercase()), dict, default_active)
            })
            .collect();
        Self::new_internal(dictionaries, config)
    }

    fn new_internal(dictionaries: Vec<DictionaryEntry>, config: ValidatorConfig) -> Self {
        let any_dict_has_forbidden = dictionaries.iter().any(|d| d.has_forbidden);
        let any_dict_has_no_suggest = dictionaries.iter().any(|d| d.has_no_suggest);
        let all_dicts_case_insensitive = !dictionaries.iter().any(|d| d.case_sensitive);
        let any_dict_has_expensive_forms = dictionaries.iter().any(|d| d.has_expensive_forms);
        let mut default_active_indices: Vec<usize> = dictionaries
            .iter()
            .enumerate()
            .filter(|(_, d)| d.default_active)
            .map(|(i, _)| i)
            .collect();
        let mut default_active_forbidden_indices: Vec<usize> = dictionaries
            .iter()
            .enumerate()
            .filter(|(_, d)| d.default_active && d.has_forbidden)
            .map(|(i, _)| i)
            .collect();
        let mut default_active_no_suggest_indices: Vec<usize> = dictionaries
            .iter()
            .enumerate()
            .filter(|(_, d)| d.default_active && d.has_no_suggest)
            .map(|(i, _)| i)
            .collect();
        let mut default_active_expensive_indices: Vec<usize> = dictionaries
            .iter()
            .enumerate()
            .filter(|(_, d)| d.default_active && d.has_expensive_forms)
            .map(|(i, _)| i)
            .collect();
        // Sort by dictionary size descending so .any() short-circuits faster
        // on the largest (most likely to match) dictionary.
        default_active_indices.sort_by(|&a, &b| dictionaries[b].len.cmp(&dictionaries[a].len));
        default_active_forbidden_indices
            .sort_by(|&a, &b| dictionaries[b].len.cmp(&dictionaries[a].len));
        default_active_no_suggest_indices
            .sort_by(|&a, &b| dictionaries[b].len.cmp(&dictionaries[a].len));
        default_active_expensive_indices
            .sort_by(|&a, &b| dictionaries[b].len.cmp(&dictionaries[a].len));

        Self {
            dictionaries,
            config,
            any_dict_has_forbidden,
            any_dict_has_no_suggest,
            all_dicts_case_insensitive,
            any_dict_has_expensive_forms,
            word_cache: None,
            local_word_cache: RefCell::new(LocalWordCache::default()),
            default_active_indices,
            default_active_forbidden_indices,
            default_active_no_suggest_indices,
            default_active_expensive_indices,
        }
    }

    /// Add a dictionary after construction (e.g., per-directory extra words).
    pub fn add_dictionary(&mut self, name: String, dict: Arc<dyn Dictionary>, active: bool) {
        let idx = self.dictionaries.len();
        let entry = DictionaryEntry::new(Some(name.to_lowercase()), dict, active);
        self.any_dict_has_forbidden |= entry.has_forbidden;
        self.any_dict_has_no_suggest |= entry.has_no_suggest;
        self.all_dicts_case_insensitive &= !entry.case_sensitive;
        self.any_dict_has_expensive_forms |= entry.has_expensive_forms;
        self.local_word_cache.get_mut().clear();
        self.dictionaries.push(entry);
        if active {
            self.default_active_indices.push(idx);
            self.default_active_indices
                .sort_by(|&a, &b| self.dictionaries[b].len.cmp(&self.dictionaries[a].len));
            if self.dictionaries[idx].has_forbidden {
                self.default_active_forbidden_indices.push(idx);
                self.default_active_forbidden_indices
                    .sort_by(|&a, &b| self.dictionaries[b].len.cmp(&self.dictionaries[a].len));
            }
            if self.dictionaries[idx].has_no_suggest {
                self.default_active_no_suggest_indices.push(idx);
                self.default_active_no_suggest_indices
                    .sort_by(|&a, &b| self.dictionaries[b].len.cmp(&self.dictionaries[a].len));
            }
            if self.dictionaries[idx].has_expensive_forms {
                self.default_active_expensive_indices.push(idx);
                self.default_active_expensive_indices
                    .sort_by(|&a, &b| self.dictionaries[b].len.cmp(&self.dictionaries[a].len));
            }
        }
    }

    pub fn set_word_cache(&mut self, cache: WordCache) {
        self.word_cache = Some(cache);
    }

    pub fn new_word_cache() -> WordCache {
        Arc::new(papaya::HashMap::new())
    }

    fn local_word_cache_get(&self, word: &str) -> Option<bool> {
        self.local_word_cache.borrow_mut().get(word)
    }

    fn local_word_cache_insert(&self, word: &str, found: bool) {
        self.local_word_cache.borrow_mut().insert(word, found);
    }

    /// Validate text and return all spelling issues found.
    pub fn validate_text(&self, text: &str) -> Vec<ValidationIssue> {
        // Pin a lock-free guard once per file for the shared cache.
        // All lookups/inserts go through this guard with zero contention.
        let cache_guard: Option<papaya::LocalGuard<'_>> =
            self.word_cache.as_ref().map(|c| c.guard());

        let mut issues = Vec::new();
        let mut disabled = false;
        let mut disable_next_line = false;
        let mut inline_words: HashSet<String> = HashSet::new();
        let mut directive_ignore_patterns: Vec<Regex> = Vec::new();
        let mut directive_dictionaries: Option<HashSet<String>> = None;
        let mut allow_compound_words = self.config.allow_compound_words;
        let mut word_issue_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        let mut directive_flag_words: HashSet<String> = HashSet::new();
        let mut directive_include_patterns: Vec<Regex> = Vec::new();
        let mut case_sensitive = self.config.case_sensitive;

        // Sparse directive scan: use AC prefilter to find candidate byte offsets,
        // map to line indices via byte-level newline scan, parse only matching lines.
        // Avoids allocating per-line Vec<&str>, Vec<bool>, Vec<Option<Directive>>.
        let mut directive_map: hashbrown::HashMap<usize, Directive> = hashbrown::HashMap::new();
        {
            let prefilter = &*directives::DIRECTIVE_PREFILTER;
            // Collect line indices that contain directive keywords
            let mut keyword_lines: hashbrown::HashSet<usize> = hashbrown::HashSet::new();
            {
                let bytes = text.as_bytes();
                let mut line_idx = 0usize;
                let mut next_newline = memchr_newline(bytes, 0);
                for mat in prefilter.find_iter(text) {
                    let pos = mat.start();
                    while pos >= next_newline {
                        line_idx += 1;
                        next_newline = memchr_newline(bytes, next_newline);
                    }
                    keyword_lines.insert(line_idx);
                }
            }

            if !keyword_lines.is_empty() {
                // Parse directives and check typos only on keyword lines
                for (i, line_info) in text_lines(text).enumerate() {
                    let line = line_info.text;
                    if keyword_lines.contains(&i) {
                        if let Some(directive) = directives::parse_directive_no_prefilter(line) {
                            directive_map.insert(i, directive);
                        } else if let Some(name) = directives::extract_directive_name(line) {
                            // Typo check: AC matched but no valid directive parsed
                            if let Some(warning) = directives::check_directive_typo(&name) {
                                let col =
                                    line.to_lowercase().find(&name).map(|p| p + 1).unwrap_or(1);
                                issues.push(ValidationIssue {
                                    word: warning.found,
                                    offset: line_info.offset + col - 1,
                                    line: i + 1,
                                    column: col,
                                    is_forbidden: false,
                                    is_known_typo: false,
                                    suggestions: vec![warning.suggestion],
                                });
                            }
                        }
                    }
                }
            }
        }

        // Collect file-wide directives from sparse map
        for directive in directive_map.values() {
            match directive {
                Directive::Ignore(words)
                | Directive::Words(words)
                | Directive::LocalWords(words) => {
                    for w in words {
                        inline_words.insert(w.to_lowercase());
                    }
                }
                Directive::ForbidWords(words) => {
                    for w in words {
                        directive_flag_words.insert(w.to_lowercase());
                    }
                }
                Directive::IgnoreRegExp(pattern) => {
                    if let Some(re) = parse_regex_pattern(pattern) {
                        directive_ignore_patterns.push(re);
                    }
                }
                Directive::IncludeRegExp(pattern) => {
                    if let Some(re) = parse_regex_pattern(pattern) {
                        directive_include_patterns.push(re);
                    }
                }
                _ => {}
            }
        }

        // cspell computes inclusion ranges for the full document, then validates
        // only those segments. Ignore patterns remove just the matched span,
        // they do not suppress an overlapping token wholesale.
        let doc_skip_ranges = self.compute_doc_skip_ranges(text, &directive_ignore_patterns);
        let has_include_config =
            !self.config.include_patterns.is_empty() || !directive_include_patterns.is_empty();
        let doc_include_ranges = self.compute_doc_include_ranges(text, &directive_include_patterns);
        let doc_validation_ranges = compute_doc_validation_ranges(
            text.len(),
            &doc_include_ranges,
            &doc_skip_ranges,
            has_include_config,
        );
        if has_include_config && doc_validation_ranges.is_empty() {
            return issues;
        }

        let mut code_tokens_buf: Vec<splitter::Word<'_>> = Vec::new();
        let mut possible_words_buf: Vec<splitter::Word<'_>> = Vec::new();
        let mut words_buf: Vec<splitter::Word<'_>> = Vec::new();
        let mut camel_parts_buf: Vec<&str> = Vec::new();
        let mut split_buffers = splitter::SplitBuffers::new();
        let mut prevalidated_ranges: Vec<(usize, usize)> = Vec::new();
        let mut known_successful_words: HashSet<String> = HashSet::new();
        let mut known_compat_issues: std::collections::HashMap<String, KnownCompatIssues> =
            std::collections::HashMap::new();
        let mut known_cspell_words: std::collections::HashMap<String, KnownCspellWordInfo> =
            std::collections::HashMap::new();
        let mut validation_range_pos = 0usize;

        for (line_idx, line_info) in text_lines(text).enumerate() {
            let line_num = line_idx + 1;
            let line = line_info.text;
            let line_start_offset = line_info.offset;

            if let Some(directive) = directive_map.get(&line_idx) {
                match directive {
                    Directive::Disable => {
                        disabled = true;
                        continue;
                    }
                    Directive::Enable => {
                        disabled = false;
                        continue;
                    }
                    Directive::DisableNextLine => {
                        disable_next_line = true;
                        continue;
                    }
                    Directive::DisableLine => {
                        continue;
                    }
                    Directive::Ignore(_)
                    | Directive::Words(_)
                    | Directive::LocalWords(_)
                    | Directive::ForbidWords(_)
                    | Directive::IgnoreRegExp(_)
                    | Directive::IncludeRegExp(_) => {
                        // Already collected in first pass
                        continue;
                    }
                    Directive::Dictionaries(dicts) => {
                        // Additive: merge with existing active set (matches cspell behavior).
                        let new_dicts: Vec<String> =
                            dicts.iter().map(|d| d.to_lowercase()).collect();
                        match directive_dictionaries {
                            Some(ref mut set) => {
                                set.extend(new_dicts);
                            }
                            None => {
                                // First directive: start from the base active set and add
                                let mut set: HashSet<String> = self
                                    .dictionaries
                                    .iter()
                                    .filter(|d| d.default_active)
                                    .filter_map(|d| d.name.clone())
                                    .collect();
                                set.extend(new_dicts);
                                directive_dictionaries = Some(set);
                            }
                        }
                        continue;
                    }
                    Directive::EnableCompoundWords => {
                        allow_compound_words = true;
                        continue;
                    }
                    Directive::DisableCompoundWords => {
                        allow_compound_words = false;
                        continue;
                    }
                    Directive::EnableCaseSensitive => {
                        case_sensitive = true;
                        continue;
                    }
                    Directive::DisableCaseSensitive => {
                        case_sensitive = false;
                        continue;
                    }
                    Directive::Language(_) => {
                        continue;
                    }
                }
            }

            if disabled {
                continue;
            }

            if disable_next_line {
                // cspell skips blank lines and applies disable to the next non-empty line
                if line.trim().is_empty() {
                    continue;
                }
                disable_next_line = false;
                continue;
            }

            if self.config.cspell_compat_mode {
                let line_end_offset = line_start_offset + line.len();
                while validation_range_pos < doc_validation_ranges.len()
                    && doc_validation_ranges[validation_range_pos].1 <= line_start_offset
                {
                    validation_range_pos += 1;
                }

                let mut seg_pos = validation_range_pos;
                while seg_pos < doc_validation_ranges.len()
                    && doc_validation_ranges[seg_pos].0 < line_end_offset
                {
                    let (range_start, range_end) = doc_validation_ranges[seg_pos];
                    let seg_start = range_start.max(line_start_offset);
                    let seg_end = range_end.min(line_end_offset);

                    if seg_start < seg_end {
                        let rel_seg_start = seg_start - line_start_offset;
                        let rel_seg_end = seg_end - line_start_offset;
                        self.validate_line_cspell_compat(
                            line,
                            &line[rel_seg_start..rel_seg_end],
                            rel_seg_start,
                            line_num,
                            line_start_offset,
                            &inline_words,
                            &directive_flag_words,
                            directive_dictionaries.as_ref(),
                            allow_compound_words,
                            case_sensitive,
                            cache_guard.as_ref(),
                            &mut known_successful_words,
                            &mut known_compat_issues,
                            &mut known_cspell_words,
                            &mut possible_words_buf,
                            &mut words_buf,
                            &mut camel_parts_buf,
                            &mut split_buffers,
                            &mut word_issue_counts,
                            &mut issues,
                        );
                    }

                    if range_end > line_end_offset {
                        break;
                    }
                    seg_pos += 1;
                }
                validation_range_pos = seg_pos;
                continue;
            }

            // Pre-check: extract broad code tokens (including underscores/digits)
            // and check whole identifiers against dictionaries. If found, mark
            // their ranges as valid so sub-words are skipped.
            splitter::extract_code_tokens_into(line, &mut code_tokens_buf);
            prevalidated_ranges.clear();
            for ct in &code_tokens_buf {
                // Only pre-validate tokens that contain underscores or digits
                // (i.e., those that would be split into sub-tokens)
                if ct
                    .text
                    .as_bytes()
                    .iter()
                    .any(|&b| b == b'_' || b == b'-' || b.is_ascii_digit())
                    && (self.is_word_valid(
                        ct.text,
                        directive_dictionaries.as_ref(),
                        allow_compound_words,
                        case_sensitive,
                        cache_guard.as_ref(),
                    ) || inline_words.contains(&ct.text.to_lowercase())
                        || self.is_underscore_compound_valid(
                            ct.text,
                            directive_dictionaries.as_ref(),
                            allow_compound_words,
                            case_sensitive,
                            cache_guard.as_ref(),
                        ))
                {
                    prevalidated_ranges.push((ct.offset, ct.offset + ct.text.len()));
                }
            }

            splitter::extract_words_into(line, &mut words_buf);

            for token in &words_buf {
                let abs_token_offset = line_start_offset + token.offset;

                if !doc_include_ranges.is_empty()
                    && !is_in_sorted_ranges(&doc_include_ranges, abs_token_offset)
                {
                    continue;
                }

                if is_in_sorted_ranges(&doc_skip_ranges, abs_token_offset) {
                    continue;
                }

                let token_lower = ascii_lowercase(token.text);
                let token_is_ignored = self.is_word_ignored_in_active_dicts(
                    token.text,
                    directive_dictionaries.as_ref(),
                    case_sensitive,
                );
                let token_is_flagged = !token_is_ignored
                    && (self.config.flag_words.contains(token_lower.as_ref())
                        || directive_flag_words.contains(token_lower.as_ref())
                        || self.is_word_forbidden_in_active_dicts(
                            token.text,
                            token_lower.as_ref(),
                            directive_dictionaries.as_ref(),
                        ));

                if token_is_ignored
                    || (!token_is_flagged
                        && (inline_words.contains(token_lower.as_ref())
                            || self.config.ignore_words.contains(token_lower.as_ref())))
                {
                    continue;
                }

                splitter::split_camel_case_into(
                    token.text,
                    &mut camel_parts_buf,
                    &mut split_buffers,
                );
                // Use stack array for single-part tokens (common case) to avoid heap allocation
                let single_word = [*token];
                let multi_words;
                let single_word_token = camel_parts_buf.len() <= 1;

                // When camelCase splitting produces multiple parts, check the
                // WHOLE token against dictionaries first. Words in the config
                // 'words' list (e.g. "jsDelivr") should be recognized as-is
                // without decomposition. This matches cspell behavior.
                if !single_word_token && !token_is_flagged {
                    let whole_valid =
                        self.has_in_active_dicts(token.text, directive_dictionaries.as_ref());
                    let shift_valid = !whole_valid
                        && self.all_camel_parts_valid_with_boundary_shift(
                            token.text,
                            &camel_parts_buf,
                            directive_dictionaries.as_ref(),
                            allow_compound_words,
                            case_sensitive,
                            cache_guard.as_ref(),
                        );
                    if whole_valid || shift_valid {
                        continue;
                    }
                }

                let sub_words: &[splitter::Word<'_>] = if single_word_token {
                    &single_word
                } else {
                    let mut subs = Vec::with_capacity(camel_parts_buf.len());
                    let base_ptr = token.text.as_ptr() as usize;
                    for &part in &camel_parts_buf {
                        let part_byte_offset = part.as_ptr() as usize - base_ptr;
                        subs.push(splitter::Word {
                            text: part,
                            offset: token.offset + part_byte_offset,
                        });
                    }
                    multi_words = subs;
                    &multi_words
                };

                for word in sub_words {
                    // Skip words that fall within a pre-validated code token range
                    if prevalidated_ranges.iter().any(|(start, end)| {
                        word.offset >= *start && word.offset + word.text.len() <= *end
                    }) {
                        continue;
                    }

                    let lower = ascii_lowercase(word.text);

                    let is_ignored = self.is_word_ignored_in_active_dicts(
                        word.text,
                        directive_dictionaries.as_ref(),
                        case_sensitive,
                    );
                    let is_forbidden = !is_ignored
                        && if single_word_token {
                            token_is_flagged
                        } else {
                            self.config.flag_words.contains(lower.as_ref())
                                || directive_flag_words.contains(lower.as_ref())
                                || self.is_word_forbidden_in_active_dicts(
                                    word.text,
                                    lower.as_ref(),
                                    directive_dictionaries.as_ref(),
                                )
                        };

                    if is_forbidden {
                        let count = word_issue_counts.entry(word.text.to_string()).or_insert(0);
                        *count += 1;
                        if *count <= self.config.max_duplicate_problems {
                            issues.push(ValidationIssue {
                                word: word.text.to_string(),
                                offset: line_start_offset + word.offset,
                                line: line_num,
                                column: byte_offset_to_char_col(line, word.offset),
                                is_forbidden: true,
                                is_known_typo: false,
                                suggestions: Vec::new(),
                            });
                        }
                        continue;
                    }

                    if word.text.chars().count() < self.config.min_word_length {
                        continue;
                    }

                    // Skip words made of repeating characters (e.g. HHHH, aaaa)
                    if is_repeating_char(word.text) {
                        continue;
                    }

                    if is_ignored
                        || inline_words.contains(lower.as_ref())
                        || self.config.ignore_words.contains(lower.as_ref())
                    {
                        continue;
                    }

                    if self
                        .config
                        .ignore_patterns
                        .iter()
                        .any(|re| re.is_match(word.text))
                        || directive_ignore_patterns
                            .iter()
                            .any(|re| re.is_match(word.text))
                    {
                        continue;
                    }

                    if !self.is_word_valid(
                        word.text,
                        directive_dictionaries.as_ref(),
                        allow_compound_words,
                        case_sensitive,
                        cache_guard.as_ref(),
                    ) {
                        // Fallback: strip trailing possessive/contraction and re-check
                        if let Some(base) = strip_trailing_suffix(word.text)
                            && (base.chars().count() < self.config.min_word_length
                                || self.is_word_valid(
                                    base,
                                    directive_dictionaries.as_ref(),
                                    allow_compound_words,
                                    case_sensitive,
                                    cache_guard.as_ref(),
                                ))
                        {
                            continue;
                        }

                        // Fallback: ALL-CAPS + English suffix (e.g. REPLs → REPL)
                        // cspell's isAllCapsWithTrailingCommonEnglishSuffix:
                        //   1. If stem is flagged → not valid
                        //   2. If stem is found in dictionary → valid
                        //   3. If stem is too short (< minWordLength) → valid
                        if let Some(base) = strip_all_caps_suffix(word.text) {
                            let base_lower = ascii_lowercase(base);
                            let base_flagged =
                                !self.is_word_ignored_in_active_dicts(
                                    base,
                                    directive_dictionaries.as_ref(),
                                    case_sensitive,
                                ) && (self.config.flag_words.contains(base_lower.as_ref())
                                    || directive_flag_words.contains(base_lower.as_ref())
                                    || self.is_word_forbidden_in_active_dicts(
                                        base,
                                        base_lower.as_ref(),
                                        directive_dictionaries.as_ref(),
                                    ));
                            if !base_flagged
                                && (self.is_word_valid(
                                    base,
                                    directive_dictionaries.as_ref(),
                                    allow_compound_words,
                                    case_sensitive,
                                    cache_guard.as_ref(),
                                ) || base.chars().count() < self.config.min_word_length)
                            {
                                continue;
                            }
                        }

                        // Fallback: split at apostrophe and check all parts
                        // (e.g. f'hello → f + hello, both found in dicts)
                        if (word.text.contains('\'') || word.text.contains('\u{2019}'))
                            && self.all_apostrophe_parts_valid(
                                word.text,
                                directive_dictionaries.as_ref(),
                                allow_compound_words,
                                case_sensitive,
                                cache_guard.as_ref(),
                            )
                        {
                            continue;
                        }
                        // Fall through to report the whole word (cspell behavior:
                        // reports "Zakas's" not just "Zakas") // cspell:ignore Zakas

                        // Fallback: if preceded by '\', drop first char and retry.
                        // Handles regex escapes (\s, \n, etc.) where the escape char
                        // is joined with the following word. Only accept if the
                        // remainder is a valid dictionary word (no minWordLength
                        // bypass — cspell's A* splitter also requires validity).
                        if word.offset > 0
                            && line.as_bytes().get(word.offset - 1) == Some(&b'\\')
                            && word.text.len() > 1
                        {
                            let first_len =
                                word.text.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                            let without_first = &word.text[first_len..];
                            // Strip leading apostrophe if present
                            let effective = without_first
                                .strip_prefix('\'')
                                .or_else(|| without_first.strip_prefix('\u{2019}'))
                                .unwrap_or(without_first);
                            if self.is_word_valid(
                                effective,
                                directive_dictionaries.as_ref(),
                                allow_compound_words,
                                case_sensitive,
                                cache_guard.as_ref(),
                            ) {
                                continue;
                            }
                        }

                        // cspell counts duplicates case-sensitively
                        let count = word_issue_counts.entry(word.text.to_string()).or_insert(0);
                        *count += 1;
                        if *count > self.config.max_duplicate_problems {
                            continue;
                        }
                        let typo_correction =
                            typos_dict::WORD.find(&unicase::UniCase::new(word.text));

                        let mut suggestions = if self.config.compute_suggestions {
                            self.get_suggestions(word.text, directive_dictionaries.as_ref())
                        } else {
                            Vec::new()
                        };

                        if let Some(corrections) = typo_correction {
                            for &c in corrections.iter().rev() {
                                if !suggestions.iter().any(|s| s.eq_ignore_ascii_case(c)) {
                                    suggestions.insert(0, c.to_string());
                                }
                            }
                        }

                        issues.push(ValidationIssue {
                            word: word.text.to_string(),
                            offset: line_start_offset + word.offset,
                            line: line_num,
                            column: byte_offset_to_char_col(line, word.offset),
                            is_forbidden: false,
                            is_known_typo: typo_correction.is_some(),
                            suggestions,
                        });
                    }
                }
            }
        }

        self.filter_issues_with_fancy_patterns(text, &mut issues);
        issues
    }

    #[allow(clippy::too_many_arguments)]
    fn validate_line_cspell_compat<'a>(
        &self,
        line: &'a str,
        segment: &'a str,
        segment_start_in_line: usize,
        line_num: usize,
        line_start_offset: usize,
        inline_words: &HashSet<String>,
        directive_flag_words: &HashSet<String>,
        directive_dictionaries: Option<&HashSet<String>>,
        allow_compound_words: bool,
        case_sensitive: bool,
        cache_guard: Option<&papaya::LocalGuard<'_>>,
        known_successful_words: &mut HashSet<String>,
        known_compat_issues: &mut std::collections::HashMap<String, KnownCompatIssues>,
        known_cspell_words: &mut std::collections::HashMap<String, KnownCspellWordInfo>,
        possible_words_buf: &mut Vec<splitter::Word<'a>>,
        words_buf: &mut Vec<splitter::Word<'a>>,
        camel_parts_buf: &mut Vec<&'a str>,
        split_buffers: &mut splitter::SplitBuffers,
        word_issue_counts: &mut std::collections::HashMap<String, usize>,
        issues: &mut Vec<ValidationIssue>,
    ) {
        splitter::extract_possible_words_into(segment, possible_words_buf);
        let line_word = splitter::Word {
            text: line,
            offset: line_start_offset,
        };
        let segment_word = splitter::Word {
            text: segment,
            offset: line_start_offset + segment_start_in_line,
        };

        for seg_word in possible_words_buf.iter().copied() {
            let possible_word = splitter::Word {
                text: seg_word.text,
                offset: segment_start_in_line + seg_word.offset,
            };
            if self.config.ignore_random_strings
                && random::is_random_string(possible_word.text, self.config.min_random_length)
            {
                continue;
            }

            if known_successful_words.contains(possible_word.text) {
                continue;
            }

            let compat_issues = self.check_possible_word_cspell(
                line_word,
                segment_word,
                possible_word,
                inline_words,
                directive_flag_words,
                directive_dictionaries,
                allow_compound_words,
                case_sensitive,
                cache_guard,
                known_successful_words,
                known_compat_issues,
                known_cspell_words,
                words_buf,
                camel_parts_buf,
                split_buffers,
            );

            for issue in compat_issues {
                self.report_compat_issue(
                    issue,
                    line,
                    line_num,
                    line_start_offset,
                    directive_dictionaries,
                    word_issue_counts,
                    issues,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn check_possible_word_cspell<'a>(
        &self,
        line: splitter::Word<'a>,
        segment: splitter::Word<'a>,
        possible_word: splitter::Word<'a>,
        inline_words: &HashSet<String>,
        directive_flag_words: &HashSet<String>,
        directive_dictionaries: Option<&HashSet<String>>,
        allow_compound_words: bool,
        case_sensitive: bool,
        cache_guard: Option<&papaya::LocalGuard<'_>>,
        known_successful_words: &mut HashSet<String>,
        known_compat_issues: &mut std::collections::HashMap<String, KnownCompatIssues>,
        known_cspell_words: &mut std::collections::HashMap<String, KnownCspellWordInfo>,
        words_buf: &mut Vec<splitter::Word<'a>>,
        camel_parts_buf: &mut Vec<&'a str>,
        split_buffers: &mut splitter::SplitBuffers,
    ) -> Vec<CompatIssue> {
        let possible_word_abs_offset = line.offset + possible_word.offset;
        if let Some(known) = known_compat_issues.get(possible_word.text) {
            return rebase_known_compat_issues(possible_word_abs_offset, known);
        }

        if let Some(flagged) = self.check_for_flagged_word_cspell(
            possible_word,
            directive_flag_words,
            directive_dictionaries,
            case_sensitive,
            known_cspell_words,
        ) {
            let issues = vec![CompatIssue {
                word: flagged.text.to_string(),
                offset: line.offset + flagged.offset,
                is_forbidden: true,
            }];
            known_compat_issues.insert(
                possible_word.text.to_string(),
                KnownCompatIssues {
                    possible_word_abs_offset,
                    issues: issues.clone(),
                },
            );
            return issues;
        }

        let mut mismatches = Vec::new();
        splitter::extract_words_into(possible_word.text, words_buf);
        for sub_word in words_buf.iter().copied() {
            let word = splitter::Word {
                text: sub_word.text,
                offset: possible_word.offset + sub_word.offset,
            };
            if known_successful_words.contains(word.text) {
                continue;
            }
            let flagged = self.cached_is_word_flagged_cspell(
                word.text,
                directive_flag_words,
                directive_dictionaries,
                case_sensitive,
                known_cspell_words,
            );
            if !flagged && js_string_len(word.text) < self.config.min_word_length {
                continue;
            }
            mismatches.extend(self.check_full_word_cspell(
                line,
                word,
                inline_words,
                directive_flag_words,
                directive_dictionaries,
                allow_compound_words,
                case_sensitive,
                cache_guard,
                known_successful_words,
                known_cspell_words,
                camel_parts_buf,
                split_buffers,
            ));
        }
        if mismatches.is_empty() {
            return mismatches;
        }

        let hex_sequences = if self.config.ignore_random_strings {
            let filtered: Vec<splitter::Word<'_>> = self
                .extract_hex_sequences_cspell(possible_word.text, 8)
                .into_iter()
                .filter(|w| {
                    (w.text == w.text.to_lowercase() || w.text == w.text.to_uppercase())
                        && w.text.chars().any(|ch| ch.is_ascii_digit() || ch == '-')
                })
                .map(|w| splitter::Word {
                    text: w.text,
                    offset: line.offset + possible_word.offset + w.offset,
                })
                .collect();
            if filtered.is_empty() {
                None
            } else {
                Some(filtered)
            }
        } else {
            None
        };

        if let Some(excluded) = hex_sequences.as_ref() {
            mismatches = filter_excluded_compat_issues(mismatches, excluded);
        }

        if mismatches.is_empty() {
            return mismatches;
        }

        let split_result = splitter::split(
            segment,
            line.offset + possible_word.offset,
            |split_word| {
                self.splitter_is_valid_cspell(
                    line,
                    split_word,
                    inline_words,
                    directive_flag_words,
                    directive_dictionaries,
                    allow_compound_words,
                    case_sensitive,
                    cache_guard,
                    known_successful_words,
                    known_cspell_words,
                )
            },
            None,
        );

        let mut filtered = Vec::new();
        for split_word in split_result.words {
            if split_word.is_found {
                continue;
            }

            if let Some(base) = strip_all_caps_suffix(split_word.text)
                && !self.cached_is_word_flagged_cspell(
                    base,
                    directive_flag_words,
                    directive_dictionaries,
                    case_sensitive,
                    known_cspell_words,
                )
                && (js_string_len(base) < self.config.min_word_length
                    || self.is_word_valid(
                        base,
                        directive_dictionaries,
                        allow_compound_words,
                        case_sensitive,
                        cache_guard,
                    ))
            {
                continue;
            }

            filtered.push(CompatIssue {
                word: split_word.text.to_string(),
                offset: split_word.offset,
                is_forbidden: self.cached_is_word_flagged_cspell(
                    split_word.text,
                    directive_flag_words,
                    directive_dictionaries,
                    case_sensitive,
                    known_cspell_words,
                ),
            });
        }

        if let Some(excluded) = hex_sequences.as_ref() {
            filtered = filter_excluded_compat_issues(filtered, excluded);
        }

        if filtered.len() < mismatches.len() {
            known_compat_issues.insert(
                possible_word.text.to_string(),
                KnownCompatIssues {
                    possible_word_abs_offset,
                    issues: filtered.clone(),
                },
            );
            return filtered;
        }

        known_compat_issues.insert(
            possible_word.text.to_string(),
            KnownCompatIssues {
                possible_word_abs_offset,
                issues: mismatches.clone(),
            },
        );
        mismatches
    }

    #[allow(clippy::too_many_arguments)]
    fn check_full_word_cspell<'a>(
        &self,
        line: splitter::Word<'a>,
        word: splitter::Word<'a>,
        inline_words: &HashSet<String>,
        directive_flag_words: &HashSet<String>,
        directive_dictionaries: Option<&HashSet<String>>,
        allow_compound_words: bool,
        case_sensitive: bool,
        cache_guard: Option<&papaya::LocalGuard<'_>>,
        known_successful_words: &mut HashSet<String>,
        known_cspell_words: &mut std::collections::HashMap<String, KnownCspellWordInfo>,
        camel_parts_buf: &mut Vec<&'a str>,
        split_buffers: &mut splitter::SplitBuffers,
    ) -> Vec<CompatIssue> {
        if self.cached_is_word_flagged_cspell(
            word.text,
            directive_flag_words,
            directive_dictionaries,
            case_sensitive,
            known_cspell_words,
        ) {
            return vec![CompatIssue {
                word: word.text.to_string(),
                offset: line.offset + word.offset,
                is_forbidden: true,
            }];
        }

        if self.is_all_caps_with_suffix_ok_cspell(
            word,
            directive_flag_words,
            directive_dictionaries,
            allow_compound_words,
            case_sensitive,
            cache_guard,
            known_cspell_words,
        ) {
            known_successful_words.insert(word.text.to_string());
            return Vec::new();
        }

        let lower = ascii_lowercase(word.text);
        if inline_words.contains(lower.as_ref())
            || self.config.ignore_words.contains(lower.as_ref())
        {
            known_successful_words.insert(word.text.to_string());
            return Vec::new();
        }

        let checked = self.check_word_cspell(
            line,
            word,
            directive_flag_words,
            directive_dictionaries,
            allow_compound_words,
            case_sensitive,
            cache_guard,
            known_cspell_words,
        );
        if checked.is_found == Some(true) {
            known_successful_words.insert(word.text.to_string());
            return Vec::new();
        }

        if checked.is_flagged {
            return vec![CompatIssue {
                word: word.text.to_string(),
                offset: line.offset + word.offset,
                is_forbidden: true,
            }];
        }

        let code_word_results = self.check_camel_case_word_cspell(
            line,
            word,
            inline_words,
            directive_flag_words,
            directive_dictionaries,
            allow_compound_words,
            case_sensitive,
            cache_guard,
            known_successful_words,
            known_cspell_words,
            camel_parts_buf,
            split_buffers,
        );

        if code_word_results.is_empty() {
            known_successful_words.insert(word.text.to_string());
        }

        code_word_results
    }

    #[allow(clippy::too_many_arguments)]
    fn check_camel_case_word_cspell<'a>(
        &self,
        line: splitter::Word<'a>,
        word: splitter::Word<'a>,
        inline_words: &HashSet<String>,
        directive_flag_words: &HashSet<String>,
        directive_dictionaries: Option<&HashSet<String>>,
        allow_compound_words: bool,
        case_sensitive: bool,
        cache_guard: Option<&papaya::LocalGuard<'_>>,
        known_successful_words: &mut HashSet<String>,
        known_cspell_words: &mut std::collections::HashMap<String, KnownCspellWordInfo>,
        camel_parts_buf: &mut Vec<&'a str>,
        split_buffers: &mut splitter::SplitBuffers,
    ) -> Vec<CompatIssue> {
        splitter::split_camel_case_into(word.text, camel_parts_buf, split_buffers);

        let mut multi_words = Vec::new();
        let parts: &[splitter::Word<'_>] = if camel_parts_buf.len() <= 1 {
            std::slice::from_ref(&word)
        } else {
            let base_ptr = word.text.as_ptr() as usize;
            for &part in camel_parts_buf.iter() {
                let part_byte_offset = part.as_ptr() as usize - base_ptr;
                multi_words.push(splitter::Word {
                    text: part,
                    offset: word.offset + part_byte_offset,
                });
            }
            &multi_words
        };

        let mut issues = Vec::new();
        for part in parts.iter().copied() {
            if known_successful_words.contains(part.text) {
                continue;
            }

            let lower = ascii_lowercase(part.text);
            if inline_words.contains(lower.as_ref())
                || self.config.ignore_words.contains(lower.as_ref())
            {
                known_successful_words.insert(part.text.to_string());
                continue;
            }

            let flagged = self.cached_is_word_flagged_cspell(
                part.text,
                directive_flag_words,
                directive_dictionaries,
                case_sensitive,
                known_cspell_words,
            );
            if !flagged && js_string_len(part.text) < self.config.min_word_length {
                continue;
            }

            if flagged {
                issues.push(CompatIssue {
                    word: part.text.to_string(),
                    offset: line.offset + part.offset,
                    is_forbidden: true,
                });
                continue;
            }

            let checked = self.check_word_cspell(
                line,
                part,
                directive_flag_words,
                directive_dictionaries,
                allow_compound_words,
                case_sensitive,
                cache_guard,
                known_cspell_words,
            );
            if checked.is_found == Some(true) || is_repeating_char(part.text) {
                known_successful_words.insert(part.text.to_string());
                continue;
            }

            issues.push(CompatIssue {
                word: part.text.to_string(),
                offset: line.offset + part.offset,
                is_forbidden: false,
            });
        }

        issues
    }

    #[allow(clippy::too_many_arguments)]
    fn splitter_is_valid_cspell(
        &self,
        line: splitter::Word<'_>,
        word: splitter::Word<'_>,
        inline_words: &HashSet<String>,
        directive_flag_words: &HashSet<String>,
        directive_dictionaries: Option<&HashSet<String>>,
        allow_compound_words: bool,
        case_sensitive: bool,
        cache_guard: Option<&papaya::LocalGuard<'_>>,
        known_successful_words: &HashSet<String>,
        known_cspell_words: &mut std::collections::HashMap<String, KnownCspellWordInfo>,
    ) -> bool {
        if known_successful_words.contains(word.text) {
            return true;
        }

        if self.cached_is_word_flagged_cspell(
            word.text,
            directive_flag_words,
            directive_dictionaries,
            case_sensitive,
            known_cspell_words,
        ) {
            return false;
        }

        let lower = ascii_lowercase(word.text);
        if inline_words.contains(lower.as_ref())
            || self.config.ignore_words.contains(lower.as_ref())
            || self.cached_is_word_ignored_cspell(
                word.text,
                directive_dictionaries,
                case_sensitive,
                known_cspell_words,
            )
            || self.is_word_valid_with_escape_retry_cspell(
                line,
                word,
                directive_dictionaries,
                allow_compound_words,
                case_sensitive,
                cache_guard,
                known_cspell_words,
            )
        {
            return true;
        }

        if self.is_word_too_short_cspell(word, line, false) {
            return true;
        }

        self.is_all_caps_with_suffix_ok_cspell(
            word,
            directive_flag_words,
            directive_dictionaries,
            allow_compound_words,
            case_sensitive,
            cache_guard,
            known_cspell_words,
        )
    }

    fn cached_is_word_flagged_cspell(
        &self,
        word: &str,
        directive_flag_words: &HashSet<String>,
        directive_dictionaries: Option<&HashSet<String>>,
        case_sensitive: bool,
        known_cspell_words: &mut std::collections::HashMap<String, KnownCspellWordInfo>,
    ) -> bool {
        if let Some(flagged) = known_cspell_words
            .get(word)
            .and_then(|info| info.is_flagged)
        {
            return flagged;
        }

        let lower = ascii_lowercase(word);
        let flagged = (self.config.flag_words.contains(lower.as_ref())
            || directive_flag_words.contains(lower.as_ref())
            || self.is_word_forbidden_in_active_dicts(
                word,
                lower.as_ref(),
                directive_dictionaries,
            ))
            && !self.cached_is_word_ignored_cspell(
                word,
                directive_dictionaries,
                case_sensitive,
                known_cspell_words,
            );

        known_cspell_words
            .entry(word.to_string())
            .or_default()
            .is_flagged = Some(flagged);
        flagged
    }

    fn cached_is_word_ignored_cspell(
        &self,
        word: &str,
        directive_dictionaries: Option<&HashSet<String>>,
        case_sensitive: bool,
        known_cspell_words: &mut std::collections::HashMap<String, KnownCspellWordInfo>,
    ) -> bool {
        if let Some(ignored) = known_cspell_words
            .get(word)
            .and_then(|info| info.is_ignored)
        {
            return ignored;
        }

        let ignored =
            self.is_word_ignored_in_active_dicts(word, directive_dictionaries, case_sensitive);

        known_cspell_words
            .entry(word.to_string())
            .or_default()
            .is_ignored = Some(ignored);
        ignored
    }

    #[allow(clippy::too_many_arguments)]
    fn is_word_valid_with_escape_retry_cspell(
        &self,
        line: splitter::Word<'_>,
        word: splitter::Word<'_>,
        directive_dictionaries: Option<&HashSet<String>>,
        allow_compound_words: bool,
        case_sensitive: bool,
        cache_guard: Option<&papaya::LocalGuard<'_>>,
        known_cspell_words: &mut std::collections::HashMap<String, KnownCspellWordInfo>,
    ) -> bool {
        if self.has_word_check_cspell(
            word.text,
            directive_dictionaries,
            allow_compound_words,
            case_sensitive,
            cache_guard,
            known_cspell_words,
        ) {
            return true;
        }

        let rel = compat_word_rel_offset(word, line);
        if rel > 0 && line.text.as_bytes().get(rel - 1) == Some(&b'\\') {
            let first_len = word.text.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            return self.has_word_check_cspell(
                &word.text[first_len..],
                directive_dictionaries,
                allow_compound_words,
                case_sensitive,
                cache_guard,
                known_cspell_words,
            );
        }

        false
    }

    #[allow(clippy::too_many_arguments)]
    fn has_word_check_cspell(
        &self,
        word: &str,
        directive_dictionaries: Option<&HashSet<String>>,
        allow_compound_words: bool,
        case_sensitive: bool,
        cache_guard: Option<&papaya::LocalGuard<'_>>,
        known_cspell_words: &mut std::collections::HashMap<String, KnownCspellWordInfo>,
    ) -> bool {
        if word.contains('\\') {
            let stripped: String = word.chars().filter(|&ch| ch != '\\').collect();
            return self.has_dict_word_cspell(
                &stripped,
                directive_dictionaries,
                allow_compound_words,
                case_sensitive,
                cache_guard,
                known_cspell_words,
            );
        }

        self.has_dict_word_cspell(
            word,
            directive_dictionaries,
            allow_compound_words,
            case_sensitive,
            cache_guard,
            known_cspell_words,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn has_dict_word_cspell(
        &self,
        word: &str,
        directive_dictionaries: Option<&HashSet<String>>,
        allow_compound_words: bool,
        case_sensitive: bool,
        cache_guard: Option<&papaya::LocalGuard<'_>>,
        known_cspell_words: &mut std::collections::HashMap<String, KnownCspellWordInfo>,
    ) -> bool {
        if let Some(info) = known_cspell_words.get(word) {
            if let Some(is_found) = info.is_found {
                return is_found;
            }
            if info.is_flagged == Some(true) {
                return true;
            }
        }

        let is_found = self.is_word_valid(
            word,
            directive_dictionaries,
            allow_compound_words,
            case_sensitive,
            cache_guard,
        );
        known_cspell_words
            .entry(word.to_string())
            .or_default()
            .is_found = Some(is_found);
        is_found
    }

    #[allow(clippy::too_many_arguments)]
    fn check_word_cspell(
        &self,
        line: splitter::Word<'_>,
        word: splitter::Word<'_>,
        directive_flag_words: &HashSet<String>,
        directive_dictionaries: Option<&HashSet<String>>,
        allow_compound_words: bool,
        case_sensitive: bool,
        cache_guard: Option<&papaya::LocalGuard<'_>>,
        known_cspell_words: &mut std::collections::HashMap<String, KnownCspellWordInfo>,
    ) -> CheckedCspellWord {
        if let Some(info) = known_cspell_words.get(word.text)
            && info.fin
        {
            let is_flagged = info.is_flagged.unwrap_or(false) && !info.is_ignored.unwrap_or(false);
            return CheckedCspellWord {
                is_flagged,
                is_found: if is_flagged { None } else { info.is_found },
            };
        }

        let is_ignored = self.cached_is_word_ignored_cspell(
            word.text,
            directive_dictionaries,
            case_sensitive,
            known_cspell_words,
        );
        let is_flagged = self.cached_is_word_flagged_cspell(
            word.text,
            directive_flag_words,
            directive_dictionaries,
            case_sensitive,
            known_cspell_words,
        ) && !is_ignored;
        let is_found = if is_flagged {
            Some(false)
        } else {
            Some(
                is_ignored
                    || self.is_word_valid_with_escape_retry_cspell(
                        line,
                        word,
                        directive_dictionaries,
                        allow_compound_words,
                        case_sensitive,
                        cache_guard,
                        known_cspell_words,
                    ),
            )
        };

        let entry = known_cspell_words.entry(word.text.to_string()).or_default();
        entry.is_ignored = Some(is_ignored);
        entry.is_flagged = Some(is_flagged);
        entry.is_found = is_found;
        entry.fin = true;

        CheckedCspellWord {
            is_flagged,
            is_found: if is_flagged { None } else { is_found },
        }
    }

    fn is_word_too_short_cspell(
        &self,
        word: splitter::Word<'_>,
        line: splitter::Word<'_>,
        ignore_suffix: bool,
    ) -> bool {
        if js_string_len(word.text) >= self.config.min_word_length * 2
            || word.text.chars().count() >= self.config.min_word_length
        {
            return false;
        }

        let rel = compat_word_rel_offset(word, line);
        let prefix = &line.text[..rel];
        if prefix
            .chars()
            .next_back()
            .is_some_and(splitter::is_cspell_letter)
        {
            return false;
        }

        if ignore_suffix {
            return true;
        }

        let end = rel + word.text.len();
        !line.text[end..]
            .chars()
            .next()
            .is_some_and(splitter::is_cspell_letter)
    }

    #[allow(clippy::too_many_arguments)]
    fn is_all_caps_with_suffix_ok_cspell(
        &self,
        word: splitter::Word<'_>,
        directive_flag_words: &HashSet<String>,
        directive_dictionaries: Option<&HashSet<String>>,
        allow_compound_words: bool,
        case_sensitive: bool,
        cache_guard: Option<&papaya::LocalGuard<'_>>,
        known_cspell_words: &mut std::collections::HashMap<String, KnownCspellWordInfo>,
    ) -> bool {
        let Some(base) = strip_all_caps_suffix(word.text) else {
            return false;
        };

        if self.cached_is_word_flagged_cspell(
            base,
            directive_flag_words,
            directive_dictionaries,
            case_sensitive,
            known_cspell_words,
        ) {
            return false;
        }

        self.is_word_valid(
            base,
            directive_dictionaries,
            allow_compound_words,
            case_sensitive,
            cache_guard,
        ) || js_string_len(base) < self.config.min_word_length
    }

    fn check_for_flagged_word_cspell<'a>(
        &self,
        possible_word: splitter::Word<'a>,
        directive_flag_words: &HashSet<String>,
        directive_dictionaries: Option<&HashSet<String>>,
        case_sensitive: bool,
        known_cspell_words: &mut std::collections::HashMap<String, KnownCspellWordInfo>,
    ) -> Option<splitter::Word<'a>> {
        if self.cached_is_word_flagged_cspell(
            possible_word.text,
            directive_flag_words,
            directive_dictionaries,
            case_sensitive,
            known_cspell_words,
        ) {
            return Some(possible_word);
        }

        if possible_word.text.ends_with('.') && possible_word.text.len() > 1 {
            let trimmed = splitter::Word {
                text: &possible_word.text[..possible_word.text.len() - 1],
                offset: possible_word.offset,
            };
            if self.cached_is_word_flagged_cspell(
                trimmed.text,
                directive_flag_words,
                directive_dictionaries,
                case_sensitive,
                known_cspell_words,
            ) {
                return Some(trimmed);
            }
        }

        None
    }

    #[allow(clippy::too_many_arguments)]
    fn report_compat_issue(
        &self,
        issue: CompatIssue,
        line: &str,
        line_num: usize,
        line_start_offset: usize,
        directive_dictionaries: Option<&HashSet<String>>,
        word_issue_counts: &mut std::collections::HashMap<String, usize>,
        issues: &mut Vec<ValidationIssue>,
    ) {
        let count = word_issue_counts.entry(issue.word.clone()).or_insert(0);
        *count += 1;
        if *count > self.config.max_duplicate_problems {
            return;
        }

        let typo_correction = typos_dict::WORD.find(&unicase::UniCase::new(issue.word.as_str()));
        let mut suggestions = if self.config.compute_suggestions {
            self.get_suggestions(&issue.word, directive_dictionaries)
        } else {
            Vec::new()
        };

        if let Some(corrections) = typo_correction {
            for &c in corrections.iter().rev() {
                if !suggestions.iter().any(|s| s.eq_ignore_ascii_case(c)) {
                    suggestions.insert(0, c.to_string());
                }
            }
        }

        issues.push(ValidationIssue {
            word: issue.word,
            offset: issue.offset,
            line: line_num,
            column: byte_offset_to_char_col(line, issue.offset - line_start_offset),
            is_forbidden: issue.is_forbidden,
            is_known_typo: typo_correction.is_some(),
            suggestions,
        });
    }

    fn extract_hex_sequences_cspell<'a>(
        &self,
        text: &'a str,
        min_length: usize,
    ) -> Vec<splitter::Word<'a>> {
        static HEX_SEQUENCE_RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"[0-9A-Fa-f][-0-9A-Fa-f]*[0-9A-Fa-f]").unwrap());

        HEX_SEQUENCE_RE
            .find_iter(text)
            .filter_map(|m| {
                let matched = m.as_str();
                let prev_is_letter = text[..m.start()]
                    .chars()
                    .next_back()
                    .is_some_and(splitter::is_cspell_letter);
                let next_is_letter = text[m.end()..]
                    .chars()
                    .next()
                    .is_some_and(splitter::is_cspell_letter);

                (!prev_is_letter && !next_is_letter && matched.len() >= min_length).then_some(
                    splitter::Word {
                        text: matched,
                        offset: m.start(),
                    },
                )
            })
            .collect()
    }

    /// Compute skip ranges over the full document text (absolute byte offsets).
    /// Uses Aho-Corasick literal prefilter for anchor-triggered patterns
    /// and hand-written byte scanners for common patterns.
    /// Returns ranges sorted by start offset for binary search lookup.
    fn compute_doc_skip_ranges(
        &self,
        text: &str,
        directive_patterns: &[Regex],
    ) -> Vec<(usize, usize)> {
        let mut ranges = Vec::new();
        let prefilter = &*BUILTIN_SKIP_PREFILTER;
        let bytes = text.as_bytes();

        // Hand-written byte scanners (no regex overhead).
        scan_commit_hashes(bytes, &mut ranges);
        scan_uuids(bytes, &mut ranges);
        scan_scientific_notation(bytes, &mut ranges);
        if self.config.custom_ignore_patterns.has_base64() {
            scan_base64(bytes, &mut ranges);
        }
        if self.config.custom_ignore_patterns.has_hash_strings() {
            scan_hash_strings(bytes, &mut ranges);
        }
        if self.config.custom_ignore_patterns.has_ada_word_break() {
            scan_ada_word_breaks(bytes, &mut ranges);
        }
        if self
            .config
            .custom_ignore_patterns
            .has_latex_macro_function_names()
        {
            scan_latex_macro_function_names(bytes, &mut ranges);
        }
        if self
            .config
            .custom_ignore_patterns
            .has_latex_macros_multiline()
        {
            scan_latex_macros_multiline(bytes, &mut ranges);
        }
        if self.config.custom_ignore_patterns.has_latex_math() {
            scan_latex_math(bytes, &mut ranges);
        }
        scan_base64_single_line(bytes, &mut ranges);
        scan_base64_multiline(bytes, &mut ranges);

        // AC prefilter: determine which anchor-triggered patterns are needed
        let mut needed = [false; 20];
        for mat in prefilter.ac.find_iter(text) {
            for &regex_idx in prefilter.anchor_to_regex[mat.pattern().as_usize()] {
                needed[regex_idx] = true;
            }
        }
        for (i, pattern) in BUILTIN_SKIP_PATTERNS.iter().enumerate() {
            if needed[i] {
                for m in pattern.find_iter(text) {
                    ranges.push((m.start(), m.end()));
                }
            }
        }

        for pattern in self
            .config
            .ignore_patterns
            .iter()
            .chain(directive_patterns.iter())
        {
            for m in pattern.find_iter(text) {
                ranges.push((m.start(), m.end()));
            }
        }
        merge_sorted_ranges(&mut ranges);
        ranges
    }

    /// Compute include ranges over the full document text (absolute byte offsets).
    /// Returns merged, sorted ranges for binary search lookup.
    fn compute_doc_include_ranges(
        &self,
        text: &str,
        directive_patterns: &[Regex],
    ) -> Vec<(usize, usize)> {
        let mut ranges = Vec::new();
        for pattern in self
            .config
            .include_patterns
            .iter()
            .chain(directive_patterns.iter())
        {
            for m in pattern.find_iter(text) {
                ranges.push((m.start(), m.end()));
            }
        }
        merge_sorted_ranges(&mut ranges);
        ranges
    }

    fn compute_fancy_ranges(patterns: &[FancyRegex], text: &str) -> Vec<(usize, usize)> {
        if patterns.is_empty() {
            return Vec::new();
        }

        let mut ranges = Vec::new();
        for pattern in patterns {
            for m in pattern.find_iter(text).filter_map(Result::ok) {
                ranges.push((m.start(), m.end()));
            }
        }
        merge_sorted_ranges(&mut ranges);
        ranges
    }

    /// Expensive fancy-regex patterns are evaluated only after the cheap pass
    /// has produced candidate issues. This avoids scanning clean files with
    /// backtracking-heavy expressions while preserving document-wide semantics.
    fn filter_issues_with_fancy_patterns(&self, text: &str, issues: &mut Vec<ValidationIssue>) {
        if issues.is_empty()
            || (self.config.ignore_patterns_fancy.is_empty()
                && self.config.include_patterns_fancy.is_empty())
        {
            return;
        }

        let ignore_ranges = Self::compute_fancy_ranges(&self.config.ignore_patterns_fancy, text);
        let include_ranges = Self::compute_fancy_ranges(&self.config.include_patterns_fancy, text);

        if ignore_ranges.is_empty() && include_ranges.is_empty() {
            return;
        }

        issues.retain(|issue| {
            if !include_ranges.is_empty() && !is_in_sorted_ranges(&include_ranges, issue.offset) {
                return false;
            }
            !is_in_sorted_ranges(&ignore_ranges, issue.offset)
        });
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

    fn dict_forbidden_contains(
        &self,
        entry: &DictionaryEntry,
        word: &str,
        ascii_lower: &str,
    ) -> bool {
        if self.config.case_sensitive || entry.case_sensitive || !word.is_ascii() {
            return entry.dict.is_forbidden(word);
        }
        entry.dict.is_forbidden_pre_normalized(word, ascii_lower)
    }

    fn is_word_forbidden_in_active_dicts(
        &self,
        word: &str,
        ascii_lower: &str,
        directive_dictionaries: Option<&HashSet<String>>,
    ) -> bool {
        if !self.any_dict_has_forbidden {
            return false;
        }

        if let Some(active) = directive_dictionaries {
            return self
                .dictionaries
                .iter()
                .filter(|d| self.is_dict_active(d, Some(active)) && d.has_forbidden)
                .any(|d| self.dict_forbidden_contains(d, word, ascii_lower));
        }

        self.default_active_forbidden_indices
            .iter()
            .any(|&i| self.dict_forbidden_contains(&self.dictionaries[i], word, ascii_lower))
    }

    fn is_word_ignored_in_active_dicts(
        &self,
        word: &str,
        directive_dictionaries: Option<&HashSet<String>>,
        case_sensitive: bool,
    ) -> bool {
        if !self.any_dict_has_no_suggest {
            return false;
        }

        let ascii_lower = word.is_ascii().then(|| ascii_lowercase(word));
        let mut lower = None;

        if let Some(active) = directive_dictionaries {
            return self
                .dictionaries
                .iter()
                .filter(|d| self.is_dict_active(d, Some(active)) && d.has_no_suggest)
                .any(|d| {
                    self.dict_contains_no_suggest(
                        d,
                        word,
                        ascii_lower.as_deref(),
                        &mut lower,
                        case_sensitive,
                    )
                });
        }

        self.default_active_no_suggest_indices.iter().any(|&i| {
            self.dict_contains_no_suggest(
                &self.dictionaries[i],
                word,
                ascii_lower.as_deref(),
                &mut lower,
                case_sensitive,
            )
        })
    }

    fn dict_contains_no_suggest(
        &self,
        entry: &DictionaryEntry,
        word: &str,
        ascii_lower: Option<&str>,
        lower: &mut Option<String>,
        case_sensitive: bool,
    ) -> bool {
        if let Some(ascii_lower) = ascii_lower {
            if self.config.case_sensitive || entry.case_sensitive {
                if entry.dict.is_no_suggest(word) {
                    return true;
                }
                if !case_sensitive {
                    let lower = lower.get_or_insert_with(|| word.to_lowercase());
                    return lower.as_str() != word && entry.dict.is_no_suggest(lower);
                }
                return false;
            }

            return entry.dict.is_no_suggest_pre_normalized(word, ascii_lower);
        }

        if entry.dict.is_no_suggest(word) {
            return true;
        }

        if !case_sensitive && entry.case_sensitive {
            let lower = lower.get_or_insert_with(|| word.to_lowercase());
            return lower.as_str() != word && entry.dict.is_no_suggest(lower);
        }

        false
    }

    fn dict_contains_direct(
        &self,
        entry: &DictionaryEntry,
        word: &str,
        ascii_lower: Option<&str>,
    ) -> bool {
        if let Some(ascii_lower) = ascii_lower {
            if self.config.case_sensitive {
                return entry.dict.has_direct_only(word);
            }
            return entry.dict.has_pre_normalized_direct_only(word, ascii_lower);
        }

        entry.dict.has_direct_only(word)
    }

    fn dict_contains_full(
        &self,
        entry: &DictionaryEntry,
        word: &str,
        ascii_lower: Option<&str>,
    ) -> bool {
        if let Some(ascii_lower) = ascii_lower {
            if self.config.case_sensitive {
                return entry.dict.has(word);
            }
            return entry.dict.has_pre_normalized(word, ascii_lower);
        }

        entry.dict.has(word)
    }

    fn is_word_valid(
        &self,
        word: &str,
        directive_dictionaries: Option<&HashSet<String>>,
        allow_compound_words: bool,
        case_sensitive: bool,
        cache_guard: Option<&papaya::LocalGuard<'_>>,
    ) -> bool {
        let can_cache = directive_dictionaries.is_none()
            && allow_compound_words == self.config.allow_compound_words
            && case_sensitive == self.config.case_sensitive;

        if can_cache && let Some(result) = self.local_word_cache_get(word) {
            return result;
        }

        if let (true, Some(cache), Some(guard)) = (
            can_cache && cache_guard.is_some(),
            self.word_cache.as_ref(),
            cache_guard,
        ) {
            if let Some(&result) = cache.get::<str>(word, guard) {
                self.local_word_cache_insert(word, result);
                return result;
            }
            let result = self.is_word_valid_inner(
                word,
                directive_dictionaries,
                allow_compound_words,
                case_sensitive,
            );
            cache.insert(CompactString::from(word), result, guard);
            self.local_word_cache_insert(word, result);
            return result;
        }

        let result = self.is_word_valid_inner(
            word,
            directive_dictionaries,
            allow_compound_words,
            case_sensitive,
        );
        if can_cache {
            self.local_word_cache_insert(word, result);
        }
        result
    }

    fn is_word_valid_inner(
        &self,
        word: &str,
        directive_dictionaries: Option<&HashSet<String>>,
        allow_compound_words: bool,
        case_sensitive: bool,
    ) -> bool {
        if self.has_in_active_dicts(word, directive_dictionaries) {
            return true;
        }

        // cspell searches a dictionary's no-case index when validation is
        // case-insensitive, even if the underlying dictionary preserves case.
        if !case_sensitive
            && !self.all_dicts_case_insensitive
            && self.has_in_active_dicts_ignore_case(word, directive_dictionaries)
        {
            return true;
        }

        // Curly apostrophe normalization: replace \u{2019} with ASCII apostrophe
        if word.contains('\u{2019}') {
            let normalized: String = word.replace('\u{2019}', "'");
            if self.has_in_active_dicts(&normalized, directive_dictionaries) {
                return true;
            }
            if !case_sensitive
                && !self.all_dicts_case_insensitive
                && self.has_in_active_dicts_ignore_case(&normalized, directive_dictionaries)
            {
                return true;
            }
        }

        // Escape retry: if word contains backslashes, strip them and retry
        if word.contains('\\') {
            let stripped: String = word.chars().filter(|c| *c != '\\').collect();
            if !stripped.is_empty() {
                if self.has_in_active_dicts(&stripped, directive_dictionaries) {
                    return true;
                }
                if !case_sensitive
                    && !self.all_dicts_case_insensitive
                    && self.has_in_active_dicts_ignore_case(&stripped, directive_dictionaries)
                {
                    return true;
                }
            }
        }

        let compound_mode = if allow_compound_words {
            // If compound words are enabled but mode is None, default to JoinWords
            // for backward compatibility
            if self.config.compound_words_mode == CompoundWordsMode::None {
                CompoundWordsMode::JoinWords
            } else {
                self.config.compound_words_mode
            }
        } else {
            CompoundWordsMode::None
        };

        if compound_mode == CompoundWordsMode::None {
            return false;
        }

        self.is_compound_valid(word, directive_dictionaries, case_sensitive, compound_mode)
    }

    fn has_in_active_dicts(
        &self,
        word: &str,
        directive_dictionaries: Option<&HashSet<String>>,
    ) -> bool {
        let ascii_lower = word.is_ascii().then(|| ascii_lowercase(word));

        if let Some(active) = directive_dictionaries {
            let mut saw_expensive = false;
            for entry in self
                .dictionaries
                .iter()
                .filter(|d| self.is_dict_active(d, Some(active)))
            {
                if self.dict_contains_direct(entry, word, ascii_lower.as_deref()) {
                    return true;
                }
                saw_expensive |= entry.has_expensive_forms;
            }

            if !saw_expensive {
                return false;
            }

            return self
                .dictionaries
                .iter()
                .filter(|d| self.is_dict_active(d, Some(active)) && d.has_expensive_forms)
                .any(|d| self.dict_contains_full(d, word, ascii_lower.as_deref()));
        }

        if self.default_active_indices.iter().any(|&i| {
            self.dict_contains_direct(&self.dictionaries[i], word, ascii_lower.as_deref())
        }) {
            return true;
        }

        if !self.any_dict_has_expensive_forms {
            return false;
        }

        self.default_active_expensive_indices
            .iter()
            .any(|&i| self.dict_contains_full(&self.dictionaries[i], word, ascii_lower.as_deref()))
    }

    fn has_in_active_dicts_ignore_case(
        &self,
        word: &str,
        directive_dictionaries: Option<&HashSet<String>>,
    ) -> bool {
        let normalized = if word.is_ascii() {
            ascii_lowercase(word)
        } else {
            std::borrow::Cow::Owned(word.to_lowercase())
        };

        if let Some(active) = directive_dictionaries {
            let mut saw_expensive = false;
            for entry in self
                .dictionaries
                .iter()
                .filter(|d| self.is_dict_active(d, Some(active)))
            {
                if entry
                    .dict
                    .has_pre_normalized_direct_only(word, normalized.as_ref())
                {
                    return true;
                }
                saw_expensive |= entry.has_expensive_forms;
            }

            if !saw_expensive {
                return false;
            }

            return self
                .dictionaries
                .iter()
                .filter(|d| self.is_dict_active(d, Some(active)) && d.has_expensive_forms)
                .any(|d| d.dict.has_pre_normalized(word, normalized.as_ref()));
        }

        if self.default_active_indices.iter().any(|&i| {
            self.dictionaries[i]
                .dict
                .has_pre_normalized_direct_only(word, normalized.as_ref())
        }) {
            return true;
        }

        if !self.any_dict_has_expensive_forms {
            return false;
        }

        self.default_active_expensive_indices.iter().any(|&i| {
            self.dictionaries[i]
                .dict
                .has_pre_normalized(word, normalized.as_ref())
        })
    }

    fn has_in_single_dict(&self, word: &str, dict: &DictionaryEntry) -> bool {
        let ascii_lower = word.is_ascii().then(|| ascii_lowercase(word));
        self.dict_contains_direct(dict, word, ascii_lower.as_deref())
    }

    fn is_compound_valid(
        &self,
        word: &str,
        directive_dictionaries: Option<&HashSet<String>>,
        case_sensitive: bool,
        mode: CompoundWordsMode,
    ) -> bool {
        match mode {
            CompoundWordsMode::JoinWords => self.is_compound_valid_join_recursive(
                word,
                directive_dictionaries,
                case_sensitive,
                0,
            ),
            CompoundWordsMode::SeparateWords => self
                .dictionaries
                .iter()
                .filter(|d| self.is_dict_active(d, directive_dictionaries))
                .any(|d| {
                    self.is_compound_valid_in_single_dict_recursive(word, d, case_sensitive, 0)
                }),
            CompoundWordsMode::None => false,
        }
    }

    /// Iterative compound decomposition for legacy join-words behavior.
    /// Parts may come from different active dictionaries.
    #[allow(clippy::needless_range_loop)]
    fn is_compound_valid_join_recursive(
        &self,
        word: &str,
        directive_dictionaries: Option<&HashSet<String>>,
        case_sensitive: bool,
        depth: usize,
    ) -> bool {
        const MAX_DEPTH: usize = 6;
        const MIN_PART_LEN: usize = 3;

        if word.len() < MIN_PART_LEN || depth > MAX_DEPTH {
            return false;
        }

        let mut boundaries: Vec<usize> = word.char_indices().map(|(i, _)| i).collect();
        boundaries.push(word.len());

        let is_valid_part = |part: &str| {
            self.has_in_active_dicts(part, directive_dictionaries)
                || (!case_sensitive && !self.all_dicts_case_insensitive && {
                    let lower = part.to_lowercase();
                    lower != part && self.has_in_active_dicts(&lower, directive_dictionaries)
                })
        };

        let mut stack = vec![(0usize, depth)];
        let mut seen = std::collections::HashSet::new();

        while let Some((start_idx, depth)) = stack.pop() {
            if !seen.insert((start_idx, depth)) {
                continue;
            }
            let start = boundaries[start_idx];

            for next_idx in start_idx + 1..boundaries.len().saturating_sub(1) {
                let split = boundaries[next_idx];
                let left = &word[start..split];
                let right = &word[split..];
                if left.len() < MIN_PART_LEN || right.len() < MIN_PART_LEN {
                    continue;
                }
                if !is_valid_part(left) {
                    continue;
                }
                if is_valid_part(right) {
                    return true;
                }
                if depth < MAX_DEPTH {
                    stack.push((next_idx, depth + 1));
                }
            }
        }

        false
    }

    /// Iterative compound decomposition constrained to a single dictionary.
    /// This matches cspell's legacy `allowCompoundWords` behavior.
    #[allow(clippy::needless_range_loop)]
    fn is_compound_valid_in_single_dict_recursive(
        &self,
        word: &str,
        dict: &DictionaryEntry,
        case_sensitive: bool,
        depth: usize,
    ) -> bool {
        const MAX_DEPTH: usize = 6;
        const MIN_PART_LEN: usize = 3;

        if word.len() < MIN_PART_LEN || depth > MAX_DEPTH {
            return false;
        }

        let mut boundaries: Vec<usize> = word.char_indices().map(|(i, _)| i).collect();
        boundaries.push(word.len());

        let is_valid_part = |part: &str| {
            self.has_in_single_dict(part, dict)
                || (!case_sensitive && !self.all_dicts_case_insensitive && {
                    let lower = part.to_lowercase();
                    lower != part && self.has_in_single_dict(&lower, dict)
                })
        };

        let mut stack = vec![(0usize, depth)];
        let mut seen = std::collections::HashSet::new();

        while let Some((start_idx, depth)) = stack.pop() {
            if !seen.insert((start_idx, depth)) {
                continue;
            }
            let start = boundaries[start_idx];

            for next_idx in start_idx + 1..boundaries.len().saturating_sub(1) {
                let split = boundaries[next_idx];
                let left = &word[start..split];
                let right = &word[split..];
                if left.len() < MIN_PART_LEN || right.len() < MIN_PART_LEN {
                    continue;
                }
                if !is_valid_part(left) {
                    continue;
                }
                if is_valid_part(right) {
                    return true;
                }
                if depth < MAX_DEPTH {
                    stack.push((next_idx, depth + 1));
                }
            }
        }

        false
    }

    /// Check if a code token with underscores is valid by trying all possible
    /// splits at underscore boundaries. If any split produces parts that are
    /// all individually valid, the token is considered valid.
    ///
    /// For example, `S_IMODE_method` can be split as `S_IMODE` + `method`,
    /// where `S_IMODE` is in the python dict and `method` is in en_us.
    #[allow(clippy::needless_range_loop)]
    fn is_underscore_compound_valid(
        &self,
        token: &str,
        directive_dictionaries: Option<&HashSet<String>>,
        allow_compound_words: bool,
        case_sensitive: bool,
        cache_guard: Option<&papaya::LocalGuard<'_>>,
    ) -> bool {
        if !token.contains('_') {
            return false;
        }

        // Split at underscores and find indices of each underscore
        let segments: Vec<&str> = token.split('_').collect();
        if segments.len() < 2 {
            return false;
        }

        // Try all ways to group consecutive segments with underscores between them.
        // For N segments, try all 2^(N-1) combinations of joining/splitting.
        // Limit to reasonable size to avoid exponential blowup.
        if segments.len() > 8 {
            return false;
        }

        let n = segments.len();
        let combos = 1u32 << (n - 1);
        for mask in 0..combos {
            let mut parts: Vec<String> = Vec::new();
            let mut current = segments[0].to_string();
            for i in 1..n {
                if mask & (1 << (i - 1)) != 0 {
                    // Join with underscore
                    current.push('_');
                    current.push_str(segments[i]);
                } else {
                    // Split here
                    parts.push(std::mem::take(&mut current));
                    current = segments[i].to_string();
                }
            }
            parts.push(current);

            // Check if all parts are valid
            let all_valid = parts.iter().all(|part| {
                part.is_empty()
                    || self.is_word_valid(
                        part,
                        directive_dictionaries,
                        allow_compound_words,
                        case_sensitive,
                        cache_guard,
                    )
            });
            if all_valid {
                // Ensure at least one non-empty part
                if parts.iter().any(|p| !p.is_empty()) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a word with an apostrophe is valid by splitting at the
    /// apostrophe and verifying all parts exist in active dictionaries.
    fn all_apostrophe_parts_valid(
        &self,
        word: &str,
        directive_dictionaries: Option<&HashSet<String>>,
        allow_compound_words: bool,
        case_sensitive: bool,
        cache_guard: Option<&papaya::LocalGuard<'_>>,
    ) -> bool {
        let parts: Vec<&str> = word.split(['\'', '\u{2019}']).collect();
        if parts.len() < 2 {
            return false;
        }
        parts.iter().all(|part| {
            if part.is_empty() {
                return true; // empty parts between consecutive apostrophes
            }
            // Parts shorter than minWordLength are considered valid
            if part.chars().count() < self.config.min_word_length {
                return true;
            }
            self.is_word_valid(
                part,
                directive_dictionaries,
                allow_compound_words,
                case_sensitive,
                cache_guard,
            )
        })
    }

    /// Dictionary-guided camelCase boundary search.
    ///
    /// cspell's expensive word splitter explores alternate breakpoints around
    /// camel/acronym transitions instead of trusting a single regex split.
    /// We don't port the full general splitter here, but we do mirror the
    /// important break candidates for camelCase tokens:
    /// - `lower,Upper` => one break candidate
    /// - `Upper,Upper,lower` => two break candidates
    ///
    /// This is enough to recover cspell-compatible splits for tokens like:
    /// - `ASTnode` => `AST` + `node`
    /// - `DBforPostgreSQL` => `DB` + `for` + `Postgre` + `SQL` // cspell:ignore Postgre
    /// - `markUIAsReady` => `mark` + `UI` + `As` + `Ready`
    fn all_camel_parts_valid_with_boundary_shift(
        &self,
        token: &str,
        parts: &[&str],
        directive_dictionaries: Option<&HashSet<String>>,
        allow_compound_words: bool,
        case_sensitive: bool,
        cache_guard: Option<&papaya::LocalGuard<'_>>,
    ) -> bool {
        if parts.len() < 2 {
            return false;
        }

        // If any original part is a flag word, don't bypass the per-word
        // validation loop, which is responsible for reporting it.
        for part in parts {
            let part_lower = part.to_lowercase();
            if self.config.flag_words.contains(part_lower.as_str()) {
                return false;
            }
        }

        let is_part_valid = |part: &str| -> bool {
            self.is_word_valid(
                part,
                directive_dictionaries,
                allow_compound_words,
                case_sensitive,
                cache_guard,
            )
        };
        let min_wl = self.config.min_word_length;
        let is_part_valid_or_short =
            |part: &str| -> bool { part.chars().count() < min_wl || is_part_valid(part) };

        let char_offsets: Vec<(usize, char)> = token.char_indices().collect();
        if char_offsets.len() < 2 {
            return false;
        }

        let mut break_positions = vec![0usize, token.len()];
        for i in 1..char_offsets.len() {
            let prev = char_offsets[i - 1].1;
            let curr = char_offsets[i].1;

            if splitter::is_cspell_lowercase_letter(prev)
                && splitter::is_cspell_uppercase_letter(curr)
            {
                break_positions.push(char_offsets[i].0);
            }

            if i >= 2 {
                let prev_prev = char_offsets[i - 2].1;
                if splitter::is_cspell_uppercase_letter(prev_prev)
                    && splitter::is_cspell_uppercase_letter(prev)
                    && splitter::is_cspell_lowercase_letter(curr)
                {
                    break_positions.push(char_offsets[i - 1].0);
                    break_positions.push(char_offsets[i].0);
                }
            }
        }
        break_positions.sort_unstable();
        break_positions.dedup();

        let mut memo: std::collections::HashMap<(usize, bool), bool> =
            std::collections::HashMap::new();
        let mut valid_cache: std::collections::HashMap<(usize, usize), (bool, bool)> =
            std::collections::HashMap::new();

        fn part_status(
            token: &str,
            start: usize,
            end: usize,
            min_wl: usize,
            valid_cache: &mut std::collections::HashMap<(usize, usize), (bool, bool)>,
            is_part_valid_or_short: &impl Fn(&str) -> bool,
            is_part_valid: &impl Fn(&str) -> bool,
        ) -> (bool, bool) {
            if let Some(status) = valid_cache.get(&(start, end)) {
                return *status;
            }

            let part = &token[start..end];
            let chars = part.chars().count();
            let ok = is_part_valid_or_short(part);
            let has_valid = chars >= min_wl && is_part_valid(part);
            let status = (ok, has_valid);
            valid_cache.insert((start, end), status);
            status
        }

        #[allow(clippy::too_many_arguments)]
        fn search(
            token: &str,
            positions: &[usize],
            idx: usize,
            has_valid_part: bool,
            min_wl: usize,
            memo: &mut std::collections::HashMap<(usize, bool), bool>,
            valid_cache: &mut std::collections::HashMap<(usize, usize), (bool, bool)>,
            is_part_valid_or_short: &impl Fn(&str) -> bool,
            is_part_valid: &impl Fn(&str) -> bool,
        ) -> bool {
            if let Some(cached) = memo.get(&(idx, has_valid_part)) {
                return *cached;
            }

            if idx + 1 >= positions.len() {
                memo.insert((idx, has_valid_part), has_valid_part);
                return has_valid_part;
            }

            let start = positions[idx];
            for next_idx in idx + 1..positions.len() {
                let end = positions[next_idx];
                if end <= start {
                    continue;
                }

                let (ok, segment_is_valid) = part_status(
                    token,
                    start,
                    end,
                    min_wl,
                    valid_cache,
                    is_part_valid_or_short,
                    is_part_valid,
                );
                if !ok {
                    continue;
                }

                if search(
                    token,
                    positions,
                    next_idx,
                    has_valid_part || segment_is_valid,
                    min_wl,
                    memo,
                    valid_cache,
                    is_part_valid_or_short,
                    is_part_valid,
                ) {
                    memo.insert((idx, has_valid_part), true);
                    return true;
                }
            }

            memo.insert((idx, has_valid_part), false);
            false
        }

        search(
            token,
            &break_positions,
            0,
            false,
            min_wl,
            &mut memo,
            &mut valid_cache,
            &is_part_valid_or_short,
            &is_part_valid,
        )
    }

    fn get_suggestions(
        &self,
        word: &str,
        directive_dictionaries: Option<&HashSet<String>>,
    ) -> Vec<String> {
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

/// Convert to ASCII lowercase, avoiding allocation if already lowercase.
#[inline]
fn ascii_lowercase(s: &str) -> std::borrow::Cow<'_, str> {
    if s.bytes().all(|b| !b.is_ascii_uppercase()) {
        std::borrow::Cow::Borrowed(s)
    } else {
        std::borrow::Cow::Owned(s.to_ascii_lowercase())
    }
}

/// Sort ranges by start offset, then merge overlapping/adjacent ranges.
/// This ensures `is_in_sorted_ranges` works correctly with binary search,
/// since overlapping ranges (e.g. URL containing a commit hash) would
/// otherwise cause the binary search to miss the outer range.
fn merge_sorted_ranges(ranges: &mut Vec<(usize, usize)>) {
    ranges.sort_unstable_by_key(|&(s, _)| s);
    let mut write = 0;
    for read in 1..ranges.len() {
        if ranges[read].0 <= ranges[write].1 {
            // Overlapping or adjacent: extend current range
            ranges[write].1 = ranges[write].1.max(ranges[read].1);
        } else {
            write += 1;
            ranges[write] = ranges[read];
        }
    }
    if !ranges.is_empty() {
        ranges.truncate(write + 1);
    }
}

/// Compute the final inclusion ranges for cspell-compatible validation.
/// This mirrors cspell's `calcTextInclusionRanges`:
/// - if `includeRegExpList` is empty, the entire document is included
/// - `ignoreRegExpList` subtracts only the matched spans
fn compute_doc_validation_ranges(
    text_len: usize,
    include_ranges: &[(usize, usize)],
    skip_ranges: &[(usize, usize)],
    has_include_config: bool,
) -> Vec<(usize, usize)> {
    let base_ranges: Vec<(usize, usize)> = if has_include_config {
        include_ranges.to_vec()
    } else if text_len == 0 {
        Vec::new()
    } else {
        vec![(0, text_len)]
    };

    exclude_sorted_ranges(&base_ranges, skip_ranges)
}

fn exclude_sorted_ranges(
    include_ranges: &[(usize, usize)],
    exclude_ranges: &[(usize, usize)],
) -> Vec<(usize, usize)> {
    if include_ranges.is_empty() {
        return Vec::new();
    }
    if exclude_ranges.is_empty() {
        return include_ranges.to_vec();
    }

    let mut result = Vec::new();
    let mut exclude_idx = 0usize;

    for &(include_start, include_end) in include_ranges {
        if include_start >= include_end {
            continue;
        }

        while exclude_idx < exclude_ranges.len() && exclude_ranges[exclude_idx].1 <= include_start {
            exclude_idx += 1;
        }

        let mut cursor = include_start;
        let mut idx = exclude_idx;
        while idx < exclude_ranges.len() {
            let (exclude_start, exclude_end) = exclude_ranges[idx];
            if exclude_start >= include_end {
                break;
            }

            if cursor < exclude_start {
                result.push((cursor, exclude_start.min(include_end)));
            }

            cursor = cursor.max(exclude_end);
            if cursor >= include_end {
                break;
            }

            idx += 1;
        }

        if cursor < include_end {
            result.push((cursor, include_end));
        }
    }

    result
}

/// Binary search in sorted, non-overlapping (start, end) ranges to check
/// if `offset` falls within any range. O(log n).
#[inline]
/// Convert a byte offset within a line to a 1-based character column.
/// cspell reports columns as character (code point) offsets, not byte offsets.
fn byte_offset_to_char_col(line: &str, byte_offset: usize) -> usize {
    if line.is_ascii() {
        byte_offset + 1
    } else {
        line[..byte_offset].chars().count() + 1
    }
}

fn is_in_sorted_ranges(ranges: &[(usize, usize)], offset: usize) -> bool {
    // Find the rightmost range whose start <= offset
    let idx = ranges.partition_point(|&(s, _)| s <= offset);
    if idx > 0 {
        let (_, end) = ranges[idx - 1];
        if offset < end {
            return true;
        }
    }
    false
}

#[allow(dead_code)]
fn overlaps_sorted_ranges(ranges: &[(usize, usize)], start: usize, end: usize) -> bool {
    if start >= end {
        return false;
    }

    // Find the last range whose start is before the end of the token.
    let idx = ranges.partition_point(|&(range_start, _)| range_start < end);
    if idx > 0 {
        let (range_start, range_end) = ranges[idx - 1];
        return range_start < end && range_end > start;
    }
    false
}

fn parse_regex_pattern(value: &str) -> Option<Regex> {
    crate::regex_pattern::parse_regex_pattern(value)
}

/// Check if a word is made of a single repeating character (4+ times).
///
/// Matches cspell's `regExRepeatedChar`: `/^(\w)\1{3,}$/i`
fn is_repeating_char(word: &str) -> bool {
    let mut chars = word.chars();
    let first = match chars.next() {
        Some(c) => c.to_lowercase().next().unwrap_or(c),
        None => return false,
    };
    let mut count = 1usize;
    for ch in chars {
        if ch.to_lowercase().next().unwrap_or(ch) != first {
            return false;
        }
        count += 1;
    }
    count >= 4
}

/// Strip trailing possessive/contraction endings from a word.
///
/// Handles patterns like `word's`, `word'd`, `word't` (possessives and
/// contractions). Returns the base word if a suffix was stripped.
///
/// Matches cspell's word splitter behavior where `accessor's` is split
/// into `accessor` + `'s`, and `accessor` is then checked against
/// the dictionary.
fn strip_trailing_suffix(word: &str) -> Option<&str> {
    // Look for trailing 's, 'd, 't, 'll, 've, 're (contractions)
    for apos in ['\'', '\u{2019}'] {
        if let Some(pos) = word.rfind(apos) {
            let suffix = &word[pos + apos.len_utf8()..];
            let suffix_lower = suffix.to_lowercase();
            if matches!(
                suffix_lower.as_str(),
                "s" | "d" | "t" | "ll" | "ve" | "re" | "m"
            ) {
                let base = &word[..pos];
                if !base.is_empty() {
                    return Some(base);
                }
            }
        }
    }
    None
}

/// Strip trailing English suffixes from ALL-CAPS words.
///
// cspell:ignore nings ings ning
/// Matches cspell's `regExUpperCaseWithTrailingCommonEnglishSuffix`:
///   `/^([\p{Lu}\p{M}]{2,})['']?(?:s|ing|ies|es|ings|ize|ed|ning)$/u`
/// From lineValidatorFactory.ts line 201-202.
///
/// Examples: `REPLs` → `REPL`, `ERRORS` → `ERROR`, `ERROR'S` → `ERROR`
fn strip_all_caps_suffix(word: &str) -> Option<&str> {
    let chars: Vec<char> = word.chars().collect();
    if chars.len() < 3 {
        return None;
    }

    // Find where the uppercase prefix ends
    let mut upper_end = 0;
    for (i, &ch) in chars.iter().enumerate() {
        if splitter::is_cspell_uppercase_letter(ch) {
            upper_end = i + 1;
        } else {
            break;
        }
    }

    // Need at least 2 uppercase chars
    if upper_end < 2 {
        return None;
    }

    // Collect the rest after uppercase prefix
    let rest: String = chars[upper_end..].iter().collect();
    let rest_lower = rest.to_lowercase();

    // Check for optional apostrophe + suffix
    let suffix_part = if rest_lower.starts_with('\'') || rest_lower.starts_with('\u{2019}') {
        &rest_lower[rest_lower.chars().next().unwrap().len_utf8()..]
    } else {
        &rest_lower
    };

    // cspell's regExUpperCaseWithTrailingCommonEnglishSuffix (lineValidatorFactory.ts:202)
    // cspell:ignore nings ings ning
    if matches!(
        suffix_part,
        "s" | "ing" | "ies" | "es" | "ings" | "ize" | "ed" | "ning"
    ) {
        // Return the uppercase base - compute byte offset
        let byte_end: usize = chars[..upper_end].iter().map(|c| c.len_utf8()).sum();
        Some(&word[..byte_end])
    } else {
        None
    }
}

fn filter_excluded_compat_issues(
    issues: Vec<CompatIssue>,
    excluded: &[splitter::Word<'_>],
) -> Vec<CompatIssue> {
    if excluded.is_empty() {
        return issues;
    }

    issues
        .into_iter()
        .filter(|issue| {
            issue.is_forbidden
                || !excluded.iter().any(|excluded| {
                    issue.offset >= excluded.offset
                        && issue.offset < excluded.offset + excluded.text.len()
                })
        })
        .collect()
}

fn rebase_known_compat_issues(
    possible_word_abs_offset: usize,
    known: &KnownCompatIssues,
) -> Vec<CompatIssue> {
    let delta = possible_word_abs_offset as isize - known.possible_word_abs_offset as isize;
    known
        .issues
        .iter()
        .cloned()
        .map(|mut issue| {
            issue.offset = issue.offset.saturating_add_signed(delta);
            issue
        })
        .collect()
}

#[inline]
fn compat_word_rel_offset(word: splitter::Word<'_>, line: splitter::Word<'_>) -> usize {
    if word.offset >= line.offset {
        word.offset - line.offset
    } else {
        word.offset
    }
}

// cspell:disable
#[cfg(test)]
mod tests {
    use super::*;
    use matchum_dict::hashdict::HashDictionary;
    use matchum_dict::loader::trie_v3::load_trie_v3;
    use matchum_dict::repmap::RepMap;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingDict {
        inner: HashDictionary,
        has_calls: AtomicUsize,
    }

    impl CountingDict {
        fn new(words: &[&str]) -> Self {
            let mut inner = HashDictionary::new(false);
            for word in words {
                inner.add_word(word);
            }
            Self {
                inner,
                has_calls: AtomicUsize::new(0),
            }
        }

        fn has_calls(&self) -> usize {
            self.has_calls.load(Ordering::Relaxed)
        }

        fn count_lookup(&self) {
            self.has_calls.fetch_add(1, Ordering::Relaxed);
        }
    }

    impl Dictionary for CountingDict {
        fn has(&self, word: &str) -> bool {
            self.count_lookup();
            self.inner.has(word)
        }

        fn has_direct_only(&self, word: &str) -> bool {
            self.count_lookup();
            <HashDictionary as Dictionary>::has_direct_only(&self.inner, word)
        }

        fn has_with_compounds(&self, word: &str) -> bool {
            self.count_lookup();
            self.inner.has_with_compounds(word)
        }

        fn is_forbidden(&self, word: &str) -> bool {
            self.inner.is_forbidden(word)
        }

        fn suggest(&self, word: &str, limit: usize) -> Vec<String> {
            self.inner.suggest(word, limit)
        }

        fn find(&self, word: &str) -> matchum_dict::dictionary::FindResult {
            self.inner.find(word)
        }

        fn find_with_compounds(&self, word: &str) -> matchum_dict::dictionary::FindResult {
            self.inner.find_with_compounds(word)
        }

        fn len(&self) -> usize {
            self.inner.len()
        }

        fn has_forbidden_words(&self) -> bool {
            self.inner.has_forbidden_words()
        }

        fn has_no_suggest_words(&self) -> bool {
            self.inner.has_no_suggest_words()
        }

        fn is_case_sensitive(&self) -> bool {
            self.inner.is_case_sensitive()
        }

        fn is_no_suggest(&self, word: &str) -> bool {
            self.inner.is_no_suggest(word)
        }

        fn is_no_suggest_pre_normalized(&self, word: &str, normalized: &str) -> bool {
            self.inner.is_no_suggest_pre_normalized(word, normalized)
        }

        fn has_pre_normalized(&self, word: &str, normalized: &str) -> bool {
            self.count_lookup();
            self.inner.has_pre_normalized(word, normalized)
        }

        fn has_pre_normalized_direct_only(&self, word: &str, normalized: &str) -> bool {
            self.count_lookup();
            <HashDictionary as Dictionary>::has_pre_normalized_direct_only(
                &self.inner,
                word,
                normalized,
            )
        }

        fn has_pre_normalized_with_compounds(&self, word: &str, normalized: &str) -> bool {
            self.count_lookup();
            self.inner
                .has_pre_normalized_with_compounds(word, normalized)
        }

        fn has_expensive_forms(&self) -> bool {
            self.inner.has_expensive_forms()
        }

        fn is_forbidden_pre_normalized(&self, word: &str, normalized: &str) -> bool {
            self.inner.is_forbidden_pre_normalized(word, normalized)
        }
    }

    fn make_dict(words: &[&str]) -> Box<dyn Dictionary> {
        let mut dict = HashDictionary::new(false);
        for w in words {
            dict.add_word(w);
        }
        Box::new(dict)
    }

    fn en_us_dict_path() -> Option<PathBuf> {
        let candidates = [
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("dictionaries/node_modules/@cspell/dict-en_us/en_US.trie.gz"),
            {
                let home = std::env::var("HOME").unwrap_or_default();
                PathBuf::from(home)
                    .join(".matchum_cache/packages/node_modules/@cspell/dict-en_us/en_US.trie.gz")
            },
        ];
        candidates.iter().find(|p| p.exists()).cloned()
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

    #[test]
    fn test_local_word_cache_reuses_results_without_shared_cache() {
        let dict = Arc::new(CountingDict::new(&["hello", "world"]));
        let validator = Validator::new_named(
            vec![(
                "count".to_string(),
                dict.clone() as Arc<dyn Dictionary>,
                true,
            )],
            ValidatorConfig::default(),
        );

        let first = validator.validate_text("hello world xyzzy");
        assert_eq!(first.len(), 1);
        let first_calls = dict.has_calls();
        assert!(
            first_calls > 0,
            "expected dictionary lookups on first validation"
        );

        let second = validator.validate_text("hello world xyzzy");
        assert_eq!(second.len(), 1);
        assert_eq!(
            dict.has_calls(),
            first_calls,
            "local validator cache should satisfy repeated validations without new dictionary lookups"
        );
    }

    #[test]
    fn test_validator_still_accepts_dictionary_native_compounds_after_direct_miss() {
        let mut dict = HashDictionary::new(false);
        dict.add_compound_part_explicit("iscsi", true, true, true);
        dict.add_compound_part_explicit("servers", true, true, true);
        let validator = Validator::new(vec![Box::new(dict)], ValidatorConfig::default());

        let issues = validator.validate_text("iscsiservers");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_validator_still_accepts_repmap_forms_after_direct_miss() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("column");
        dict.set_repmap(RepMap::new(vec![("colum".into(), "column".into())]));
        let validator = Validator::new(vec![Box::new(dict)], ValidatorConfig::default());

        let issues = validator.validate_text("colum");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_crlf_offsets_preserve_second_line_start() {
        let dict = make_dict(&["hello"]);
        let validator = Validator::new(vec![dict], ValidatorConfig::default());

        let issues = validator.validate_text("hello\r\nxyzzy\r\n");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].word, "xyzzy");
        assert_eq!(issues[0].line, 2);
        assert_eq!(issues[0].column, 1);
        assert_eq!(issues[0].offset, 7);
    }

    #[test]
    fn test_cspell_compat_skips_urls_in_crlf_files() {
        let dict = make_dict(&[]);
        let validator = Validator::new(
            vec![dict],
            ValidatorConfig {
                cspell_compat_mode: true,
                ..ValidatorConfig::default()
            },
        );

        let issues = validator.validate_text(
            "-- https://mf4.xiph.org/jenkins/view/opus/job/opus/ws/doc/html/group__opus__multistream.html#gaec819b8d4b38350aba6959cee7d33f94\r\n",
        );

        assert!(
            issues.is_empty(),
            "expected URL contents to be skipped in CRLF text, got: {:?}",
            issues
                .iter()
                .map(|i| (&i.word, i.offset))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_builtin_patterns_all_compile() {
        // 17 patterns: 14 from cspell's definedDefaultRegExpExcludeList
        // + 3 numeric literals
        // (SpellCheckerDisable and SpellCheckerIgnoreInDocSetting handled by directive system)
        // Note: HexValues (\\u and \\x{}) are NOT in cspell's default ignoreRegExpList
        assert_eq!(
            BUILTIN_SKIP_PATTERNS.len(),
            17,
            "Expected 17 builtin skip patterns"
        );
        // AC prefilter should be initialized
        assert!(!BUILTIN_SKIP_PREFILTER.anchor_to_regex.is_empty());
    }

    #[test]
    fn test_email_pattern_matches() {
        let email_text = "send email to user@example.com";
        let matched = BUILTIN_SKIP_PATTERNS.iter().any(|p| {
            p.find(email_text)
                .map_or(false, |m| m.as_str().contains("@"))
        });
        assert!(matched, "email should be matched by a builtin pattern");
    }

    #[test]
    fn test_email_pattern_keeps_non_ascii_domains_visible() {
        let dict = make_dict(&["contact", "example", "org"]);
        let validator = Validator::new(
            vec![dict],
            ValidatorConfig {
                cspell_compat_mode: true,
                ..ValidatorConfig::default()
            },
        );

        let issues = validator.validate_text("contact mike@ıxample.org");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(
            words.contains(&"ıxample"),
            "ASCII-only email regex should not hide non-ASCII domains: {words:?}"
        );
    }

    #[test]
    fn test_cspell_compat_reports_idn_labels_after_ascii_email_prefix() {
        let validator = Validator::new(
            vec![make_dict(&["test", "domain", "with", "idn", "tld"])],
            ValidatorConfig {
                cspell_compat_mode: true,
                ..ValidatorConfig::default()
            },
        );

        let issues = validator.validate_text("test@domain.with.idn.tld.उदाहरण.परीक्षा");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert_eq!(words, vec!["उदाहरण", "परीक्षा"]);
    }

    #[test]
    fn test_cspell_compat_reports_idn_domain_labels() {
        let validator = Validator::new(
            vec![make_dict(&["domain", "with", "idn", "tld"])],
            ValidatorConfig {
                cspell_compat_mode: true,
                ..ValidatorConfig::default()
            },
        );

        let issues = validator.validate_text("domain.with.idn.tld.उदाहरण.परीक्ष");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert_eq!(words, vec!["उदाहरण", "परीक्ष"]);
    }

    #[test]
    fn test_splitter_extracts_idn_domain_labels() {
        let words: Vec<&str> = splitter::extract_words(".उदाहरण.परीक्षा")
            .into_iter()
            .map(|word| word.text)
            .collect();

        assert_eq!(words, vec!["उदाहरण", "परीक्षा"]);
    }

    #[test]
    fn test_splitter_extracts_idn_domain_labels_after_ascii_labels() {
        let words: Vec<&str> = splitter::extract_words("domain.with.idn.tld.उदाहरण.परीक्षा")
            .into_iter()
            .map(|word| word.text)
            .collect();

        assert_eq!(
            words,
            vec!["domain", "with", "idn", "tld", "उदाहरण", "परीक्षा"]
        );
    }

    #[test]
    fn test_email_skip_range_stops_before_idn_suffix_labels() {
        let validator = Validator::new(
            vec![make_dict(&[])],
            ValidatorConfig {
                cspell_compat_mode: true,
                ..ValidatorConfig::default()
            },
        );
        let text = "test@domain.with.idn.tld.उदाहरण.परीक्षा";
        let ranges = validator.compute_doc_skip_ranges(text, &[]);

        assert_eq!(ranges.len(), 1);
        assert_eq!(&text[ranges[0].0..ranges[0].1], "test@domain.with.idn.tld");
    }

    #[test]
    fn test_expensive_splitter_keeps_idn_labels_intact() {
        let validator = Validator::new(
            vec![make_dict(&["domain", "with", "idn", "tld"])],
            ValidatorConfig {
                cspell_compat_mode: true,
                ..ValidatorConfig::default()
            },
        );
        let line = splitter::Word {
            text: "domain.with.idn.tld.उदाहरण.परीक्षा",
            offset: 0,
        };
        let inline_words = HashSet::new();
        let directive_flag_words = HashSet::new();
        let known_successful_words = HashSet::from_iter(
            ["domain", "with", "idn", "tld"]
                .into_iter()
                .map(str::to_string),
        );
        let mut known_cspell_words = std::collections::HashMap::new();

        let result = splitter::split(
            line,
            0,
            |split_word| {
                validator.splitter_is_valid_cspell(
                    line,
                    split_word,
                    &inline_words,
                    &directive_flag_words,
                    None,
                    false,
                    false,
                    None,
                    &known_successful_words,
                    &mut known_cspell_words,
                )
            },
            None,
        );
        let words: Vec<&str> = result
            .words
            .into_iter()
            .filter(|word| !word.is_found)
            .map(|word| word.text)
            .collect();

        assert_eq!(words, vec!["उदाहरण", "परीक्षा"]);
    }

    #[test]
    fn test_cspell_compat_keeps_lua_metamethod_after_dot() {
        let validator = Validator::new(
            vec![make_dict(&[
                "function",
                "configs",
                "__newindex",
                "config",
                "name",
                "def",
            ])],
            ValidatorConfig {
                cspell_compat_mode: true,
                ..ValidatorConfig::default()
            },
        );

        let issues =
            validator.validate_text("function configs.__newindex(t, config_name, config_def)\n");

        assert!(
            issues.is_empty(),
            "Lua metamethods after dots should remain valid: {:?}",
            issues
                .iter()
                .map(|issue| issue.word.as_str())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_cspell_compat_reports_base64_camel_suffix_like_cspell() {
        let validator = Validator::new(
            vec![make_dict(&["data", "text", "plain", "base", "true"])],
            ValidatorConfig {
                cspell_compat_mode: true,
                ..ValidatorConfig::default()
            },
        );

        let issues =
            validator.validate_text("data:text/plain;base64,SGVsbG8sIFdvcmxkIQ== // -> true\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert_eq!(words, vec!["Fdvcmxk"]);
    }

    #[test]
    fn test_cspell_compat_reports_base64_caps_suffix_like_cspell() {
        let validator = Validator::new(
            vec![make_dict(&["gif"])],
            ValidatorConfig {
                cspell_compat_mode: true,
                ..ValidatorConfig::default()
            },
        );

        let issues = validator.validate_text("['R0lGODlhAQABAAAAADs=', 'gif']");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert_eq!(words, vec!["AQABAAAAADs"]);
    }

    #[test]
    fn test_strip_all_caps_suffix() {
        assert_eq!(strip_all_caps_suffix("REPLs"), Some("REPL"));
        assert_eq!(strip_all_caps_suffix("APIs"), Some("API"));
        assert_eq!(strip_all_caps_suffix("HTMLs"), Some("HTML"));
        assert_eq!(strip_all_caps_suffix("URLed"), Some("URL"));
        assert_eq!(strip_all_caps_suffix("APQs"), Some("APQ"));
        assert_eq!(strip_all_caps_suffix("ERROR'S"), Some("ERROR"));
        assert_eq!(strip_all_caps_suffix("URLs"), Some("URL"));
        // Not ALL-CAPS - should return None
        assert_eq!(strip_all_caps_suffix("hello"), None);
        assert_eq!(strip_all_caps_suffix("Hello"), None);
        // Too short prefix
        assert_eq!(strip_all_caps_suffix("As"), None);
    }

    #[test]
    fn test_all_caps_suffix_stem_too_short_skipped() {
        // APQs: stem is "APQ" (3 chars), minWordLength=4 → skip (valid)
        let dict = make_dict(&["hello"]);
        let config = ValidatorConfig {
            min_word_length: 4,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![dict], config);
        let issues = validator.validate_text("hello APQs");
        assert!(
            issues.is_empty(),
            "APQs should be skipped: stem APQ is shorter than minWordLength"
        );
    }

    #[test]
    fn test_all_caps_suffix_stem_found_in_dict() {
        // REPLs: stem is "REPL" (4 chars >= minWordLength) and REPL is in dict → skip
        let dict = make_dict(&["hello", "REPL"]);
        let config = ValidatorConfig {
            min_word_length: 4,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![dict], config);
        let issues = validator.validate_text("hello REPLs");
        assert!(
            issues.is_empty(),
            "REPLs should be skipped: stem REPL is in dictionary"
        );
    }

    #[test]
    fn test_unicode_escape_not_skipped() {
        let dict = make_dict(&["hello", "const", "bom"]);
        let validator = Validator::new(vec![dict], ValidatorConfig::default());
        // \uFEFF is NOT in the default ignoreRegExpList — FEFF should be flagged
        let issues = validator.validate_text(r#"const bom = "\uFEFF";"#);
        assert!(
            issues.iter().any(|i| i.word == "FEFF"),
            "FEFF should be flagged (not in default skip patterns), got: {:?}",
            issues.iter().map(|i| &i.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_all_caps_suffix_stem_not_found_flagged() {
        // XYZZYs: stem is "XYZZY" (5 chars >= minWordLength), not in dict → flagged
        let dict = make_dict(&["hello"]);
        let config = ValidatorConfig {
            min_word_length: 4,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![dict], config);
        let issues = validator.validate_text("hello XYZZYs");
        assert_eq!(
            issues.len(),
            1,
            "XYZZYs should be flagged: stem XYZZY not in dict"
        );
        assert_eq!(issues[0].word, "XYZZYs");
    }

    #[test]
    fn test_camel_boundary_shift_astnode() {
        // ASTnode: regex splits as AS + Tnode, but boundary shift gives AST + node
        let dict = make_dict(&["hello", "AST", "node"]);
        let config = ValidatorConfig {
            min_word_length: 4,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![dict], config);
        let issues = validator.validate_text("hello ASTnode");
        assert!(
            issues.is_empty(),
            "ASTnode should be valid via boundary shift (AST + node): {:?}",
            issues.iter().map(|i| &i.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_camel_boundary_shift_ariatabs() {
        // ARIAtabs: regex splits as ARI + Atabs, but boundary shift gives ARIA + tabs
        let dict = make_dict(&["hello", "ARIA", "tabs"]);
        let config = ValidatorConfig {
            min_word_length: 4,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![dict], config);
        let issues = validator.validate_text("hello ARIAtabs");
        assert!(
            issues.is_empty(),
            "ARIAtabs should be valid via boundary shift (ARIA + tabs): {:?}",
            issues.iter().map(|i| &i.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_camel_boundary_shift_not_applied_when_invalid() {
        // XYZZYfoo: neither XYZZY+foo shift nor XYZZ+Yfoo is valid
        let dict = make_dict(&["hello"]);
        let config = ValidatorConfig {
            min_word_length: 4,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![dict], config);
        let issues = validator.validate_text("hello XYZZYfoo");
        assert!(
            !issues.is_empty(),
            "XYZZYfoo should be flagged: no valid boundary shift"
        );
    }

    #[test]
    fn test_escaped_apostrophe_contraction_valid() {
        // doesn\'t should be treated as doesn't (valid contraction)
        let dict = make_dict(&["doesn't", "work"]);
        let validator = Validator::new(vec![dict], ValidatorConfig::default());
        let issues = validator.validate_text(r"it doesn\'t work");
        assert!(
            !issues.iter().any(|i| i.word == "doesn" || i.word == "t"),
            "doesn\\'t should be valid as a contraction: {:?}",
            issues.iter().map(|i| &i.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_escaped_apostrophe_contraction_unknown() {
        // xyzzy\'s should be flagged since xyzzy is not in dict
        let dict = make_dict(&["hello"]);
        let validator = Validator::new(vec![dict], ValidatorConfig::default());
        let issues = validator.validate_text(r"hello xyzzy\'s");
        assert!(
            issues.iter().any(|i| i.word.contains("xyzzy")),
            "xyzzy\\'s should be flagged: {:?}",
            issues.iter().map(|i| &i.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_column_is_char_offset_not_byte() {
        // Multi-byte chars before the flagged word should use char count, not byte count.
        // U+201C (left double quotation mark) is 3 bytes in UTF-8.
        let dict = make_dict(&["hello"]);
        let validator = Validator::new(vec![dict], ValidatorConfig::default());
        // "hello \u{201c}xyzzy\u{201d}" — xyzzy starts at char 8 (h,e,l,l,o,space,",x)
        let issues = validator.validate_text("hello \u{201c}xyzzy\u{201d}");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].word, "xyzzy");
        // Char column: 'h'(1) 'e'(2) 'l'(3) 'l'(4) 'o'(5) ' '(6) '\u{201c}'(7) 'x'(8)
        assert_eq!(
            issues[0].column, 8,
            "column should be char-based, not byte-based"
        );
    }

    #[test]
    fn test_custom_base64_pattern_ignores_long_path_like_runs() {
        let text = "          - managers/devices/iscsiservers/disks/metrics\n";
        let compat = ValidatorConfig {
            cspell_compat_mode: true,
            ..ValidatorConfig::default()
        };

        let validator = Validator::new(
            vec![make_dict(&["managers", "devices", "disks", "metrics"])],
            compat.clone(),
        );
        let issues = validator.validate_text(text);
        assert!(
            issues.iter().any(|issue| issue.word == "iscsiservers"),
            "without Base64 custom ignore, iscsiservers should be reported"
        );

        let mut compat_base64 = compat;
        compat_base64.custom_ignore_patterns.enable_base64();
        let validator = Validator::new(
            vec![make_dict(&["managers", "devices", "disks", "metrics"])],
            compat_base64,
        );
        let issues = validator.validate_text(text);
        assert!(
            issues.is_empty(),
            "Base64 custom ignore should skip the full long run"
        );
    }

    #[test]
    fn test_base64_single_line_skips_data_url_payloads() {
        let text = "[doop](data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7)\n";
        let mut config = ValidatorConfig {
            cspell_compat_mode: true,
            ..ValidatorConfig::default()
        };
        config.custom_ignore_patterns.enable_hash_strings();
        let validator = Validator::new(
            vec![make_dict(&["doop", "data", "image", "gif", "base64"])],
            config,
        );

        let issues = validator.validate_text(text);
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(
            !words.contains(&"AQABAIAAAAAAAP"),
            "data URL payload should be ignored, got {words:?}"
        );
        assert!(
            !words.contains(&"BAEAAAAALAAAAAABAAEAAAIBRAA"),
            "data URL payload should be ignored, got {words:?}"
        );
    }

    #[test]
    fn test_default_hash_strings_skip_data_url_payloads_without_explicit_mask() {
        let text = "[doop](data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7)\n";
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(
            vec![make_dict(&["doop", "data", "image", "gif", "base64"])],
            config,
        );

        let issues = validator.validate_text(text);
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(
            !words.contains(&"AQABAIAAAAAAAP"),
            "default hash-string ignores should skip data URL payloads, got {words:?}"
        );
        assert!(
            !words.contains(&"BAEAAAAALAAAAAABAAEAAAIBRAA"),
            "default hash-string ignores should skip trailing data URL payloads, got {words:?}"
        );
    }

    #[test]
    fn test_default_base64_multiline_skips_yaml_base64_blocks() {
        let text = concat!(
            "base64-contents: |\n",
            "  /9j/4AAQSkZJRgABAQAA8ADwAAD/4QN6RXhpZgAATU0AKgAAAAgACgEGAAMAAAABAAIAAAEPAAIAAAAS\n",
            "  AAAAhgEQAAIAAAALAAAAmAESAAMAAAABAAEAAAEaAAUAAAABAAAApAEbAAUAAAABAAAArAEoAAMAAAAB\n",
            "  AAIAAAExAAIAAAARAAAAtAEyAAIAAAAUAAAAxodpAAQAAAABAAAA2gAAAABOSUtPTiBDT1JQT1JBVElP\n",
            "  TgBOSUtPTiBEODUwAAAAAADwAAAAAQAAAPAAAAABUGl4ZWxtYXRvciAzLjguNQAAMjAxOTowNzoxNyAx\n",
        );
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&["base64", "contents"])], config);

        let issues = validator.validate_text(text);
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(
            !words.iter().any(|word| {
                matches!(
                    *word,
                    "AESAAMAAAABAAEAAAEaAAUAAAABAAAApAEbAAUAAAABAAAArAEoAAMAAAAB"
                        | "AAIAAAExAAIAAAARAAAAtAEyAAIAAAAUAAAAxodpAAQAAAABAAAA2gAAAABOSUtPTiBDT1JQT1JBVElP"
                        | "TgBOSUtPTiBEODUwAAAAAADwAAAAAQAAAPAAAAABUGl4ZWxtYXRvciAzLjguNQAAMjAxOTowNzoxNyAx"
                )
            }),
            "default multiline base64 ignores should skip YAML payloads, got {words:?}"
        );
    }

    #[test]
    fn test_default_base64_multiline_skips_padded_final_line() {
        let text = concat!(
            "base64-contents: |\n",
            "  /9j/4AAQSkZJRgABAQAA8ADwAAD/4QN6RXhpZgAATU0AKgAAAAgACgEGAAMAAAABAAIAAAEPAAIAAAAS\n",
            "  AAAAhgEQAAIAAAALAAAAmAESAAMAAAABAAEAAAEaAAUAAAABAAAApAEbAAUAAAABAAAArAEoAAMAAAAB\n",
            "  iklJfM8n8X6ZYHWnZowSQD1Pv6Vy/wDZmnf88R+Z/wAa7bxf/wAhlv8AdH9a5eraITdj/9k=\n",
        );
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&["base64", "contents"])], config);

        let issues = validator.validate_text(text);

        assert!(
            issues.is_empty(),
            "multiline base64 with padded final line should be ignored: {issues:?}"
        );
    }

    #[test]
    fn test_custom_latex_patterns_skip_macros_but_keep_text_macro_arguments() {
        let mut config = ValidatorConfig {
            cspell_compat_mode: true,
            min_word_length: 3,
            ..ValidatorConfig::default()
        };
        config
            .custom_ignore_patterns
            .enable_latex_macro_function_names();
        config
            .custom_ignore_patterns
            .enable_latex_macros_multiline();
        let validator = Validator::new(vec![make_dict(&[])], config);

        let issues = validator.validate_text("\\mathbb{R} \\section{Surjektiv}\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(
            !words.contains(&"mathbb"),
            "macro name should be ignored: {words:?}"
        );
        assert!(
            !words.contains(&"R"),
            "macro body should be ignored for non-text macros: {words:?}"
        );
        assert!(
            !words.contains(&"section"),
            "text macro name should still be ignored: {words:?}"
        );
        assert!(
            words.contains(&"Surjektiv"),
            "text macro content should remain visible: {words:?}"
        );
    }

    #[test]
    fn test_custom_latex_patterns_keep_prefix_text_macro_arguments() {
        let mut config = ValidatorConfig {
            cspell_compat_mode: true,
            min_word_length: 3,
            ..ValidatorConfig::default()
        };
        config
            .custom_ignore_patterns
            .enable_latex_macro_function_names();
        config
            .custom_ignore_patterns
            .enable_latex_macros_multiline();
        let validator = Validator::new(vec![make_dict(&[])], config);

        let issues = validator.validate_text(
            "\\footnotetext{Snelting}\n\\titletext{Semantiv}\n\\captionsetup{labelformat=empty}\n",
        );
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(
            !words.contains(&"footnotetext")
                && !words.contains(&"titletext")
                && !words.contains(&"captionsetup"),
            "macro names should still be ignored: {words:?}"
        );
        assert!(
            words.contains(&"Snelting")
                && words.contains(&"Semantiv")
                && words.contains(&"labelformat"),
            "prefix-based text macro arguments should remain visible: {words:?}"
        );
    }

    #[test]
    fn test_custom_latex_math_pattern_skips_dollar_math_blocks() {
        let mut config = ValidatorConfig {
            cspell_compat_mode: true,
            min_word_length: 3,
            ..ValidatorConfig::default()
        };
        config.custom_ignore_patterns.enable_latex_math();
        let validator = Validator::new(vec![make_dict(&[])], config);

        let issues = validator.validate_text("before $Schmoeger$ after\n% $HiddenMath$\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(
            !words.contains(&"Schmoeger"),
            "math body should be ignored: {words:?}"
        );
        assert!(
            words.contains(&"before") && words.contains(&"after"),
            "non-math text should still be checked: {words:?}"
        );
    }

    #[test]
    fn test_escape_retry_with_short_word() {
        // \n'qur' — word extraction gives "n'qur". Apostrophe part splitting
        // gives "n" (1 char < minWordLength → ok) and "qur" (3 chars < minWordLength → ok).
        // All parts pass, so no issue is reported (matches cspell behavior).
        let dict = make_dict(&["hello"]);
        let config = ValidatorConfig {
            min_word_length: 4,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![dict], config);
        let issues = validator.validate_text(r"hello \n'qur'");
        assert!(
            issues.is_empty(),
            "n'qur should not be flagged (apostrophe parts are all below minWordLength): {:?}",
            issues.iter().map(|i| &i.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_cspell_compat_escape_retry_works_with_absolute_line_offsets() {
        let validator = Validator::new(
            vec![make_dict(&["smith"])],
            ValidatorConfig {
                cspell_compat_mode: true,
                ..ValidatorConfig::default()
            },
        );

        let issues = validator.validate_text("prefix line\nuser CONTOSO\\jsmith\n");

        assert!(
            issues.iter().all(|issue| issue.word != "jsmith"),
            "backslash escape retry should suppress jsmith on later lines, got {issues:?}"
        );
    }

    #[test]
    fn test_cspell_compat_prior_unknown_word_blocks_later_escape_retry_success_cache() {
        let validator = Validator::new(
            vec![make_dict(&["user", "path", "file", "doe", "pictures"])],
            ValidatorConfig {
                cspell_compat_mode: true,
                ..ValidatorConfig::default()
            },
        );

        let issues = validator
            .validate_text("user 'jdoe'\npath 'C:\\Pictures\\jdoe.png'\nfile `jdoe.png`\n");
        let issue_lines: Vec<usize> = issues
            .into_iter()
            .filter(|issue| issue.word == "jdoe")
            .map(|issue| issue.line)
            .collect();

        assert_eq!(issue_lines, vec![1, 3]);
    }

    #[test]
    fn test_cspell_compat_escape_retry_success_carries_forward_without_prior_unknown_word() {
        let validator = Validator::new(
            vec![make_dict(&["path", "file", "doe", "pictures"])],
            ValidatorConfig {
                cspell_compat_mode: true,
                ..ValidatorConfig::default()
            },
        );

        let issues = validator.validate_text("path 'C:\\Pictures\\jdoe.png'\nfile `jdoe.png`\n");
        let issue_lines: Vec<usize> = issues
            .into_iter()
            .filter(|issue| issue.word == "jdoe")
            .map(|issue| issue.line)
            .collect();

        assert!(
            issue_lines.is_empty(),
            "unexpected jdoe issues: {issue_lines:?}"
        );
    }

    #[test]
    fn test_cspell_compat_keeps_regex_camel_issues_when_splitter_cost_ties() {
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            min_word_length: 4,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&[])], config);

        let issues = validator.validate_text("ABCDHJKmsu");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert_eq!(words, vec!["ABCDHJ", "Kmsu"]);
    }

    #[test]
    fn test_cspell_compat_keeps_apostrophe_word_when_splitter_cost_ties() {
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            min_word_length: 4,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(
            vec![make_dict(&["Range", "Object", "Size", "loop"])],
            config,
        );

        let issues =
            validator.validate_text("for I in Gamepads'Range loop\nWNDCLASSEX'Object_Size\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert_eq!(words, vec!["Gamepads'Range", "WNDCLASSEX'Object"]);
    }

    #[test]
    fn test_cspell_compat_ada_word_break_splits_on_apostrophe_only() {
        let mut custom_ignore_patterns = CustomIgnorePatternMask::default();
        custom_ignore_patterns.enable_ada_word_break();
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            min_word_length: 4,
            custom_ignore_patterns,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(
            vec![make_dict(&["Range", "Object", "Size", "loop", "don", "t"])],
            config,
        );

        let issues = validator
            .validate_text("don't\nfor I in Gamepads'Range loop\nWNDCLASSEX'Object_Size\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert_eq!(words, vec!["Gamepads", "WNDCLASSEX"]);
    }

    #[test]
    fn test_cspell_compat_ada_word_break_skip_ranges_preserve_contractions() {
        let mut custom_ignore_patterns = CustomIgnorePatternMask::default();
        custom_ignore_patterns.enable_ada_word_break();
        let validator = Validator::new(
            vec![make_dict(&[])],
            ValidatorConfig {
                cspell_compat_mode: true,
                custom_ignore_patterns,
                ..ValidatorConfig::default()
            },
        );
        let text = "don't Gamepads'Range";
        let ranges = validator.compute_doc_skip_ranges(text, &[]);

        assert_eq!(ranges.len(), 1);
        assert_eq!(&text[ranges[0].0..ranges[0].1], "'");
        assert_eq!(ranges[0].0, "don't Gamepads".len());
    }

    #[test]
    fn test_cspell_compat_does_not_split_french_elision_into_proper_noun() {
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            min_word_length: 4,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&[])], config);

        let issues = validator.validate_text("Histoire de l'Académie Royale des Sciences\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(words.contains(&"l'Académie"));
        assert!(!words.contains(&"Académie"));
    }

    #[test]
    fn test_cspell_compat_reports_camel_suffix_from_all_caps_prefix() {
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            min_word_length: 4,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&["enum"])], config);

        let issues = validator.validate_text("ALCenum");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert_eq!(words, vec!["Cenum"]);
    }

    #[test]
    fn test_cspell_compat_reports_repeated_camel_suffixes_up_to_duplicate_limit() {
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            min_word_length: 4,
            max_duplicate_problems: 5,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&["device"])], config);

        let issues =
            validator.validate_text("ALCdevice ALCdevice ALCdevice ALCdevice ALCdevice ALCdevice");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert_eq!(
            words,
            vec!["Cdevice", "Cdevice", "Cdevice", "Cdevice", "Cdevice"]
        );
    }

    #[test]
    fn test_cspell_compat_keeps_word_after_ignored_prefix_range() {
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            ignore_patterns: vec![Regex::new(r"(?i)\b(?:rf|fr|f|r|u|ur|b|br)'").unwrap()],
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&["if", "re", "search", "line"])], config);

        let issues = validator.validate_text("if re.search(r'playerclip', line):\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert_eq!(words, vec!["playerclip"]);
    }

    #[test]
    fn test_cspell_compat_skips_commit_hash_inside_backticks() {
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&[])], config);

        let issues = validator.validate_text("`51acbeb`");

        assert!(
            issues.is_empty(),
            "commit hash should be ignored: {issues:?}"
        );
    }

    #[test]
    fn test_cspell_compat_skips_hex_literal_inside_quotes() {
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&[])], config);

        let issues = validator.validate_text("'0xffff0000'");

        assert!(
            issues.is_empty(),
            "hex literal should be ignored: {issues:?}"
        );
    }

    #[test]
    fn test_cspell_compat_skips_base64_inside_backticks() {
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&[])], config);
        let text = concat!(
            "`MIIDSzCCAjOgAwIBAgIUfIRObjWNUA4jxQ/0x8BOCvE2Vw4wDQYJKoZIhvcNAQELBQAw",
            "FjEUMBIGA1UEAwwLRWFzeS1SU0EgQ0EwHhcNMTkwODI4MTYyNTU5WhcNMjkwODI1MTYy",
            "NTU5WjAWMRQwEgYDVQQDDAtFYXN5LVJTQSBDQTCCASIwDQYJKoZIhvcNAQEBBQADggEP",
            "ADCCAQoCggEBAK5m5elxhQfMp/3aVJ4JnpN9PUSz6LlP6LePAPFU7gqohVVFVtDkChJAG",
            "3FNkNQNlieVTja/bgH9IcC6oKbROwdY1h0MvNV8AHHigvl03WuJD8g2ReVFXXwsnrPmK",
            "XCFzQyMI6TYk3m2gYrXsZOU1GLnfMRC3KAMRgE2F45twOs9hqG169YJ6mM2eQjzjCHWI6",
            "S2/iUYvYxRkCOlYUbLsMD/AhgAf1plzg6LPqNxtdlwxZnA0ytgkmhK67HtzJu0+ovUCs",
            "Mv0RwcMhsEo9T8nyFAGt9XLZ63X5WpBCTUApaAUhnG0XnerjmUWb6eUWw4zev54sEfY5F",
            "3x002iQaW6cECAwEAAaOBkDCBjTAdBgNVHQ4EFgQU4CBUbZsS2GaNIkGRz/cBsD5ivjs",
            "wUQYDVR0jBEowSIAU4CBUbZsS2GaNIkGRz/cBsD5ivjuhGqQYMBYxFDASBgNVBAMMC0Vh",
            "c3ktUlNBIENBghR8hE5uNY1QDiPFD/THwE4K8TZXDjAMBgNVHRMEBTADAQH/MAsGA1Ud",
            "DwQEAwIBBjANBgkqhkiG9w0BAQsFAAOCAQEAKB3V4HIzoiO/Ch6WMj9bLJ2FGbpkMrcb",
            "/Eq01hT5zcfKD66lVS1MlK+cRL446Z2b2KDP1oFyVs+qmrmtdwrWgD+nfe2sBmmIHo9m",
            "9KygMkEOfG3MghGTEcS+0cTKEcoHYWYyOqQh6jnedXY8Cdm4GM1hAc9MiL3/sqV8YCVS",
            "LNnkoNysmr06/rZ0MCUZPGUtRmfd0heWhrfzAKw2HLgX+RAmpOE2MZqWcjvqKGyaRiaZ",
            "ks4nJkP6521aC2Lgp0HhCz1j8/uQ5ldoDszCnu/iro0NAsNtudTMD+YoLQxLqdleIh6CW",
            "+illc2VdXwj7mn6J04yns9jfE2jRjW/yTLFuQ==`"
        );

        let issues = validator.validate_text(text);

        assert!(issues.is_empty(), "base64 should be ignored: {issues:?}");
    }

    #[test]
    fn test_cspell_compat_skips_quoted_multiline_base64_blocks() {
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&["password"])], config);
        let text = concat!(
            "password = (\n",
            "    \"VSK0UYV6FFQVZ0KG88DYN9WADAADZO1CTSIVDJUNZSUML6IBX7LN7ZS3R5\"\n",
            "    \"JGB3RGZ7VI7G7DJQ9NI8BQFSRPTG6UWTTVESA5ZPUN\"\n",
            ")\n"
        );

        let issues = validator.validate_text(text);

        assert!(
            issues.is_empty(),
            "quoted multiline base64 should be ignored: {issues:?}"
        );
    }

    #[test]
    fn test_cspell_compat_does_not_treat_mangled_name_after_underscore_as_base64() {
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(
            vec![make_dict(&["physx", "Px", "Transform", "transform", "Inv"])],
            config,
        );

        let issues = validator.validate_text("\"_ZNK5physx11PxTransform12transformInvERKS0_\"\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert_eq!(words, vec!["ERKS"]);
    }

    #[test]
    fn test_compound_validation_handles_long_tokens_without_recursion_overflow() {
        let config = ValidatorConfig {
            allow_compound_words: true,
            compound_words_mode: CompoundWordsMode::JoinWords,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&["alpha", "beta", "gamma"])], config);
        let long = "alphabeta".repeat(512);

        let issues = validator.validate_text(&long);

        assert!(!issues.is_empty());
    }

    #[test]
    fn test_cspell_compat_allow_compound_words_matches_en_us_examples() {
        let Some(dict_path) = en_us_dict_path() else {
            eprintln!("Skipping: en_US dictionary not found");
            return;
        };
        let dict = load_trie_v3(&dict_path).expect("Failed to load en_US trie");
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            allow_compound_words: true,
            compound_words_mode: CompoundWordsMode::SeparateWords,
            min_word_length: 4,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![Box::new(dict)], config);

        let multiapi = validator.validate_text("multiapi\n");
        assert!(
            multiapi.is_empty(),
            "allowCompoundWords should accept multiapi: {multiapi:?}"
        );
        let multiapi_yaml = validator.validate_text("multiapi: true\n");
        assert!(
            multiapi_yaml.is_empty(),
            "allowCompoundWords should accept multiapi in yaml-like text: {multiapi_yaml:?}"
        );

        let multiapiscript = validator.validate_text("multiapiscript\n");
        assert!(
            multiapiscript.is_empty(),
            "allowCompoundWords should accept multiapiscript: {multiapiscript:?}"
        );
        let multiapiscript_yaml = validator.validate_text("multiapiscript: true\n");
        assert!(
            multiapiscript_yaml.is_empty(),
            "allowCompoundWords should accept multiapiscript in yaml-like text: {multiapiscript_yaml:?}"
        );

        let splashscreen = validator.validate_text("splashscreen\n");
        assert!(
            splashscreen.is_empty(),
            "allowCompoundWords should accept splashscreen (splash+screen): {splashscreen:?}"
        );
    }

    #[test]
    fn test_cspell_compat_reports_splashscreen_in_flutter_contexts() {
        let Some(dict_path) = en_us_dict_path() else {
            eprintln!("Skipping: en_US dictionary not found");
            return;
        };
        let dict = load_trie_v3(&dict_path).expect("Failed to load en_US trie");
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            min_word_length: 4,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![Box::new(dict)], config);

        let import_line = validator.validate_text(
            "import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen\n",
        );
        let import_words: Vec<&str> = import_line.iter().map(|i| i.word.as_str()).collect();
        assert!(
            import_words.contains(&"splashscreen"),
            "dotted import should still report splashscreen, got {import_words:?}"
        );

        let const_line = validator
            .validate_text("const val SPLASHSCREEN_ALPHA_ANIMATION_DURATION = 500 as Long\n");
        let const_words: Vec<&str> = const_line.iter().map(|i| i.word.as_str()).collect();
        assert!(
            const_words.contains(&"SPLASHSCREEN"),
            "underscore constant should still report SPLASHSCREEN, got {const_words:?}"
        );
    }

    #[test]
    fn test_cspell_compat_does_not_cache_short_camel_prefixes() {
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            min_word_length: 4,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&["FOOBAR", "device"])], config);

        let issues = validator.validate_text("ALCdevice\nALC_FOOBAR\nALCdevice\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert_eq!(words, vec!["Cdevice", "Cdevice"]);
    }

    #[test]
    fn test_cspell_compat_applies_ignore_directive_to_camel_subwords() {
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            min_word_length: 4,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&["use", "std", "collections"])], config);

        let issues = validator.validate_text(
            "// cSpell: ignore deque subcomponent\nuse std::collections::VecDeque;\n",
        );
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert_eq!(words, Vec::<&str>::new());
    }

    #[test]
    fn test_cspell_compat_does_not_reapply_ignore_regex_to_split_words() {
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            ignore_patterns: vec![
                Regex::new(r"(\b|0x|#|_)?[argbARGB][argbARGB][argbARGB]+(\b|_|Color)").unwrap(),
            ],
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&["pixel"])], config);

        let issues = validator.validate_text("pixel-bgra8888\nBgra8888Pixel\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert!(
            words.contains(&"bgra"),
            "expected contextual color regex not to hide bgra inside bgra8888, got {words:?}"
        );
        assert!(
            words.contains(&"Bgra"),
            "expected contextual color regex not to hide Bgra inside Bgra8888Pixel, got {words:?}"
        );
    }

    #[test]
    fn test_cspell_compat_reports_word_fragment_left_after_ignore_span() {
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            ignore_patterns: vec![
                Regex::new(r"(\b|0x|#|_)?[argbARGB][argbARGB][argbARGB]+(\b|_|Color)").unwrap(),
            ],
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(
            vec![make_dict(&[
                "type",
                "shared",
                "string",
                "arg",
                "sharedstring",
            ])],
            config,
        );

        let issues = validator.validate_text("type StringArg = (SharedString,);\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert_eq!(words, vec!["Strin"]);
    }

    #[test]
    fn test_fancy_ignore_patterns_are_applied_after_candidate_collection() {
        let config = ValidatorConfig {
            ignore_patterns_fancy: vec![FancyRegex::new(r"(?<=x )badd").unwrap()],
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&[])], config);

        let issues = validator.validate_text("x badd nope\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert_eq!(words, vec!["nope"]);
    }

    #[test]
    fn test_fancy_include_patterns_are_applied_after_candidate_collection() {
        let config = ValidatorConfig {
            include_patterns_fancy: vec![FancyRegex::new(r"(?<=keep )badd").unwrap()],
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&[])], config);

        let issues = validator.validate_text("dropx keep badd\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert_eq!(words, vec!["badd"]);
    }

    #[test]
    fn test_cspell_compat_embedded_ignore_span_splits_validation_ranges() {
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            ignore_patterns: vec![
                Regex::new(r"(\b|0x|#|_)?[argbARGB][argbARGB][argbARGB]+(\b|_|Color)").unwrap(),
            ],
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&[])], config);
        let text = "type StringArg = (SharedString,);\n";

        let skip_ranges = validator.compute_doc_skip_ranges(text, &[]);
        let validation_ranges = compute_doc_validation_ranges(text.len(), &[], &skip_ranges, false);

        assert!(
            skip_ranges
                .iter()
                .any(|&(start, end)| &text[start..end] == "gArg"),
            "expected embedded color regex span to be excluded, got {skip_ranges:?}"
        );
        assert!(
            validation_ranges
                .iter()
                .any(|&(start, end)| &text[start..end] == "type Strin"),
            "expected validation ranges to preserve the left fragment, got {validation_ranges:?}"
        );
    }

    #[test]
    fn test_cspell_compat_reports_strin_without_embedded_ignore_processing() {
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            ignore_patterns: vec![
                Regex::new(r"(\b|0x|#|_)?[argbARGB][argbARGB][argbARGB]+(\b|_|Color)").unwrap(),
            ],
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(
            vec![make_dict(&[
                "type",
                "shared",
                "string",
                "arg",
                "sharedstring",
            ])],
            config,
        );

        let issues = validator.validate_text("type Strin = (SharedString,);\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert_eq!(words, vec!["Strin"]);
    }

    #[test]
    fn test_cspell_compat_skips_short_subwords_before_checking_full_words() {
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            min_word_length: 4,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&["FALSE", "enum"])], config);

        let issues = validator.validate_text("ALC_FALSE\nALCenum\n");
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert_eq!(words, vec!["Cenum"]);
    }

    #[test]
    fn test_cspell_compat_uses_js_length_for_min_word_length() {
        let config = ValidatorConfig {
            cspell_compat_mode: true,
            min_word_length: 4,
            ..ValidatorConfig::default()
        };
        let validator = Validator::new(vec![make_dict(&["round", "trip"])], config);

        let issues = validator.validate_text(
            "round_trip(\"☢🐣  ᖇ𝓤𝕊тⓟ𝕐𝕥卄σ𝔫  ♬👣\")\nround_trip(\"💀👌  ק𝔂tℍⓞ𝓷 ３  🔥👤\")\n",
        );
        let words: Vec<&str> = issues.iter().map(|issue| issue.word.as_str()).collect();

        assert_eq!(words, vec!["𝕐𝕥", "ק𝔂t"]);
    }
}
