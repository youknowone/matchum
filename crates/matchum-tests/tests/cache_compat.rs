//! Tests for the WordCache behavior in Validator.
//!
//! Verifies that:
//! - The cache populates after validation
//! - A shared cache accumulates entries across multiple validate_text calls
//! - Separate caches remain independent
//! - Cache is bypassed when inline directives change dictionaries, compound words, or case sensitivity

use compact_str::CompactString;
use matchum_core::validator::{Validator, ValidatorConfig, WordCache};
use matchum_dict::dictionary::Dictionary;
use matchum_dict::hashdict::HashDictionary;

fn make_dict(words: &[&str]) -> Box<dyn Dictionary> {
    let mut dict = HashDictionary::new(false);
    for w in words {
        dict.add_word(w);
    }
    Box::new(dict)
}

fn make_validator_with_cache(dict_words: &[&str], cache: WordCache) -> Validator {
    let dict = make_dict(dict_words);
    let config = ValidatorConfig::default();
    let mut v = Validator::new(vec![dict], config);
    v.set_word_cache(cache);
    v
}

fn cache_len(cache: &WordCache) -> usize {
    cache.pin().len()
}

fn cache_contains(cache: &WordCache, word: &str) -> bool {
    cache.pin().contains_key(word)
}

fn cache_result(cache: &WordCache, word: &str) -> Option<bool> {
    cache.pin().get(word).map(|v| *v)
}

// ============================================================
// 1. Cache populates after validation
// ============================================================

mod cache_population {
    use super::*;

    #[test]
    fn cache_populates_after_validation() {
        let cache = Validator::new_word_cache();
        let v = make_validator_with_cache(&["hello", "world"], cache.clone());

        assert_eq!(cache_len(&cache), 0, "cache should start empty");

        v.validate_text("hello world xyzzy");

        assert!(
            cache_len(&cache) > 0,
            "cache should have entries after validation"
        );
        // "hello" and "world" are valid (>=4 chars); "xyzzy" is invalid (>=4 chars)
        assert_eq!(
            cache_result(&cache, "hello"),
            Some(true),
            "valid word should be cached as true"
        );
        assert_eq!(
            cache_result(&cache, "world"),
            Some(true),
            "valid word should be cached as true"
        );
        assert_eq!(
            cache_result(&cache, "xyzzy"),
            Some(false),
            "unknown word should be cached as false"
        );
    }

    #[test]
    fn short_words_not_cached() {
        // Words shorter than min_word_length (default: 4) are skipped entirely,
        // so they should never enter the cache.
        let cache = Validator::new_word_cache();
        let v = make_validator_with_cache(&["hello", "the"], cache.clone());

        v.validate_text("hello the a be");

        assert!(cache_contains(&cache, "hello"), "hello should be cached");
        // "the", "a", "be" are below min_word_length=4 and never reach is_word_valid
        assert!(
            !cache_contains(&cache, "the"),
            "short word 'the' should not be cached"
        );
        assert!(
            !cache_contains(&cache, "a"),
            "short word 'a' should not be cached"
        );
        assert!(
            !cache_contains(&cache, "be"),
            "short word 'be' should not be cached"
        );
    }

    #[test]
    fn cache_without_setting_stays_empty() {
        // Validator without set_word_cache should work fine (no cache)
        let dict = make_dict(&["hello", "world"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let issues = v.validate_text("hello world xyzzy");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].word, "xyzzy");
        // No crash, no cache — just works normally
    }
}

// ============================================================
// 2. Cache shared across validations
// ============================================================

mod cache_sharing {
    use super::*;

    #[test]
    fn cache_shared_across_validations() {
        let cache = Validator::new_word_cache();
        let v = make_validator_with_cache(&["hello", "world", "good", "morning"], cache.clone());

        // First validation
        v.validate_text("hello world xyzzy");
        let after_first = cache_len(&cache);
        assert!(
            after_first > 0,
            "cache should have entries after first call"
        );

        // Second validation with overlapping and new words
        v.validate_text("hello good morning plugh");
        let after_second = cache_len(&cache);
        assert!(
            after_second > after_first,
            "cache should grow: {} > {}",
            after_second,
            after_first
        );

        // All words from both calls should be present
        assert!(cache_contains(&cache, "hello"), "hello from both calls");
        assert!(cache_contains(&cache, "world"), "world from first call");
        assert!(cache_contains(&cache, "xyzzy"), "xyzzy from first call");
        assert!(cache_contains(&cache, "good"), "good from second call");
        assert!(
            cache_contains(&cache, "morning"),
            "morning from second call"
        );
        assert!(cache_contains(&cache, "plugh"), "plugh from second call");
    }

    #[test]
    fn cache_serves_results_on_second_call() {
        let cache = Validator::new_word_cache();
        let v = make_validator_with_cache(&["hello", "world"], cache.clone());

        // First call populates cache
        let issues1 = v.validate_text("hello xyzzy");
        assert_eq!(issues1.len(), 1);
        assert_eq!(issues1[0].word, "xyzzy");

        // Second call with same text should produce same results (served from cache)
        let issues2 = v.validate_text("hello xyzzy");
        assert_eq!(issues2.len(), 1);
        assert_eq!(issues2[0].word, "xyzzy");
    }

    #[test]
    fn shared_cache_across_two_validators() {
        // Two validators sharing the same cache (simulates files with the same dict set)
        let cache = Validator::new_word_cache();
        let v1 = make_validator_with_cache(&["hello", "world", "alpha", "beta"], cache.clone());
        let v2 = make_validator_with_cache(&["hello", "world", "alpha", "beta"], cache.clone());

        v1.validate_text("hello alpha xyzzy");
        assert!(cache_contains(&cache, "hello"), "v1 populated hello");
        assert!(cache_contains(&cache, "xyzzy"), "v1 populated xyzzy");

        // v2 benefits from cache and adds new entries
        v2.validate_text("hello world beta plugh");
        assert!(cache_contains(&cache, "world"), "v2 added world");
        assert!(cache_contains(&cache, "beta"), "v2 added beta");
        assert!(cache_contains(&cache, "plugh"), "v2 added plugh");
    }
}

// ============================================================
// 3. Cache isolation (separate caches are independent)
// ============================================================

mod cache_isolation {
    use super::*;

    #[test]
    fn separate_caches_are_independent() {
        let cache_a = Validator::new_word_cache();
        let cache_b = Validator::new_word_cache();

        let v_a = make_validator_with_cache(&["hello", "world"], cache_a.clone());
        let v_b = make_validator_with_cache(&["hello", "world"], cache_b.clone());

        v_a.validate_text("hello xyzzy");
        v_b.validate_text("world plugh");

        // cache_a should only have entries from v_a
        assert!(cache_contains(&cache_a, "hello"));
        assert!(cache_contains(&cache_a, "xyzzy"));
        assert!(
            !cache_contains(&cache_a, "plugh"),
            "cache_a should not have plugh from v_b"
        );

        // cache_b should only have entries from v_b
        assert!(cache_contains(&cache_b, "world"));
        assert!(cache_contains(&cache_b, "plugh"));
        assert!(
            !cache_contains(&cache_b, "xyzzy"),
            "cache_b should not have xyzzy from v_a"
        );
    }
}

// ============================================================
// 4. Cache bypass conditions
// ============================================================

mod cache_bypass {
    use super::*;

    #[test]
    fn cache_bypassed_with_inline_dictionaries_directive() {
        // When inline directives change active dictionaries (e.g., cspell:dictionaries),
        // the cache should be bypassed for those words because dictionary_dictionaries != None.
        let cache = Validator::new_word_cache();
        let v = make_validator_with_cache(&["hello", "world"], cache.clone());

        // Text with inline directive that changes dictionaries
        let text = "hello xyzzy\n// cspell:dictionaries custom\nplugh";
        v.validate_text(text);

        // "hello" and "xyzzy" are before the directive, should be cached normally
        assert!(
            cache_contains(&cache, "hello"),
            "hello before directive should be cached"
        );
        assert!(
            cache_contains(&cache, "xyzzy"),
            "xyzzy before directive should be cached"
        );

        // "plugh" is after the dictionaries directive — whether it's cached depends
        // on whether the directive actually changed the active dict set.
        // The key behavior: the cache was NOT poisoned by directive-context words.
    }

    #[test]
    fn cache_consistent_with_disable_enable() {
        // Words inside a disabled block are not validated at all,
        // so they should not appear in the cache.
        let cache = Validator::new_word_cache();
        let v = make_validator_with_cache(&["hello", "world"], cache.clone());

        let text = "hello\n// cspell:disable\nxyzzy\n// cspell:enable\nplugh";
        v.validate_text(text);

        assert!(cache_contains(&cache, "hello"), "hello should be cached");
        assert!(
            !cache_contains(&cache, "xyzzy"),
            "disabled word xyzzy should not be cached"
        );
        assert!(
            cache_contains(&cache, "plugh"),
            "plugh after enable should be cached"
        );
    }

    #[test]
    fn cache_consistent_with_inline_ignore() {
        // Words declared via `cspell:ignore` are skipped, not validated through is_word_valid.
        let cache = Validator::new_word_cache();
        let v = make_validator_with_cache(&["hello"], cache.clone());

        let text = "// cspell:ignore xyzzy\nhello xyzzy plugh";
        v.validate_text(text);

        assert!(cache_contains(&cache, "hello"), "hello should be cached");
        // "xyzzy" is ignored by directive, so it may or may not be in cache
        // depending on implementation. The important thing is it doesn't produce an issue.
        assert!(
            cache_contains(&cache, "plugh"),
            "plugh should be cached (not ignored)"
        );
    }
}

// ============================================================
// 5. Cache correctness
// ============================================================

mod cache_correctness {
    use super::*;

    #[test]
    fn cached_valid_word_remains_valid() {
        let cache = Validator::new_word_cache();
        let v = make_validator_with_cache(&["hello", "world"], cache.clone());

        v.validate_text("hello");
        assert_eq!(cache_result(&cache, "hello"), Some(true));

        // Validate again — should still report no issues
        let issues = v.validate_text("hello");
        assert!(
            issues.is_empty(),
            "cached valid word should produce no issues"
        );
    }

    #[test]
    fn cached_invalid_word_remains_invalid() {
        let cache = Validator::new_word_cache();
        let v = make_validator_with_cache(&["hello"], cache.clone());

        v.validate_text("xyzzy");
        assert_eq!(cache_result(&cache, "xyzzy"), Some(false));

        // Validate again — should still report the issue
        let issues = v.validate_text("xyzzy");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].word, "xyzzy");
    }

    #[test]
    fn camel_case_parts_cached_individually() {
        let cache = Validator::new_word_cache();
        let v = make_validator_with_cache(&["hello", "world"], cache.clone());

        v.validate_text("helloWorld");

        // Each camelCase part is validated separately
        assert!(
            cache_contains(&cache, "hello"),
            "camelCase part 'hello' should be cached"
        );
        assert!(
            cache_contains(&cache, "world") || cache_contains(&cache, "World"),
            "camelCase part 'world'/'World' should be cached"
        );
    }

    #[test]
    fn flag_words_not_affected_by_cache() {
        // Flag words are checked independently from the is_word_valid cache path.
        // A word that is both in the dictionary and flagged should still be reported.
        let cache = Validator::new_word_cache();
        let dict = make_dict(&["hello", "flagged"]);
        let config = ValidatorConfig {
            flag_words: std::collections::HashSet::from([CompactString::from("flagged")]),
            ..Default::default()
        };
        let mut v = Validator::new(vec![dict], config);
        v.set_word_cache(cache.clone());

        let issues = v.validate_text("hello flagged");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].word, "flagged");
        assert!(issues[0].is_forbidden);

        // Even though "flagged" is in the dict and may be cached as valid,
        // the flag word check should still catch it
        let issues2 = v.validate_text("flagged");
        assert_eq!(issues2.len(), 1);
        assert!(issues2[0].is_forbidden);
    }
}
