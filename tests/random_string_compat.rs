use matchum_core::random::{is_random_string, word_likeness_score};

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
fn gibberish_is_random() {
    assert!(is_random_string("xjfklsd", 6));
    assert!(is_random_string("qwzxjk", 6));
    assert!(is_random_string("bvnmxz", 6));
}

#[test]
fn base64_strings_are_random() {
    assert!(is_random_string("H4sIAAAAAAAAA72d3ZLjNpK", 6));
    assert!(is_random_string("izfrNTmQLnfsLzi2Wb9xPz2Qj9fQYG", 6));
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
    let hello_score = word_likeness_score("hello");
    let gibberish_score = word_likeness_score("xjfklsd");
    assert!(
        hello_score > gibberish_score,
        "hello ({hello_score}) should score higher than xjfklsd ({gibberish_score})"
    );
}
