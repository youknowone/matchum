use regex::Regex;
use std::sync::LazyLock;

const MAX_NOISE_TO_LENGTH_RATIO: f64 = 0.5;

static DIGITS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());
static LOWER_MARKS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\p{Ll}\p{M}+").unwrap());
static UPPER_MARKS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\p{Lu}\p{M}+").unwrap());
static WORD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\p{Lu}?\p{Ll}+").unwrap());
static UPPER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\p{Lu}+").unwrap());
static MARK_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\p{M}").unwrap());
static PUNCT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[-_.']+").unwrap());

use crate::js_string_len;

/// Port of cspell's `categorizeString` from `isRandomString.ts`.
pub fn categorize_string(s: &str) -> String {
    let s = DIGITS_RE.replace_all(s, "0");
    let s = LOWER_MARKS_RE.replace_all(&s, "a");
    let s = UPPER_MARKS_RE.replace_all(&s, "A");
    let s = WORD_RE.replace_all(&s, "1");
    let s = UPPER_RE.replace_all(&s, "2");
    let s = MARK_RE.replace_all(&s, "4");
    let s = s.replace('_', "");
    PUNCT_RE.replace_all(&s, "3").into_owned()
}

/// Port of cspell's `scoreRandomString`.
pub fn score_random_string(s: &str) -> f64 {
    if s.is_empty() {
        return 0.0;
    }
    categorize_string(s).len() as f64 / js_string_len(s) as f64
}

/// Port of cspell's `isRandomString`.
pub fn is_random_string(word: &str, min_length: usize) -> bool {
    js_string_len(word) >= min_length && score_random_string(word) >= MAX_NOISE_TO_LENGTH_RATIO
}

// cspell:disable
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorize_string_matches_cspell_examples() {
        assert_eq!(categorize_string("abc123"), "10");
        assert_eq!(categorize_string("camelCase"), "11");
        assert_eq!(categorize_string("HTMLParser"), "21");
        assert_eq!(categorize_string("snake_case"), "11");
        assert_eq!(categorize_string("foo-bar.baz"), "13131");
    }

    #[test]
    fn alphabet_sequences_are_not_random_like_cspell() {
        let s = "abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRTUVWXY23456789";
        assert!(score_random_string(s) < 0.5);
        assert!(!is_random_string(s, 40));
    }

    #[test]
    fn repeated_word_sequences_are_not_random_like_cspell() {
        let s = "abpabpabpabpabpabpabpabpabpabpabpabpabpabpabpabpabpabpabpabpabpabpabp";
        assert!(score_random_string(s) < 0.5);
        assert!(!is_random_string(s, 40));
    }

    #[test]
    fn mixed_noise_string_can_be_random() {
        let s = "A9bQ2xT7pL4mK8zR1vN6yW3cH5";
        assert!(score_random_string(s) >= 0.5);
        assert!(is_random_string(s, 10));
    }

    #[test]
    fn multibyte_idn_labels_do_not_cross_length_threshold_by_bytes() {
        let s = "domain.with.idn.tld.उदाहरण.परीक्षा";

        assert!(s.len() > 40, "byte length should exceed cspell threshold");
        assert!(
            js_string_len(s) < 40,
            "JS length should stay below threshold"
        );
        assert!(!is_random_string(s, 40));
    }
}
