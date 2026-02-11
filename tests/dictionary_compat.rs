//! Dictionary tests ported from cspell's SpellingDictionary and DictionaryLoader tests.
//!
//! Sources:
//! - vendor/cspell/packages/cspell-dictionary/src/SpellingDictionary/SpellingDictionary.test.ts
//! - vendor/cspell/packages/cspell-dictionary/src/SpellingDictionary/FlagWordsDictionary.test.ts
//! - vendor/cspell/packages/cspell-dictionary/src/SpellingDictionary/IgnoreWordsDictionary.test.ts
//! - vendor/cspell/packages/cspell-lib/src/lib/SpellingDictionary/DictionaryLoader.test.ts
//! - vendor/cspell/packages/cspell-trie-lib/src/lib/SimpleDictionaryParser.test.ts

use ruspell_dict::dictionary::Dictionary;
use ruspell_dict::hashdict::HashDictionary;
use ruspell_dict::loader;
use std::path::PathBuf;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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
        project_root().join("vendor/cspell/packages/cspell-lib/samples/words.txt")
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
        use unicode_normalization::UnicodeNormalization;
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
        project_root().join("vendor/cspell/packages/cspell-lib/samples/words.txt.gz")
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
        project_root().join("dictionaries/node_modules/@cspell/dict-en_us/en_US.trie.gz")
    }

    #[test]
    fn load_en_us_trie() {
        let path = en_us_trie_path();
        if !path.exists() {
            return;
        }
        let dict =
            loader::trie_v3::load_trie_v3(&path).expect("load en_US");
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
        let dict =
            loader::trie_v3::load_trie_v3(&path).expect("load en_US");
        assert!(dict.has("Hello"), "Hello");
        assert!(dict.has("HELLO"), "HELLO");
    }

    #[test]
    fn trie_contractions() {
        let path = en_us_trie_path();
        if !path.exists() {
            return;
        }
        let dict =
            loader::trie_v3::load_trie_v3(&path).expect("load en_US");
        assert!(dict.has("don't"), "don't");
        assert!(dict.has("can't"), "can't");
    }

    #[test]
    fn trie_common_words() {
        let path = en_us_trie_path();
        if !path.exists() {
            return;
        }
        let dict =
            loader::trie_v3::load_trie_v3(&path).expect("load en_US");
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
        let path = project_root().join("vendor/cspell/packages/cspell-lib/samples/words.txt");
        if !path.exists() {
            return;
        }
        let dict = loader::load_dictionary(&path).expect("auto-detect txt");
        assert!(dict.has("apple"));
    }

    #[test]
    fn auto_detect_trie_gz() {
        let path = project_root()
            .join("dictionaries/node_modules/@cspell/dict-en_us/en_US.trie.gz");
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
        let dir = std::env::temp_dir().join("ruspell_test_forbidden");
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
        let dir = std::env::temp_dir().join("ruspell_test_comments");
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
        project_root().join(
            "dictionaries/node_modules/@cspell/dict-software-terms/dict/softwareTerms.txt",
        )
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
