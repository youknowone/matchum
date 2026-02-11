/// A word extracted from text, with its byte offset in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub text: String,
    pub offset: usize,
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

    let chars: Vec<char> = word.chars().collect();
    let len = chars.len();
    if len <= 1 {
        return vec![word];
    }

    // Collect byte offsets for each char index
    let byte_offsets: Vec<usize> = word
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(word.len()))
        .collect();

    let mut boundaries: Vec<usize> = vec![0]; // char indices of split points

    let mut i = 1;
    while i < len {
        let prev = chars[i - 1];
        let curr = chars[i];

        // Rule 1: lowercase → Uppercase
        if prev.is_lowercase() && curr.is_uppercase() {
            boundaries.push(i);
            i += 1;
            continue;
        }

        // Rule 2: Uppercase → Uppercase+Lowercase (acronym end)
        // e.g., "HTMLParser" → split before 'P': "HTML" | "Parser"
        // But preserve English suffixes: "URLs" stays "URLs", "WALKing" stays "WALKing"
        if i >= 2 && chars[i - 2].is_uppercase() && prev.is_uppercase() && curr.is_lowercase() {
            // Check if this is an English suffix that should stay attached
            if !is_english_suffix_at(&chars, i - 1) {
                boundaries.push(i - 1);
            }
            i += 1;
            continue;
        }

        i += 1;
    }

    boundaries.push(len);

    boundaries
        .windows(2)
        .filter_map(|pair| {
            let start = byte_offsets[pair[0]];
            let end = byte_offsets[pair[1]];
            let slice = &word[start..end];
            if slice.is_empty() {
                None
            } else {
                Some(slice)
            }
        })
        .collect()
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
    // pos should be an uppercase letter (the candidate split point)
    if pos >= chars.len() || !chars[pos].is_uppercase() {
        return false;
    }

    // Look at chars[pos+1..] to see if it forms an English suffix
    let suffix_start = pos + 1;
    if suffix_start >= chars.len() {
        return false;
    }

    let remaining: String = chars[suffix_start..].iter().collect();
    let remaining_lower = remaining.to_lowercase();

    for suffix in &["nings", "ings", "ning", "ing", "ies", "es", "ed", "s"] {
        if remaining_lower.starts_with(suffix) {
            let after = suffix_start + suffix.len();
            // Suffix must not be followed by a lowercase letter
            if after >= chars.len() || !chars[after].is_lowercase() {
                return true;
            }
        }
    }
    false
}

/// Extract word tokens from text.
///
/// A word is a sequence of Unicode letters (with optional combining marks),
/// possibly containing apostrophes for contractions (e.g., `couldn't`).
/// CJK characters (Han, Hiragana, Katakana, Hangul) are skipped.
///
/// This mirrors cspell's `extractWordsFromText` using `regExWords`.
pub fn extract_words(text: &str) -> Vec<Word> {
    let mut words = Vec::new();
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
                    // Handle apostrophes inside words (contractions)
                    Some(&(apos_offset, c))
                        if (c == '\'' || c == '\u{2019}')
                            && is_letter_after_apostrophe(text, apos_offset) =>
                    {
                        end += c.len_utf8();
                        chars.next();
                        // Consume the letter(s) after apostrophe
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
                    text: word_text.to_string(),
                    offset: start,
                });
            }
        } else {
            chars.next();
        }
    }

    words
}

/// Extract words from code: first extract word tokens, then split each by
/// camelCase boundaries.
///
/// This mirrors cspell's `extractWordsFromCode`.
pub fn extract_words_from_code(text: &str) -> Vec<Word> {
    let tokens = extract_words(text);
    let mut result = Vec::new();

    for token in &tokens {
        let parts = split_camel_case(&token.text);
        if parts.len() <= 1 {
            result.push(token.clone());
        } else {
            let mut offset_in_token = 0;
            for part in parts {
                let part_start = token.text[offset_in_token..]
                    .find(part)
                    .map(|pos| offset_in_token + pos)
                    .unwrap_or(offset_in_token);
                result.push(Word {
                    text: part.to_string(),
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
    let after_apos = apos_byte_offset + text[apos_byte_offset..].chars().next().map_or(0, |c| c.len_utf8());
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
        assert_eq!(
            split_camel_case("URLsAndDBAs"),
            vec!["URLs", "And", "DBAs"]
        );
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
        let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, vec!["split", "Camel", "Case", "Word"]);
    }

    #[test]
    fn test_code_expression() {
        let words = extract_words_from_code("regExp.match(first_line)");
        let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, vec!["reg", "Exp", "match", "first", "line"]);
    }

    #[test]
    fn test_code_a_hello() {
        let words = extract_words_from_code("aHELLO");
        let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, vec!["a", "HELLO"]);
    }

    #[test]
    fn test_code_html_input() {
        let words = extract_words_from_code("HTMLInput.value");
        let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, vec!["HTML", "Input", "value"]);
    }
}
