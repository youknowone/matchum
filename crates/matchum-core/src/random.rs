/// Bigram frequency-based random string detection.
///
/// Scores how "word-like" a string is by analyzing consecutive letter pairs
/// against known English bigram frequencies. Used to filter out Base64,
/// hex hashes, and other gibberish that would otherwise produce false positives.

/// Top ~100 English bigrams, ranked roughly by frequency.
/// Each entry is a `(u8, u8)` pair of ASCII lowercase letters.
const COMMON_BIGRAMS: &[(u8, u8)] = &[
    // Rank 1-20
    (b't', b'h'),
    (b'h', b'e'),
    (b'i', b'n'),
    (b'e', b'r'),
    (b'a', b'n'),
    (b'r', b'e'),
    (b'o', b'n'),
    (b'a', b't'),
    (b'e', b'n'),
    (b'n', b'd'),
    (b't', b'i'),
    (b'e', b's'),
    (b'o', b'r'),
    (b't', b'e'),
    (b'o', b'f'),
    (b'e', b'd'),
    (b'i', b's'),
    (b'i', b't'),
    (b'a', b'l'),
    (b'a', b'r'),
    // Rank 21-40
    (b's', b't'),
    (b't', b'o'),
    (b'n', b't'),
    (b'n', b'g'),
    (b's', b'e'),
    (b'h', b'a'),
    (b'a', b's'),
    (b'o', b'u'),
    (b'i', b'o'),
    (b'l', b'e'),
    (b'v', b'e'),
    (b'c', b'o'),
    (b'm', b'e'),
    (b'd', b'e'),
    (b'h', b'i'),
    (b'r', b'i'),
    (b'r', b'o'),
    (b'i', b'c'),
    (b'n', b'e'),
    (b'e', b'a'),
    // Rank 41-60
    (b'r', b'a'),
    (b'c', b'e'),
    (b'l', b'i'),
    (b'c', b'h'),
    (b'l', b'l'),
    (b'b', b'e'),
    (b'm', b'a'),
    (b's', b'i'),
    (b'o', b'm'),
    (b'u', b'r'),
    (b'c', b'a'),
    (b'e', b'l'),
    (b't', b'a'),
    (b'l', b'a'),
    (b'n', b's'),
    (b'd', b'i'),
    (b'a', b'm'),
    (b'u', b'n'),
    (b't', b's'),
    (b'e', b'c'),
    // Rank 61-80
    (b'p', b'e'),
    (b'u', b's'),
    (b'c', b't'),
    (b'w', b'h'),
    (b'i', b'l'),
    (b'w', b'a'),
    (b's', b'o'),
    (b'l', b'o'),
    (b'u', b't'),
    (b'w', b'i'),
    (b'f', b'o'),
    (b'n', b'o'),
    (b'p', b'r'),
    (b'a', b'c'),
    (b'o', b't'),
    (b'p', b'l'),
    (b'a', b'i'),
    (b'n', b'i'),
    (b'o', b'l'),
    (b'a', b'd'),
    // Rank 81-100
    (b'u', b'l'),
    (b'g', b'e'),
    (b'r', b's'),
    (b'i', b'g'),
    (b'i', b'v'),
    (b'u', b'e'),
    (b'o', b'w'),
    (b'e', b'v'),
    (b'p', b'o'),
    (b'a', b'b'),
    (b'r', b'd'),
    (b'e', b'm'),
    (b'a', b'g'),
    (b'u', b'd'),
    (b's', b'h'),
    (b'i', b'r'),
    (b'i', b'd'),
    (b'i', b'm'),
    (b'a', b'p'),
    (b'o', b'p'),
];

/// Precomputed 26x26 lookup table: `true` if the bigram is common in English.
/// Index = `(first - b'a') * 26 + (second - b'a')`.
const fn build_bigram_table() -> [bool; 26 * 26] {
    let mut table = [false; 26 * 26];
    let mut i = 0;
    while i < COMMON_BIGRAMS.len() {
        let (a, b) = COMMON_BIGRAMS[i];
        let idx = (a - b'a') as usize * 26 + (b - b'a') as usize;
        table[idx] = true;
        i += 1;
    }
    table
}

static BIGRAM_TABLE: [bool; 26 * 26] = build_bigram_table();

#[inline]
fn is_common_bigram(a: u8, b: u8) -> bool {
    let a = a.to_ascii_lowercase();
    let b = b.to_ascii_lowercase();
    if !a.is_ascii_lowercase() || !b.is_ascii_lowercase() {
        return false;
    }
    let idx = (a - b'a') as usize * 26 + (b - b'a') as usize;
    BIGRAM_TABLE[idx]
}

/// Score how "word-like" a string is based on English letter-pair frequencies.
///
/// Returns a value between 0.0 (random/gibberish) and 1.0 (word-like).
/// The score combines two signals:
/// - Bigram frequency: what fraction of consecutive letter pairs are common in English
/// - Letter purity: what fraction of alphanumeric characters are letters (not digits)
///
/// Strings with many digits mixed with letters (hex hashes, Base64 with numbers)
/// get penalized even if the letter-only bigrams happen to look word-like.
pub fn word_likeness_score(word: &str) -> f64 {
    let bytes = word.as_bytes();

    let letter_count = bytes.iter().filter(|b| b.is_ascii_alphabetic()).count();
    let digit_count = bytes.iter().filter(|b| b.is_ascii_digit()).count();
    let alnum_count = letter_count + digit_count;

    // Collect ASCII letter bytes only for bigram analysis
    let letters: Vec<u8> = bytes
        .iter()
        .copied()
        .filter(|b| b.is_ascii_alphabetic())
        .collect();

    if letters.len() < 2 {
        // A single letter or empty string is not scorable; treat as word-like
        // so it won't be flagged.
        return 1.0;
    }

    let total_bigrams = letters.len() - 1;
    let common_count = letters
        .windows(2)
        .filter(|pair| is_common_bigram(pair[0], pair[1]))
        .count();

    let bigram_score = common_count as f64 / total_bigrams as f64;

    // Penalize strings that mix digits with letters (hex hashes, Base64, etc.).
    // Pure-letter strings get no penalty; strings like "a1b2c3" get a heavy penalty.
    let letter_purity = if alnum_count > 0 {
        letter_count as f64 / alnum_count as f64
    } else {
        1.0
    };

    bigram_score * letter_purity
}

/// Returns `true` if the word appears to be a random string.
///
/// Words shorter than `min_length` are never considered random.
pub fn is_random_string(word: &str, min_length: usize) -> bool {
    word.len() >= min_length && word_likeness_score(word) < 0.4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bigram_table_consistency() {
        // Every entry in COMMON_BIGRAMS should be marked true in the table.
        for &(a, b) in COMMON_BIGRAMS {
            assert!(
                is_common_bigram(a, b),
                "bigram ({}, {}) should be common",
                a as char,
                b as char
            );
        }
    }

    #[test]
    fn uncommon_bigrams() {
        assert!(!is_common_bigram(b'q', b'z'));
        assert!(!is_common_bigram(b'z', b'x'));
        assert!(!is_common_bigram(b'x', b'j'));
    }

    #[test]
    fn case_insensitive() {
        assert!(is_common_bigram(b'T', b'H'));
        assert!(is_common_bigram(b't', b'H'));
        assert!(is_common_bigram(b'T', b'h'));
    }

    #[test]
    fn real_words_score_high() {
        assert!(word_likeness_score("the") > 0.8);
        assert!(word_likeness_score("hello") > 0.5);
        assert!(word_likeness_score("function") > 0.5);
        assert!(word_likeness_score("algorithm") > 0.4);
        assert!(word_likeness_score("implementation") > 0.4);
    }

    #[test]
    fn gibberish_scores_low() {
        assert!(word_likeness_score("xjfklsd") < 0.4);
        assert!(word_likeness_score("qwzxjk") < 0.4);
        assert!(word_likeness_score("bvnmxz") < 0.4);
    }

    #[test]
    fn single_char_not_scorable() {
        assert_eq!(word_likeness_score("a"), 1.0);
        assert_eq!(word_likeness_score(""), 1.0);
    }
}
