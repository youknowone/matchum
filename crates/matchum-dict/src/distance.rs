// spell-checker:ignore abab ababa qwerty

/// Weighted edit costs for Damerau-Levenshtein distance.
///
/// Default costs are 100 per operation (using centesimal scale to allow
/// fractional-style costs like "half price for keyboard adjacency").
/// Case changes cost 0 by default to rank case-only differences higher.
#[derive(Debug, Clone)]
pub struct EditCosts {
    pub insertion: usize,
    pub deletion: usize,
    pub substitution: usize,
    pub transposition: usize,
    /// Cost of substituting a character that differs only in case (e.g. 'a' -> 'A').
    pub case_change: usize,
    /// Cost of substituting between adjacent keys on QWERTY keyboard.
    /// Applied instead of `substitution` when both chars are adjacent.
    pub adjacent_key: usize,
}

impl Default for EditCosts {
    fn default() -> Self {
        Self {
            insertion: 100,
            deletion: 100,
            substitution: 100,
            transposition: 100,
            case_change: 0,
            adjacent_key: 75,
        }
    }
}

impl EditCosts {
    /// Standard unweighted costs (each operation = 1).
    pub fn unweighted() -> Self {
        Self {
            insertion: 1,
            deletion: 1,
            substitution: 1,
            transposition: 1,
            case_change: 1,
            adjacent_key: 1,
        }
    }
}

/// QWERTY keyboard adjacency map.
/// For each key, lists its immediate neighbors on a standard QWERTY layout.
fn qwerty_neighbors(c: char) -> &'static [char] {
    match c.to_ascii_lowercase() {
        'q' => &['w', 'a'],
        'w' => &['q', 'e', 'a', 's'],
        'e' => &['w', 'r', 's', 'd'],
        'r' => &['e', 't', 'd', 'f'],
        't' => &['r', 'y', 'f', 'g'],
        'y' => &['t', 'u', 'g', 'h'],
        'u' => &['y', 'i', 'h', 'j'],
        'i' => &['u', 'o', 'j', 'k'],
        'o' => &['i', 'p', 'k', 'l'],
        'p' => &['o', 'l'],
        'a' => &['q', 'w', 's', 'z'],
        's' => &['w', 'e', 'a', 'd', 'z', 'x'],
        'd' => &['e', 'r', 's', 'f', 'x', 'c'],
        'f' => &['r', 't', 'd', 'g', 'c', 'v'],
        'g' => &['t', 'y', 'f', 'h', 'v', 'b'],
        'h' => &['y', 'u', 'g', 'j', 'b', 'n'],
        'j' => &['u', 'i', 'h', 'k', 'n', 'm'],
        'k' => &['i', 'o', 'j', 'l', 'm'],
        'l' => &['o', 'p', 'k'],
        'z' => &['a', 's', 'x'],
        'x' => &['s', 'd', 'z', 'c'],
        'c' => &['d', 'f', 'x', 'v'],
        'v' => &['f', 'g', 'c', 'b'],
        'b' => &['g', 'h', 'v', 'n'],
        'n' => &['h', 'j', 'b', 'm'],
        'm' => &['j', 'k', 'n'],
        _ => &[],
    }
}

/// Check if two characters are adjacent on a QWERTY keyboard.
fn is_keyboard_adjacent(a: char, b: char) -> bool {
    if !a.is_ascii_alphabetic() || !b.is_ascii_alphabetic() {
        return false;
    }
    let neighbors = qwerty_neighbors(a);
    neighbors.contains(&b.to_ascii_lowercase())
}

/// Weighted Damerau-Levenshtein edit distance.
///
/// Uses `EditCosts` to assign different costs to different operations.
/// Case changes and keyboard adjacency substitutions get reduced costs
/// to produce better suggestion rankings.
pub fn weighted_damerau_levenshtein(a: &str, b: &str, costs: &EditCosts) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();

    if a_len == 0 {
        return b_len * costs.insertion;
    }
    if b_len == 0 {
        return a_len * costs.deletion;
    }

    let width = b_len + 1;
    let mut pp = vec![0usize; width];
    let mut p = vec![0usize; width];
    let mut c = vec![0usize; width];

    for j in 0..width {
        p[j] = j * costs.insertion;
    }

    for i in 1..=a_len {
        c[0] = i * costs.deletion;

        for j in 1..=b_len {
            let ac = a_chars[i - 1];
            let bc = b_chars[j - 1];

            let sub_cost = if ac == bc {
                0
            } else if ac.to_lowercase().eq(bc.to_lowercase()) {
                costs.case_change
            } else if is_keyboard_adjacent(ac, bc) {
                costs.adjacent_key
            } else {
                costs.substitution
            };

            // substitution or match
            let mut min_val = p[j - 1] + sub_cost;
            // deletion
            min_val = min_val.min(c[j - 1] + costs.deletion);
            // insertion
            min_val = min_val.min(p[j] + costs.insertion);

            // transposition
            if i >= 2
                && j >= 2
                && a_chars[i - 1] == b_chars[j - 2]
                && a_chars[i - 2] == b_chars[j - 1]
            {
                min_val = min_val.min(pp[j - 2] + costs.transposition);
            }

            c[j] = min_val;
        }

        std::mem::swap(&mut pp, &mut p);
        std::mem::swap(&mut p, &mut c);
    }

    p[b_len]
}

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

/// Suggestion candidate with both raw and weighted distance.
#[derive(Debug, Clone)]
pub struct SuggestionCandidate {
    pub word: String,
    /// Unweighted Damerau-Levenshtein distance (for filtering by max_edits).
    pub raw_distance: usize,
    /// Weighted distance (for ranking). Lower = better match.
    pub weighted_distance: usize,
    /// Whether this is a preferred suggestion (marked with `!*` in dictionary).
    pub preferred: bool,
}

/// Find nearest words using weighted distance for ranking.
///
/// Candidates are filtered by unweighted `max_edits`, then ranked by
/// weighted distance. Preferred words always sort first within each
/// distance tier.
pub fn select_nearest_words_weighted<'a>(
    word: &str,
    candidates: impl Iterator<Item = &'a str>,
    max_edits: usize,
    limit: usize,
    costs: &EditCosts,
    preferred: Option<&hashbrown::HashSet<compact_str::CompactString>>,
) -> Vec<SuggestionCandidate> {
    let mut results: Vec<SuggestionCandidate> = candidates
        .filter_map(|candidate| {
            // Quick length-based pruning
            let len_diff = (word.len() as isize - candidate.len() as isize).unsigned_abs();
            if len_diff > max_edits {
                return None;
            }
            let raw = damerau_levenshtein(word, candidate);
            if raw > max_edits {
                return None;
            }
            let weighted = weighted_damerau_levenshtein(word, candidate, costs);
            let is_preferred = preferred
                .map(|set| set.contains(candidate))
                .unwrap_or(false);
            Some(SuggestionCandidate {
                word: candidate.to_string(),
                raw_distance: raw,
                weighted_distance: weighted,
                preferred: is_preferred,
            })
        })
        .collect();

    // Sort: preferred first, then by weighted distance, then alphabetically
    results.sort_by(|a, b| {
        b.preferred
            .cmp(&a.preferred)
            .then_with(|| a.weighted_distance.cmp(&b.weighted_distance))
            .then_with(|| a.word.cmp(&b.word))
    });
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
        let results = select_nearest_words("talk", candidates.iter().copied(), 5, 3);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], ("walk".into(), 1));
        assert_eq!(results[1], ("talked".into(), 2));
    }

    // Weighted distance tests

    #[test]
    fn weighted_case_change_zero_cost() {
        let costs = EditCosts::default();
        // 'a' -> 'A' should cost 0 (case_change)
        assert_eq!(weighted_damerau_levenshtein("abc", "Abc", &costs), 0);
        assert_eq!(weighted_damerau_levenshtein("hello", "Hello", &costs), 0);
        assert_eq!(weighted_damerau_levenshtein("hello", "HELLO", &costs), 0);
    }

    #[test]
    fn weighted_adjacent_key_reduced_cost() {
        let costs = EditCosts::default();
        // 's' and 'd' are adjacent on QWERTY
        let sd = weighted_damerau_levenshtein("sat", "dat", &costs);
        // 's' and 'z' are not adjacent on QWERTY... wait, they are.
        // 's' and 'b' are NOT adjacent
        let sb = weighted_damerau_levenshtein("sat", "bat", &costs);
        assert!(
            sd < sb,
            "adjacent key cost ({sd}) should be less than non-adjacent ({sb})"
        );
    }

    #[test]
    fn weighted_matches_unweighted_for_unit_costs() {
        let costs = EditCosts::unweighted();
        assert_eq!(
            weighted_damerau_levenshtein("kitten", "sitting", &costs),
            damerau_levenshtein("kitten", "sitting")
        );
        assert_eq!(
            weighted_damerau_levenshtein("ab", "ba", &costs),
            damerau_levenshtein("ab", "ba")
        );
    }

    #[test]
    fn weighted_empty_strings() {
        let costs = EditCosts::default();
        assert_eq!(weighted_damerau_levenshtein("", "", &costs), 0);
        assert_eq!(weighted_damerau_levenshtein("abc", "", &costs), 300);
        assert_eq!(weighted_damerau_levenshtein("", "abc", &costs), 300);
    }

    #[test]
    fn keyboard_adjacency_check() {
        assert!(is_keyboard_adjacent('s', 'd'));
        assert!(is_keyboard_adjacent('d', 's'));
        assert!(is_keyboard_adjacent('q', 'w'));
        assert!(!is_keyboard_adjacent('q', 'z'));
        assert!(!is_keyboard_adjacent('a', 'm'));
    }
}
