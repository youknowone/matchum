use std::path::PathBuf;

use matchum_dict::dictionary::Dictionary;
use matchum_dict::loader::hunspell::{expand_word, parse_aff, parse_dic};
use matchum_dict::loader::load_dictionary;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hunspell")
}

// --- parse_aff tests ---

#[test]
fn test_parse_aff_suffix_rules() {
    let aff = "\
SET UTF-8
SFX S Y 1
SFX S 0 s .
";
    let (prefixes, suffixes) = parse_aff(aff);
    assert!(prefixes.is_empty());
    assert!(suffixes.contains_key(&'S'));
    let rules = &suffixes[&'S'];
    assert_eq!(rules.len(), 1);
    assert!(rules[0].strip.is_empty());
    assert_eq!(rules[0].add, "s");
    assert!(rules[0].cross_product);
}

#[test]
fn test_parse_aff_prefix_rules() {
    let aff = "\
SET UTF-8
PFX U Y 1
PFX U 0 un .
";
    let (prefixes, suffixes) = parse_aff(aff);
    assert!(suffixes.is_empty());
    assert!(prefixes.contains_key(&'U'));
    let rules = &prefixes[&'U'];
    assert_eq!(rules.len(), 1);
    assert!(rules[0].strip.is_empty());
    assert_eq!(rules[0].add, "un");
    assert!(rules[0].cross_product);
}

#[test]
fn test_parse_aff_multiple_suffix_rules() {
    let aff = "\
SET UTF-8
SFX D Y 2
SFX D 0 ed [^e]
SFX D e ed e
";
    let (_, suffixes) = parse_aff(aff);
    let rules = &suffixes[&'D'];
    assert_eq!(rules.len(), 2);
    // First rule: strip nothing, add "ed", condition [^e]
    assert!(rules[0].strip.is_empty());
    assert_eq!(rules[0].add, "ed");
    assert_eq!(rules[0].condition.as_deref(), Some("[^e]"));
    // Second rule: strip "e", add "ed", condition "e"
    assert_eq!(rules[1].strip, "e");
    assert_eq!(rules[1].add, "ed");
    assert_eq!(rules[1].condition.as_deref(), Some("e"));
}

#[test]
fn test_parse_aff_no_cross_product() {
    let aff = "\
SET UTF-8
SFX X N 1
SFX X 0 ly .
";
    let (_, suffixes) = parse_aff(aff);
    let rules = &suffixes[&'X'];
    assert_eq!(rules.len(), 1);
    assert!(!rules[0].cross_product);
}

// --- parse_dic tests ---

#[test]
fn test_parse_dic_basic() {
    let dic = "\
3
hello/S
world
run/SU
";
    let words = parse_dic(dic);
    assert_eq!(words.len(), 3);
    assert_eq!(words[0].0, "hello");
    assert_eq!(words[0].1, vec!['S']);
    assert_eq!(words[1].0, "world");
    assert!(words[1].1.is_empty());
    assert_eq!(words[2].0, "run");
    assert_eq!(words[2].1, vec!['S', 'U']);
}

#[test]
fn test_parse_dic_no_count_line() {
    // If the first line is not a number, treat it as a word
    let dic = "\
hello/S
world
";
    let words = parse_dic(dic);
    assert_eq!(words.len(), 2);
    assert_eq!(words[0].0, "hello");
    assert_eq!(words[1].0, "world");
}

// --- expand_word tests ---

#[test]
fn test_expand_suffix_only() {
    let aff = "\
SET UTF-8
SFX S Y 1
SFX S 0 s .
";
    let (prefixes, suffixes) = parse_aff(aff);
    let forms = expand_word("run", &['S'], &prefixes, &suffixes);
    assert!(forms.contains(&"run".to_string()));
    assert!(forms.contains(&"runs".to_string()));
    assert_eq!(forms.len(), 2);
}

#[test]
fn test_expand_prefix_only() {
    let aff = "\
SET UTF-8
PFX U Y 1
PFX U 0 un .
";
    let (prefixes, suffixes) = parse_aff(aff);
    let forms = expand_word("happy", &['U'], &prefixes, &suffixes);
    assert!(forms.contains(&"happy".to_string()));
    assert!(forms.contains(&"unhappy".to_string()));
    assert_eq!(forms.len(), 2);
}

#[test]
fn test_expand_cross_product() {
    let aff = "\
SET UTF-8
SFX S Y 1
SFX S 0 s .
PFX U Y 1
PFX U 0 un .
";
    let (prefixes, suffixes) = parse_aff(aff);
    let forms = expand_word("run", &['S', 'U'], &prefixes, &suffixes);
    assert!(forms.contains(&"run".to_string()));
    assert!(forms.contains(&"runs".to_string()));
    assert!(forms.contains(&"unrun".to_string()));
    assert!(forms.contains(&"unruns".to_string()));
    assert_eq!(forms.len(), 4);
}

#[test]
fn test_expand_condition_matching() {
    let aff = "\
SET UTF-8
SFX D Y 2
SFX D 0 ed [^e]
SFX D e ed e
";
    let (prefixes, suffixes) = parse_aff(aff);

    // "walk" ends with 'k' (not 'e'), so first rule applies: walk + ed = walked
    let forms = expand_word("walk", &['D'], &prefixes, &suffixes);
    assert!(forms.contains(&"walk".to_string()));
    assert!(forms.contains(&"walked".to_string()));
    // Second rule should not match (condition "e" doesn't match "walk")
    assert!(!forms.contains(&"walke".to_string()));

    // "make" ends with 'e', so second rule applies: strip "e", add "ed" = maked
    let forms = expand_word("make", &['D'], &prefixes, &suffixes);
    assert!(forms.contains(&"make".to_string()));
    assert!(forms.contains(&"maked".to_string()));
    // First rule should NOT match since condition [^e] doesn't match 'e' ending
    assert!(!forms.contains(&"makeed".to_string()));
}

#[test]
fn test_expand_strip_suffix() {
    let aff = "\
SET UTF-8
SFX G Y 2
SFX G 0 ing [^e]
SFX G e ing e
";
    let (prefixes, suffixes) = parse_aff(aff);

    // "make" ends with 'e': strip 'e', add 'ing' = "making"
    let forms = expand_word("make", &['G'], &prefixes, &suffixes);
    assert!(forms.contains(&"make".to_string()));
    assert!(forms.contains(&"making".to_string()));
    assert_eq!(forms.len(), 2);

    // "walk" ends with 'k' (not 'e'): add 'ing' = "walking"
    let forms = expand_word("walk", &['G'], &prefixes, &suffixes);
    assert!(forms.contains(&"walk".to_string()));
    assert!(forms.contains(&"walking".to_string()));
    assert_eq!(forms.len(), 2);
}

#[test]
fn test_expand_no_matching_flags() {
    let aff = "\
SET UTF-8
SFX S Y 1
SFX S 0 s .
";
    let (prefixes, suffixes) = parse_aff(aff);

    // Word has flag 'X' which has no rules defined
    let forms = expand_word("test", &['X'], &prefixes, &suffixes);
    assert_eq!(forms, vec!["test".to_string()]);
}

// --- load_dictionary integration tests ---

#[test]
fn test_load_hunspell_fixture() {
    let dic_path = fixtures_dir().join("test.dic");
    let dict = load_dictionary(&dic_path).unwrap();

    // Base words
    assert!(dict.has("hello"), "should have 'hello'");
    assert!(dict.has("world"), "should have 'world'");
    assert!(dict.has("run"), "should have 'run'");
    assert!(dict.has("happy"), "should have 'happy'");
    assert!(dict.has("walk"), "should have 'walk'");
    assert!(dict.has("make"), "should have 'make'");
    assert!(dict.has("build"), "should have 'build'");

    // Suffix expansions
    assert!(dict.has("hellos"), "should have 'hellos' (S suffix)");
    assert!(dict.has("runs"), "should have 'runs' (S suffix)");
    assert!(dict.has("walks"), "should have 'walks' (S suffix)");
    assert!(dict.has("builds"), "should have 'builds' (S suffix)");

    // Prefix expansions
    assert!(dict.has("unrun"), "should have 'unrun' (U prefix)");
    assert!(dict.has("unhappy"), "should have 'unhappy' (U prefix)");
    assert!(dict.has("rebuild"), "should have 'rebuild' (R prefix)");

    // Cross product: prefix + suffix for build/SR
    assert!(
        dict.has("rebuilds"),
        "should have 'rebuilds' (R prefix + S suffix cross product)"
    );

    // D suffix: walked
    assert!(dict.has("walked"), "should have 'walked' (D suffix)");

    // G suffix: walking, making
    assert!(dict.has("walking"), "should have 'walking' (G suffix)");
    assert!(dict.has("making"), "should have 'making' (G suffix)");

    // Should NOT have random words
    assert!(!dict.has("xyzzy"), "should not have random word");
}

#[test]
fn test_load_hunspell_cross_product_prefix_suffix() {
    let dic_path = fixtures_dir().join("test.dic");
    let dict = load_dictionary(&dic_path).unwrap();

    // run has flags S and U, both Y cross-product
    // So we get: run, runs, unrun, unruns
    assert!(dict.has("run"));
    assert!(dict.has("runs"));
    assert!(dict.has("unrun"));
    assert!(dict.has("unruns"));
}

#[test]
fn test_load_hunspell_dict_size() {
    let dic_path = fixtures_dir().join("test.dic");
    let dict = load_dictionary(&dic_path).unwrap();

    // Verify dictionary has a reasonable number of words
    // 7 base words + their expansions
    assert!(
        dict.len() > 7,
        "dictionary should have more than base words, got {}",
        dict.len()
    );
}
