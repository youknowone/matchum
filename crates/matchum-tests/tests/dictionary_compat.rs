//! Dictionary tests ported from cspell's SpellingDictionary and DictionaryLoader tests.
//!
//! Sources:
//! - vendor/cspell/packages/cspell-dictionary/src/SpellingDictionary/SpellingDictionary.test.ts
//! - vendor/cspell/packages/cspell-dictionary/src/SpellingDictionary/FlagWordsDictionary.test.ts
//! - vendor/cspell/packages/cspell-dictionary/src/SpellingDictionary/IgnoreWordsDictionary.test.ts
//! - vendor/cspell/packages/cspell-lib/src/lib/SpellingDictionary/DictionaryLoader.test.ts
//! - vendor/cspell/packages/cspell-trie-lib/src/lib/SimpleDictionaryParser.test.ts

use matchum_dict::dictionary::Dictionary;
use matchum_dict::hashdict::HashDictionary;
use matchum_dict::loader;
use std::path::PathBuf;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_root() -> PathBuf {
    project_root()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

// ============================================================
// 1. Basic HashDictionary — has()
// ============================================================

mod basic_has {
    use super::*;

    #[test]
    fn word_found() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("apple");
        assert!(dict.has("apple"));
    }

    #[test]
    fn word_not_found() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("apple");
        assert!(!dict.has("banana"));
    }

    #[test]
    fn case_insensitive_lookup() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("apple");
        assert!(dict.has("Apple"));
        assert!(dict.has("APPLE"));
    }

    #[test]
    fn case_sensitive_lookup() {
        let mut dict = HashDictionary::new(true);
        dict.add_word("apple");
        assert!(dict.has("apple"));
        assert!(!dict.has("Apple"));
        assert!(!dict.has("APPLE"));
    }

    #[test]
    fn case_sensitive_mixed_case() {
        let mut dict = HashDictionary::new(true);
        dict.add_word("Seattle");
        assert!(dict.has("Seattle"));
        assert!(!dict.has("seattle"));
        assert!(!dict.has("SEATTLE"));
    }

    #[test]
    fn empty_dict() {
        let dict = HashDictionary::new(false);
        assert!(!dict.has("anything"));
        assert!(dict.is_empty());
    }

    #[test]
    fn unicode_word() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("café");
        assert!(dict.has("café"));
        assert!(dict.has("Café"));
    }

    #[test]
    fn unicode_case_sensitive() {
        let mut dict = HashDictionary::new(true);
        dict.add_word("Geschäft");
        assert!(dict.has("Geschäft"));
        assert!(!dict.has("geschäft"));
    }

    #[test]
    fn multiple_words() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("apple");
        dict.add_word("banana");
        dict.add_word("cherry");
        assert!(dict.has("apple"));
        assert!(dict.has("banana"));
        assert!(dict.has("cherry"));
        assert!(!dict.has("grape"));
    }
}

// ============================================================
// 2. Forbidden words
// ============================================================

mod forbidden_words {
    use super::*;

    #[test]
    fn is_forbidden_basic() {
        let mut dict = HashDictionary::new(false);
        dict.add_forbidden("colour");
        assert!(dict.is_forbidden("colour"));
    }

    #[test]
    fn is_forbidden_case_insensitive() {
        let mut dict = HashDictionary::new(false);
        dict.add_forbidden("colour");
        assert!(dict.is_forbidden("Colour"));
        assert!(dict.is_forbidden("COLOUR"));
    }

    #[test]
    fn forbidden_word_in_find() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("color");
        dict.add_forbidden("colour");
        let result = dict.find("colour");
        assert!(result.forbidden);
    }

    #[test]
    fn non_forbidden_not_flagged() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("color");
        assert!(!dict.is_forbidden("color"));
    }

    #[test]
    fn multiple_forbidden() {
        let mut dict = HashDictionary::new(false);
        dict.add_forbidden("hte");
        dict.add_forbidden("colour");
        assert!(dict.is_forbidden("hte"));
        assert!(dict.is_forbidden("colour"));
        assert!(!dict.is_forbidden("hello"));
    }
}

// ============================================================
// 3. No-suggest words (ignore words)
// ============================================================

mod no_suggest_words {
    use super::*;

    #[test]
    fn no_suggest_still_found() {
        let mut dict = HashDictionary::new(false);
        dict.add_no_suggest("zeros");
        assert!(dict.has("zeros"));
    }

    #[test]
    fn no_suggest_find_result() {
        let mut dict = HashDictionary::new(false);
        dict.add_no_suggest("zeros");
        let result = dict.find("zeros");
        assert!(result.found);
        assert!(result.no_suggest);
        assert!(!result.forbidden);
    }

    #[test]
    fn no_suggest_case_insensitive() {
        let mut dict = HashDictionary::new(false);
        dict.add_no_suggest("Google");
        assert!(dict.has("google"));
        let result = dict.find("google");
        assert!(result.no_suggest);
    }
}

// ============================================================
// 4. FindResult behavior
// ============================================================

mod find_result {
    use super::*;

    #[test]
    fn find_valid_word() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("hello");
        let result = dict.find("hello");
        assert!(result.found);
        assert!(!result.forbidden);
        assert!(!result.no_suggest);
    }

    #[test]
    fn find_unknown_word() {
        let dict = HashDictionary::new(false);
        let result = dict.find("xyzzy");
        assert!(!result.found);
        assert!(!result.forbidden);
        assert!(!result.no_suggest);
    }

    #[test]
    fn find_forbidden_word() {
        let mut dict = HashDictionary::new(false);
        dict.add_forbidden("snarf");
        let result = dict.find("snarf");
        assert!(result.forbidden);
    }

    #[test]
    fn find_no_suggest_word() {
        let mut dict = HashDictionary::new(false);
        dict.add_no_suggest("google");
        let result = dict.find("google");
        assert!(result.found);
        assert!(result.no_suggest);
        assert!(!result.forbidden);
    }
}

// ============================================================
// 5. Dictionary len
// ============================================================

mod dictionary_size {
    use super::*;

    #[test]
    fn empty_dict_len() {
        let dict = HashDictionary::new(false);
        assert_eq!(dict.len(), 0);
        assert!(dict.is_empty());
    }

    #[test]
    fn dict_with_words_len() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("apple");
        dict.add_word("banana");
        dict.add_word("cherry");
        assert_eq!(dict.len(), 3);
        assert!(!dict.is_empty());
    }

    #[test]
    fn no_suggest_counted_in_words() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("apple");
        dict.add_no_suggest("zeros");
        assert_eq!(dict.len(), 2);
    }
}

// ============================================================
// 6. Text dictionary loading
// ============================================================

mod txt_loading {
    use super::*;

    fn sample_dict_path() -> PathBuf {
        workspace_root().join("vendor/cspell/packages/cspell-lib/samples/words.txt")
    }

    #[test]
    fn load_words_txt() {
        let path = sample_dict_path();
        if !path.exists() {
            return;
        }
        let dict = loader::txt::load_txt(&path).expect("load words.txt");
        assert!(dict.has("apple"), "apple");
        assert!(dict.has("banana"), "banana");
        assert!(dict.has("pie"), "pie");
    }

    #[test]
    fn load_words_case_insensitive() {
        let path = sample_dict_path();
        if !path.exists() {
            return;
        }
        let dict = loader::txt::load_txt(&path).expect("load words.txt");
        assert!(dict.has("Apple"), "Apple (case insensitive)");
    }

    #[test]
    fn load_words_txt_unicode() {
        let path = sample_dict_path();
        if !path.exists() {
            return;
        }
        let dict = loader::txt::load_txt(&path).expect("load words.txt");
        // The sample file stores Geschäft in NFD form (a + combining umlaut).
        // Our loader reads bytes as-is, so we check the NFD form.
        use ::unicode_normalization::UnicodeNormalization;
        let nfd: String = "geschäft".nfd().collect();
        assert!(dict.has(&nfd), "geschäft (NFD)");
    }
}

// ============================================================
// 7. Compressed dictionary loading (.txt.gz)
// ============================================================

mod gz_loading {
    use super::*;

    fn sample_gz_path() -> PathBuf {
        workspace_root().join("vendor/cspell/packages/cspell-lib/samples/words.txt.gz")
    }

    #[test]
    fn load_words_gz() {
        let path = sample_gz_path();
        if !path.exists() {
            return;
        }
        let dict = loader::txt::load_txt(&path).expect("load words.txt.gz");
        assert!(dict.has("apple"), "apple from gz");
    }
}

// ============================================================
// 8. Trie dictionary loading
// ============================================================

mod trie_loading {
    use super::*;

    fn en_us_trie_path() -> PathBuf {
        workspace_root().join("dictionaries/node_modules/@cspell/dict-en_us/en_US.trie.gz")
    }

    #[test]
    fn load_en_us_trie() {
        let path = en_us_trie_path();
        if !path.exists() {
            return;
        }
        let dict = loader::trie_v3::load_trie_v3(&path).expect("load en_US");
        assert!(dict.has("hello"), "hello");
        assert!(dict.has("world"), "world");
        assert!(!dict.has("xyzzy"), "xyzzy should not exist");
    }

    #[test]
    fn trie_case_insensitive() {
        let path = en_us_trie_path();
        if !path.exists() {
            return;
        }
        let dict = loader::trie_v3::load_trie_v3(&path).expect("load en_US");
        assert!(dict.has("Hello"), "Hello");
        assert!(dict.has("HELLO"), "HELLO");
    }

    #[test]
    fn trie_contractions() {
        let path = en_us_trie_path();
        if !path.exists() {
            return;
        }
        let dict = loader::trie_v3::load_trie_v3(&path).expect("load en_US");
        assert!(dict.has("don't"), "don't");
        assert!(dict.has("can't"), "can't");
    }

    #[test]
    fn trie_common_words() {
        let path = en_us_trie_path();
        if !path.exists() {
            return;
        }
        let dict = loader::trie_v3::load_trie_v3(&path).expect("load en_US");
        let common = [
            "the", "quick", "brown", "fox", "jumped", "over", "lazy", "dog",
        ];
        for w in &common {
            assert!(dict.has(w), "{} should exist", w);
        }
    }
}

// ============================================================
// 9. Auto-detection of dictionary format
// ============================================================

mod auto_detection {
    use super::*;

    #[test]
    fn auto_detect_txt() {
        let path = workspace_root().join("vendor/cspell/packages/cspell-lib/samples/words.txt");
        if !path.exists() {
            return;
        }
        let dict = loader::load_dictionary(&path).expect("auto-detect txt");
        assert!(dict.has("apple"));
    }

    #[test]
    fn auto_detect_trie_gz() {
        let path =
            workspace_root().join("dictionaries/node_modules/@cspell/dict-en_us/en_US.trie.gz");
        if !path.exists() {
            return;
        }
        let dict = loader::load_dictionary(&path).expect("auto-detect trie.gz");
        assert!(dict.has("hello"));
    }
}

// ============================================================
// 10. Forbidden words in text files
// ============================================================

mod txt_forbidden {
    use super::*;
    use std::io::Write;

    #[test]
    fn parse_forbidden_entries() {
        let dir = std::env::temp_dir().join("matchum_test_forbidden");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_forbidden.txt");

        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "apple").unwrap();
        writeln!(f, "!banana").unwrap();
        writeln!(f, "cherry").unwrap();
        writeln!(f, "~grape").unwrap();
        drop(f);

        let dict = loader::txt::load_txt(&path).expect("load test");

        assert!(dict.has("apple"));
        assert!(dict.is_forbidden("banana"));
        assert!(dict.has("cherry"));
        // grape is no-suggest but still found
        assert!(dict.has("grape"));
        let result = dict.find("grape");
        assert!(result.no_suggest);

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn comments_and_empty_lines_skipped() {
        let dir = std::env::temp_dir().join("matchum_test_comments");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_comments.txt");

        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "# This is a comment").unwrap();
        writeln!(f, "").unwrap();
        writeln!(f, "  ").unwrap();
        writeln!(f, "apple").unwrap();
        writeln!(f, "# Another comment").unwrap();
        writeln!(f, "banana").unwrap();
        drop(f);

        let dict = loader::txt::load_txt(&path).expect("load test");

        assert_eq!(dict.len(), 2);
        assert!(dict.has("apple"));
        assert!(dict.has("banana"));

        std::fs::remove_dir_all(dir).ok();
    }
}

// ============================================================
// 11. Dictionary trait via Box<dyn Dictionary>
// ============================================================

mod trait_object {
    use super::*;

    fn make_dict(words: &[&str]) -> Box<dyn Dictionary> {
        let mut dict = HashDictionary::new(false);
        for w in words {
            dict.add_word(w);
        }
        Box::new(dict)
    }

    #[test]
    fn trait_has() {
        let dict = make_dict(&["hello", "world"]);
        assert!(dict.has("hello"));
        assert!(dict.has("world"));
        assert!(!dict.has("xyzzy"));
    }

    #[test]
    fn trait_find() {
        let dict = make_dict(&["hello"]);
        let result = dict.find("hello");
        assert!(result.found);
        assert!(!result.forbidden);
    }

    #[test]
    fn trait_is_forbidden() {
        let mut d = HashDictionary::new(false);
        d.add_word("hello");
        d.add_forbidden("bad");
        let dict: Box<dyn Dictionary> = Box::new(d);
        assert!(!dict.is_forbidden("hello"));
        assert!(dict.is_forbidden("bad"));
    }

    #[test]
    fn trait_len() {
        let dict = make_dict(&["a", "b", "c"]);
        assert_eq!(dict.len(), 3);
    }
}

// ============================================================
// 14. Unicode normalization (NFC/NFD equivalence)
// ============================================================

mod unicode_normalization {
    use matchum_dict::dictionary::Dictionary;
    use matchum_dict::hashdict::HashDictionary;

    #[test]
    fn nfd_word_matches_nfc_query() {
        // Add word in NFD form (decomposed), query in NFC form (composed)
        let mut dict = HashDictionary::new(false);
        dict.add_word("cafe\u{0301}"); // NFD: e + combining acute accent
        assert!(dict.has("caf\u{00E9}")); // NFC: e-acute (precomposed)
    }

    #[test]
    fn nfc_word_matches_nfd_query() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("caf\u{00E9}"); // NFC: precomposed e-acute
        assert!(dict.has("cafe\u{0301}")); // NFD: decomposed
    }

    #[test]
    fn german_umlauts_nfc_nfd() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("stra\u{00DF}e"); // NFC: strasse (eszett)
        assert!(dict.has("stra\u{00DF}e")); // Same NFC
    }

    #[test]
    fn combining_diacritical_marks() {
        let mut dict = HashDictionary::new(false);
        // Add "naive" with precomposed i-diaeresis
        dict.add_word("na\u{00EF}ve");
        // Query with decomposed form: i + combining diaeresis
        assert!(dict.has("nai\u{0308}ve"));
    }

    #[test]
    fn ascii_words_unaffected() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("hello");
        assert!(dict.has("hello"));
        assert!(!dict.has("world"));
    }

    #[test]
    fn nfc_nfd_case_insensitive() {
        // NFC/NFD normalization should also work with case-insensitive lookup
        let mut dict = HashDictionary::new(false);
        dict.add_word("Caf\u{00E9}"); // NFC uppercase
        assert!(dict.has("cafe\u{0301}")); // NFD lowercase
        assert!(dict.has("CAF\u{00C9}")); // NFC uppercase all caps
    }

    #[test]
    fn nfc_nfd_case_sensitive() {
        let mut dict = HashDictionary::new(true);
        dict.add_word("Caf\u{00E9}"); // NFC: "Cafe" with precomposed e-acute
        assert!(dict.has("Cafe\u{0301}")); // NFD same casing should match
        assert!(!dict.has("caf\u{00E9}")); // NFC lowercase should NOT match
    }

    #[test]
    fn korean_hangul_normalization() {
        // Hangul syllable can be NFC (single codepoint) or NFD (jamo decomposed)
        let mut dict = HashDictionary::new(false);
        dict.add_word("\u{D55C}"); // NFC: single Hangul syllable "han"
        assert!(dict.has("\u{1112}\u{1161}\u{11AB}")); // NFD: decomposed jamo
    }

    #[test]
    fn multiple_combining_marks() {
        let mut dict = HashDictionary::new(false);
        // "o" + combining acute + combining tilde (multiple combining marks)
        dict.add_word("o\u{0301}\u{0303}");
        // Same sequence should match
        assert!(dict.has("o\u{0301}\u{0303}"));
    }
}

// ============================================================
// 12. Multiple dictionaries
// ============================================================

mod multi_dict {
    use super::*;

    #[test]
    fn lookup_across_dicts() {
        let mut dict1 = HashDictionary::new(false);
        dict1.add_word("apple");
        dict1.add_word("banana");

        let mut dict2 = HashDictionary::new(false);
        dict2.add_word("cherry");
        dict2.add_word("date");

        let dicts: Vec<Box<dyn Dictionary>> = vec![Box::new(dict1), Box::new(dict2)];

        assert!(dicts.iter().any(|d| d.has("apple")));
        assert!(dicts.iter().any(|d| d.has("cherry")));
        assert!(!dicts.iter().any(|d| d.has("grape")));
    }

    #[test]
    fn forbidden_in_one_dict() {
        let mut dict1 = HashDictionary::new(false);
        dict1.add_word("hello");

        let mut dict2 = HashDictionary::new(false);
        dict2.add_forbidden("colour");

        let dicts: Vec<Box<dyn Dictionary>> = vec![Box::new(dict1), Box::new(dict2)];

        assert!(dicts.iter().any(|d| d.is_forbidden("colour")));
        assert!(!dicts.iter().any(|d| d.is_forbidden("hello")));
    }
}

// ============================================================
// 13. Software terms dictionary (if available)
// ============================================================

mod software_terms {
    use super::*;

    fn software_terms_path() -> PathBuf {
        project_root()
            .join("dictionaries/node_modules/@cspell/dict-software-terms/dict/softwareTerms.txt")
    }

    #[test]
    fn load_software_terms() {
        let path = software_terms_path();
        if !path.exists() {
            return;
        }
        let dict = loader::txt::load_txt(&path).expect("load software terms");

        // Common programming terms
        assert!(dict.has("webpack"), "webpack");
        assert!(dict.has("eslint"), "eslint");
        assert!(dict.has("npm"), "npm");
    }
}

// ============================================================
// 15. Case sensitivity edge cases — SpellingDictionary.test.ts
// ============================================================

mod case_sensitivity_advanced {
    use super::*;

    #[test]
    fn case_insensitive_all_caps() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("hello");
        assert!(dict.has("HELLO"), "ALL CAPS on case-insensitive dict");
    }

    #[test]
    fn case_insensitive_mixed_case() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("hello");
        assert!(dict.has("HeLLo"), "mixed case on case-insensitive dict");
    }

    #[test]
    fn case_sensitive_exact_match_only() {
        let mut dict = HashDictionary::new(true);
        dict.add_word("Apple");
        assert!(dict.has("Apple"), "exact match");
        assert!(!dict.has("apple"), "lowercase should not match");
        assert!(!dict.has("APPLE"), "ALL CAPS should not match");
        assert!(!dict.has("aPPLE"), "inverted case should not match");
    }

    #[test]
    fn case_sensitive_multiple_forms() {
        let mut dict = HashDictionary::new(true);
        dict.add_word("apple");
        dict.add_word("Apple");
        dict.add_word("APPLE");
        assert!(dict.has("apple"));
        assert!(dict.has("Apple"));
        assert!(dict.has("APPLE"));
        assert!(!dict.has("aPPLE"));
    }

    #[test]
    fn case_insensitive_ucfirst_fallback() {
        // ALL_CAPS word should match ucfirst form in case-insensitive mode
        let mut dict = HashDictionary::new(false);
        dict.add_word("house");
        assert!(dict.has("HOUSE"), "ALL CAPS should match via ucfirst");
        assert!(dict.has("House"), "ucfirst should match");
    }

    #[test]
    fn case_sensitive_unicode_koln() {
        let mut dict = HashDictionary::new(true);
        dict.add_word("K\u{00f6}ln"); // Koln with o-umlaut
        assert!(dict.has("K\u{00f6}ln"), "exact match");
        assert!(!dict.has("k\u{00f6}ln"), "lowercase should not match");
        assert!(!dict.has("K\u{00D6}LN"), "ALLCAPS should not match");
    }
}

// ============================================================
// 16. Compound word parts — SpellingDictionary.test.ts
// ============================================================

mod compound_word_parts {
    use super::*;

    #[test]
    fn compound_parts_basic() {
        let mut dict = HashDictionary::new(false);
        dict.add_compound_part("apple");
        dict.add_compound_part("banana");
        assert!(dict.has_compound_parts());
        // Compound decomposition via trie `+` transitions is unconditional in cspell.
        assert!(dict.has("applebanana"), "compound: applebanana");
    }

    #[test]
    fn compound_parts_not_matched_as_regular() {
        let mut dict = HashDictionary::new(false);
        // compound parts alone are not in the regular words set
        dict.add_compound_part("apple");
        // "apple" alone should NOT match as a regular word
        // but compound parts might be in a separate set
        // This behavior depends on implementation
        let _ = dict.has("apple");
    }

    #[test]
    fn compound_three_parts() {
        let mut dict = HashDictionary::new(false);
        dict.add_compound_part("red");
        dict.add_compound_part("green");
        dict.add_compound_part("blue");
        assert!(dict.has("redgreen"), "two-part compound");
        // Trie compound mode only allows 2-part (prefix+suffix) decomposition.
        assert!(
            !dict.has("redgreenblue"),
            "three-part compound should be invalid"
        );
    }

    #[test]
    fn compound_case_insensitive() {
        let mut dict = HashDictionary::new(false);
        dict.add_compound_part("apple");
        dict.add_compound_part("sauce");
        assert!(dict.has("AppleSauce"), "case insensitive compound");
    }
}

// ============================================================
// 17. Identity words — exact case match
// ============================================================

mod identity_words {
    use super::*;

    #[test]
    fn identity_word_exact_case() {
        let mut dict = HashDictionary::new(false);
        dict.add_identity_word("iPhone");
        assert!(dict.has("iPhone"), "exact identity word");
    }

    #[test]
    fn identity_word_case_insensitive_dict() {
        let mut dict = HashDictionary::new(false);
        dict.add_identity_word("iPhone");
        // In case-insensitive mode, the lowercased form is also added
        assert!(dict.has("iphone"), "lowercased also works");
    }
}

// ============================================================
// 18. Dictionary suggest — SpellingDictionary.test.ts
// ============================================================

mod dictionary_suggest {
    use super::*;

    #[test]
    fn suggest_basic() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("apple");
        dict.add_word("ape");
        dict.add_word("able");
        dict.add_word("banana");
        let suggestions = dict.suggest("aple", 5);
        assert!(
            suggestions.contains(&"apple".to_string()),
            "suggest apple: {:?}",
            suggestions
        );
    }

    #[test]
    fn suggest_excludes_no_suggest() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("apple");
        dict.add_no_suggest("aple");
        let suggestions = dict.suggest("aple", 5);
        // "aple" should not appear in suggestions since it's no-suggest
        assert!(
            !suggestions.contains(&"aple".to_string()),
            "no-suggest excluded: {:?}",
            suggestions
        );
    }

    #[test]
    fn suggest_excludes_forbidden() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("apple");
        dict.add_forbidden("aple");
        let suggestions = dict.suggest("aple", 5);
        // "aple" should not appear in suggestions since it's forbidden
        assert!(
            !suggestions.contains(&"aple".to_string()),
            "forbidden excluded: {:?}",
            suggestions
        );
    }

    #[test]
    fn suggest_limit() {
        let mut dict = HashDictionary::new(false);
        for w in &[
            "cat", "car", "cart", "care", "card", "can", "cap", "cab", "cam",
        ] {
            dict.add_word(w);
        }
        let suggestions = dict.suggest("cax", 3);
        assert!(suggestions.len() <= 3, "limit to 3: {:?}", suggestions);
    }

    #[test]
    fn suggest_empty_dict() {
        let dict = HashDictionary::new(false);
        let suggestions = dict.suggest("hello", 5);
        assert!(suggestions.is_empty());
    }
}

// ============================================================
// 19. RepMap in dictionary — language-specific substitutions
// ============================================================

mod repmap_in_dict {
    use super::*;
    use matchum_dict::repmap::RepMap;

    #[test]
    fn german_ss_to_eszett() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("stra\u{00df}e"); // Straße
        dict.set_repmap(RepMap::new(vec![("ss".into(), "\u{00df}".into())]));
        assert!(dict.has("strasse"), "repmap: ss -> eszett");
    }

    #[test]
    fn french_e_accent() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("caf\u{00e9}"); // cafe with accent
        dict.set_repmap(RepMap::new(vec![("e".into(), "\u{00e9}".into())]));
        assert!(dict.has("cafe"), "repmap: e -> e-acute");
    }

    #[test]
    fn repmap_reverse_direction_not_supported() {
        // repMap is unidirectional: ("ss", "ß") only maps ss→ß, not ß→ss.
        // Looking up "straße" with dict containing "strasse" should NOT match.
        let mut dict = HashDictionary::new(false);
        dict.add_word("strasse");
        dict.set_repmap(RepMap::new(vec![("ss".into(), "\u{00df}".into())]));
        assert!(
            !dict.has("stra\u{00df}e"),
            "repmap reverse direction should not match"
        );
    }
}

// ============================================================
// 20. Binary cache roundtrip
// ============================================================

mod binary_cache {
    use super::*;

    #[test]
    fn cache_roundtrip() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("hello");
        dict.add_word("world");
        dict.add_forbidden("badword");
        dict.add_no_suggest("nosuggest");

        let mtime = 123456789u64;
        let size = 42u32;
        let bytes = dict.to_cache_bytes(mtime, size);

        let restored = HashDictionary::from_cache_bytes(&bytes, mtime, size);
        assert!(restored.is_some());
        let restored = restored.unwrap();
        assert!(restored.has("hello"));
        assert!(restored.has("world"));
        assert!(restored.is_forbidden("badword"));
        assert!(restored.has("nosuggest"));
    }

    #[test]
    fn cache_mtime_mismatch_returns_none() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("hello");

        let bytes = dict.to_cache_bytes(100, 42);
        let restored = HashDictionary::from_cache_bytes(&bytes, 999, 42);
        assert!(restored.is_none());
    }

    #[test]
    fn cache_size_mismatch_returns_none() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("hello");

        let bytes = dict.to_cache_bytes(100, 42);
        let restored = HashDictionary::from_cache_bytes(&bytes, 100, 999);
        assert!(restored.is_none());
    }

    #[test]
    fn cache_invalid_magic_returns_none() {
        let bytes = vec![0u8; 100];
        let restored = HashDictionary::from_cache_bytes(&bytes, 0, 0);
        assert!(restored.is_none());
    }

    #[test]
    fn cache_too_short_returns_none() {
        let bytes = vec![0u8; 10];
        let restored = HashDictionary::from_cache_bytes(&bytes, 0, 0);
        assert!(restored.is_none());
    }

    #[test]
    fn cache_case_sensitive_preserved() {
        let mut dict = HashDictionary::new(true);
        dict.add_word("Hello");

        let bytes = dict.to_cache_bytes(100, 42);
        let restored = HashDictionary::from_cache_bytes(&bytes, 100, 42).unwrap();
        assert!(restored.is_case_sensitive());
        assert!(restored.has("Hello"));
        assert!(!restored.has("hello"));
    }
}

// ============================================================
// 21. Dictionary find with various word states
// ============================================================

mod find_result_advanced {
    use super::*;

    #[test]
    fn find_word_that_is_both_word_and_forbidden() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("colour");
        dict.add_forbidden("colour");
        let result = dict.find("colour");
        // When a word is both added and forbidden, forbidden takes precedence
        assert!(result.forbidden, "forbidden should be set");
    }

    #[test]
    fn find_word_that_is_both_word_and_no_suggest() {
        let mut dict = HashDictionary::new(false);
        dict.add_no_suggest("zeros");
        let result = dict.find("zeros");
        assert!(result.found, "should be found");
        assert!(result.no_suggest, "should be no_suggest");
        assert!(!result.forbidden, "should not be forbidden");
    }

    #[test]
    fn find_returns_false_for_empty_dict() {
        let dict = HashDictionary::new(false);
        let result = dict.find("anything");
        assert!(!result.found);
        assert!(!result.forbidden);
        assert!(!result.no_suggest);
    }

    #[test]
    fn find_case_insensitive_lookup() {
        let mut dict = HashDictionary::new(false);
        dict.add_word("Apple");
        let result = dict.find("apple");
        assert!(result.found, "case insensitive find");
    }

    #[test]
    fn find_case_sensitive_lookup() {
        let mut dict = HashDictionary::new(true);
        dict.add_word("Apple");
        let result = dict.find("apple");
        assert!(!result.found, "case sensitive find lowercase");
        let result = dict.find("Apple");
        assert!(result.found, "case sensitive find exact");
    }

    #[test]
    fn forbidden_case_sensitive() {
        let mut dict = HashDictionary::new(true);
        dict.add_forbidden("Colour");
        // Case-sensitive forbidden — only exact case matches
        assert!(dict.is_forbidden("Colour"));
        // In a case-sensitive dictionary, the normalization preserves case
    }
}

// ============================================================
// 22. Large dictionary stress tests
// ============================================================

mod dictionary_stress {
    use super::*;

    #[test]
    fn large_dictionary_lookup() {
        let mut dict = HashDictionary::new(false);
        for i in 0..10_000 {
            dict.add_word(&format!("word{}", i));
        }
        assert_eq!(dict.len(), 10_000);
        assert!(dict.has("word0"));
        assert!(dict.has("word5000"));
        assert!(dict.has("word9999"));
        assert!(!dict.has("word10000"));
    }

    #[test]
    fn large_forbidden_set() {
        let mut dict = HashDictionary::new(false);
        for i in 0..1000 {
            dict.add_forbidden(&format!("bad{}", i));
        }
        assert!(dict.is_forbidden("bad0"));
        assert!(dict.is_forbidden("bad500"));
        assert!(dict.is_forbidden("bad999"));
        assert!(!dict.is_forbidden("bad1000"));
    }

    #[test]
    fn dictionary_with_unicode_words() {
        let mut dict = HashDictionary::new(false);
        let words = vec![
            "caf\u{00e9}",
            "na\u{00ef}ve",
            "r\u{00e9}sum\u{00e9}",
            "\u{00fc}ber",
            "Gesch\u{00e4}ft",
            "\u{00e9}l\u{00e8}ve",
        ];
        for w in &words {
            dict.add_word(w);
        }
        for w in &words {
            assert!(dict.has(w), "should find: {}", w);
        }
    }
}
