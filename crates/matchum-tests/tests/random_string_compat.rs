use matchum_core::random::{is_random_string, score_random_string};

#[test]
fn common_words_are_not_random() {
    assert!(!is_random_string("hello", 6));
    assert!(!is_random_string("function", 6));
    assert!(!is_random_string("algorithm", 6));
    assert!(!is_random_string("the", 6)); // too short
    assert!(!is_random_string("implementation", 6));
    assert!(!is_random_string("interface", 6));
}

#[test]
fn all_lowercase_gibberish_is_not_random() {
    // All-lowercase strings match \p{Ll}+ as a single "word" unit,
    // so categorizeString compresses them to "1" → low score → not random.
    assert!(!is_random_string("xjfklsd", 6));
    assert!(!is_random_string("qwzxjk", 6));
    assert!(!is_random_string("bvnmxz", 6));
}

#[test]
fn base64_strings_scoring() {
    // categorizeString compresses these below the 0.5 random threshold.
    assert!(!is_random_string("H4sIAAAAAAAAA72d3ZLjNpK", 6));
    // This has enough category transitions to score above threshold.
    let score = score_random_string("izfrNTmQLnfsLzi2Wb9xPz2Qj9fQYG");
    assert!(
        score >= 0.5,
        "mixed base64 should score >= 0.5, got {score}"
    );
}

#[test]
fn hex_hashes_are_random() {
    assert!(is_random_string("a1b2c3d4e5f6", 6));
    assert!(is_random_string("8b338dea4f2c", 6));
}

#[test]
fn short_words_never_random() {
    assert!(!is_random_string("xyz", 6));
    assert!(!is_random_string("ab", 6));
    assert!(!is_random_string("qwert", 6));
}

#[test]
fn camel_case_parts_not_random() {
    // Individual camelCase parts should not be random
    assert!(!is_random_string("sample", 6));
    assert!(!is_random_string("standard", 6));
    assert!(!is_random_string("multisample", 6));
}

#[test]
fn score_ordering() {
    // Real words should score higher than gibberish
    let hello_score = score_random_string("hello");
    let gibberish_score = score_random_string("xjfklsd");
    assert!(
        hello_score > gibberish_score,
        "hello ({hello_score}) should score higher than xjfklsd ({gibberish_score})"
    );
}
