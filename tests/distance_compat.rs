//! Edit distance and suggestion tests ported from cspell's
//! levenshtein.test.ts, distance.test.ts, and suggestion-related tests.
//!
//! Sources:
//! - vendor/cspell/packages/cspell-trie-lib/src/lib/distance/levenshtein.test.ts
//! - vendor/cspell/packages/cspell-trie-lib/src/lib/distance/distance.test.ts

use matchum_dict::dictionary::Dictionary;
use matchum_dict::distance::{
    damerau_levenshtein, select_nearest_words, weighted_damerau_levenshtein, EditCosts,
};
use matchum_dict::hashdict::HashDictionary;

// ============================================================
// 1. Damerau-Levenshtein distance — levenshtein.test.ts
// ============================================================

mod levenshtein {
    use super::*;

    #[test]
    fn identical() {
        assert_eq!(damerau_levenshtein("abc", "abc"), 0);
    }

    #[test]
    fn empty_vs_empty() {
        assert_eq!(damerau_levenshtein("", ""), 0);
    }

    #[test]
    fn empty_vs_string() {
        assert_eq!(damerau_levenshtein("", "abc"), 3);
        assert_eq!(damerau_levenshtein("abc", ""), 3);
    }

    #[test]
    fn single_deletion() {
        assert_eq!(damerau_levenshtein("abc", "ab"), 1);
    }

    #[test]
    fn single_insertion() {
        assert_eq!(damerau_levenshtein("ab", "abc"), 1);
    }

    #[test]
    fn single_substitution() {
        assert_eq!(damerau_levenshtein("abc", "axc"), 1);
    }

    #[test]
    fn transposition() {
        assert_eq!(damerau_levenshtein("ab", "ba"), 1);
    }

    #[test]
    fn kitten_sitting() {
        assert_eq!(damerau_levenshtein("kitten", "sitting"), 3);
    }

    #[test]
    fn saturday_sunday() {
        assert_eq!(damerau_levenshtein("Saturday", "Sunday"), 3);
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
    fn symmetry_all_pairs() {
        let pairs = [
            ("abc", "abc"),
            ("abc", "ab"),
            ("abc", ""),
            ("kitten", "sitting"),
            ("Saturday", "Sunday"),
            ("ab", "ba"),
            ("aba", "bab"),
            ("abab", "baba"),
            ("appear", "apple"),
            ("appease", "apple"),
        ];
        for (a, b) in &pairs {
            assert_eq!(
                damerau_levenshtein(a, b),
                damerau_levenshtein(b, a),
                "symmetry: ({a}, {b})"
            );
        }
    }

    #[test]
    fn single_char_operations() {
        // Insert
        assert_eq!(damerau_levenshtein("a", "ab"), 1);
        // Delete
        assert_eq!(damerau_levenshtein("ab", "a"), 1);
        // Substitute
        assert_eq!(damerau_levenshtein("a", "b"), 1);
    }

    #[test]
    fn longer_transposition() {
        // "teh" → "the" via transposition
        assert_eq!(damerau_levenshtein("teh", "the"), 1);
    }

    #[test]
    fn real_typos() {
        // Common spelling errors
        assert_eq!(damerau_levenshtein("recieve", "receive"), 1); // transposition
        assert_eq!(damerau_levenshtein("seperate", "separate"), 1); // substitution
        assert_eq!(damerau_levenshtein("occured", "occurred"), 1); // insertion
        assert_eq!(damerau_levenshtein("definately", "definitely"), 1);
    }

    #[test]
    fn unicode_strings() {
        assert_eq!(damerau_levenshtein("café", "cafe"), 1);
        assert_eq!(damerau_levenshtein("naïve", "naive"), 1);
    }
}

// ============================================================
// 2. Select nearest words — levenshtein.test.ts
// ============================================================

mod nearest_words {
    use super::*;

    fn sample_words() -> Vec<&'static str> {
        vec![
            "talk", "talked", "talker", "talking", "talks", "walk", "walked", "walker", "walking",
            "walks", "help", "helped", "helper", "helpful", "helping", "kelp", "kelps", "joy",
            "joyful", "joyless", "joyous", "toy", "toys", "box", "boxes", "fox", "foxes",
        ]
    }

    #[test]
    fn nearest_to_talk() {
        let words = sample_words();
        let results = select_nearest_words("talk", words.iter().copied(), 3, 5);
        // "talk" itself is distance 0
        assert!(results.iter().any(|(w, d)| w == "talk" && *d == 0));
        // "walk" is distance 1
        assert!(results.iter().any(|(w, d)| w == "walk" && *d == 1));
    }

    #[test]
    fn nearest_to_helpful() {
        let words = sample_words();
        let results = select_nearest_words("helpful", words.iter().copied(), 3, 5);
        assert!(results.iter().any(|(w, d)| w == "helpful" && *d == 0));
    }

    #[test]
    fn nearest_to_walk() {
        let words = sample_words();
        let results = select_nearest_words("walk", words.iter().copied(), 3, 5);
        assert!(results.iter().any(|(w, d)| w == "walk" && *d == 0));
        assert!(results.iter().any(|(w, d)| w == "talk" && *d == 1));
        // "walks" is distance 1
        assert!(results.iter().any(|(w, d)| w == "walks" && *d == 1));
    }

    #[test]
    fn nearest_to_alk() {
        let words = sample_words();
        let results = select_nearest_words("alk", words.iter().copied(), 3, 10);
        // "talk" and "walk" are both distance 1
        assert!(results.iter().any(|(w, d)| w == "talk" && *d == 1));
        assert!(results.iter().any(|(w, d)| w == "walk" && *d == 1));
    }

    #[test]
    fn nearest_to_talker() {
        let words = sample_words();
        let results = select_nearest_words("talker", words.iter().copied(), 3, 5);
        assert!(results.iter().any(|(w, d)| w == "talker" && *d == 0));
        assert!(results.iter().any(|(w, d)| w == "walker" && *d == 1));
        assert!(results.iter().any(|(w, d)| w == "talked" && *d == 1));
    }

    #[test]
    fn max_edits_filters() {
        let words = sample_words();
        let results = select_nearest_words("xyz", words.iter().copied(), 1, 10);
        // No words within 1 edit of "xyz"
        assert!(results.is_empty(), "got: {:?}", results);
    }

    #[test]
    fn limit_restricts_count() {
        let words = sample_words();
        let results = select_nearest_words("talk", words.iter().copied(), 5, 2);
        assert!(results.len() <= 2);
    }

    #[test]
    fn results_sorted_by_distance() {
        let words = sample_words();
        let results = select_nearest_words("talk", words.iter().copied(), 3, 10);
        for pair in results.windows(2) {
            assert!(pair[0].1 <= pair[1].1, "not sorted: {:?}", results);
        }
    }
}

// ============================================================
// 3. HashDictionary suggest() integration
// ============================================================

mod dict_suggestions {
    use super::*;

    fn make_dict(words: &[&str]) -> HashDictionary {
        let mut d = HashDictionary::new(false);
        for w in words {
            d.add_word(w);
        }
        d
    }

    #[test]
    fn suggest_returns_close_words() {
        let dict = make_dict(&["hello", "help", "held", "world", "helm"]);
        let suggestions = dict.suggest("helo", 5);
        assert!(
            suggestions.contains(&"hello".to_string()),
            "suggestions: {:?}",
            suggestions
        );
    }

    #[test]
    fn suggest_respects_limit() {
        let dict = make_dict(&["hello", "help", "held", "world", "helm", "hell"]);
        let suggestions = dict.suggest("helo", 2);
        assert!(suggestions.len() <= 2);
    }

    #[test]
    fn suggest_no_results_for_distant_word() {
        let dict = make_dict(&["apple", "banana", "cherry"]);
        let suggestions = dict.suggest("xyz", 5);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn suggest_excludes_no_suggest_words() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("hello");
        dict.add_no_suggest("hallo"); // close to "helo" but no-suggest
        dict.add_word("world");

        let suggestions = dict.suggest("helo", 5);
        assert!(
            !suggestions.contains(&"hallo".to_string()),
            "no-suggest words should not appear: {:?}",
            suggestions
        );
    }

    #[test]
    fn suggest_excludes_forbidden_words() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("hello");
        dict.add_forbidden("hallo");

        let suggestions = dict.suggest("helo", 5);
        assert!(
            !suggestions.contains(&"hallo".to_string()),
            "forbidden words should not appear: {:?}",
            suggestions
        );
    }

    #[test]
    fn suggest_common_typos() {
        let dict = make_dict(&[
            "receive",
            "separate",
            "occurred",
            "definitely",
            "the",
            "their",
            "there",
        ]);

        let r1 = dict.suggest("recieve", 5);
        assert!(r1.contains(&"receive".to_string()), "recieve: {:?}", r1);

        let r2 = dict.suggest("seperate", 5);
        assert!(r2.contains(&"separate".to_string()), "seperate: {:?}", r2);

        let r3 = dict.suggest("teh", 5);
        assert!(r3.contains(&"the".to_string()), "teh: {:?}", r3);
    }

    #[test]
    fn suggest_case_insensitive() {
        let dict = make_dict(&["Hello", "World"]);
        let suggestions = dict.suggest("helo", 5);
        // Dictionary normalizes to lowercase, so "hello" should be suggested
        assert!(
            suggestions.contains(&"hello".to_string()),
            "case insensitive: {:?}",
            suggestions
        );
    }
}

// ============================================================
// 4. Weighted Damerau-Levenshtein distance
// ============================================================

mod weighted_distance {
    use super::*;

    #[test]
    fn case_change_costs_zero_by_default() {
        let costs = EditCosts::default();
        assert_eq!(weighted_damerau_levenshtein("hello", "Hello", &costs), 0);
        assert_eq!(weighted_damerau_levenshtein("abc", "ABC", &costs), 0);
        assert_eq!(weighted_damerau_levenshtein("color", "Color", &costs), 0);
    }

    #[test]
    fn identical_strings_zero() {
        let costs = EditCosts::default();
        assert_eq!(weighted_damerau_levenshtein("hello", "hello", &costs), 0);
    }

    #[test]
    fn pure_case_change_cheaper_than_substitution() {
        let costs = EditCosts::default();
        let case_dist = weighted_damerau_levenshtein("color", "Color", &costs);
        let sub_dist = weighted_damerau_levenshtein("color", "dolor", &costs);
        assert!(
            case_dist < sub_dist,
            "case change ({case_dist}) should be cheaper than substitution ({sub_dist})"
        );
    }

    #[test]
    fn keyboard_adjacent_cheaper_than_distant() {
        let costs = EditCosts::default();
        // 'f' and 'g' are adjacent
        let adj = weighted_damerau_levenshtein("fat", "gat", &costs);
        // 'f' and 'l' are not adjacent
        let far = weighted_damerau_levenshtein("fat", "lat", &costs);
        assert!(
            adj < far,
            "adjacent key ({adj}) should be cheaper than distant key ({far})"
        );
    }

    #[test]
    fn unweighted_matches_classic() {
        let costs = EditCosts::unweighted();
        let pairs = [
            ("kitten", "sitting", 3),
            ("Saturday", "Sunday", 3),
            ("ab", "ba", 1),
            ("abc", "ab", 1),
            ("appear", "apple", 3),
        ];
        for (a, b, expected) in &pairs {
            assert_eq!(
                weighted_damerau_levenshtein(a, b, &costs),
                *expected,
                "unweighted({a}, {b})"
            );
        }
    }

    #[test]
    fn transposition_with_default_cost() {
        let costs = EditCosts::default();
        // "teh" -> "the" is a transposition, cost = 100
        assert_eq!(weighted_damerau_levenshtein("teh", "the", &costs), 100);
    }

    #[test]
    fn insertion_and_deletion_with_default_cost() {
        let costs = EditCosts::default();
        // "helo" -> "hello" is an insertion, cost = 100
        assert_eq!(weighted_damerau_levenshtein("helo", "hello", &costs), 100);
        // "helloo" -> "hello" is a deletion, cost = 100
        assert_eq!(weighted_damerau_levenshtein("helloo", "hello", &costs), 100);
    }

    #[test]
    fn empty_strings() {
        let costs = EditCosts::default();
        assert_eq!(weighted_damerau_levenshtein("", "", &costs), 0);
        assert_eq!(
            weighted_damerau_levenshtein("abc", "", &costs),
            3 * costs.deletion
        );
        assert_eq!(
            weighted_damerau_levenshtein("", "abc", &costs),
            3 * costs.insertion
        );
    }

    #[test]
    fn mixed_case_and_edit() {
        let costs = EditCosts::default();
        // "color" -> "Color" is case (0), "color" -> "colours" is case(0) + ins('u') + ins('s') = 200
        let dist = weighted_damerau_levenshtein("color", "Colours", &costs);
        // case('c'->'C')=0, ins('u')=100, ins('s')=100 = 200
        assert_eq!(dist, 200);
    }
}
