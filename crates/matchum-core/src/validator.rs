use crate::issue::ValidationIssue;
use crate::splitter;
use aho_corasick::{AhoCorasick, AhoCorasickKind};
use compact_str::CompactString;
use regex::Regex;
use matchum_config::directives::{self, Directive};
use matchum_dict::dictionary::Dictionary;
use std::collections::HashSet;
use std::sync::{Arc, LazyLock, RwLock};

pub type WordCache = Arc<RwLock<hashbrown::HashMap<CompactString, bool>>>;

/// Configuration for the validation pipeline.
#[derive(Clone)]
pub struct ValidatorConfig {
    pub min_word_length: usize,
    pub case_sensitive: bool,
    pub ignore_patterns: Vec<Regex>,
    pub include_patterns: Vec<Regex>,
    pub flag_words: HashSet<CompactString>,
    pub ignore_words: HashSet<CompactString>,
    pub allow_compound_words: bool,
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
            include_patterns: Vec::new(),
            flag_words: HashSet::new(),
            ignore_words: HashSet::new(),
            allow_compound_words: false,
            compute_suggestions: true,
            max_duplicate_problems: 5,
        }
    }
}

struct DictionaryEntry {
    name: Option<String>,
    dict: Arc<dyn Dictionary>,
    default_active: bool,
}

/// Raw pattern strings for builtin skip patterns.
/// Matches cspell's `definedDefaultRegExpExcludeList` from DefaultSettings.ts.
/// SpellCheckerDisable/SpellCheckerIgnoreInDocSetting are handled by the directive system.
/// Patterns that require JS-only features (backreferences, lookaheads, lookbehinds) are
/// simplified to best-effort Rust regex approximations.
const BUILTIN_SKIP_PATTERN_STRS: &[&str] = &[
    // Urls
    r#"(?i)(?:https?|ftp)://[^\s"]+"#,
    // Email (simplified bounds to avoid DFA size explosion, non-capturing for DFA)
    r"(?i)\b[-\w.+]+@\w+(?:\.\w+){1,4}\b",
    // RsaCert (simplified: no backreference)
    r"-{5}BEGIN\s+[\w\s]+-{5}[\w=+\-/\\\s]+?-{5}END\s+[\w\s]+-{5}",
    // SshRsa (simplified: no negative lookahead)
    r"(?i)ssh-rsa\s+[A-Za-z0-9/+]{28,}={0,3}",
    // Base64MultiLine (simplified: no lookbehind/lookahead)
    r"(?m)[A-Za-z0-9/+]{40,}\n(?:\s*[A-Za-z0-9/+]{40,}\n)*(?:\s*[A-Za-z0-9/+]+=*)?",
    // Base64SingleLine (simplified: no lookbehind/lookahead)
    r"[A-Za-z0-9/+]{40,}={0,3}",
    // CommitHash (simplified: no negative lookahead)
    r"(?i)\b(?:0x)?[0-9a-f]{7,}\b",
    // CommitHashLink
    r"(?i)\[[0-9a-f]{7,}\]",
    // CStyleHexValue
    r"(?i)\b0x[0-9a-f_]+n?\b",
    // CSSHexValue
    r"(?i)#[0-9a-f]{3,8}\b",
    // SHA
    r"(?i)\bsha\d+-[A-Za-z0-9+/]{25,}={0,3}",
    // HashStrings (simplified: no lookahead)
    r"(?i)(?:\b(?:sha\d+|md5|base64|crypt|bcrypt|scrypt|security-token|assertion)[-,:$=]|#code[/])[-\w/+%.]{25,}={0,3}",
    // UnicodeRef
    r"(?i)\bU\+[0-9a-f]{4,5}(?:-[0-9a-f]{4,5})?",
    // UUID
    r"(?i)\b[0-9a-fx]{8}-[0-9a-fx]{4}-[0-9a-fx]{4}-[0-9a-fx]{4}-[0-9a-fx]{12}\b",
    // BinaryLiteral
    r"(?i)\b0b[01_]+\b",
    // OctalLiteral
    r"(?i)\b0o[0-7_]+\b",
    // ScientificNotation
    r"\b\d+\.?\d*[eE][+-]?\d+\b",
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
        table[i as usize] = matches!(i as u8, b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F');
        i += 1;
    }
    table
};

/// Scan for CommitHash matches: `(?i)\b(?:0x)?[0-9a-f]{7,}\b`
/// Hand-written byte scanner replacing regex for this always-check pattern.
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
        while i < len && HEX_DIGIT[text[i] as usize] {
            i += 1;
        }
        let hex_len = i - start;
        if hex_len >= 7 {
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
fn scan_base64_single_line(text: &[u8], ranges: &mut Vec<(usize, usize)>) {
    let len = text.len();
    let mut i = 0;
    while i < len {
        if !BASE64_CHAR[text[i] as usize] {
            i += 1;
            continue;
        }
        let start = i;
        while i < len && BASE64_CHAR[text[i] as usize] {
            i += 1;
        }
        let b64_len = i - start;
        if b64_len >= 40 {
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

/// Scan for Base64 multiline:
/// `(?m)[A-Za-z0-9/+]{40,}\n(?:\s*[A-Za-z0-9/+]{40,}\n)*(?:\s*[A-Za-z0-9/+]+=*)?`
fn scan_base64_multiline(text: &[u8], ranges: &mut Vec<(usize, usize)>) {
    let len = text.len();
    let mut i = 0;
    while i < len {
        // Look for a line starting with ≥40 base64 chars followed by '\n'
        if !BASE64_CHAR[text[i] as usize] {
            i += 1;
            continue;
        }
        let start = i;
        while i < len && BASE64_CHAR[text[i] as usize] {
            i += 1;
        }
        let b64_len = i - start;
        if b64_len < 40 || i >= len || text[i] != b'\n' {
            continue;
        }
        // Consumed first line of ≥40 base64 chars + '\n'
        i += 1; // skip '\n'
        let mut end = i;

        // Consume continuation lines: `\s*[A-Za-z0-9/+]{40,}\n`
        loop {
            let line_start = i;
            // Skip leading whitespace
            while i < len && (text[i] == b' ' || text[i] == b'\t') {
                i += 1;
            }
            // Count base64 chars
            let b64_start = i;
            while i < len && BASE64_CHAR[text[i] as usize] {
                i += 1;
            }
            let line_b64_len = i - b64_start;
            if line_b64_len >= 40 && i < len && text[i] == b'\n' {
                i += 1; // skip '\n'
                end = i;
            } else {
                // Not a continuation line — backtrack
                i = line_start;
                break;
            }
        }

        // Optional final line: `\s*[A-Za-z0-9/+]+=*`
        {
            let saved = i;
            // Skip leading whitespace
            while i < len && (text[i] == b' ' || text[i] == b'\t') {
                i += 1;
            }
            let b64_start = i;
            while i < len && BASE64_CHAR[text[i] as usize] {
                i += 1;
            }
            let final_b64_len = i - b64_start;
            if final_b64_len > 0 {
                // Consume trailing '='
                while i < len && text[i] == b'=' {
                    i += 1;
                }
                end = i;
            } else {
                i = saved;
            }
        }

        if end > start {
            ranges.push((start, end));
        }
    }
}

/// Validates text against a set of dictionaries.
pub struct Validator {
    dictionaries: Vec<DictionaryEntry>,
    config: ValidatorConfig,
    /// Pre-computed flag: whether any dictionary has forbidden words.
    /// When false, we can skip all is_forbidden() calls entirely.
    any_dict_has_forbidden: bool,
    /// Pre-computed flag: true when all dictionaries are case-insensitive.
    /// When true, skip the validator's own lowercase fallback in is_word_valid
    /// because each dict's has() already normalizes to lowercase internally.
    all_dicts_case_insensitive: bool,
    /// Shared word validation cache across files with the same dictionary set.
    /// Key: word text, Value: is_word_valid result.
    word_cache: Option<WordCache>,
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
        let any_dict_has_forbidden = dictionaries.iter().any(|d| d.dict.has_forbidden_words());
        let all_dicts_case_insensitive = !dictionaries.iter().any(|d| d.dict.is_case_sensitive());
        Self {
            dictionaries,
            config,
            any_dict_has_forbidden,
            all_dicts_case_insensitive,
            word_cache: None,
        }
    }

    pub fn set_word_cache(&mut self, cache: WordCache) {
        self.word_cache = Some(cache);
    }

    pub fn new_word_cache() -> WordCache {
        Arc::new(RwLock::new(hashbrown::HashMap::new()))
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
        let mut word_issue_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        let mut directive_flag_words: HashSet<String> = HashSet::new();
        let mut directive_include_patterns: Vec<Regex> = Vec::new();
        let mut case_sensitive = self.config.case_sensitive;

        // Single pass: parse all directives once and cache results.
        // Use document-level AC scan to find candidate lines, then parse only those.
        let lines: Vec<&str> = text.lines().collect();
        let mut has_keyword = vec![false; lines.len()];
        {
            let prefilter = &*directives::DIRECTIVE_PREFILTER;
            let mut line_idx = 0;
            let mut line_end = lines.first().map_or(0, |l| l.len());
            for mat in prefilter.find_iter(text) {
                let pos = mat.start();
                while line_idx + 1 < lines.len() && pos >= line_end + 1 {
                    line_idx += 1;
                    line_end += 1 + lines[line_idx].len();
                }
                has_keyword[line_idx] = true;
            }
        }
        let line_directives: Vec<Option<Directive>> = lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                if has_keyword[i] {
                    directives::parse_directive_no_prefilter(line)
                } else {
                    None
                }
            })
            .collect();

        // Collect file-wide directives from cached results
        for directive in line_directives.iter().filter_map(|d| d.as_ref()) {
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

        // Compute document-level skip and include ranges (absolute byte offsets).
        // cspell applies patterns to the full document text, not per-line.
        let doc_skip_ranges = self.compute_doc_skip_ranges(text, &directive_ignore_patterns);
        let doc_include_ranges = self.compute_doc_include_ranges(text, &directive_include_patterns);

        let mut line_start_offset = 0;
        let mut code_tokens_buf: Vec<splitter::Word<'_>> = Vec::new();
        let mut words_buf: Vec<splitter::Word<'_>> = Vec::new();
        let mut camel_parts_buf: Vec<&str> = Vec::new();
        let mut split_buffers = splitter::SplitBuffers::new();
        let mut prevalidated_ranges: Vec<(usize, usize)> = Vec::new();

        for (line_idx, line) in lines.iter().enumerate() {
            let line_num = line_idx + 1;

            if let Some(ref directive) = line_directives[line_idx] {
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
                    Directive::Ignore(_)
                    | Directive::Words(_)
                    | Directive::LocalWords(_)
                    | Directive::ForbidWords(_)
                    | Directive::IgnoreRegExp(_)
                    | Directive::IncludeRegExp(_) => {
                        // Already collected in first pass
                        line_start_offset += line.len() + 1;
                        continue;
                    }
                    Directive::Dictionaries(dicts) => {
                        let set: HashSet<String> = dicts.iter().map(|d| d.to_lowercase()).collect();
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
                    Directive::EnableCaseSensitive => {
                        case_sensitive = true;
                        line_start_offset += line.len() + 1;
                        continue;
                    }
                    Directive::DisableCaseSensitive => {
                        case_sensitive = false;
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
                {
                    if self.is_word_valid(
                        ct.text,
                        directive_dictionaries.as_ref(),
                        allow_compound_words,
                        case_sensitive,
                    ) || inline_words.contains(&ct.text.to_lowercase())
                        || self.is_underscore_compound_valid(
                            ct.text,
                            directive_dictionaries.as_ref(),
                            allow_compound_words,
                            case_sensitive,
                        )
                    {
                        prevalidated_ranges.push((ct.offset, ct.offset + ct.text.len()));
                    }
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
                let token_is_flagged = self.config.flag_words.contains(token_lower.as_ref())
                    || directive_flag_words.contains(token_lower.as_ref())
                    || (self.any_dict_has_forbidden
                        && self
                            .dictionaries
                            .iter()
                            .filter(|d| self.is_dict_active(d, directive_dictionaries.as_ref()))
                            .any(|d| {
                                d.dict
                                    .is_forbidden_pre_normalized(token.text, token_lower.as_ref())
                            }));

                if !token_is_flagged
                    && (inline_words.contains(token_lower.as_ref())
                        || self.config.ignore_words.contains(token_lower.as_ref()))
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

                    let is_forbidden = if single_word_token {
                        token_is_flagged
                    } else {
                        self.config.flag_words.contains(lower.as_ref())
                            || directive_flag_words.contains(lower.as_ref())
                            || (self.any_dict_has_forbidden
                                && self
                                    .dictionaries
                                    .iter()
                                    .filter(|d| {
                                        self.is_dict_active(d, directive_dictionaries.as_ref())
                                    })
                                    .any(|d| {
                                        d.dict
                                            .is_forbidden_pre_normalized(word.text, lower.as_ref())
                                    }))
                    };

                    if is_forbidden {
                        let count = word_issue_counts
                            .entry(lower.clone().into_owned())
                            .or_insert(0);
                        *count += 1;
                        if *count <= self.config.max_duplicate_problems {
                            issues.push(ValidationIssue {
                                word: word.text.to_string(),
                                offset: line_start_offset + word.offset,
                                line: line_num,
                                column: word.offset + 1,
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

                    if inline_words.contains(lower.as_ref())
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
                    ) {
                        // Fallback: strip trailing possessive/contraction and re-check
                        if let Some(base) = strip_trailing_suffix(word.text) {
                            if base.chars().count() < self.config.min_word_length
                                || self.is_word_valid(
                                    base,
                                    directive_dictionaries.as_ref(),
                                    allow_compound_words,
                                    case_sensitive,
                                )
                            {
                                continue;
                            }
                        }

                        // Fallback: ALL-CAPS + English suffix (e.g. REPLs → REPL)
                        if let Some(base) = strip_all_caps_suffix(word.text) {
                            if self.is_word_valid(
                                base,
                                directive_dictionaries.as_ref(),
                                allow_compound_words,
                                case_sensitive,
                            ) {
                                continue;
                            }
                        }

                        // Fallback: split at apostrophe and check all parts
                        // (e.g. f'hello → f + hello, both found in dicts)
                        if word.text.contains('\'') || word.text.contains('\u{2019}') {
                            if self.all_apostrophe_parts_valid(
                                word.text,
                                directive_dictionaries.as_ref(),
                                allow_compound_words,
                                case_sensitive,
                            ) {
                                continue;
                            }
                        }

                        // Fallback: if preceded by '\', drop first char and retry
                        if word.offset > 0
                            && line.as_bytes().get(word.offset - 1) == Some(&b'\\')
                            && word.text.len() > 1
                        {
                            let first_len =
                                word.text.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                            let without_first = &word.text[first_len..];
                            if self.is_word_valid(
                                without_first,
                                directive_dictionaries.as_ref(),
                                allow_compound_words,
                                case_sensitive,
                            ) {
                                continue;
                            }
                        }

                        let count = word_issue_counts
                            .entry(lower.clone().into_owned())
                            .or_insert(0);
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
                            column: word.offset + 1,
                            is_forbidden: false,
                            is_known_typo: typo_correction.is_some(),
                            suggestions,
                        });
                    }
                }
            }

            line_start_offset += line.len() + 1;
        }

        issues
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
        scan_base64_single_line(bytes, &mut ranges);
        scan_base64_multiline(bytes, &mut ranges);

        // AC prefilter: determine which anchor-triggered patterns are needed
        let mut needed = [false; 17];
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
        case_sensitive: bool,
    ) -> bool {
        let can_cache = directive_dictionaries.is_none()
            && allow_compound_words == self.config.allow_compound_words
            && case_sensitive == self.config.case_sensitive;

        if can_cache {
            if let Some(ref cache) = self.word_cache {
                if let Ok(guard) = cache.read() {
                    if let Some(&result) = guard.get(word) {
                        return result;
                    }
                }
            }
        }

        let result = self.is_word_valid_inner(
            word,
            directive_dictionaries,
            allow_compound_words,
            case_sensitive,
        );

        if can_cache {
            if let Some(ref cache) = self.word_cache {
                if let Ok(mut guard) = cache.write() {
                    guard.insert(CompactString::from(word), result);
                }
            }
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

        // Only try lowercase fallback when needed:
        // Skip if all dicts are case-insensitive (they normalize internally).
        if !case_sensitive && !self.all_dicts_case_insensitive {
            let lower = word.to_lowercase();
            if lower != word && self.has_in_active_dicts(&lower, directive_dictionaries) {
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
                if !case_sensitive && !self.all_dicts_case_insensitive {
                    let lower = stripped.to_lowercase();
                    if self.has_in_active_dicts(&lower, directive_dictionaries) {
                        return true;
                    }
                }
            }
        }

        if !allow_compound_words {
            return false;
        }

        self.is_compound_valid(word, directive_dictionaries, case_sensitive)
    }

    fn has_in_active_dicts(
        &self,
        word: &str,
        directive_dictionaries: Option<&HashSet<String>>,
    ) -> bool {
        // Fast path: normalize once and pass to all dicts.
        // Saves N-1 redundant normalize() calls (each potentially allocating).
        if self.all_dicts_case_insensitive && word.is_ascii() {
            let lower = ascii_lowercase(word);
            return self
                .dictionaries
                .iter()
                .filter(|d| self.is_dict_active(d, directive_dictionaries))
                .any(|d| d.dict.has_pre_normalized(word, lower.as_ref()));
        }
        self.dictionaries
            .iter()
            .filter(|d| self.is_dict_active(d, directive_dictionaries))
            .any(|d| d.dict.has(word))
    }

    fn is_compound_valid(
        &self,
        word: &str,
        directive_dictionaries: Option<&HashSet<String>>,
        case_sensitive: bool,
    ) -> bool {
        if word.len() < 2 {
            return false;
        }

        let mut boundaries: Vec<usize> = word.char_indices().map(|(i, _)| i).collect();
        boundaries.push(word.len());

        for split in boundaries
            .iter()
            .copied()
            .skip(1)
            .take(boundaries.len().saturating_sub(2))
        {
            let (left, right) = word.split_at(split);
            if left.is_empty() || right.is_empty() {
                continue;
            }
            if self.has_in_active_dicts(left, directive_dictionaries)
                && self.has_in_active_dicts(right, directive_dictionaries)
            {
                return true;
            }

            if !case_sensitive && !self.all_dicts_case_insensitive {
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

    /// Check if a code token with underscores is valid by trying all possible
    /// splits at underscore boundaries. If any split produces parts that are
    /// all individually valid, the token is considered valid.
    ///
    /// For example, `S_IMODE_method` can be split as `S_IMODE` + `method`,
    /// where `S_IMODE` is in the python dict and `method` is in en_us.
    fn is_underscore_compound_valid(
        &self,
        token: &str,
        directive_dictionaries: Option<&HashSet<String>>,
        allow_compound_words: bool,
        case_sensitive: bool,
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
    ) -> bool {
        let parts: Vec<&str> = word.split(|c: char| c == '\'' || c == '\u{2019}').collect();
        if parts.len() < 2 {
            return false;
        }
        parts.iter().all(|part| {
            !part.is_empty()
                && self.is_word_valid(
                    part,
                    directive_dictionaries,
                    allow_compound_words,
                    case_sensitive,
                )
        })
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

/// Binary search in sorted, non-overlapping (start, end) ranges to check
/// if `offset` falls within any range. O(log n).
#[inline]
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
                // 'u' — Rust regex is Unicode by default, ignore
                // 'g' — find_iter handles global matching, ignore
                // 'x' — verbose mode: strip unescaped whitespace and # comments
                let body = if flags.contains('x') {
                    strip_verbose_whitespace(body)
                } else {
                    body.to_string()
                };
                let pat = if prefix.is_empty() {
                    body
                } else {
                    format!("(?{}){}", prefix, body)
                };
                return Regex::new(&pat).ok();
            }
        }
    }
    Regex::new(s).ok()
}

/// Strip unescaped whitespace and `#` line comments for verbose (`x`) mode.
fn strip_verbose_whitespace(pattern: &str) -> String {
    let mut result = String::with_capacity(pattern.len());
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            result.push(ch);
            if let Some(next) = chars.next() {
                result.push(next);
            }
        } else if ch == '#' {
            // Skip to end of line
            for c in chars.by_ref() {
                if c == '\n' {
                    break;
                }
            }
        } else if ch.is_whitespace() {
            // Skip unescaped whitespace
        } else {
            result.push(ch);
        }
    }
    result
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
        if ch.is_uppercase() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use matchum_dict::hashdict::HashDictionary;

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

    #[test]
    fn test_builtin_patterns_all_compile() {
        // 17 patterns: 14 from cspell's definedDefaultRegExpExcludeList + 3 numeric literals
        // (SpellCheckerDisable and SpellCheckerIgnoreInDocSetting handled by directive system)
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
}
