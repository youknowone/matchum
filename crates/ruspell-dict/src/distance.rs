/// Damerau-Levenshtein edit distance.
///
/// Supports 4 operations (each cost 1):
/// - insertion
/// - deletion
/// - substitution
/// - transposition of two adjacent characters
///
/// Mirrors cspell's `levenshtein.ts` algorithm.
pub fn damerau_levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    // Use 3-row DP (prev-prev, prev, current)
    let width = b_len + 1;
    let mut pp = vec![0usize; width]; // row i-2
    let mut p = vec![0usize; width]; // row i-1
    let mut c = vec![0usize; width]; // row i

    // Initialize row 0
    for j in 0..width {
        p[j] = j;
    }

    for i in 1..=a_len {
        c[0] = i;

        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };

            // substitution or match
            let mut min_val = p[j - 1] + cost;
            // deletion
            min_val = min_val.min(c[j - 1] + 1);
            // insertion
            min_val = min_val.min(p[j] + 1);

            // transposition
            if i >= 2
                && j >= 2
                && a_chars[i - 1] == b_chars[j - 2]
                && a_chars[i - 2] == b_chars[j - 1]
            {
                min_val = min_val.min(pp[j - 2] + cost);
            }

            c[j] = min_val;
        }

        // Rotate rows: pp ← p, p ← c, c ← pp (reused buffer)
        std::mem::swap(&mut pp, &mut p);
        std::mem::swap(&mut p, &mut c);
    }

    p[b_len]
}

/// Find nearest words from a word list within a maximum edit distance.
///
/// Returns words sorted by distance (ascending), then alphabetically.
pub fn select_nearest_words<'a>(
    word: &str,
    candidates: impl Iterator<Item = &'a str>,
    max_edits: usize,
    limit: usize,
) -> Vec<(String, usize)> {
    let mut results: Vec<(String, usize)> = candidates
        .filter_map(|candidate| {
            // Quick length-based pruning
            let len_diff = (word.len() as isize - candidate.len() as isize).unsigned_abs();
            if len_diff > max_edits {
                return None;
            }
            let dist = damerau_levenshtein(word, candidate);
            if dist <= max_edits {
                Some((candidate.to_string(), dist))
            } else {
                None
            }
        })
        .collect();

    results.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    results.truncate(limit);
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from cspell's levenshtein.test.ts

    #[test]
    fn identical_strings() {
        assert_eq!(damerau_levenshtein("abc", "abc"), 0);
    }

    #[test]
    fn single_deletion() {
        assert_eq!(damerau_levenshtein("abc", "ab"), 1);
    }

    #[test]
    fn all_deletions() {
        assert_eq!(damerau_levenshtein("abc", ""), 3);
    }

    #[test]
    fn empty_to_empty() {
        assert_eq!(damerau_levenshtein("", ""), 0);
    }

    #[test]
    fn classic_kitten_sitting() {
        assert_eq!(damerau_levenshtein("kitten", "sitting"), 3);
    }

    #[test]
    fn saturday_sunday() {
        assert_eq!(damerau_levenshtein("Saturday", "Sunday"), 3);
    }

    #[test]
    fn transposition() {
        assert_eq!(damerau_levenshtein("ab", "ba"), 1);
    }

    #[test]
    fn aba_bab() {
        assert_eq!(damerau_levenshtein("aba", "bab"), 2);
    }

    #[test]
    fn abab_baba() {
        assert_eq!(damerau_levenshtein("abab", "baba"), 2);
    }

    #[test]
    fn abab_ababa() {
        assert_eq!(damerau_levenshtein("abab", "ababa"), 1);
    }

    #[test]
    fn appear_apple() {
        assert_eq!(damerau_levenshtein("appear", "apple"), 3);
    }

    #[test]
    fn appease_apple() {
        assert_eq!(damerau_levenshtein("appease", "apple"), 3);
    }

    #[test]
    fn symmetry() {
        let pairs = [
            ("abc", "abc"),
            ("abc", "ab"),
            ("kitten", "sitting"),
            ("ab", "ba"),
            ("appear", "apple"),
        ];
        for (a, b) in &pairs {
            assert_eq!(
                damerau_levenshtein(a, b),
                damerau_levenshtein(b, a),
                "symmetry failed for ({}, {})",
                a,
                b
            );
        }
    }

    #[test]
    fn apple_apples() {
        assert_eq!(damerau_levenshtein("apple", "apples"), 1);
    }

    #[test]
    fn apple_maple() {
        assert_eq!(damerau_levenshtein("apple", "maple"), 2);
    }

    #[test]
    fn grapple_maples() {
        assert_eq!(damerau_levenshtein("grapple", "maples"), 4);
    }

    #[test]
    fn select_nearest_basic() {
        let candidates = ["walk", "talked"];
        let results = select_nearest_words(
            "talk",
            candidates.iter().copied(),
            5,
            3,
        );
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], ("walk".into(), 1));
        assert_eq!(results[1], ("talked".into(), 2));
    }
}
