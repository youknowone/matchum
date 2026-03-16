//! Integration tests ported from cspell's test suite.
//!
//! These verify that matchum produces identical results to cspell
//! for word splitting, text extraction, directives, and validation.
//!
//! Sources:
//! - vendor/cspell/packages/cspell-lib/src/lib/util/text.test.ts
//! - vendor/cspell/packages/cspell-lib/src/lib/util/wordSplitter.test.ts
//! - vendor/cspell/packages/cspell-lib/src/lib/textValidation/validator.test.ts
//! - vendor/cspell/packages/cspell-lib/src/lib/Settings/InDocSettings.test.ts
//! - vendor/cspell/packages/cspell-trie-lib/src/lib/io/importExport.test.ts

use matchum_config::directives::{self, Directive};
use matchum_core::issue::ValidationIssue;
use matchum_core::splitter;
use matchum_core::validator::{Validator, ValidatorConfig};
use matchum_dict::dictionary::Dictionary;
use matchum_dict::hashdict::HashDictionary;

fn workspace_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

// 1. splitCamelCaseWord — from text.test.ts lines 32-44

mod split_camel_case {
    use super::*;

    fn split(word: &str) -> Vec<&str> {
        splitter::split_camel_case(word)
    }

    // Exact cspell test cases from text.test.ts:
    //   ${'hello'}          | ${'hello'.split('|')}
    //   ${'helloThere'}     | ${['hello', 'There']}
    //   ${'HelloThere'}     | ${['Hello', 'There']}
    //   ${'BigÁpple'}       | ${['Big', 'Ápple']}
    //   ${'ASCIIToUTF16'}   | ${['ASCII', 'To', 'UTF16']}
    //   ${'URLsAndDBAs'}    | ${['URLs', 'And', 'DBAs']}
    //   ${'WALKingRUNning'} | ${['WALKing', 'RUNning']}
    //   ${'c0de'}           | ${['c0de']}

    #[test]
    fn hello() {
        assert_eq!(split("hello"), vec!["hello"]);
    }

    #[test]
    fn hello_there() {
        assert_eq!(split("helloThere"), vec!["hello", "There"]);
    }

    #[test]
    fn hello_there_pascal() {
        assert_eq!(split("HelloThere"), vec!["Hello", "There"]);
    }

    #[test]
    fn big_apple_unicode() {
        assert_eq!(split("BigÁpple"), vec!["Big", "Ápple"]);
    }

    #[test]
    fn ascii_to_utf16() {
        assert_eq!(split("ASCIIToUTF16"), vec!["ASCII", "To", "UTF16"]);
    }

    #[test]
    fn urls_and_dbas() {
        assert_eq!(split("URLsAndDBAs"), vec!["URLs", "And", "DBAs"]);
    }

    #[test]
    fn walking_running() {
        assert_eq!(split("WALKingRUNning"), vec!["WALKing", "RUNning"]);
    }

    #[test]
    fn c0de_with_digit() {
        assert_eq!(split("c0de"), vec!["c0de"]);
    }

    // Additional cspell-compatible cases from wordSplitter.test.ts
    #[test]
    fn error_code() {
        assert_eq!(split("ERRORCode"), vec!["ERROR", "Code"]);
    }

    #[test]
    fn html_parser() {
        assert_eq!(split("HTMLParser"), vec!["HTML", "Parser"]);
    }

    #[test]
    fn xml_http_request() {
        assert_eq!(split("XMLHttpRequest"), vec!["XML", "Http", "Request"]);
    }

    #[test]
    fn html_input() {
        assert_eq!(split("HTMLInput"), vec!["HTML", "Input"]);
    }

    #[test]
    fn get_url() {
        assert_eq!(split("getURL"), vec!["get", "URL"]);
    }

    #[test]
    fn a_hello() {
        assert_eq!(split("aHELLO"), vec!["a", "HELLO"]);
    }

    #[test]
    fn mark_ui_as_ready() {
        // splitCamelCaseWord's English suffix exception treats "As" as suffix "s"
        // after uppercase "A", keeping "UIAs" together. The wordSplitter in cspell
        // uses a different regex that DOES split this into ['mark', 'UI', 'As', 'Ready'].
        assert_eq!(split("markUIAsReady"), vec!["mark", "UIAs", "Ready"]);
    }

    #[test]
    fn screaming_snake_gets_split_by_extract_words() {
        // Pure camelCase splitter should NOT split "SCREAMING" (no boundaries)
        assert_eq!(split("SCREAMING"), vec!["SCREAMING"]);
    }

    #[test]
    fn single_char() {
        assert_eq!(split("a"), vec!["a"]);
        assert_eq!(split("A"), vec!["A"]);
    }

    #[test]
    fn empty() {
        let empty: Vec<&str> = Vec::new();
        assert_eq!(split(""), empty);
    }

    #[test]
    fn all_upper() {
        assert_eq!(split("HTML"), vec!["HTML"]);
        assert_eq!(split("API"), vec!["API"]);
    }
}

// 2. extractWordsFromText

mod extract_words_from_text {
    use super::*;

    fn word_texts(text: &str) -> Vec<String> {
        splitter::extract_words(text)
            .into_iter()
            .map(|w| w.text.to_string())
            .collect()
    }

    // From text.test.ts line 59-76: contractions
    #[test]
    fn contractions() {
        let text = "could've would've couldn't've wasn't y'all 'twas shouldn\u{2019}t";
        let ws = word_texts(text);
        assert!(
            ws.contains(&"could've".to_string()),
            "should extract could've, got: {:?}",
            ws
        );
        assert!(
            ws.contains(&"couldn't've".to_string()),
            "should extract couldn't've, got: {:?}",
            ws
        );
        assert!(
            ws.contains(&"wasn't".to_string()),
            "should extract wasn't, got: {:?}",
            ws
        );
    }

    // From text.test.ts line 79-96: code with parens
    #[test]
    fn words_from_code_line() {
        let text = "expect(splitCamelCaseWord('hello')).to.deep.equal(['hello']);";
        let ws = word_texts(text);
        assert!(ws.contains(&"expect".to_string()));
        assert!(ws.contains(&"splitCamelCaseWord".to_string()));
        assert!(ws.contains(&"to".to_string()));
        assert!(ws.contains(&"deep".to_string()));
        assert!(ws.contains(&"equal".to_string()));
    }

    // From text.test.ts line 203-212: Chinese characters skipping
    #[test]
    fn skip_chinese_characters() {
        let text = r#"<a href="http://www.ctrip.com" title="携程旅行网">携程旅行网</a>"#;
        let ws = word_texts(text);
        // cspell expects: ['a', 'href', 'http', 'www', 'ctrip', 'com', 'title', 'a']
        assert!(ws.contains(&"href".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"ctrip".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"title".to_string()), "got: {:?}", ws);
        // CJK chars should not appear
        assert!(
            !ws.iter().any(|w| w.contains('携')),
            "should not contain Chinese chars"
        );
    }

    // From text.test.ts line 215-226: Japanese characters skipping
    #[test]
    fn skip_japanese_characters() {
        let text = "Example text: gitのpackageのみ際インストール";
        let ws = word_texts(text);
        // cspell expects: ['Example', 'text', 'git', 'package']
        assert!(ws.contains(&"Example".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"text".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"git".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"package".to_string()), "got: {:?}", ws);
    }

    // From text.test.ts line 229-238: Greek characters kept
    #[test]
    fn keep_greek_characters() {
        let text = "Γ γ\tgamma γάμμα";
        let ws = word_texts(text);
        // cspell expects: ['Γ', 'γ', 'gamma', 'γάμμα']
        assert!(ws.contains(&"Γ".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"γ".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"gamma".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"γάμμα".to_string()), "got: {:?}", ws);
    }

    // From text.test.ts line 241-252: Unicode NFC/NFD
    #[test]
    fn unicode_nfc_cafe() {
        let nfc = "café";
        let ws = word_texts(nfc);
        assert_eq!(ws, vec!["café"]);
    }

    #[test]
    fn unicode_nfd_cafe() {
        use unicode_normalization::UnicodeNormalization;
        let nfd: String = "café".nfd().collect();
        let ws = word_texts(&nfd);
        // NFD café should still be extracted as one word
        assert_eq!(ws.len(), 1, "got: {:?}", ws);
    }

    #[test]
    fn snake_case_split() {
        let ws = word_texts("first_line");
        assert_eq!(ws, vec!["first", "line"]);
    }

    #[test]
    fn dot_separated() {
        let ws = word_texts("regExp.match");
        assert_eq!(ws, vec!["regExp", "match"]);
    }
}

// ============================================================
// 3. extractWordsFromCode — from text.test.ts lines 129-192
// ============================================================

mod extract_words_from_code {
    use super::*;

    fn code_word_texts(text: &str) -> Vec<String> {
        splitter::extract_words_from_code(text)
            .into_iter()
            .map(|w| w.text.to_string())
            .collect()
    }

    // From text.test.ts line 129-149: splitCamelCaseWord
    #[test]
    fn split_camel_case_word() {
        let text = "expect(splitCamelCaseWord('hello')).to.deep.equal(['hello']);";
        let ws = code_word_texts(text);
        // cspell expects: ['expect', 'split', 'Camel', 'Case', 'Word', 'hello', 'to', 'deep', 'equal', 'hello']
        assert!(ws.contains(&"expect".to_string()));
        assert!(ws.contains(&"split".to_string()));
        assert!(ws.contains(&"Camel".to_string()));
        assert!(ws.contains(&"Case".to_string()));
        assert!(ws.contains(&"Word".to_string()));
    }

    // From text.test.ts line 150-165: regExp.match
    #[test]
    fn reg_exp_match() {
        let text = "expect(regExp.match(first_line));";
        let ws = code_word_texts(text);
        // cspell expects: ['expect', 'reg', 'Exp', 'match', 'first', 'line']
        assert_eq!(ws, vec!["expect", "reg", "Exp", "match", "first", "line"]);
    }

    // From text.test.ts line 166-178: aHELLO
    #[test]
    fn a_hello() {
        let text = "expect(aHELLO);";
        let ws = code_word_texts(text);
        // cspell expects: ['expect', 'a', 'HELLO']
        assert_eq!(ws, vec!["expect", "a", "HELLO"]);
    }

    // From text.test.ts line 189-191: HTMLInput
    #[test]
    fn html_input_value() {
        let text = "var value = HTMLInput.value;";
        let ws = code_word_texts(text);
        // cspell expects: ['var', 'value', 'HTML', 'Input', 'value']
        assert_eq!(ws, vec!["var", "value", "HTML", "Input", "value"]);
    }

    // CJK in code context
    #[test]
    fn chinese_in_code() {
        let text = r#"<a href="http://www.ctrip.com" title="携程旅行网">携程旅行网</a>"#;
        let ws = code_word_texts(text);
        assert!(ws.contains(&"href".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"ctrip".to_string()), "got: {:?}", ws);
        assert!(
            !ws.iter().any(|w| w.contains('携')),
            "should not contain Chinese"
        );
    }
}

// ============================================================
// 4. Inline Directives — from InDocSettings.test.ts
// ============================================================

mod inline_directives {
    use super::*;

    // From InDocSettings.test.ts: various directive formats
    #[test]
    fn cspell_enable() {
        assert_eq!(
            directives::parse_directive("// cSpell:enable"),
            Some(Directive::Enable)
        );
    }

    #[test]
    fn cspell_disable() {
        assert_eq!(
            directives::parse_directive("// cSpell:disable"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn spell_checker_disable() {
        assert_eq!(
            directives::parse_directive("/* spell-checker:disable */"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn spellchecker_enable() {
        assert_eq!(
            directives::parse_directive("// spellchecker:enable"),
            Some(Directive::Enable)
        );
    }

    #[test]
    fn case_insensitive_cspell() {
        assert_eq!(
            directives::parse_directive("// CSPELL:DISABLE"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn cspell_words() {
        let d = directives::parse_directive("// cSpell:words myword anotherword");
        assert_eq!(
            d,
            Some(Directive::Words(vec![
                "myword".into(),
                "anotherword".into()
            ]))
        );
    }

    #[test]
    fn cspell_ignore() {
        let d = directives::parse_directive("// cspell:ignore brouwn jumpped lazzy");
        assert_eq!(
            d,
            Some(Directive::Ignore(vec![
                "brouwn".into(),
                "jumpped".into(),
                "lazzy".into()
            ]))
        );
    }

    #[test]
    fn cspell_ignore_comma_separated() {
        let d = directives::parse_directive("// cspell:ignore foo,bar,baz");
        assert_eq!(
            d,
            Some(Directive::Ignore(vec![
                "foo".into(),
                "bar".into(),
                "baz".into()
            ]))
        );
    }

    #[test]
    fn disable_line() {
        assert_eq!(
            directives::parse_directive("const x = 1; // spell-checker:disable-line"),
            Some(Directive::DisableLine)
        );
    }

    #[test]
    fn disable_next_line() {
        assert_eq!(
            directives::parse_directive("// cspell:disable-next-line"),
            Some(Directive::DisableNextLine)
        );
    }

    #[test]
    fn html_comment_directive() {
        assert_eq!(
            directives::parse_directive("<!-- cspell:disable -->"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn no_directive() {
        assert_eq!(
            directives::parse_directive("// this is a regular comment"),
            None
        );
    }

    #[test]
    fn hash_comment_directive() {
        assert_eq!(
            directives::parse_directive("# spell-checker: disable"),
            Some(Directive::Disable)
        );
    }
}

// 5. Validator — from validator.test.ts

mod validation {
    use super::*;

    fn make_dict(words: &[&str]) -> Box<dyn Dictionary> {
        let mut dict = HashDictionary::new(false);
        for w in words {
            dict.add_word(w);
        }
        Box::new(dict)
    }

    fn make_validator(
        dict_words: &[&str],
        flag_words: &[&str],
        ignore_words: &[&str],
    ) -> Validator {
        let dict = make_dict(dict_words);
        let config = ValidatorConfig {
            flag_words: flag_words
                .iter()
                .map(|w| compact_str::CompactString::from(w.to_lowercase()))
                .collect(),
            ignore_words: ignore_words
                .iter()
                .map(|w| compact_str::CompactString::from(w.to_lowercase()))
                .collect(),
            ..Default::default()
        };
        Validator::new(vec![dict], config)
    }

    fn issue_words(issues: &[ValidationIssue]) -> Vec<&str> {
        issues.iter().map(|i| i.word.as_str()).collect()
    }

    // From validator.test.ts line 18-25: basic misspelling detection
    #[test]
    fn detects_misspelled_words() {
        // The quick brouwn fox jumpped over the lazzy dog.
        // cspell detects: brouwn, jumpped, lazzy
        let common_words = &[
            "the", "quick", "brown", "fox", "jumped", "over", "lazy", "dog",
        ];
        let v = make_validator(common_words, &[], &[]);
        let text = "The quick brouwn fox jumpped over the lazzy dog.";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        assert!(words.contains(&"brouwn"), "got: {:?}", words);
        assert!(words.contains(&"jumpped"), "got: {:?}", words);
        assert!(words.contains(&"lazzy"), "got: {:?}", words);
    }

    // From validator.test.ts line 27-34: case insensitive
    #[test]
    fn case_insensitive_validation() {
        let common_words = &[
            "the", "quick", "brown", "fox", "jumped", "over", "lazy", "dog",
        ];
        let v = make_validator(common_words, &[], &[]);
        let text = "The Quick brown fox Jumped over the lazy dog.";
        let issues = v.validate_text(text);
        assert!(
            issues.is_empty(),
            "case-insensitive should match: {:?}",
            issue_words(&issues)
        );
    }

    // From validator.test.ts line 106-119: Issue #7 - obvious misspellings
    #[test]
    fn issue_7_obvious_misspellings() {
        let common_words = &[
            "fails",
            "to",
            "detect",
            "obviously",
            "misspelt",
            "words",
            "such",
            "as",
            "hello",
            "apple",
            "banana",
            "respect",
        ];
        let v = make_validator(common_words, &[], &[]);
        let text = "Fails to detect obviously misspelt words, such as:\nhellosd\napplesq\nbananasa\nrespectss";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        assert!(words.contains(&"hellosd"), "got: {:?}", words);
        assert!(words.contains(&"applesq"), "got: {:?}", words);
        assert!(words.contains(&"bananasa"), "got: {:?}", words);
        assert!(words.contains(&"respectss"), "got: {:?}", words);
    }

    // From validator.test.ts line 150-165: flag words and ignore words
    #[test]
    fn flag_words_detected() {
        let sample_words = &[
            "and", "ant", "apple", "ate", "big", "elephant", "giraffe", "grape", "little", "mango",
            "orange", "purple", "the", "tiger", "worm", "hello", "flagged",
        ];
        let flag_words = &["hte", "flagged"];
        let ignore_words = &["ignored"];

        let v = make_validator(sample_words, flag_words, ignore_words);

        // "flagged" is a flag word → should be reported as forbidden
        let issues = v.validate_text("hello flagged");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].word, "flagged");
        assert!(issues[0].is_forbidden);
    }

    #[test]
    fn ignore_words_not_flagged() {
        let sample_words = &["hello"];
        let v = make_validator(sample_words, &[], &["ignored"]);

        let issues = v.validate_text("hello ignored");
        assert!(
            issues.is_empty(),
            "ignored word should not be reported: {:?}",
            issue_words(&issues)
        );
    }

    // From validator.test.ts line 178-192: disable/enable blocks
    #[test]
    fn disable_enable_blocks() {
        let v = make_validator(
            &["verify", "urls", "get", "checked", "const", "value", "url"],
            &[],
            &[],
        );

        let text = r#"// Verify urls get checked.
const value = 'hello';

/* spell-checker:disable */

const xebia = 'zando';
const zooloo = 'ctrip';

/* spell-checker:enable */

const wrongg = 'mispelled';"#;

        let issues = v.validate_text(text);
        let words = issue_words(&issues);

        // Words in disabled block should NOT be flagged
        assert!(!words.contains(&"xebia"), "disabled block: {:?}", words);
        assert!(!words.contains(&"zando"), "disabled block: {:?}", words);
        assert!(!words.contains(&"zooloo"), "disabled block: {:?}", words);
        assert!(!words.contains(&"ctrip"), "disabled block: {:?}", words);

        // Words after enable should be checked
        assert!(words.contains(&"wrongg"), "after enable: {:?}", words);
        assert!(words.contains(&"mispelled"), "after enable: {:?}", words);
    }

    // Inline ignore directive
    #[test]
    fn inline_ignore_directive() {
        let v = make_validator(&["hello"], &[], &[]);

        let text = "// cspell:ignore xyzzy plugh\nhello xyzzy plugh";
        let issues = v.validate_text(text);
        assert!(
            issues.is_empty(),
            "ignored words should not be flagged: {:?}",
            issue_words(&issues)
        );
    }

    // Inline words directive
    #[test]
    fn inline_words_directive() {
        let v = make_validator(&["hello"], &[], &[]);

        let text = "// cspell:words xyzzy plugh\nhello xyzzy plugh";
        let issues = v.validate_text(text);
        assert!(
            issues.is_empty(),
            "words directive should add to dictionary: {:?}",
            issue_words(&issues)
        );
    }

    // disable-next-line
    #[test]
    fn disable_next_line() {
        let v = make_validator(&["hello"], &[], &[]);

        let text = "// cspell:disable-next-line\nxyzzy\nxyzzy";
        let issues = v.validate_text(text);
        assert_eq!(
            issues.len(),
            1,
            "only second xyzzy: {:?}",
            issue_words(&issues)
        );
        assert_eq!(issues[0].line, 3);
    }

    // disable-line
    #[test]
    fn disable_line() {
        let v = make_validator(&["hello"], &[], &[]);

        let text = "xyzzy // spell-checker:disable-line\nxyzzy";
        let issues = v.validate_text(text);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].line, 2);
    }

    // camelCase splitting in validation
    #[test]
    fn camel_case_validation() {
        let v = make_validator(&["hello", "world"], &[], &[]);
        let issues = v.validate_text("helloWorld");
        assert!(
            issues.is_empty(),
            "camelCase: both parts in dict: {:?}",
            issue_words(&issues)
        );
    }

    // Position tracking
    #[test]
    fn position_tracking() {
        let v = make_validator(&["hello"], &[], &[]);
        let text = "hello\nxyzzy\nabcdef";
        let issues = v.validate_text(text);
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].word, "xyzzy");
        assert_eq!(issues[0].line, 2);
        assert_eq!(issues[1].word, "abcdef");
        assert_eq!(issues[1].line, 3);
    }

    // min_word_length
    #[test]
    fn min_word_length() {
        let v = make_validator(&["hello"], &[], &[]);
        // Default min_word_length = 4, so "ab" and "xyz" are skipped
        let issues = v.validate_text("hello ab xyz");
        assert!(issues.is_empty());
    }
}

// ============================================================
// 6. TrieXv3 dictionary loading — from importExport.test.ts
// ============================================================

mod trie_loading {
    use super::*;
    use matchum_dict::loader;

    #[test]
    fn load_sample_trie_v3() {
        let sample_path =
            workspace_root().join("vendor/cspell/packages/cspell-trie-lib/Samples/sampleV3.trie");

        if !sample_path.exists() {
            eprintln!("Skipping: sample trie not found");
            return;
        }

        let dict = loader::trie_v3::load_trie_v3(&sample_path).expect("load sample trie");

        // Verify sample words from sampleData.ts
        let expected_words = [
            "journal",
            "journalism",
            "journalist",
            "journalistic",
            "journals",
            "journey",
            "journeyer",
            "journeyman",
            "joy",
            "joyful",
            "joyfuller",
            "joyfullest",
            "joyfulness",
            "joyless",
            "joylessness",
            "joyous",
            "joyousness",
            "Big Apple",
            "New York",
            "apple",
            "big apple",
            "fun journey",
            "long walk",
            "fun walk",
            "walk",
            "walked",
            "walker",
            "walking",
            "walks",
            "talk",
            "talked",
            "talker",
            "talking",
            "talks",
            "chalk",
            "chalked",
            "chalker",
            "chalking",
            "chalks",
            "lift",
            "lifted",
            "lifter",
            "lifting",
            "lifts",
            "turn",
            "turned",
            "turner",
            "turning",
            "turns",
            "burn",
            "burned",
            "burner",
            "burning",
            "burns",
            "churn",
            "churned",
            "churner",
            "churning",
            "churns",
        ];

        for word in &expected_words {
            assert!(dict.has(word), "trie should contain '{}'", word);
        }

        // Special characters from sampleData.ts specialCharacters
        assert!(dict.has("arrow <"), "should have 'arrow <'");
        assert!(dict.has("escape \\"), "should have 'escape \\\\'");
        assert!(dict.has("eol \n"), "should have 'eol \\n'");
        assert!(dict.has("eow $"), "should have 'eow $'");
        assert!(dict.has("ref #"), "should have 'ref #'");
        assert!(
            dict.has("Numbers 0123456789"),
            "should have 'Numbers 0123456789'"
        );
        assert!(dict.has("Braces: {}[]()"), "should have 'Braces: {{}}[]()'");

        // Non-latin scripts
        assert!(dict.has("ᐊᓂᔑᓈᐯᒧᐎᓐ"), "should have Anishinaabemowin");
        assert!(dict.has("ᓀᐦᐃᔭᐍᐏᐣ"), "should have Nehiyawewin");
    }

    #[test]
    fn load_en_us_trie() {
        let dict_path =
            workspace_root().join("dictionaries/node_modules/@cspell/dict-en_us/en_US.trie.gz");

        if !dict_path.exists() {
            eprintln!("Skipping: en_US dict not found");
            return;
        }

        let dict = loader::trie_v3::load_trie_v3(&dict_path).expect("load en_US trie");

        // Basic words
        assert!(dict.has("hello"));
        assert!(dict.has("world"));
        assert!(dict.has("the"));
        assert!(dict.has("computer"));
        assert!(dict.has("programming"));

        // Case variants
        assert!(dict.has("Hello")); // HashDict is case-insensitive
        assert!(dict.has("THE"));

        // Should not have gibberish
        assert!(!dict.has("xyzzyplugh"));
        assert!(!dict.has("brouwn"));

        // Word count sanity
        assert!(dict.len() > 50_000, "en_US should have >50K words");
    }

    #[test]
    fn load_txt_dictionary() {
        let dict_path = workspace_root()
            .join("dictionaries/node_modules/@cspell/dict-software-terms/dict/softwareTerms.txt");

        if !dict_path.exists() {
            eprintln!("Skipping: software-terms dict not found");
            return;
        }

        let dict = loader::load_dictionary(&dict_path).expect("load software terms dict");

        assert!(dict.has("API"), "should have 'API'");
        assert!(dict.has("JSON"), "should have 'JSON'");
    }

    #[test]
    fn format_auto_detection() {
        // .trie.gz → trie loader
        let trie_path =
            workspace_root().join("dictionaries/node_modules/@cspell/dict-en_us/en_US.trie.gz");

        if !trie_path.exists() {
            return;
        }

        let dict = loader::load_dictionary(&trie_path).expect("auto-detect trie format");
        assert!(dict.has("hello"));

        // .txt → txt loader
        let txt_path = workspace_root()
            .join("dictionaries/node_modules/@cspell/dict-software-terms/dict/softwareTerms.txt");

        if !txt_path.exists() {
            return;
        }

        let dict = loader::load_dictionary(&txt_path).expect("auto-detect txt format");
        assert!(dict.has("API"));
    }
}

// ============================================================
// 7. End-to-end validation with real dictionaries
// ============================================================

mod e2e_validation {
    use super::*;
    use matchum_dict::loader;

    fn load_en_us() -> Option<Box<dyn Dictionary>> {
        let dict_path =
            workspace_root().join("dictionaries/node_modules/@cspell/dict-en_us/en_US.trie.gz");
        if !dict_path.exists() {
            return None;
        }
        Some(Box::new(
            loader::trie_v3::load_trie_v3(&dict_path).expect("load en_US"),
        ))
    }

    #[test]
    fn validate_with_real_dictionary() {
        let dict = match load_en_us() {
            Some(d) => d,
            None => {
                eprintln!("Skipping: en_US dict not found");
                return;
            }
        };

        let v = Validator::new(vec![dict], ValidatorConfig::default());

        // Valid text should have no issues
        let issues = v.validate_text("The quick brown fox jumped over the lazy dog.");
        assert!(
            issues.is_empty(),
            "valid text should pass: {:?}",
            issues.iter().map(|i| &i.word).collect::<Vec<_>>()
        );

        // Misspelled words should be detected
        let issues = v.validate_text("The quick brouwn fox jumpped over the lazzy dog.");
        let words: Vec<&str> = issues.iter().map(|i| i.word.as_str()).collect();
        assert!(words.contains(&"brouwn"), "got: {:?}", words);
        assert!(words.contains(&"jumpped"), "got: {:?}", words);
        assert!(words.contains(&"lazzy"), "got: {:?}", words);
    }

    #[test]
    fn validate_code_with_camel_case() {
        let dict = match load_en_us() {
            Some(d) => d,
            None => return,
        };

        let v = Validator::new(vec![dict], ValidatorConfig::default());

        // camelCase identifiers with valid words should pass
        let issues = v.validate_text("getElementById");
        assert!(
            issues.is_empty(),
            "camelCase of valid words: {:?}",
            issues.iter().map(|i| &i.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn validate_with_disable_enable() {
        let dict = match load_en_us() {
            Some(d) => d,
            None => return,
        };

        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = r#"hello world
// cspell:disable
xyzzy brouwn jumpped
// cspell:enable
misspeled"#;

        let issues = v.validate_text(text);
        let words: Vec<&str> = issues.iter().map(|i| i.word.as_str()).collect();

        // disabled block should be skipped
        assert!(!words.contains(&"xyzzy"));
        assert!(!words.contains(&"brouwn"));
        assert!(!words.contains(&"jumpped"));

        // after enable, should check again
        assert!(words.contains(&"misspeled"), "got: {:?}", words);
    }

    #[test]
    fn validate_with_inline_ignore() {
        let dict = match load_en_us() {
            Some(d) => d,
            None => return,
        };

        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "// cspell:ignore xyzzy\nhello xyzzy";
        let issues = v.validate_text(text);
        assert!(
            issues.is_empty(),
            "inline ignored word: {:?}",
            issues.iter().map(|i| &i.word).collect::<Vec<_>>()
        );
    }

    // From validator.test.ts: contractions should be valid
    #[test]
    fn contractions_valid() {
        let dict = match load_en_us() {
            Some(d) => d,
            None => return,
        };

        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "We have a bit of text to check. Don't look too hard.";
        let issues = v.validate_text(text);
        assert!(
            issues.is_empty(),
            "contractions should be valid: {:?}",
            issues.iter().map(|i| &i.word).collect::<Vec<_>>()
        );
    }
}
