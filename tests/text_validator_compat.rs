// spell-checker:ignore aardvark
//! Text validation tests ported from cspell's textValidator.test.ts
//! and docValidator.test.ts.
//!
//! Sources:
//! - vendor/cspell/packages/cspell-lib/src/lib/textValidation/textValidator.test.ts
//! - vendor/cspell/packages/cspell-lib/src/lib/textValidation/docValidator.test.ts

use matchum_core::issue::ValidationIssue;
use matchum_core::validator::{Validator, ValidatorConfig};
use matchum_dict::dictionary::Dictionary;
use matchum_dict::hashdict::HashDictionary;

fn make_dict(words: &[&str]) -> Box<dyn Dictionary> {
    let mut dict = HashDictionary::new(false);
    for w in words {
        dict.add_word(w);
    }
    Box::new(dict)
}

fn make_dict_with_forbidden(words: &[&str], forbidden: &[&str]) -> Box<dyn Dictionary> {
    let mut dict = HashDictionary::new(false);
    for w in words {
        dict.add_word(w);
    }
    for w in forbidden {
        dict.add_forbidden(w);
    }
    Box::new(dict)
}

fn issue_words(issues: &[ValidationIssue]) -> Vec<&str> {
    issues.iter().map(|i| i.word.as_str()).collect()
}

/// Dictionary sets matching cspell's textValidator.test.ts
fn sample_dicts() -> Vec<Box<dyn Dictionary>> {
    let colors = make_dict(&[
        "red", "green", "blue", "black", "white", "orange", "purple", "yellow", "gray", "brown",
        "light", "dark",
    ]);
    let fruit = make_dict(&[
        "apple",
        "banana",
        "orange",
        "pear",
        "pineapple",
        "mango",
        "avocado",
        "grape",
        "strawberry",
        "blueberry",
        "blackberry",
        "berry",
    ]);
    let animals = make_dict(&[
        "ape", "lion", "tiger", "elephant", "monkey", "gazelle", "antelope", "aardvark", "hyena",
    ]);
    let insects = make_dict(&[
        "ant",
        "snail",
        "beetle",
        "worm",
        "stink",
        "bug",
        "centipede",
        "millipede",
        "flea",
        "fly",
    ]);
    let common_words = make_dict(&[
        "allowed",
        "and",
        "ate",
        "be",
        "been",
        "better",
        "big",
        "dark",
        "done",
        "fixes",
        "has",
        "have",
        "here",
        "is",
        "known",
        "light",
        "little",
        "multiple",
        "not",
        "problems",
        "published",
        "should",
        "the",
        "there",
        "they",
        "this",
        "to",
        "we",
    ]);

    vec![colors, fruit, animals, insects, common_words]
}

fn make_sample_validator() -> Validator {
    Validator::new(sample_dicts(), ValidatorConfig::default())
}

const SAMPLE_TEXT: &str = "The elephant and giraffe\n\
     The lightbrown worm ate the apple, mango, and, strawberry.\n\
     The little ant ate the big purple grape.\n\
     The orange tiger ate the whiteberry and the redberry.";

// ============================================================
// 1. Basic text validation — textValidator.test.ts
// ============================================================

mod basic_text_validation {
    use super::*;

    #[test]
    fn detects_unknown_words() {
        let v = make_sample_validator();
        let issues = v.validate_text(SAMPLE_TEXT);
        let words = issue_words(&issues);

        // "giraffe" is not in animals dict (only "gazelle", etc.)
        assert!(words.contains(&"giraffe"), "got: {:?}", words);
        // "lightbrown" is a compound of light+brown but compounds not enabled
        assert!(words.contains(&"lightbrown"), "got: {:?}", words);
        // "whiteberry" and "redberry" aren't in the fruit dict
        assert!(words.contains(&"whiteberry"), "got: {:?}", words);
        assert!(words.contains(&"redberry"), "got: {:?}", words);
    }

    #[test]
    fn valid_words_not_flagged() {
        let v = make_sample_validator();
        let issues = v.validate_text(SAMPLE_TEXT);
        let words = issue_words(&issues);

        assert!(!words.contains(&"elephant"), "got: {:?}", words);
        assert!(!words.contains(&"apple"), "got: {:?}", words);
        assert!(!words.contains(&"mango"), "got: {:?}", words);
        assert!(!words.contains(&"worm"), "got: {:?}", words);
        assert!(!words.contains(&"grape"), "got: {:?}", words);
        assert!(!words.contains(&"tiger"), "got: {:?}", words);
        assert!(!words.contains(&"orange"), "got: {:?}", words);
    }
}

// ============================================================
// 2. Flag words with contractions
// ============================================================

mod flag_words_contractions {
    use super::*;

    fn make_validator_with_flags(dict_words: &[&str], flag_words: &[&str]) -> Validator {
        let dict = make_dict(dict_words);
        let config = ValidatorConfig {
            flag_words: flag_words
                .iter()
                .map(|w| compact_str::CompactString::from(w.to_lowercase()))
                .collect(),
            ..Default::default()
        };
        Validator::new(vec![dict], config)
    }

    #[test]
    fn flag_ate_in_sentence() {
        // flagWords should be checked even for words below min_word_length
        let common = &["the", "ant", "ate", "antelope"];
        let v = make_validator_with_flags(common, &["ate"]);
        let issues = v.validate_text("The ant ate the antelope.");
        let words = issue_words(&issues);
        assert!(words.contains(&"ate"), "got: {:?}", words);
        assert!(
            issues.iter().any(|i| i.word == "ate" && i.is_forbidden),
            "ate should be forbidden"
        );
    }

    #[test]
    fn flag_antelope_in_sentence() {
        let common = &["the", "ant", "ate", "antelope"];
        let v = make_validator_with_flags(common, &["antelope"]);
        let issues = v.validate_text("The ant ate the antelope.");
        let words = issue_words(&issues);
        assert!(words.contains(&"antelope"), "got: {:?}", words);
        assert!(issues
            .iter()
            .any(|i| i.word == "antelope" && i.is_forbidden));
    }

    #[test]
    fn flag_word_not_in_text() {
        let common = &["the", "ant", "ate", "antelope"];
        let v = make_validator_with_flags(common, &["fbd"]);
        let issues = v.validate_text("The ant ate the antelope.");
        assert!(issues.is_empty(), "got: {:?}", issue_words(&issues));
    }

    #[test]
    fn no_issues_on_valid_text() {
        let common = &["this", "should", "be", "ok"];
        let v = make_validator_with_flags(common, &[]);
        let issues = v.validate_text("This should be ok");
        assert!(issues.is_empty());
    }
}

// ============================================================
// 3. Forbidden words in dictionary
// ============================================================

mod forbidden_in_dict {
    use super::*;

    #[test]
    fn dict_forbidden_word_reported() {
        let dict = make_dict_with_forbidden(&["color"], &["colour"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());
        let issues = v.validate_text("colour");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].word, "colour");
        assert!(issues[0].is_forbidden);
    }

    #[test]
    fn dict_forbidden_case_insensitive() {
        let dict = make_dict_with_forbidden(&["color"], &["colour"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());
        let issues = v.validate_text("Colour");
        assert_eq!(issues.len(), 1);
        assert!(issues[0].is_forbidden);
    }

    #[test]
    fn flag_word_in_camel_case() {
        let common = &["the", "ant", "ate", "antelope"];
        let dict = make_dict(common);
        let config = ValidatorConfig {
            flag_words: ["ate"]
                .iter()
                .map(|w| compact_str::CompactString::from(w.to_lowercase()))
                .collect(),
            ..Default::default()
        };
        let v = Validator::new(vec![dict], config);
        // "ate" inside camelCase: theANT_ateThe_antelope
        let issues = v.validate_text("theANT_ateThe_antelope");
        let words = issue_words(&issues);
        // "ate" should be extracted from camelCase and flagged
        assert!(
            words.contains(&"ate"),
            "ate in camelCase should be flagged: {:?}",
            words
        );
    }
}

// ============================================================
// 4. Repeated single letter words
// ============================================================

mod repeated_letters {
    use super::*;

    #[test]
    fn single_repeated_letters_skipped() {
        // tttt, gggg, xxxxxxx, jjjjj — all same character repeated 4+ times
        // cspell skips these via regExRepeatedChar: /^(\w)\1{3,}$/i
        let v = make_sample_validator();
        let text = "tttt gggg xxxxxxx jjjjj";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        assert!(
            words.is_empty(),
            "repeated letters should be skipped: {:?}",
            words
        );
    }

    #[test]
    fn mixed_repeated_with_real_words() {
        let v = make_sample_validator();
        let text = "xxxkxxxx xxxbxxxx";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        // These should be flagged as unknown
        assert!(words.contains(&"xxxkxxxx"), "got: {:?}", words);
        assert!(words.contains(&"xxxbxxxx"), "got: {:?}", words);
    }
}

// ============================================================
// 5. Multiple lines and positions
// ============================================================

mod multiline_positions {
    use super::*;

    #[test]
    fn correct_line_numbers() {
        let text = "hello apple\ngiraffe worm\nxyzzy tiger";

        let dict = make_dict(&["hello", "apple", "worm", "tiger"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let issues = v.validate_text(text);
        let giraffe = issues.iter().find(|i| i.word == "giraffe");
        let xyzzy = issues.iter().find(|i| i.word == "xyzzy");

        assert!(giraffe.is_some(), "giraffe not found");
        assert_eq!(giraffe.unwrap().line, 2);

        assert!(xyzzy.is_some(), "xyzzy not found");
        assert_eq!(xyzzy.unwrap().line, 3);
    }

    #[test]
    fn correct_column_positions() {
        let dict = make_dict(&["hello"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "hello xyzzy world";
        let issues = v.validate_text(text);

        let xyzzy = issues.iter().find(|i| i.word == "xyzzy");
        let world = issues.iter().find(|i| i.word == "world");

        assert!(xyzzy.is_some());
        assert_eq!(xyzzy.unwrap().column, 7); // 0-based offset=6, column=7

        assert!(world.is_some());
        assert_eq!(world.unwrap().column, 13);
    }
}

// ============================================================
// 6. Disable/enable directives in full text
// ============================================================

mod directive_validation {
    use super::*;

    #[test]
    fn disable_block_skips_words() {
        let dict = make_dict(&[
            "hello", "world", "main", "console", "log", "message", "const",
        ]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "// cspell:disable-next-line\n\
                     const message = 'Helllo world';\n\
                     \n\
                     // cspell:disable\n\
                     const messages = ['muawhahahaha', 'grrrr', 'uuug', 'aarf'];\n\
                     // cspell:enable\n\
                     \n\
                     function main() {\n\
                         console.log(message);\n\
                     }";

        let issues = v.validate_text(text);
        let words = issue_words(&issues);

        // "Helllo" on line 2 is covered by disable-next-line
        assert!(
            !words.contains(&"Helllo"),
            "disabled next line: {:?}",
            words
        );

        // Words inside disable block should not be flagged
        assert!(
            !words.contains(&"muawhahahaha"),
            "disabled block: {:?}",
            words
        );
        assert!(!words.contains(&"grrrr"), "disabled block: {:?}", words);

        // "function" after enable should be checked
        assert!(words.contains(&"function"), "after enable: {:?}", words);
    }

    #[test]
    fn inline_words_directive_adds_words() {
        let dict = make_dict(&["hello"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "// cspell:words customword specialterm\nhello customword specialterm";
        let issues = v.validate_text(text);
        assert!(
            issues.is_empty(),
            "words directive: {:?}",
            issue_words(&issues)
        );
    }

    #[test]
    fn inline_ignore_case_insensitive() {
        let dict = make_dict(&["hello"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "// cspell:ignore CUSTOMWORD\nhello CustomWord";
        let issues = v.validate_text(text);
        assert!(
            issues.is_empty(),
            "ignore should be case insensitive: {:?}",
            issue_words(&issues)
        );
    }
}

// ============================================================
// 7. E2E with real dictionaries — docValidator-like tests
// ============================================================

mod e2e_doc_validation {
    use super::*;
    use matchum_dict::loader;
    use std::path::PathBuf;

    fn project_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn load_en_us() -> Option<Box<dyn Dictionary>> {
        let dict_path =
            project_root().join("dictionaries/node_modules/@cspell/dict-en_us/en_US.trie.gz");
        if !dict_path.exists() {
            return None;
        }
        Some(Box::new(
            loader::trie_v3::load_trie_v3(&dict_path).expect("load en_US"),
        ))
    }

    #[test]
    fn sample_with_errors() {
        let dict = match load_en_us() {
            Some(d) => d,
            None => return,
        };
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        // Similar to docValidator's sample-with-errors.ts
        let text = "/**\n * This is the dockblock comment for the main function.\n */\n\
                     function main() {\n    const message = 'Helllo world';\n    console.log(message);\n}\n\nmain();";

        let issues = v.validate_text(text);
        let words = issue_words(&issues);

        assert!(
            words.contains(&"dockblock"),
            "dockblock should be flagged: {:?}",
            words
        );
        assert!(
            words.contains(&"Helllo"),
            "Helllo should be flagged: {:?}",
            words
        );
    }

    #[test]
    fn sample_with_many_errors() {
        let dict = match load_en_us() {
            Some(d) => d,
            None => return,
        };
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "/**\n * Creates a reciever withe a naame\n * @param naame - the naame of the reciever.\n */\n\
                     export function createReciever(naame: string) {\n\
                         return {\n\
                             naame: naame,\n\
                             kount: 0,\n\
                         };\n\
                     }";

        let issues = v.validate_text(text);
        let words = issue_words(&issues);

        assert!(words.contains(&"reciever"), "got: {:?}", words);
        assert!(words.contains(&"naame"), "got: {:?}", words);
        assert!(words.contains(&"kount"), "got: {:?}", words);
    }

    #[test]
    fn sample_with_cspell_directives() {
        let dict = match load_en_us() {
            Some(d) => d,
            None => return,
        };
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "// cspell:disable-next-line\n\
                     const message = 'Helllo world';\n\
                     \n\
                     // cspell:disable\n\
                     const messages = ['muawhahahaha', 'grrrr', 'uuug', 'aarf'];\n\
                     // cspell:enable\n\
                     \n\
                     function main() {\n\
                         console.log(message);\n\
                     }\n\
                     \n\
                     main();";

        let issues = v.validate_text(text);
        let words = issue_words(&issues);

        // Disabled areas should not be flagged
        assert!(!words.contains(&"Helllo"), "disable-next-line: {:?}", words);
        assert!(
            !words.contains(&"muawhahahaha"),
            "disabled block: {:?}",
            words
        );
        assert!(!words.contains(&"grrrr"), "disabled block: {:?}", words);
    }

    #[test]
    fn validate_real_fixture_file() {
        let dict = match load_en_us() {
            Some(d) => d,
            None => return,
        };

        let fixture = project_root()
            .join("vendor/cspell/packages/cspell-lib/fixtures/docValidator/sample-with-errors.ts");
        if !fixture.exists() {
            return;
        }

        let text = std::fs::read_to_string(&fixture).unwrap();
        let v = Validator::new(vec![dict], ValidatorConfig::default());
        let issues = v.validate_text(&text);
        let words = issue_words(&issues);

        assert!(
            words.contains(&"Helllo") || words.contains(&"dockblock"),
            "should find errors in fixture: {:?}",
            words
        );
    }

    #[test]
    fn validate_multiple_errors_fixture() {
        let dict = match load_en_us() {
            Some(d) => d,
            None => return,
        };

        let fixture = project_root().join(
            "vendor/cspell/packages/cspell-lib/fixtures/docValidator/sample-with-many-errors.ts",
        );
        if !fixture.exists() {
            return;
        }

        let text = std::fs::read_to_string(&fixture).unwrap();
        let v = Validator::new(vec![dict], ValidatorConfig::default());
        let issues = v.validate_text(&text);
        let words = issue_words(&issues);

        // Known errors from docValidator.test.ts
        assert!(words.contains(&"reciever"), "got: {:?}", words);
        assert!(words.contains(&"naame"), "got: {:?}", words);
    }
}

// ============================================================
// 8. URL/email/hex pattern skipping in complex text
// ============================================================

mod pattern_skipping {
    use super::*;

    #[test]
    fn complex_url_skipped() {
        let dict = make_dict(&["visit", "website", "for", "details", "the"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text =
            "Visit the website https://www.example.com/path/to/page?q=hello&lang=en for details.";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);

        assert!(!words.contains(&"https"), "URL: {:?}", words);
        assert!(!words.contains(&"example"), "URL: {:?}", words);
        assert!(!words.contains(&"page"), "URL: {:?}", words);
    }

    #[test]
    fn email_address_skipped() {
        let dict = make_dict(&["contact", "at", "send", "email", "to", "the"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "Send email to user@example.com for contact.";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);

        assert!(!words.contains(&"user"), "email: {:?}", words);
        assert!(!words.contains(&"example"), "email: {:?}", words);
    }

    #[test]
    fn sha_hash_skipped() {
        let dict = make_dict(&["commit", "hash", "the"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "commit hash: a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);

        // SHA-like hash should be skipped
        assert!(
            !words.iter().any(|w| w.len() >= 40),
            "SHA hash: {:?}",
            words
        );
    }

    #[test]
    fn hex_literal_skipped() {
        let dict = make_dict(&["const", "value", "the"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "const value = 0xDEADBEEF;";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);

        assert!(!words.contains(&"DEADBEEF"), "hex literal: {:?}", words);
    }
}

// ============================================================
// 9. Config-level ignore patterns
// ============================================================

mod config_ignore_patterns {
    use super::*;

    #[test]
    fn ignore_regexp_skips_matching_words() {
        let dict = make_dict(&["hello", "world"]);
        let patterns: Vec<regex::Regex> = vec![regex::Regex::new(r"mis\w+ed").unwrap()];
        let config = ValidatorConfig {
            ignore_patterns: patterns,
            ..Default::default()
        };
        let v = Validator::new(vec![dict], config);

        let text = "hello mispelled world";
        let issues = v.validate_text(text);
        assert!(
            issues.is_empty(),
            "pattern should skip: {:?}",
            issue_words(&issues)
        );
    }

    #[test]
    fn multiple_ignore_patterns() {
        let dict = make_dict(&["hello"]);
        let patterns: Vec<regex::Regex> = vec![
            regex::Regex::new(r"x{3,}").unwrap(),
            regex::Regex::new(r"z{3,}").unwrap(),
        ];
        let config = ValidatorConfig {
            ignore_patterns: patterns,
            ..Default::default()
        };
        let v = Validator::new(vec![dict], config);

        let text = "hello xxxxx zzzzz";
        let issues = v.validate_text(text);
        assert!(
            issues.is_empty(),
            "patterns should skip: {:?}",
            issue_words(&issues)
        );
    }
}

// ============================================================
// 10. LocalWords (Emacs style) in validation
// ============================================================

mod local_words_validation {
    use super::*;

    #[test]
    fn local_words_adds_to_known() {
        let dict = make_dict(&["hello"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "% LocalWords: customterm specialword\nhello customterm specialword";
        let issues = v.validate_text(text);
        assert!(
            issues.is_empty(),
            "LocalWords should add words: {:?}",
            issue_words(&issues)
        );
    }

    #[test]
    fn local_words_case_insensitive_match() {
        let dict = make_dict(&["hello"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "% LocalWords: CustomWord\nhello customword";
        let issues = v.validate_text(text);
        assert!(
            issues.is_empty(),
            "LocalWords case insensitive: {:?}",
            issue_words(&issues)
        );
    }
}

// ============================================================
// 11. Extended directive directives skip checking
// ============================================================

mod extended_directive_validation {
    use super::*;

    #[test]
    fn language_directive_line_skipped() {
        let dict = make_dict(&["hello"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        // The directive line itself should be skipped
        let text = "// cspell:language en-US\nhello";
        let issues = v.validate_text(text);
        assert!(
            issues.is_empty(),
            "language directive: {:?}",
            issue_words(&issues)
        );
    }

    #[test]
    fn dictionaries_directive_line_skipped() {
        let dict = make_dict(&["hello"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "// cspell:dictionaries custom-dict\nhello";
        let issues = v.validate_text(text);
        assert!(
            issues.is_empty(),
            "dictionaries directive: {:?}",
            issue_words(&issues)
        );
    }

    #[test]
    fn enable_compound_words_directive_line_skipped() {
        let dict = make_dict(&["hello"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "// cspell:enableCompoundWords\nhello";
        let issues = v.validate_text(text);
        assert!(
            issues.is_empty(),
            "compound directive: {:?}",
            issue_words(&issues)
        );
    }
}

// ============================================================
// 12. Case sensitivity in validation
// ============================================================

mod case_sensitivity {
    use super::*;

    #[test]
    fn case_insensitive_default() {
        let dict = make_dict(&["Seattle"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let issues = v.validate_text("seattle");
        assert!(
            issues.is_empty(),
            "case insensitive: {:?}",
            issue_words(&issues)
        );
    }

    #[test]
    fn case_sensitive_mode() {
        let mut dict = matchum_dict::hashdict::HashDictionary::new(true);
        dict.add_word("Seattle");
        let dict: Box<dyn Dictionary> = Box::new(dict);

        let config = ValidatorConfig {
            case_sensitive: true,
            ..Default::default()
        };
        let v = Validator::new(vec![dict], config);

        let issues = v.validate_text("seattle");
        assert!(
            !issues.is_empty(),
            "case sensitive should flag lowercase: {:?}",
            issue_words(&issues)
        );
    }

    #[test]
    fn case_sensitive_correct_case() {
        let mut dict = matchum_dict::hashdict::HashDictionary::new(true);
        dict.add_word("Seattle");
        let dict: Box<dyn Dictionary> = Box::new(dict);

        let config = ValidatorConfig {
            case_sensitive: true,
            ..Default::default()
        };
        let v = Validator::new(vec![dict], config);

        let issues = v.validate_text("Seattle");
        assert!(
            issues.is_empty(),
            "exact case match: {:?}",
            issue_words(&issues)
        );
    }
}
