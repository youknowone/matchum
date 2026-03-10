// spell-checker:ignore AUTHPRIV
/// A word extracted from text, with its byte offset in the source.
///
/// Borrows the text from the input slice to avoid heap allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Word<'a> {
    pub text: &'a str,
    pub offset: usize,
}

/// Reusable buffers for camelCase splitting to avoid repeated heap allocations.
pub struct SplitBuffers {
    chars: Vec<char>,
    byte_offsets: Vec<usize>,
    boundaries: Vec<usize>,
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

        if prev.is_lowercase() && curr.is_uppercase() {
            bufs.boundaries.push(i);
            i += 1;
            continue;
        }

        if i >= 2 && bufs.chars[i - 2].is_uppercase() && prev.is_uppercase() && curr.is_lowercase()
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
    if pos >= chars.len() || !chars[pos].is_uppercase() {
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
        after >= chars.len() || !chars[after].is_lowercase()
    };

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
        if is_word_char(ch) {
            let start = byte_offset;
            let mut end = byte_offset + ch.len_utf8();
            chars.next();

            loop {
                match chars.peek() {
                    Some(&(_, c))
                        if is_word_char(c)
                            || is_combining_mark(c)
                            || c.is_ascii_digit()
                            || c == '_'
                            || c == '-' =>
                    {
                        end += c.len_utf8();
                        chars.next();
                    }
                    Some(&(apos_offset, c))
                        if (c == '\'' || c == '\u{2019}')
                            && is_letter_after_apostrophe(text, apos_offset) =>
                    {
                        end += c.len_utf8();
                        chars.next();
                        while let Some(&(_, c)) = chars.peek() {
                            if is_word_char(c)
                                || is_combining_mark(c)
                                || c.is_ascii_digit()
                                || c == '_'
                                || c == '-'
                            {
                                end += c.len_utf8();
                                chars.next();
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

            loop {
                match chars.peek() {
                    Some(&(_, c)) if is_word_char(c) || is_combining_mark(c) => {
                        end += c.len_utf8();
                        chars.next();
                    }
                    // Escaped apostrophe: \' treated as word-internal apostrophe
                    // (cspell's regExWords: \\?[''])
                    Some(&(_, '\\')) => {
                        let mut lookahead = chars.clone();
                        lookahead.next(); // skip '\'
                        if let Some(&(_, apos)) = lookahead.peek() {
                            if apos == '\'' || apos == '\u{2019}' {
                                lookahead.next(); // skip apostrophe
                                if let Some(&(_, after)) = lookahead.peek() {
                                    if after.is_alphabetic() {
                                        // Include \' + subsequent letters
                                        end += 1; // backslash
                                        chars.next();
                                        end += apos.len_utf8();
                                        chars.next();
                                        while let Some(&(_, c)) = chars.peek() {
                                            if is_word_char(c) || is_combining_mark(c) {
                                                end += c.len_utf8();
                                                chars.next();
                                            } else {
                                                break;
                                            }
                                        }
                                        continue;
                                    }
                                }
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
                        while let Some(&(_, c)) = chars.peek() {
                            if is_word_char(c) || is_combining_mark(c) {
                                end += c.len_utf8();
                                chars.next();
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

fn is_word_char(ch: char) -> bool {
    ch.is_alphabetic() && !is_cjk(ch)
}

fn is_combining_mark(ch: char) -> bool {
    use std::ops::RangeInclusive;
    const COMBINING_RANGES: &[RangeInclusive<u32>] = &[
        0x0300..=0x036F, // Combining Diacritical Marks
        0x1AB0..=0x1AFF, // Combining Diacritical Marks Extended
        0x1DC0..=0x1DFF, // Combining Diacritical Marks Supplement
        0x20D0..=0x20FF, // Combining Diacritical Marks for Symbols
        0xFE20..=0xFE2F, // Combining Half Marks
    ];
    let cp = ch as u32;
    COMBINING_RANGES.iter().any(|r| r.contains(&cp))
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
    text[after_apos..]
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
