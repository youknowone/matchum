// spell-checker:ignore AUTHPRIV
use regex::Regex;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::sync::LazyLock;
use unicode_general_category::{GeneralCategory, get_general_category};

/// A word extracted from text, with its byte offset in the source.
///
/// Borrows the text from the input slice to avoid heap allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Word<'a> {
    pub text: &'a str,
    pub offset: usize,
}

/// A split segment annotated with dictionary validity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitWord<'a> {
    pub text: &'a str,
    pub offset: usize,
    pub is_found: bool,
}

/// Result of cspell's expensive `wordSplitter.split(...)` pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitResult<'a> {
    pub line: Word<'a>,
    pub offset: usize,
    pub text: Word<'a>,
    pub words: Vec<SplitWord<'a>>,
    pub end_offset: usize,
}

#[derive(Debug, Clone, Copy)]
struct SplitLineSegment<'a> {
    line: &'a str,
    rel_start: usize,
    rel_end: usize,
}

#[derive(Debug, Clone)]
struct PossibleWordBreak {
    offset: usize,
    breaks: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Copy)]
struct SplitPathNode<'a> {
    next: Option<usize>,
    c: usize,
    text: Option<SplitWord<'a>>,
}

#[derive(Debug, Clone, Copy)]
struct SplitCandidate<'a> {
    parent: Option<usize>,
    i: usize,
    bi: usize,
    bp: (usize, usize),
    c: usize,
    text: Option<SplitWord<'a>>,
}

const IGNORE_BREAK: (usize, usize) = (usize::MAX, usize::MAX);
const WORD_SPLITTER_MAX_ATTEMPTS: usize = 1000;

static NUMERIC_LITERAL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[-+]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][-+]?\d+)?$").unwrap());

/// Reusable buffers for camelCase splitting to avoid repeated heap allocations.
pub struct SplitBuffers {
    chars: Vec<char>,
    byte_offsets: Vec<usize>,
    boundaries: Vec<usize>,
}

impl Default for SplitBuffers {
    fn default() -> Self {
        Self::new()
    }
}

impl SplitBuffers {
    pub fn new() -> Self {
        Self {
            chars: Vec::new(),
            byte_offsets: Vec::new(),
            boundaries: Vec::new(),
        }
    }
}

/// Split a camelCase/PascalCase identifier into sub-words.
///
/// Handles:
/// - `camelCase` → `["camel", "Case"]`
/// - `PascalCase` → `["Pascal", "Case"]`
/// - `HTMLParser` → `["HTML", "Parser"]`
/// - `XMLHttpRequest` → `["XML", "Http", "Request"]`
/// - `URLsAndDBAs` → `["URLs", "And", "DBAs"]`  (English suffix preserved)
/// - `WALKingRUNning` → `["WALKing", "RUNning"]`  (English suffix preserved)
///
/// The algorithm mirrors cspell's `splitCamelCaseWord` with
/// `regExpCamelCaseWordBreaksWithEnglishSuffix`.
pub fn split_camel_case(word: &str) -> Vec<&str> {
    if word.is_empty() {
        return Vec::new();
    }
    if word.len() <= 1 {
        return vec![word];
    }
    // ASCII fast path: zero internal allocations
    if word.is_ascii() {
        let mut result = Vec::new();
        split_camel_case_ascii(word, &mut result);
        return result;
    }
    // Non-ASCII: use full Unicode path
    let mut result = Vec::new();
    let mut bufs = SplitBuffers::new();
    split_camel_case_into(word, &mut result, &mut bufs);
    result
}

/// Like `split_camel_case`, but reuses provided buffers to avoid allocation.
pub fn split_camel_case_into<'a>(
    word: &'a str,
    result: &mut Vec<&'a str>,
    bufs: &mut SplitBuffers,
) {
    result.clear();
    if word.is_empty() {
        return;
    }
    if word.len() <= 1 {
        result.push(word);
        return;
    }

    // ASCII fast path: avoid all internal Vec allocations.
    // For ASCII, byte index == char index, so no char/byte_offset Vecs needed.
    if word.is_ascii() {
        split_camel_case_ascii(word, result);
        return;
    }

    bufs.chars.clear();
    bufs.chars.extend(word.chars());
    let len = bufs.chars.len();
    if len <= 1 {
        result.push(word);
        return;
    }

    bufs.byte_offsets.clear();
    bufs.byte_offsets
        .extend(word.char_indices().map(|(i, _)| i));
    bufs.byte_offsets.push(word.len());

    bufs.boundaries.clear();
    bufs.boundaries.push(0);

    let mut i = 1;
    while i < len {
        let prev = bufs.chars[i - 1];
        let curr = bufs.chars[i];

        if is_cspell_lowercase_letter(prev) && is_cspell_uppercase_letter(curr) {
            bufs.boundaries.push(i);
            i += 1;
            continue;
        }

        if i >= 2
            && is_cspell_uppercase_letter(bufs.chars[i - 2])
            && is_cspell_uppercase_letter(prev)
            && is_cspell_lowercase_letter(curr)
        {
            if !is_english_suffix_at(&bufs.chars, i - 1) {
                bufs.boundaries.push(i - 1);
            }
            i += 1;
            continue;
        }

        i += 1;
    }

    bufs.boundaries.push(len);

    for pair in bufs.boundaries.windows(2) {
        let start = bufs.byte_offsets[pair[0]];
        let end = bufs.byte_offsets[pair[1]];
        let slice = &word[start..end];
        if !slice.is_empty() {
            result.push(slice);
        }
    }
}

/// Zero-allocation ASCII fast path for camelCase splitting.
/// Operates directly on bytes, no Vec<char> or Vec<usize> needed.
#[inline]
fn split_camel_case_ascii<'a>(word: &'a str, result: &mut Vec<&'a str>) {
    let bytes = word.as_bytes();
    let len = bytes.len();
    let mut start = 0;

    let mut i = 1;
    while i < len {
        let prev = bytes[i - 1];
        let curr = bytes[i];

        // Rule 1: lowercase → Uppercase
        if prev.is_ascii_lowercase() && curr.is_ascii_uppercase() {
            result.push(&word[start..i]);
            start = i;
            i += 1;
            continue;
        }

        // Rule 2: Uppercase → Uppercase+Lowercase (acronym end)
        if i >= 2
            && bytes[i - 2].is_ascii_uppercase()
            && prev.is_ascii_uppercase()
            && curr.is_ascii_lowercase()
        {
            if !is_english_suffix_at_ascii(bytes, i - 1) {
                result.push(&word[start..i - 1]);
                start = i - 1;
            }
            i += 1;
            continue;
        }

        i += 1;
    }

    if start < len {
        result.push(&word[start..]);
    }
}

/// ASCII version of `is_english_suffix_at`, operating on bytes directly.
#[inline]
fn is_english_suffix_at_ascii(bytes: &[u8], pos: usize) -> bool {
    if pos >= bytes.len() || !bytes[pos].is_ascii_uppercase() {
        return false;
    }
    let start = pos + 1;
    let rem = &bytes[start..];
    if rem.is_empty() {
        return false;
    }

    let matches_suffix = |suf: &[u8]| -> bool {
        if rem.len() < suf.len() {
            return false;
        }
        if !rem[..suf.len()]
            .iter()
            .zip(suf)
            .all(|(&c, &b)| c.to_ascii_lowercase() == b)
        {
            return false;
        }
        let after = start + suf.len();
        after >= bytes.len() || !bytes[after].is_ascii_lowercase()
    };

    // cspell:ignore nings ings ning
    matches_suffix(b"nings")
        || matches_suffix(b"ings")
        || matches_suffix(b"ning")
        || matches_suffix(b"ing")
        || matches_suffix(b"ies")
        || matches_suffix(b"es")
        || matches_suffix(b"ed")
        || matches_suffix(b"s")
}

/// Check if the split candidate at `pos` would break an English suffix.
///
/// At position `pos`, we see an uppercase letter followed by lowercase.
/// This checks: is it `Uppercase + suffix_tail` where suffix_tail is a
/// recognized English ending (s, ing, ies, es, ings, ed, ning)?
/// The suffix must NOT be followed by a lowercase letter.
///
/// This mirrors cspell's negative lookahead:
///   `(?!\p{Lu}\p{M}?(?:s|ing|ies|es|ings|ed|ning)(?!\p{Ll}))`
fn is_english_suffix_at(chars: &[char], pos: usize) -> bool {
    if pos >= chars.len() || !is_cspell_uppercase_letter(chars[pos]) {
        return false;
    }
    let start = pos + 1;
    let rem = &chars[start..];
    if rem.is_empty() {
        return false;
    }

    // Zero-allocation suffix check: compare chars directly (ASCII lowercase)
    let matches_suffix = |suf: &[u8]| -> bool {
        if rem.len() < suf.len() {
            return false;
        }
        if !rem[..suf.len()]
            .iter()
            .zip(suf)
            .all(|(c, &b)| c.to_ascii_lowercase() == b as char)
        {
            return false;
        }
        let after = start + suf.len();
        after >= chars.len() || !is_cspell_lowercase_letter(chars[after])
    };

    // cspell:ignore nings ings ning
    matches_suffix(b"nings")
        || matches_suffix(b"ings")
        || matches_suffix(b"ning")
        || matches_suffix(b"ing")
        || matches_suffix(b"ies")
        || matches_suffix(b"es")
        || matches_suffix(b"ed")
        || matches_suffix(b"s")
}

/// Extract broad "code tokens" from text.
///
/// A code token is a sequence starting with a letter, then containing
/// letters, digits, underscores, and apostrophes. This mirrors cspell's
/// `regExWordsAndDigits`: `/\p{L}\p{M}?[\p{L}\p{M}'\w-]*/gu`
///
/// Used for pre-checking whole identifiers (e.g., `LOG_AUTHPRIV`,
// cspell:ignore nturl
/// `flate2`, `nturl2path`) against dictionaries before splitting.
pub fn extract_code_tokens<'a>(text: &'a str) -> Vec<Word<'a>> {
    let mut tokens = Vec::new();
    extract_code_tokens_into(text, &mut tokens);
    tokens
}

/// Like `extract_code_tokens`, but reuses the provided Vec to avoid allocation.
pub fn extract_code_tokens_into<'a>(text: &'a str, tokens: &mut Vec<Word<'a>>) {
    tokens.clear();
    let mut chars = text.char_indices().peekable();

    while let Some(&(byte_offset, ch)) = chars.peek() {
        if is_cjk(ch) {
            chars.next();
            continue;
        }
        if is_word_char(ch) || ch == '_' {
            let start = byte_offset;
            let mut end = byte_offset + ch.len_utf8();
            chars.next();
            let mut prev_was_mark = false;

            loop {
                match chars.peek() {
                    Some(&(_, c))
                        if is_word_char(c) || c.is_ascii_digit() || c == '_' || c == '-' =>
                    {
                        end += c.len_utf8();
                        chars.next();
                        prev_was_mark = false;
                    }
                    // Allow at most one mark after a letter (cspell: \p{L}\p{M}?)
                    Some(&(_, c)) if is_combining_mark(c) && !prev_was_mark => {
                        end += c.len_utf8();
                        chars.next();
                        prev_was_mark = true;
                    }
                    Some(&(apos_offset, c))
                        if (c == '\'' || c == '\u{2019}')
                            && is_letter_after_apostrophe(text, apos_offset) =>
                    {
                        end += c.len_utf8();
                        chars.next();
                        prev_was_mark = false;
                        while let Some(&(_, c)) = chars.peek() {
                            if is_word_char(c) || c.is_ascii_digit() || c == '_' || c == '-' {
                                end += c.len_utf8();
                                chars.next();
                                prev_was_mark = false;
                            } else if is_combining_mark(c) && !prev_was_mark {
                                end += c.len_utf8();
                                chars.next();
                                prev_was_mark = true;
                            } else {
                                break;
                            }
                        }
                    }
                    _ => break,
                }
            }

            let token_text = &text[start..end];
            if !token_text.is_empty() {
                tokens.push(Word {
                    text: token_text,
                    offset: start,
                });
            }
        } else {
            chars.next();
        }
    }
}

/// Extract word tokens from text.
///
/// A word is a sequence of Unicode letters (with optional combining marks),
/// possibly containing apostrophes for contractions (e.g., `couldn't`).
/// CJK characters (Han, Hiragana, Katakana, Hangul) are skipped.
///
/// This mirrors cspell's `extractWordsFromText` using `regExWords`.
pub fn extract_words<'a>(text: &'a str) -> Vec<Word<'a>> {
    let mut words = Vec::new();
    extract_words_into(text, &mut words);
    words
}

/// Like `extract_words`, but reuses the provided Vec to avoid allocation.
pub fn extract_words_into<'a>(text: &'a str, words: &mut Vec<Word<'a>>) {
    words.clear();
    let mut chars = text.char_indices().peekable();

    while let Some(&(byte_offset, ch)) = chars.peek() {
        if is_cjk(ch) {
            chars.next();
            continue;
        }
        if is_word_char(ch) {
            let start = byte_offset;
            let mut end = byte_offset + ch.len_utf8();
            chars.next();
            let mut prev_was_mark = false;

            loop {
                match chars.peek() {
                    Some(&(_, c)) if is_word_char(c) => {
                        end += c.len_utf8();
                        chars.next();
                        prev_was_mark = false;
                    }
                    // Allow at most one mark after a letter (cspell: \p{L}\p{M}?)
                    Some(&(_, c)) if is_combining_mark(c) && !prev_was_mark => {
                        end += c.len_utf8();
                        chars.next();
                        prev_was_mark = true;
                    }
                    // Escaped apostrophe: \' treated as word-internal apostrophe
                    // (cspell's regExWords: \\?[''])
                    Some(&(_, '\\')) => {
                        let mut lookahead = chars.clone();
                        lookahead.next(); // skip '\'
                        if let Some(&(_, apos)) = lookahead.peek()
                            && (apos == '\'' || apos == '\u{2019}') {
                                lookahead.next(); // skip apostrophe
                                if let Some(&(_, after)) = lookahead.peek()
                                    && is_word_char(after) {
                                        // Include \' + subsequent letters
                                        end += 1; // backslash
                                        chars.next();
                                        end += apos.len_utf8();
                                        chars.next();
                                        prev_was_mark = false;
                                        while let Some(&(_, c)) = chars.peek() {
                                            if is_word_char(c) {
                                                end += c.len_utf8();
                                                chars.next();
                                                prev_was_mark = false;
                                            } else if is_combining_mark(c) && !prev_was_mark {
                                                end += c.len_utf8();
                                                chars.next();
                                                prev_was_mark = true;
                                            } else {
                                                break;
                                            }
                                        }
                                        continue;
                                    }
                            }
                        break;
                    }
                    Some(&(apos_offset, c))
                        if (c == '\'' || c == '\u{2019}')
                            && is_letter_after_apostrophe(text, apos_offset) =>
                    {
                        end += c.len_utf8();
                        chars.next();
                        prev_was_mark = false;
                        while let Some(&(_, c)) = chars.peek() {
                            if is_word_char(c) {
                                end += c.len_utf8();
                                chars.next();
                                prev_was_mark = false;
                            } else if is_combining_mark(c) && !prev_was_mark {
                                end += c.len_utf8();
                                chars.next();
                                prev_was_mark = true;
                            } else {
                                break;
                            }
                        }
                    }
                    _ => break,
                }
            }

            let word_text = &text[start..end];
            if !word_text.is_empty() {
                words.push(Word {
                    text: word_text,
                    offset: start,
                });
            }
        } else {
            chars.next();
        }
    }
}

/// Extract words from code: first extract word tokens, then split each by
/// camelCase boundaries.
///
/// This mirrors cspell's `extractWordsFromCode`.
pub fn extract_words_from_code<'a>(text: &'a str) -> Vec<Word<'a>> {
    let tokens = extract_words(text);
    let mut result = Vec::new();

    for token in &tokens {
        let parts = split_camel_case(token.text);
        if parts.len() <= 1 {
            result.push(*token);
        } else {
            let mut offset_in_token = 0;
            for part in parts {
                let part_start = token.text[offset_in_token..]
                    .find(part)
                    .map(|pos| offset_in_token + pos)
                    .unwrap_or(offset_in_token);
                result.push(Word {
                    text: part,
                    offset: token.offset + part_start,
                });
                offset_in_token = part_start + part.len();
            }
        }
    }

    result
}

/// Extract broad "possible words" using cspell's `regExWordsAndDigits`.
///
/// Unlike `extract_words`, this keeps punctuation like `.`, `+`, `-`, `_`, and
/// digits inside the token so the expensive splitter can consider alternate
/// breakpoints later.
pub fn extract_possible_words<'a>(text: &'a str) -> Vec<Word<'a>> {
    let mut words = Vec::new();
    extract_possible_words_into(text, &mut words);
    words
}

/// Like `extract_possible_words`, but reuses the provided Vec.
pub fn extract_possible_words_into<'a>(text: &'a str, words: &mut Vec<Word<'a>>) {
    words.clear();
    let mut offset = 0usize;

    while offset < text.len() {
        let word = find_next_word_text(text, offset);
        if word.text.is_empty() {
            break;
        }
        offset = word.offset + word.text.len();
        words.push(word);
    }
}

/// Port of cspell's `findNextWordText`.
pub fn find_next_word_text(text: &str, offset: usize) -> Word<'_> {
    let mut search_offset = offset;

    loop {
        let mut start = None;
        let iter = text[search_offset..].char_indices().peekable();

        for (rel_idx, ch) in iter {
            if is_possible_word_start_char(ch) {
                start = Some(search_offset + rel_idx);
                break;
            }
        }

        let Some(start) = start else {
            return Word {
                text: "",
                offset: text.len(),
            };
        };

        let mut end = start;
        let mut iter = text[start..].char_indices().peekable();
        while let Some((rel_idx, ch)) = iter.next() {
            let abs_idx = start + rel_idx;
            if is_possible_word_continue_char(ch) {
                end = abs_idx + ch.len_utf8();
                continue;
            }
            if ch == '\\'
                && let Some((_, next)) = iter.peek().copied()
                    && (next == '\'' || next == '\u{2019}') {
                        end = abs_idx + ch.len_utf8() + next.len_utf8();
                        iter.next();
                        continue;
                    }
            break;
        }

        let candidate = &text[start..end];
        if NUMERIC_LITERAL_RE.is_match(candidate) {
            search_offset = end;
            continue;
        }

        return Word {
            text: candidate,
            offset: start,
        };
    }
}

/// Port of cspell's expensive `wordSplitter.split(...)`.
pub fn split<'a, F>(
    line: Word<'a>,
    offset: usize,
    mut is_valid_word: F,
    optional_word_break_characters: Option<&str>,
) -> SplitResult<'a>
where
    F: FnMut(Word<'a>) -> bool,
{
    let rel = find_next_word_text(line.text, offset.saturating_sub(line.offset));
    if rel.text.is_empty() {
        let text = Word {
            text: rel.text,
            offset: line.offset + rel.offset,
        };
        return SplitResult {
            line,
            offset,
            text,
            words: Vec::new(),
            end_offset: text.offset + text.text.len(),
        };
    }

    let text = Word {
        text: rel.text,
        offset: line.offset + rel.offset,
    };
    let line_seg = SplitLineSegment {
        line: line.text,
        rel_start: rel.offset,
        rel_end: rel.offset + rel.text.len(),
    };
    let mut possible_breaks = generate_word_breaks(line_seg, optional_word_break_characters);
    if possible_breaks.is_empty() {
        let is_found = split_segment_is_valid(text.text, || {
            is_valid_word(Word {
                text: text.text,
                offset: text.offset,
            })
        });
        return SplitResult {
            line,
            offset,
            text,
            words: vec![SplitWord {
                text: text.text,
                offset: text.offset,
                is_found,
            }],
            end_offset: text.offset + text.text.len(),
        };
    }

    // Match cspell's wordSplitter.ts: append a terminal pass-through break so
    // the iterative search can finalize the trailing segment without a special
    // case at every branch.
    possible_breaks.push(PossibleWordBreak {
        offset: line_seg.rel_end,
        breaks: vec![IGNORE_BREAK],
    });
    let words = split_into_words(line, line_seg, &possible_breaks, &mut is_valid_word);

    SplitResult {
        line,
        offset,
        text,
        words,
        end_offset: line.offset + line_seg.rel_end,
    }
}

fn split_into_words<'a, F>(
    line: Word<'a>,
    line_seg: SplitLineSegment<'a>,
    breaks: &[PossibleWordBreak],
    is_valid_word: &mut F,
) -> Vec<SplitWord<'a>>
where
    F: FnMut(Word<'a>) -> bool,
{
    let max_index = line_seg.rel_end;
    let mut known_paths_by_index: HashMap<usize, usize> = HashMap::new();
    let mut path_nodes: Vec<SplitPathNode<'a>> = Vec::new();
    let mut candidates: Vec<SplitCandidate<'a>> = Vec::new();
    let mut queue: BinaryHeap<(Reverse<usize>, usize, usize, usize)> = BinaryHeap::new();
    let mut next_seq = 0usize;
    let mut best_path: Option<usize> = None;
    let mut max_cost = line_seg.rel_end - line_seg.rel_start;
    let mut attempts = 0usize;

    append_split_candidates(
        &mut candidates,
        &mut queue,
        &mut next_seq,
        breaks,
        max_index,
        None,
        line_seg.rel_start,
        0,
        0,
    );

    while max_cost > 0 && !queue.is_empty() && attempts < WORD_SPLITTER_MAX_ATTEMPTS {
        attempts += 1;

        let Some((_, _, _, best_idx)) = queue.pop() else {
            break;
        };
        let mut best = candidates[best_idx];
        if best.c >= max_cost {
            continue;
        }

        if best.bp != IGNORE_BREAK {
            let (start, end) = best.bp;
            let segment = split_checked_segment(line, best.i, start, is_valid_word);
            let extra_cost = segment
                .as_ref()
                .filter(|word| !word.is_found)
                .map(|word| word.text.len())
                .unwrap_or(0);
            best.c += extra_cost;
            best.text = segment;
            candidates[best_idx] = best;

            if let Some(&known_path) = known_paths_by_index.get(&end) {
                if let Some(path_idx) = add_to_known_paths(
                    best_idx,
                    Some(known_path),
                    &candidates,
                    &mut path_nodes,
                    &mut known_paths_by_index,
                )
                    && best_path
                        .map(|idx| path_nodes[path_idx].c < path_nodes[idx].c)
                        .unwrap_or(true)
                    {
                        best_path = Some(path_idx);
                    }
            } else if best.c < max_cost {
                append_split_candidates(
                    &mut candidates,
                    &mut queue,
                    &mut next_seq,
                    breaks,
                    max_index,
                    segment.map(|_| best_idx).or(best.parent),
                    end,
                    best.bi + 1,
                    best.c,
                );
            }
        } else {
            let appended = append_split_candidates(
                &mut candidates,
                &mut queue,
                &mut next_seq,
                breaks,
                max_index,
                best.parent,
                best.i,
                best.bi + 1,
                best.c,
            );

            if appended == 0 {
                let segment = split_checked_segment(line, best.i, max_index, is_valid_word);
                let extra_cost = segment
                    .as_ref()
                    .filter(|word| !word.is_found)
                    .map(|word| word.text.len())
                    .unwrap_or(0);
                best.c += extra_cost;
                best.text = segment;
                candidates[best_idx] = best;

                if let Some(path_idx) = segment.map(|_| best_idx).or(best.parent).and_then(|idx| {
                    add_to_known_paths(
                        idx,
                        None,
                        &candidates,
                        &mut path_nodes,
                        &mut known_paths_by_index,
                    )
                })
                    && best_path
                        .map(|idx| path_nodes[path_idx].c < path_nodes[idx].c)
                        .unwrap_or(true)
                    {
                        best_path = Some(path_idx);
                    }
            }
        }

        if let Some(path_idx) = best_path
            && path_nodes[path_idx].c < max_cost {
                max_cost = path_nodes[path_idx].c;
            }
    }

    best_path
        .map(|path_idx| path_to_words(&path_nodes, Some(path_idx)))
        .unwrap_or_default()
}

fn append_split_candidates<'a>(
    candidates: &mut Vec<SplitCandidate<'a>>,
    queue: &mut BinaryHeap<(Reverse<usize>, usize, usize, usize)>,
    next_seq: &mut usize,
    breaks: &[PossibleWordBreak],
    max_index: usize,
    parent: Option<usize>,
    i: usize,
    mut bi: usize,
    current_cost: usize,
) -> usize {
    while bi < breaks.len() && breaks[bi].offset < i {
        bi += 1;
    }

    if bi >= breaks.len() {
        return 0;
    }

    let mut appended = 0usize;
    for &bp in &breaks[bi].breaks {
        let delta2 = if bp == IGNORE_BREAK {
            (max_index - i).saturating_mul(2)
        } else {
            (bp.0 - i) + (max_index - bp.1).saturating_mul(2)
        };
        let ec2 = current_cost.saturating_mul(2) + delta2;
        let idx = candidates.len();
        candidates.push(SplitCandidate {
            parent,
            i,
            bi,
            bp,
            c: current_cost,
            text: None,
        });
        queue.push((Reverse(ec2), i, *next_seq, idx));
        *next_seq += 1;
        appended += 1;
    }

    appended
}

fn add_to_known_paths<'a>(
    candidate_idx: usize,
    mut path_idx: Option<usize>,
    candidates: &[SplitCandidate<'a>],
    path_nodes: &mut Vec<SplitPathNode<'a>>,
    known_paths_by_index: &mut HashMap<usize, usize>,
) -> Option<usize> {
    let mut current = Some(candidate_idx);
    while let Some(idx) = current {
        let candidate = candidates[idx];
        let text = candidate.text;
        let cost = text
            .as_ref()
            .filter(|word| !word.is_found)
            .map(|word| word.text.len())
            .unwrap_or(0)
            + path_idx.map(|p| path_nodes[p].c).unwrap_or(0);

        if let Some(&existing) = known_paths_by_index.get(&candidate.i)
            && path_nodes[existing].c <= cost {
                return None;
            }

        let node_idx = path_nodes.len();
        path_nodes.push(SplitPathNode {
            next: path_idx,
            c: cost,
            text,
        });
        known_paths_by_index.insert(candidate.i, node_idx);
        path_idx = Some(node_idx);
        current = candidate.parent;
    }

    path_idx
}

fn path_to_words<'a>(
    path_nodes: &[SplitPathNode<'a>],
    mut path_idx: Option<usize>,
) -> Vec<SplitWord<'a>> {
    let mut words = Vec::new();
    while let Some(idx) = path_idx {
        let node = path_nodes[idx];
        if let Some(word) = node.text {
            words.push(word);
        }
        path_idx = node.next;
    }

    words
}

fn split_checked_segment<'a, F>(
    line: Word<'a>,
    start: usize,
    end: usize,
    is_valid_word: &mut F,
) -> Option<SplitWord<'a>>
where
    F: FnMut(Word<'a>) -> bool,
{
    let text = &line.text[start..end];
    if text.is_empty() {
        return None;
    }
    let offset = line.offset + start;
    let is_found = split_segment_is_valid(text, || is_valid_word(Word { text, offset }));
    Some(SplitWord {
        text,
        offset,
        is_found,
    })
}

fn split_segment_is_valid(text: &str, validate: impl FnOnce() -> bool) -> bool {
    if text.chars().all(|c| {
        matches!(
            c,
            '-' | '.' | '+' | '_' | '\'' | '\u{2019}' | '`' | '\\' | ' ' | '\t' | '\r' | '\n'
        ) || c.is_ascii_digit()
            || c == 'e'
            || c == 'E'
    }) {
        return true;
    }
    validate()
}

fn generate_word_breaks(
    line: SplitLineSegment<'_>,
    optional_word_break_characters: Option<&str>,
) -> Vec<PossibleWordBreak> {
    let mut breaks = Vec::new();
    breaks.extend(gen_word_break_camel(line));
    breaks.extend(gen_symbol_breaks(line));
    breaks.extend(gen_optional_word_breaks(
        line,
        optional_word_break_characters,
    ));
    // cspell uses JS Array.prototype.sort, which is stable in modern runtimes.
    // Preserve insertion order for equal offsets so candidate exploration order matches.
    breaks.sort_by_key(|b| b.offset);
    breaks
}

fn gen_word_break_camel(line: SplitLineSegment<'_>) -> Vec<PossibleWordBreak> {
    let chars: Vec<(usize, char)> = line.line[line.rel_start..line.rel_end]
        .char_indices()
        .map(|(i, ch)| (line.rel_start + i, ch))
        .collect();
    let mut breaks = Vec::new();

    for window in chars.windows(2) {
        let [(prev_pos, prev), (curr_pos, curr)] = window else {
            continue;
        };
        if is_cspell_lowercase_letter(*prev) && is_cspell_uppercase_letter(*curr) {
            breaks.push(PossibleWordBreak {
                offset: *prev_pos,
                breaks: vec![(*curr_pos, *curr_pos), IGNORE_BREAK],
            });
        }
    }

    for window in chars.windows(3) {
        let [(a_pos, a), (b_pos, b), (c_pos, c)] = window else {
            continue;
        };
        if is_cspell_uppercase_letter(*a)
            && is_cspell_uppercase_letter(*b)
            && is_cspell_lowercase_letter(*c)
        {
            breaks.push(PossibleWordBreak {
                offset: *a_pos,
                breaks: vec![(*b_pos, *b_pos), (*c_pos, *c_pos), IGNORE_BREAK],
            });
        }
    }

    breaks
}

fn gen_symbol_breaks(line: SplitLineSegment<'_>) -> Vec<PossibleWordBreak> {
    let mut symbol_breaks = Vec::new();
    let mut digit_breaks = Vec::new();
    let mut escape_breaks = Vec::new();
    let bytes = line.line.as_bytes();
    let mut i = line.rel_start;

    while i < line.rel_end {
        let ch = line.line[i..].chars().next().unwrap();
        let ch_len = ch.len_utf8();

        if matches!(
            ch,
            '-' | '+' | '_' | '\'' | '\u{2019}' | '`' | '.' | ' ' | '\t' | '\r' | '\n'
        ) {
            symbol_breaks.push(symbol_break(i, i + ch_len));
            i += ch_len;
            continue;
        }

        if ch.is_ascii_digit() {
            let start = i;
            i += ch_len;
            while i < line.rel_end && bytes[i].is_ascii_digit() {
                i += 1;
            }
            digit_breaks.push(symbol_break(start, i));
            continue;
        }

        if matches!(
            ch,
            'a' | 'n' | 'r' | 'v' | 't' | 'b' | 'f' | 'A' | 'N' | 'R' | 'V' | 'T' | 'B' | 'F'
        ) && i > 0
            && bytes[i - 1] == b'\\'
        {
            escape_breaks.push(symbol_break(i, i + ch_len));
        }

        i += ch_len;
    }

    symbol_breaks.extend(digit_breaks);
    symbol_breaks.extend(escape_breaks);
    symbol_breaks
}

fn gen_optional_word_breaks(
    line: SplitLineSegment<'_>,
    optional_word_break_characters: Option<&str>,
) -> Vec<PossibleWordBreak> {
    let mut breaks = Vec::new();
    let mut i = line.rel_start;

    while i < line.rel_end {
        let ch = line.line[i..].chars().next().unwrap();
        let ch_len = ch.len_utf8();

        if ch == '\'' && is_dangling_quote(line.line, i, line.rel_start) {
            breaks.push(optional_break(i, i + ch_len));
        }

        if let Some(end) = trailing_ending_end(line.line, i, line.rel_end) {
            breaks.push(optional_break(i, end));
        }

        if optional_word_break_characters.is_some_and(|chars| chars.contains(ch)) {
            breaks.push(optional_break(i, i + ch_len));
        }

        i += ch_len;
    }

    breaks
}

fn symbol_break(start: usize, end: usize) -> PossibleWordBreak {
    PossibleWordBreak {
        offset: start,
        breaks: vec![(start, end), (start, start), (end, end), IGNORE_BREAK],
    }
}

fn optional_break(start: usize, end: usize) -> PossibleWordBreak {
    PossibleWordBreak {
        offset: start,
        breaks: vec![(start, end), IGNORE_BREAK],
    }
}

fn is_dangling_quote(text: &str, quote_pos: usize, rel_start: usize) -> bool {
    if quote_pos == rel_start {
        return true;
    }

    let prefix = &text[rel_start..quote_pos];
    let mut chars = prefix.char_indices().collect::<Vec<_>>();
    if chars.is_empty() {
        return true;
    }

    let (_, last) = chars.pop().unwrap();
    if is_cspell_letter(last) || is_combining_mark(last) {
        let prev_is_boundary = chars
            .last()
            .map(|(_, ch)| !is_cspell_letter(*ch) && !is_combining_mark(*ch))
            .unwrap_or(true);
        return prev_is_boundary;
    }

    true
}

fn trailing_ending_end(text: &str, start: usize, rel_end: usize) -> Option<usize> {
    let suffixes = [
        "nth", "ning", "ings", "ies", "ing", "ed", "es", "th", "s", "d",
    ];

    for suffix in suffixes {
        if !text[start..rel_end].starts_with(suffix)
            && !text[start..rel_end].starts_with(&format!("'{}", suffix))
            && !text[start..rel_end].starts_with(&format!("\u{2019}{}", suffix))
        {
            continue;
        }

        let matched = if text[start..rel_end].starts_with(suffix) {
            suffix.len()
        } else if text[start..rel_end].starts_with(&format!("'{}", suffix)) {
            suffix.len() + 1
        } else {
            suffix.len() + '\u{2019}'.len_utf8()
        };

        let prefix = &text[..start];
        let mut upper_count = 0usize;
        for ch in prefix.chars().rev() {
            if is_cspell_uppercase_letter(ch) || is_combining_mark(ch) {
                if is_cspell_uppercase_letter(ch) {
                    upper_count += 1;
                }
                continue;
            }
            break;
        }
        if upper_count < 2 {
            continue;
        }

        let end = start + matched;
        if text[end..]
            .chars()
            .next()
            .is_some_and(is_cspell_lowercase_letter)
        {
            continue;
        }
        return Some(end);
    }

    None
}

fn is_possible_word_start_char(ch: char) -> bool {
    is_possible_letter(ch)
        || ch.is_ascii_digit()
        || ch == '_'
        || matches!(ch, '\'' | '\u{2019}' | '`' | '.' | '+' | '-')
}

fn is_possible_word_continue_char(ch: char) -> bool {
    is_possible_letter(ch)
        || is_combining_mark(ch)
        || ch.is_ascii_digit()
        || ch == '_'
        || matches!(ch, '\'' | '\u{2019}' | '`' | '.' | '+' | '-')
}

fn is_possible_letter(ch: char) -> bool {
    is_cspell_letter(ch)
}

/// Matches cspell's `\p{L}` — Unicode Letter, excluding CJK and Mark characters.
/// Rust's `is_alphabetic()` includes characters with the `Other_Alphabetic` property
/// (e.g. Sinhala vowel signs, Devanagari marks) which cspell's `\p{L}` excludes.
fn is_word_char(ch: char) -> bool {
    is_cspell_letter(ch) && !is_cjk(ch)
}

#[inline]
pub(crate) fn is_cspell_letter(ch: char) -> bool {
    matches!(
        get_general_category(ch),
        GeneralCategory::UppercaseLetter
            | GeneralCategory::LowercaseLetter
            | GeneralCategory::TitlecaseLetter
            | GeneralCategory::ModifierLetter
            | GeneralCategory::OtherLetter
    )
}

#[inline]
pub(crate) fn is_cspell_uppercase_letter(ch: char) -> bool {
    matches!(get_general_category(ch), GeneralCategory::UppercaseLetter)
}

#[inline]
pub(crate) fn is_cspell_lowercase_letter(ch: char) -> bool {
    matches!(get_general_category(ch), GeneralCategory::LowercaseLetter)
}

/// Unicode General_Category = Mark (Mn + Mc + Me).
/// These are combining/spacing marks in Brahmic scripts (Devanagari, Sinhala, Tamil, etc.)
/// and other writing systems.  cspell allows at most one optional `\p{M}` after each `\p{L}`,
/// but does NOT count marks as word-starting characters.
fn is_unicode_mark(ch: char) -> bool {
    use std::ops::RangeInclusive;
    const MARK_RANGES: &[RangeInclusive<u32>] = &[
        0x0300..=0x036F, // Combining Diacritical Marks (Mn)
        0x0483..=0x0489, // Cyrillic combining marks
        0x0591..=0x05BD, // Hebrew marks
        0x05BF..=0x05BF,
        0x05C1..=0x05C2,
        0x05C4..=0x05C5,
        0x05C7..=0x05C7,
        0x0610..=0x061A, // Arabic marks
        0x064B..=0x065F,
        0x0670..=0x0670,
        0x06D6..=0x06DC,
        0x06DF..=0x06E4,
        0x06E7..=0x06E8,
        0x06EA..=0x06ED,
        0x0711..=0x0711, // Syriac
        0x0730..=0x074A,
        0x07A6..=0x07B0, // Thaana
        0x07EB..=0x07F3, // NKo
        0x07FD..=0x07FD,
        0x0816..=0x0819, // Samaritan
        0x081B..=0x0823,
        0x0825..=0x0827,
        0x0829..=0x082D,
        0x0859..=0x085B, // Mandaic
        0x0898..=0x089F, // Arabic Extended-B
        0x08CA..=0x08E1,
        0x08E3..=0x0903, // Arabic/Devanagari
        0x093A..=0x093C, // Devanagari
        0x093E..=0x094F,
        0x0951..=0x0957,
        0x0962..=0x0963,
        0x0981..=0x0983, // Bengali
        0x09BC..=0x09BC,
        0x09BE..=0x09C4,
        0x09C7..=0x09C8,
        0x09CB..=0x09CD,
        0x09D7..=0x09D7,
        0x09E2..=0x09E3,
        0x09FE..=0x09FE,
        0x0A01..=0x0A03, // Gurmukhi
        0x0A3C..=0x0A3C,
        0x0A3E..=0x0A42,
        0x0A47..=0x0A48,
        0x0A4B..=0x0A4D,
        0x0A51..=0x0A51,
        0x0A70..=0x0A71,
        0x0A75..=0x0A75,
        0x0A81..=0x0A83, // Gujarati
        0x0ABC..=0x0ABC,
        0x0ABE..=0x0AC5,
        0x0AC7..=0x0AC9,
        0x0ACB..=0x0ACD,
        0x0AE2..=0x0AE3,
        0x0AFA..=0x0AFF,
        0x0B01..=0x0B03, // Oriya
        0x0B3C..=0x0B3C,
        0x0B3E..=0x0B44,
        0x0B47..=0x0B48,
        0x0B4B..=0x0B4D,
        0x0B55..=0x0B57,
        0x0B62..=0x0B63,
        0x0B82..=0x0B82, // Tamil
        0x0BBE..=0x0BC2,
        0x0BC6..=0x0BC8,
        0x0BCA..=0x0BCD,
        0x0BD7..=0x0BD7,
        0x0C00..=0x0C04, // Telugu
        0x0C3C..=0x0C3C,
        0x0C3E..=0x0C44,
        0x0C46..=0x0C48,
        0x0C4A..=0x0C4D,
        0x0C55..=0x0C56,
        0x0C62..=0x0C63,
        0x0C81..=0x0C83, // Kannada
        0x0CBC..=0x0CBC,
        0x0CBE..=0x0CC4,
        0x0CC6..=0x0CC8,
        0x0CCA..=0x0CCD,
        0x0CD5..=0x0CD6,
        0x0CE2..=0x0CE3,
        0x0CF3..=0x0CF3,
        0x0D00..=0x0D03, // Malayalam
        0x0D3B..=0x0D3C,
        0x0D3E..=0x0D44,
        0x0D46..=0x0D48,
        0x0D4A..=0x0D4D,
        0x0D57..=0x0D57,
        0x0D62..=0x0D63,
        0x0D81..=0x0D83, // Sinhala
        0x0DCA..=0x0DCA,
        0x0DCF..=0x0DD4,
        0x0DD6..=0x0DD6,
        0x0DD8..=0x0DDF,
        0x0DF2..=0x0DF3,
        0x0E31..=0x0E31, // Thai
        0x0E34..=0x0E3A,
        0x0E47..=0x0E4E,
        0x0EB1..=0x0EB1, // Lao
        0x0EB4..=0x0EBC,
        0x0EC8..=0x0ECE,
        0x0F18..=0x0F19, // Tibetan
        0x0F35..=0x0F35,
        0x0F37..=0x0F37,
        0x0F39..=0x0F39,
        0x0F3E..=0x0F3F,
        0x0F71..=0x0F84,
        0x0F86..=0x0F87,
        0x0F8D..=0x0F97,
        0x0F99..=0x0FBC,
        0x0FC6..=0x0FC6,
        0x102B..=0x103E, // Myanmar
        0x1056..=0x1059,
        0x105E..=0x1060,
        0x1062..=0x1064,
        0x1067..=0x106D,
        0x1071..=0x1074,
        0x1082..=0x108D,
        0x108F..=0x108F,
        0x109A..=0x109D,
        0x135D..=0x135F, // Ethiopic
        0x1712..=0x1715, // Tagalog, etc.
        0x1732..=0x1734,
        0x1752..=0x1753,
        0x1772..=0x1773,
        0x17B4..=0x17D3, // Khmer
        0x17DD..=0x17DD,
        0x180B..=0x180D, // Mongolian
        0x180F..=0x180F,
        0x1885..=0x1886,
        0x18A9..=0x18A9,
        0x1920..=0x192B, // Limbu, etc.
        0x1930..=0x193B,
        0x1A17..=0x1A1B, // Buginese, Tai Tham
        0x1A55..=0x1A5E,
        0x1A60..=0x1A7C,
        0x1A7F..=0x1A7F,
        0x1AB0..=0x1ACE, // Combining Diacritical Marks Extended
        0x1B00..=0x1B04, // Balinese
        0x1B34..=0x1B44,
        0x1B6B..=0x1B73,
        0x1B80..=0x1B82, // Sundanese
        0x1BA1..=0x1BAD,
        0x1BE6..=0x1BF3, // Batak
        0x1C24..=0x1C37, // Lepcha
        0x1CD0..=0x1CD2, // Vedic Extensions
        0x1CD4..=0x1CE8,
        0x1CED..=0x1CED,
        0x1CF4..=0x1CF4,
        0x1CF7..=0x1CF9,
        0x1DC0..=0x1DFF, // Combining Diacritical Marks Supplement
        0x20D0..=0x20F0, // Combining Diacritical Marks for Symbols
        0x2CEF..=0x2CF1, // Coptic
        0x2D7F..=0x2D7F, // Tifinagh
        0x2DE0..=0x2DFF, // Cyrillic Extended-A
        0xA66F..=0xA672, // Cyrillic Extended-B
        0xA674..=0xA67D,
        0xA69E..=0xA69F,
        0xA6F0..=0xA6F1, // Bamum
        0xA802..=0xA802, // Syloti Nagri
        0xA806..=0xA806,
        0xA80B..=0xA80B,
        0xA823..=0xA827,
        0xA82C..=0xA82C,
        0xA880..=0xA881, // Saurashtra
        0xA8B4..=0xA8C5,
        0xA8E0..=0xA8F1, // Devanagari Extended
        0xA8FF..=0xA8FF,
        0xA926..=0xA92D, // Kayah Li
        0xA947..=0xA953, // Rejang
        0xA980..=0xA983, // Javanese
        0xA9B3..=0xA9C0,
        0xA9E5..=0xA9E5,
        0xAA29..=0xAA36, // Cham
        0xAA43..=0xAA43,
        0xAA4C..=0xAA4D,
        0xAA7B..=0xAA7D,
        0xAAB0..=0xAAB0, // Tai Viet
        0xAAB2..=0xAAB4,
        0xAAB7..=0xAAB8,
        0xAABE..=0xAABF,
        0xAAC1..=0xAAC1,
        0xAAEB..=0xAAEF, // Meetei Mayek
        0xAAF5..=0xAAF6,
        0xABE3..=0xABEA,
        0xABEC..=0xABED,
        0xFB1E..=0xFB1E, // Hebrew
        0xFE00..=0xFE0F, // Variation Selectors
        0xFE20..=0xFE2F, // Combining Half Marks
    ];
    let cp = ch as u32;
    MARK_RANGES.iter().any(|r| r.contains(&cp))
}

/// Combining marks for word continuation.
/// cspell's `\p{L}\p{M}?` allows any Unicode Mark to follow a Letter.
/// We use the full `is_unicode_mark` set so that marks from ALL scripts
/// (Arabic, Devanagari, Sinhala, etc.) can continue words, while marks
/// cannot START words (excluded from `is_word_char`).
fn is_combining_mark(ch: char) -> bool {
    is_unicode_mark(ch)
}

fn is_cjk(ch: char) -> bool {
    let cp = ch as u32;
    // Han (CJK Unified Ideographs + extensions)
    (0x4E00..=0x9FFF).contains(&cp)
        || (0x3400..=0x4DBF).contains(&cp)
        || (0x20000..=0x2A6DF).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        // Hiragana
        || (0x3040..=0x309F).contains(&cp)
        // Katakana
        || (0x30A0..=0x30FF).contains(&cp)
        || (0x31F0..=0x31FF).contains(&cp)
        // Hangul
        || (0xAC00..=0xD7AF).contains(&cp)
        || (0x1100..=0x11FF).contains(&cp)
        || (0x3130..=0x318F).contains(&cp)
}

/// Check if there's a letter immediately after an apostrophe position,
/// which indicates a contraction (not a trailing quote).
fn is_letter_after_apostrophe(text: &str, apos_byte_offset: usize) -> bool {
    let after_apos = apos_byte_offset
        + text[apos_byte_offset..]
            .chars()
            .next()
            .map_or(0, |c| c.len_utf8());
    text[after_apos..].chars().next().is_some_and(is_word_char)
}

// cspell:disable
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // ---- split_camel_case tests (from cspell text.test.ts) ----

    #[test]
    fn test_split_simple() {
        assert_eq!(split_camel_case("hello"), vec!["hello"]);
    }

    #[test]
    fn test_split_camel_case_basic() {
        assert_eq!(split_camel_case("helloThere"), vec!["hello", "There"]);
    }

    #[test]
    fn test_split_pascal_case() {
        assert_eq!(split_camel_case("HelloThere"), vec!["Hello", "There"]);
    }

    #[test]
    fn test_split_camel_case_keeps_modifier_letters_like_cspell() {
        assert_eq!(split_camel_case("XᵀWX"), vec!["XᵀWX"]);
        assert_eq!(split_camel_case("UΣVᵀ"), vec!["UΣVᵀ"]);
    }

    #[test]
    fn test_split_acronym_to_word() {
        assert_eq!(
            split_camel_case("ASCIIToUTF16"),
            vec!["ASCII", "To", "UTF16"]
        );
    }

    #[test]
    fn test_split_urls_and_dbas() {
        assert_eq!(split_camel_case("URLsAndDBAs"), vec!["URLs", "And", "DBAs"]);
    }

    #[test]
    fn test_split_walking_running() {
        assert_eq!(
            split_camel_case("WALKingRUNning"),
            vec!["WALKing", "RUNning"]
        );
    }

    #[test]
    fn test_split_with_digits() {
        assert_eq!(split_camel_case("c0de"), vec!["c0de"]);
    }

    #[test]
    fn test_split_xml_http_request() {
        assert_eq!(
            split_camel_case("XMLHttpRequest"),
            vec!["XML", "Http", "Request"]
        );
    }

    #[test]
    fn test_split_html_parser() {
        assert_eq!(split_camel_case("HTMLParser"), vec!["HTML", "Parser"]);
    }

    #[test]
    fn test_split_html_input() {
        assert_eq!(split_camel_case("HTMLInput"), vec!["HTML", "Input"]);
    }

    #[test]
    fn test_split_get_url() {
        assert_eq!(split_camel_case("getURL"), vec!["get", "URL"]);
    }

    #[test]
    fn test_split_single_char() {
        assert_eq!(split_camel_case("a"), vec!["a"]);
        assert_eq!(split_camel_case("A"), vec!["A"]);
    }

    #[test]
    fn test_split_empty() {
        let empty: Vec<&str> = Vec::new();
        assert_eq!(split_camel_case(""), empty);
    }

    #[test]
    fn test_split_all_upper() {
        assert_eq!(split_camel_case("HTML"), vec!["HTML"]);
        assert_eq!(split_camel_case("API"), vec!["API"]);
    }

    #[test]
    fn test_split_a_hello() {
        assert_eq!(split_camel_case("aHELLO"), vec!["a", "HELLO"]);
    }

    #[test]
    fn test_split_error_code() {
        assert_eq!(split_camel_case("ERRORCode"), vec!["ERROR", "Code"]);
    }

    // ---- extract_words tests ----

    #[test]
    fn test_extract_simple_words() {
        let words = extract_words("hello world");
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "hello");
        assert_eq!(words[0].offset, 0);
        assert_eq!(words[1].text, "world");
        assert_eq!(words[1].offset, 6);
    }

    #[test]
    fn test_extract_with_punctuation() {
        let words = extract_words("hello, world!");
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "hello");
        assert_eq!(words[1].text, "world");
    }

    #[test]
    fn test_extract_contraction() {
        let words = extract_words("couldn't've");
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "couldn't've");
    }

    #[test]
    fn test_extract_skips_cjk() {
        let words = extract_words("携程旅行网 hello world");
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "hello");
        assert_eq!(words[1].text, "world");
    }

    #[test]
    fn test_extract_keeps_greek() {
        let words = extract_words("γάμμα test");
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "γάμμα");
        assert_eq!(words[1].text, "test");
    }

    #[test]
    fn test_extract_snake_case() {
        // snake_case is split by extract_words into separate tokens
        // because _ is not a word char
        let words = extract_words("first_line");
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "first");
        assert_eq!(words[1].text, "line");
    }

    #[test]
    fn test_extract_dot_separated() {
        let words = extract_words("regExp.match");
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "regExp");
        assert_eq!(words[1].text, "match");
    }

    // ---- extract_words_from_code tests ----

    #[test]
    fn test_code_camel_case() {
        let words = extract_words_from_code("splitCamelCaseWord");
        let texts: Vec<&str> = words.iter().map(|w| w.text).collect();
        assert_eq!(texts, vec!["split", "Camel", "Case", "Word"]);
    }

    #[test]
    fn test_code_expression() {
        let words = extract_words_from_code("regExp.match(first_line)");
        let texts: Vec<&str> = words.iter().map(|w| w.text).collect();
        assert_eq!(texts, vec!["reg", "Exp", "match", "first", "line"]);
    }

    #[test]
    fn test_code_a_hello() {
        let words = extract_words_from_code("aHELLO");
        let texts: Vec<&str> = words.iter().map(|w| w.text).collect();
        assert_eq!(texts, vec!["a", "HELLO"]);
    }

    #[test]
    fn test_code_html_input() {
        let words = extract_words_from_code("HTMLInput.value");
        let texts: Vec<&str> = words.iter().map(|w| w.text).collect();
        assert_eq!(texts, vec!["HTML", "Input", "value"]);
    }

    // ---- escaped apostrophe tests (cspell regExWords: \\?['']) ----

    #[test]
    fn test_extract_escaped_apostrophe_contraction() {
        // doesn\'t should be extracted as one word "doesn\'t"
        let words = extract_words(r"doesn\'t");
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, r"doesn\'t");
    }

    #[test]
    fn test_extract_escaped_apostrophe_in_string() {
        // 'it doesn\'t work' — the doesn\'t should be one token
        let words = extract_words(r"it doesn\'t work");
        assert_eq!(words.len(), 3);
        assert_eq!(words[0].text, "it");
        assert_eq!(words[1].text, r"doesn\'t");
        assert_eq!(words[2].text, "work");
    }

    #[test]
    fn test_extract_backslash_not_before_apostrophe() {
        // Backslash not followed by apostrophe should still break
        let words = extract_words(r"hello\nworld");
        // \n breaks the word — 'hello', then 'nworld' (n is a letter)
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "hello");
    }

    #[test]
    fn test_extract_trailing_backslash_apostrophe() {
        // Trailing \' without letter after — should not include
        let words = extract_words("word\\'");
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "word");
    }

    #[test]
    fn find_next_word_text_skips_long_numeric_sequences_without_recursing() {
        let mut text = String::new();
        for i in 0..20_000 {
            if i > 0 {
                text.push_str(", ");
            }
            text.push_str("19058");
        }
        text.push_str(", TRANSITION");

        let word = find_next_word_text(&text, 0);
        assert_eq!(
            word,
            Word {
                text: "TRANSITION",
                offset: text.len() - "TRANSITION".len(),
            }
        );
    }

    #[test]
    fn word_splitter_expensive_case_matches_cspell() {
        // cspell:disable-next-line
        let random_text =
            r#"token = "Usf3uVQOZ9m6uPfVonKR-EBXjPe7bjMbp3_Fq8MfsptgkkM1ojidN0BxYaT5HAEN1";"#;
        let dictionary = sample_word_set();
        let line = Word {
            text: random_text,
            offset: 0,
        };

        let result = split(line, 9, |word| sample_has(&dictionary, word), None);
        let words = result
            .words
            .iter()
            .map(|word| word.text)
            .collect::<Vec<_>>()
            .join("|");
        let unknown = result.words.iter().filter(|word| !word.is_found).count();

        assert_eq!(
            words,
            "Usf|u|VQOZ|m|u|Pf|Von|KR|EB|Xj|Pe|bj|Mbp|Fq|Mfsptgkk|M|ojid|N|Bx|Ya|T|HAEN"
        );
        assert_eq!(unknown, 7);
    }

    #[test]
    fn word_splitter_keeps_lua_metamethod_after_dot() {
        let dictionary = HashSet::from_iter(["configs".to_string(), "__newindex".to_string()]);
        let line = Word {
            text: "configs.__newindex",
            offset: 0,
        };

        let result = split(line, 0, |word| sample_has(&dictionary, word), None);
        let words: Vec<(&str, bool)> = result
            .words
            .iter()
            .map(|word| (word.text, word.is_found))
            .collect();

        assert_eq!(words, vec![("configs", true), ("__newindex", true)]);
    }

    fn sample_has(dictionary: &HashSet<String>, word: Word<'_>) -> bool {
        let text = word.text;
        text.chars().count() < 3
            || !text.chars().any(|c| c.is_alphabetic())
            || dictionary.contains(text)
            || dictionary.contains(&text.to_lowercase())
    }

    fn sample_word_set() -> HashSet<String> {
        [
            ".tensor",
            "torch",
            "'twas",
            "_errorcode42",
            "2SD",
            "begin",
            "+end",
            "64-bit",
            "bit",
            "checksum",
            "camel",
            "case",
            "can't",
            "code42",
            "const",
            "count",
            "cpp",
            "CVTPD2PS",
            "CVTTSD",
            "echo",
            "iphone",
            "ephone",
            "Geschaft",
            "error",
            "codes",
            "hello",
            "MOVSX_r_rm16",
            "one",
            "two",
            "static",
            "these",
            "are",
            "some",
            "sample",
            "words",
            "Tom's",
            "hardware",
            "well",
            "educated",
            "separated",
            "by",
            "singleQuote",
            "256-sha",
            "dogs'",
            "leashes",
            "writers",
            "planets’",
        ]
        .into_iter()
        .map(ToOwned::to_owned)
        .collect()
    }
}
