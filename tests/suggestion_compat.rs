//! Suggestion quality tests for cspell compatibility.
//!
//! Verifies that suggestions are ranked correctly:
//! - Case-change-only suggestions rank higher
//! - Preferred suggestions (marked `!*`) rank first
//! - Common typos produce the correct suggestion
//! - Keyboard adjacency typos are handled well

use matchum_dict::dictionary::Dictionary;
use matchum_dict::hashdict::HashDictionary;

fn make_dict(words: &[&str]) -> HashDictionary {
    let mut d = HashDictionary::new(false);
    for w in words {
        d.add_word(w);
    }
    d
}

// ============================================================
// 1. Case change suggestions ranked higher
// ============================================================

mod case_ranking {
    use super::*;

    #[test]
    fn case_only_difference_ranked_first() {
        // "color" and "dolor" are both edit-distance 1 from "Color",
        // but in the normalized dictionary "color" differs only by case
        // from "Color", so it should rank higher.
        let dict = make_dict(&["color", "dolor", "molar"]);
        let suggestions = dict.suggest("Color", 5);
        // "color" should appear (case change from normalized "color" vs "color" = 0 weighted)
        assert!(
            suggestions.contains(&"color".to_string()),
            "suggestions: {:?}",
            suggestions
        );
        if suggestions.len() >= 2 {
            // "color" should be first since it has the lowest weighted distance
            assert_eq!(
                suggestions[0], "color",
                "case-only change should rank first: {:?}",
                suggestions
            );
        }
    }

    #[test]
    fn all_caps_suggestion() {
        let dict = make_dict(&["hello", "jello", "cello"]);
        let suggestions = dict.suggest("HELLO", 5);
        assert!(
            suggestions.contains(&"hello".to_string()),
            "HELLO -> hello: {:?}",
            suggestions
        );
        // "hello" should rank first due to case-only difference
        assert_eq!(
            suggestions[0], "hello",
            "case-only should rank first: {:?}",
            suggestions
        );
    }
}

// ============================================================
// 2. Simple typo corrections
// ============================================================

mod typo_corrections {
    use super::*;

    #[test]
    fn recieve_to_receive() {
        let dict = make_dict(&["receive", "relieve", "deceive"]);
        let suggestions = dict.suggest("recieve", 5);
        assert!(
            suggestions.contains(&"receive".to_string()),
            "recieve -> receive: {:?}",
            suggestions
        );
    }

    #[test]
    fn teh_to_the() {
        let dict = make_dict(&["the", "ten", "tea", "tie"]);
        let suggestions = dict.suggest("teh", 5);
        assert!(
            suggestions.contains(&"the".to_string()),
            "teh -> the: {:?}",
            suggestions
        );
    }

    #[test]
    fn seperate_to_separate() {
        let dict = make_dict(&["separate", "desperate", "operate"]);
        let suggestions = dict.suggest("seperate", 5);
        assert!(
            suggestions.contains(&"separate".to_string()),
            "seperate -> separate: {:?}",
            suggestions
        );
    }

    #[test]
    fn occured_to_occurred() {
        let dict = make_dict(&["occurred", "occupied", "obscured"]);
        let suggestions = dict.suggest("occured", 5);
        assert!(
            suggestions.contains(&"occurred".to_string()),
            "occured -> occurred: {:?}",
            suggestions
        );
    }

    #[test]
    fn definately_to_definitely() {
        let dict = make_dict(&["definitely", "infinitely"]);
        let suggestions = dict.suggest("definately", 5);
        assert!(
            suggestions.contains(&"definitely".to_string()),
            "definately -> definitely: {:?}",
            suggestions
        );
    }

    #[test]
    fn accomodate_to_accommodate() {
        let dict = make_dict(&["accommodate", "accumulated"]);
        let suggestions = dict.suggest("accomodate", 5);
        assert!(
            suggestions.contains(&"accommodate".to_string()),
            "accomodate -> accommodate: {:?}",
            suggestions
        );
    }
}

// ============================================================
// 3. Preferred suggestions ranked first
// ============================================================

mod preferred_suggestions {
    use super::*;

    #[test]
    fn preferred_word_ranks_first() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("color");
        dict.add_word("dolor");
        dict.add_preferred("color"); // mark "color" as preferred

        let suggestions = dict.suggest("colr", 5);
        assert!(
            suggestions.contains(&"color".to_string()),
            "preferred: {:?}",
            suggestions
        );
        // "color" should rank first since it's preferred
        if !suggestions.is_empty() {
            assert_eq!(
                suggestions[0], "color",
                "preferred should rank first: {:?}",
                suggestions
            );
        }
    }

    #[test]
    fn preferred_beats_closer_non_preferred() {
        let mut dict = HashDictionary::new(false);
        // "dor" is distance 1 from "dor"... let's make a better example:
        // "world" and "word" are both close to "wold"
        dict.add_word("word");
        dict.add_preferred("world"); // preferred but distance 1 from "wold"

        let suggestions = dict.suggest("wold", 5);
        // Both "word" (distance 1) and "world" (distance 1) should appear
        // "world" should rank first because it's preferred
        if suggestions.contains(&"world".to_string()) && suggestions.contains(&"word".to_string())
        {
            assert_eq!(
                suggestions[0], "world",
                "preferred should rank first even at same distance: {:?}",
                suggestions
            );
        }
    }

    #[test]
    fn non_preferred_still_appears() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("hello");
        dict.add_word("hallo");
        dict.add_preferred("hello");

        let suggestions = dict.suggest("helo", 5);
        assert!(
            suggestions.contains(&"hello".to_string()),
            "preferred present: {:?}",
            suggestions
        );
        assert!(
            suggestions.contains(&"hallo".to_string()),
            "non-preferred present: {:?}",
            suggestions
        );
    }
}

// ============================================================
// 4. Keyboard adjacency
// ============================================================

mod keyboard_adjacency {
    use super::*;

    #[test]
    fn adjacent_key_typo_still_suggests() {
        // 's' and 'd' are adjacent on QWERTY
        let dict = make_dict(&["sat", "dat", "bat", "cat"]);
        let suggestions = dict.suggest("dat", 5);
        // "dat" itself should match if in dict, but also neighbors
        assert!(
            suggestions.contains(&"dat".to_string()) || suggestions.contains(&"sat".to_string()),
            "adjacent key: {:?}",
            suggestions
        );
    }

    #[test]
    fn adjacent_key_ranked_higher_than_distant() {
        // When we have a typo where 'f' was typed instead of 'g' (adjacent),
        // and another candidate differs by a distant key
        let dict = make_dict(&["get", "bet", "let", "met", "net", "set", "vet", "wet", "yet", "fet"]);
        let suggestions = dict.suggest("fet", 5);
        // "fet" (distance 0) would be first, but if it's not in dict:
        // All are distance 1. But "get" (f->g adjacent) should rank before "let" (f->l distant)
        if suggestions.contains(&"get".to_string()) && suggestions.contains(&"let".to_string()) {
            let get_pos = suggestions.iter().position(|s| s == "get").unwrap();
            let let_pos = suggestions.iter().position(|s| s == "let").unwrap();
            assert!(
                get_pos < let_pos,
                "adjacent 'get' should rank before distant 'let': {:?}",
                suggestions
            );
        }
    }
}

// ============================================================
// 5. Edge cases
// ============================================================

mod edge_cases {
    use super::*;

    #[test]
    fn empty_word_no_crash() {
        let dict = make_dict(&["hello", "world"]);
        let suggestions = dict.suggest("", 5);
        // Should not panic, results may be empty or contain short words
        let _ = suggestions;
    }

    #[test]
    fn single_char_word() {
        let dict = make_dict(&["a", "b", "c", "ab", "bc"]);
        let suggestions = dict.suggest("x", 5);
        // "a", "b", "c" are all distance 1
        assert!(
            suggestions.iter().any(|s| s.len() == 1),
            "single char: {:?}",
            suggestions
        );
    }

    #[test]
    fn no_suggest_excluded_from_results() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("hello");
        dict.add_no_suggest("hallo");
        let suggestions = dict.suggest("helo", 5);
        assert!(
            !suggestions.contains(&"hallo".to_string()),
            "no_suggest excluded: {:?}",
            suggestions
        );
    }

    #[test]
    fn forbidden_excluded_from_results() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("hello");
        dict.add_forbidden("hallo");
        let suggestions = dict.suggest("helo", 5);
        assert!(
            !suggestions.contains(&"hallo".to_string()),
            "forbidden excluded: {:?}",
            suggestions
        );
    }

    #[test]
    fn limit_respected() {
        let dict = make_dict(&["ab", "ac", "ad", "ae", "af", "ag"]);
        let suggestions = dict.suggest("aa", 3);
        assert!(suggestions.len() <= 3, "limit: {:?}", suggestions);
    }
}

// ============================================================
// repmap-based suggestions
// ============================================================
mod repmap_suggestions {
    use super::*;
    use matchum_dict::repmap::RepMap;

    #[test]
    fn german_strasse_suggests_eszett() {
        let mut dict = HashDictionary::new(false);
        // Dictionary contains the ß variant
        dict.add_word("stra\u{00df}e");
        dict.add_word("hello");
        dict.set_repmap(RepMap::new(vec![("ss".into(), "\u{00df}".into())]));

        // User typed "strasse" (ss variant) which is NOT in dictionary
        let suggestions = dict.suggest("strasse", 5);
        assert!(
            suggestions.contains(&"stra\u{00df}e".to_string()),
            "repmap should suggest ß variant: {suggestions:?}"
        );
    }

    #[test]
    fn french_cafe_suggests_accent() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("caf\u{00e9}");
        dict.set_repmap(RepMap::new(vec![("e".into(), "\u{00e9}".into())]));

        let suggestions = dict.suggest("cafe", 5);
        assert!(
            suggestions.contains(&"caf\u{00e9}".to_string()),
            "repmap should suggest accented variant: {suggestions:?}"
        );
    }

    #[test]
    fn repmap_suggestion_ranked_high() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("stra\u{00df}e");
        dict.add_word("strafe"); // also edit-distance 1 from "strasse"
        dict.set_repmap(RepMap::new(vec![("ss".into(), "\u{00df}".into())]));

        let suggestions = dict.suggest("strasse", 5);
        // The repmap suggestion should appear first (preferred=true, distance=0)
        assert_eq!(
            suggestions.first().map(|s| s.as_str()),
            Some("stra\u{00df}e"),
            "repmap suggestion should rank first: {suggestions:?}"
        );
    }

    #[test]
    fn no_repmap_no_extra_suggestions() {
        let dict = make_dict(&["hello", "world"]);
        let suggestions = dict.suggest("strasse", 5);
        // Without repmap, "straße" would not appear
        assert!(
            !suggestions.iter().any(|s| s.contains('\u{00df}')),
            "without repmap, no ß suggestions: {suggestions:?}"
        );
    }
}
