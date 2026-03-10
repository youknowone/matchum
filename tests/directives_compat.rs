// spell-checker:disable
//! Directive parsing tests ported from cspell's InDocSettings.test.ts.
//!
//! Sources:
//! - vendor/cspell/packages/cspell-lib/src/lib/Settings/InDocSettings.test.ts

use matchum_config::directives::{self, Directive, DirectiveWarning};

// ============================================================
// 1. Prefix variations
// ============================================================

mod prefix_variations {
    use super::*;

    #[test]
    fn cspell_prefix() {
        assert_eq!(
            directives::parse_directive("// cspell:disable"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn cspell_uppercase() {
        assert_eq!(
            directives::parse_directive("// CSPELL:disable"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn spell_checker_prefix() {
        assert_eq!(
            directives::parse_directive("// spell-checker:disable"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn spellchecker_prefix() {
        assert_eq!(
            directives::parse_directive("// spellchecker:disable"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn spell_prefix() {
        assert_eq!(
            directives::parse_directive("// spell:disable"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn case_insensitive_mixed() {
        assert_eq!(
            directives::parse_directive("// CSpell:Disable"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn case_insensitive_all_upper() {
        assert_eq!(
            directives::parse_directive("// CSPELL:DISABLE"),
            Some(Directive::Disable)
        );
    }
}

// ============================================================
// 2. Disable / Enable
// ============================================================

mod disable_enable {
    use super::*;

    #[test]
    fn disable() {
        assert_eq!(
            directives::parse_directive("// cspell:disable"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn enable() {
        assert_eq!(
            directives::parse_directive("// cspell:enable"),
            Some(Directive::Enable)
        );
    }

    #[test]
    fn disable_line() {
        assert_eq!(
            directives::parse_directive("const x = 1; // cspell:disable-line"),
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
    fn spell_checker_disable() {
        assert_eq!(
            directives::parse_directive("/* spell-checker:disable */"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn spell_checker_enable() {
        assert_eq!(
            directives::parse_directive("/* spell-checker:enable */"),
            Some(Directive::Enable)
        );
    }

    #[test]
    fn disable_with_space_after_colon() {
        assert_eq!(
            directives::parse_directive("// cspell: disable"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn disable_line_with_spell_checker() {
        assert_eq!(
            directives::parse_directive("xyzzy // spell-checker:disable-line"),
            Some(Directive::DisableLine)
        );
    }
}

// ============================================================
// 3. Ignore words directive
// ============================================================

mod ignore_words {
    use super::*;

    #[test]
    fn ignore_single() {
        assert_eq!(
            directives::parse_directive("// cspell:ignore myword"),
            Some(Directive::Ignore(vec!["myword".into()]))
        );
    }

    #[test]
    fn ignore_multiple_space() {
        assert_eq!(
            directives::parse_directive("// cspell:ignore word1 word2 word3"),
            Some(Directive::Ignore(vec![
                "word1".into(),
                "word2".into(),
                "word3".into(),
            ]))
        );
    }

    #[test]
    fn ignore_comma_separated() {
        assert_eq!(
            directives::parse_directive("// cspell:ignore word1, word2, word3"),
            Some(Directive::Ignore(vec![
                "word1".into(),
                "word2".into(),
                "word3".into(),
            ]))
        );
    }

    #[test]
    fn ignore_semicolon_separated() {
        let result = directives::parse_directive("// cspell:ignore word1;word2;word3");
        if let Some(Directive::Ignore(words)) = &result {
            assert!(words.contains(&"word1".to_string()), "got: {:?}", words);
            assert!(words.contains(&"word2".to_string()), "got: {:?}", words);
        } else {
            panic!("expected Ignore, got: {:?}", result);
        }
    }

    #[test]
    fn ignore_words_variant() {
        let result = directives::parse_directive("// cspell:ignoreWords tooo faullts");
        if let Some(Directive::Ignore(words)) = &result {
            assert!(words.contains(&"tooo".to_string()), "got: {:?}", words);
            assert!(words.contains(&"faullts".to_string()), "got: {:?}", words);
        } else {
            panic!("expected Ignore, got: {:?}", result);
        }
    }

    #[test]
    fn ignore_in_hash_comment() {
        assert_eq!(
            directives::parse_directive("# cspell:ignore myword"),
            Some(Directive::Ignore(vec!["myword".into()]))
        );
    }

    #[test]
    fn ignore_in_html_comment() {
        let result = directives::parse_directive("<!-- cspell:ignore myword -->");
        if let Some(Directive::Ignore(words)) = &result {
            assert!(words.contains(&"myword".to_string()), "got: {:?}", words);
        } else {
            panic!("expected Ignore, got: {:?}", result);
        }
    }

    #[test]
    fn ignore_in_block_comment() {
        let result = directives::parse_directive("/* cspell:ignore myword */");
        if let Some(Directive::Ignore(words)) = &result {
            assert!(words.contains(&"myword".to_string()), "got: {:?}", words);
        } else {
            panic!("expected Ignore, got: {:?}", result);
        }
    }
}

// ============================================================
// 4. Words directive
// ============================================================

mod words_directive {
    use super::*;

    #[test]
    fn word_single() {
        assert_eq!(
            directives::parse_directive("// cspell:word apple"),
            Some(Directive::Words(vec!["apple".into()]))
        );
    }

    #[test]
    fn words_multiple() {
        assert_eq!(
            directives::parse_directive("// cspell:words apple, banana"),
            Some(Directive::Words(vec!["apple".into(), "banana".into()]))
        );
    }

    #[test]
    fn words_space_separated() {
        assert_eq!(
            directives::parse_directive("// cspell:words whiteberry redberry lightbrown"),
            Some(Directive::Words(vec![
                "whiteberry".into(),
                "redberry".into(),
                "lightbrown".into(),
            ]))
        );
    }

    #[test]
    fn words_in_html_comment() {
        let result = directives::parse_directive("<!-- cspell:words apple -->");
        if let Some(Directive::Words(words)) = &result {
            assert!(words.contains(&"apple".to_string()), "got: {:?}", words);
        } else {
            panic!("expected Words, got: {:?}", result);
        }
    }
}

// ============================================================
// 5. No directive cases
// ============================================================

mod no_directive {
    use super::*;

    #[test]
    fn plain_comment() {
        assert_eq!(directives::parse_directive("// just a comment"), None);
    }

    #[test]
    fn code_line() {
        assert_eq!(
            directives::parse_directive("const x = 'hello world';"),
            None
        );
    }

    #[test]
    fn empty_line() {
        assert_eq!(directives::parse_directive(""), None);
    }

    #[test]
    fn just_spaces() {
        assert_eq!(directives::parse_directive("    "), None);
    }
}

// ============================================================
// 6. Comment format variations
// ============================================================

mod comment_formats {
    use super::*;

    #[test]
    fn single_line_comment() {
        assert_eq!(
            directives::parse_directive("// cspell:disable"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn block_comment() {
        assert_eq!(
            directives::parse_directive("/* cspell:disable */"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn html_comment() {
        assert_eq!(
            directives::parse_directive("<!-- cspell:disable -->"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn hash_comment() {
        assert_eq!(
            directives::parse_directive("# cspell:disable"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn triple_slash_comment() {
        assert_eq!(
            directives::parse_directive("/// cspell:disable"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn indented_comment() {
        assert_eq!(
            directives::parse_directive("    // cspell:disable"),
            Some(Directive::Disable)
        );
    }
}

// ============================================================
// 7. Directive with trailing content
// ============================================================

mod trailing_content {
    use super::*;

    #[test]
    fn disable_with_trailing_comment() {
        // "disable" starts with "disable", ignore trailing
        assert_eq!(
            directives::parse_directive("// cspell:disable some reason"),
            Some(Directive::Disable)
        );
    }

    #[test]
    fn enable_with_trailing_comment() {
        assert_eq!(
            directives::parse_directive("// cspell:enable again"),
            Some(Directive::Enable)
        );
    }
}

// ============================================================
// 8. Word directive vs ignore directive variants
// ============================================================

mod directive_variants {
    use super::*;

    #[test]
    fn ignore_with_words_suffix() {
        // "ignoreWords" should be treated same as "ignore"
        let result = directives::parse_directive("// cspell:ignoreWords apple banana");
        if let Some(Directive::Ignore(words)) = &result {
            assert!(words.contains(&"apple".to_string()), "got: {:?}", words);
            assert!(words.contains(&"banana".to_string()), "got: {:?}", words);
        } else {
            panic!("expected Ignore, got: {:?}", result);
        }
    }

    #[test]
    fn word_singular() {
        // "word" (singular) should work same as "words"
        let result = directives::parse_directive("// cspell:word apple");
        if let Some(Directive::Words(words)) = &result {
            assert!(words.contains(&"apple".to_string()), "got: {:?}", words);
        } else {
            panic!("expected Words, got: {:?}", result);
        }
    }
}

// ============================================================
// 9. Compound word directives — InDocSettings.test.ts
// ============================================================

mod compound_word_directives {
    use super::*;

    #[test]
    fn enable_compound_words() {
        assert_eq!(
            directives::parse_directive("// cspell:enableCompoundWords"),
            Some(Directive::EnableCompoundWords)
        );
    }

    #[test]
    fn disable_compound_words() {
        assert_eq!(
            directives::parse_directive("// cspell:disableCompoundWords"),
            Some(Directive::DisableCompoundWords)
        );
    }

    #[test]
    fn enable_compound_words_hyphenated() {
        assert_eq!(
            directives::parse_directive("// cspell:enable-compound-words"),
            Some(Directive::EnableCompoundWords)
        );
    }

    #[test]
    fn disable_compound_words_hyphenated() {
        assert_eq!(
            directives::parse_directive("// cspell:disable-compound-words"),
            Some(Directive::DisableCompoundWords)
        );
    }
}

// ============================================================
// 10. IgnoreRegExp, Language, Dictionaries directives
// ============================================================

mod extended_directives {
    use super::*;

    #[test]
    fn ignore_regexp() {
        let result = directives::parse_directive("// cspell:ignoreRegExp /\\/\\/\\/.*/");
        if let Some(Directive::IgnoreRegExp(pattern)) = &result {
            assert!(pattern.contains("/"), "pattern: {:?}", pattern);
        } else {
            panic!("expected IgnoreRegExp, got: {:?}", result);
        }
    }

    #[test]
    fn ignore_reg_exp_hyphenated() {
        let result = directives::parse_directive("// cspell:ignore-reg-exp /test/");
        if let Some(Directive::IgnoreRegExp(pattern)) = &result {
            assert_eq!(pattern, "/test/");
        } else {
            panic!("expected IgnoreRegExp, got: {:?}", result);
        }
    }

    #[test]
    fn language_directive() {
        let result = directives::parse_directive("// cspell:language en-US");
        assert_eq!(result, Some(Directive::Language("en-US".into())));
    }

    #[test]
    fn locale_directive() {
        let result = directives::parse_directive("// cspell:locale es-ES");
        assert_eq!(result, Some(Directive::Language("es-ES".into())));
    }

    #[test]
    fn dictionaries_directive() {
        let result = directives::parse_directive("// cspell:dictionaries lorem-ipsum custom");
        if let Some(Directive::Dictionaries(dicts)) = &result {
            assert_eq!(dicts, &["lorem-ipsum", "custom"]);
        } else {
            panic!("expected Dictionaries, got: {:?}", result);
        }
    }
}

// ============================================================
// 11. LocalWords (Emacs style)
// ============================================================

mod local_words {
    use super::*;

    #[test]
    fn local_words_basic() {
        let result = directives::parse_directive("% LocalWords: one two three");
        if let Some(Directive::LocalWords(words)) = &result {
            assert_eq!(words, &["one", "two", "three"]);
        } else {
            panic!("expected LocalWords, got: {:?}", result);
        }
    }

    #[test]
    fn local_words_in_comment() {
        let result = directives::parse_directive("// LocalWords: hello world");
        if let Some(Directive::LocalWords(words)) = &result {
            assert_eq!(words, &["hello", "world"]);
        } else {
            panic!("expected LocalWords, got: {:?}", result);
        }
    }

    #[test]
    fn local_words_case_insensitive() {
        let result = directives::parse_directive("% localwords: foo bar");
        if let Some(Directive::LocalWords(words)) = &result {
            assert_eq!(words, &["foo", "bar"]);
        } else {
            panic!("expected LocalWords, got: {:?}", result);
        }
    }
}

// ============================================================
// 12. Directive typo detection
// ============================================================

mod directive_typo_detection {
    use super::*;

    #[test]
    fn wrods_suggests_words() {
        let warning = directives::check_directive_typo("wrods");
        assert_eq!(
            warning,
            Some(DirectiveWarning {
                found: "wrods".into(),
                suggestion: "words".into(),
            })
        );
    }

    #[test]
    fn igore_suggests_ignore() {
        let warning = directives::check_directive_typo("igore");
        assert_eq!(
            warning,
            Some(DirectiveWarning {
                found: "igore".into(),
                suggestion: "ignore".into(),
            })
        );
    }

    #[test]
    fn disble_suggests_disable() {
        let warning = directives::check_directive_typo("disble");
        assert_eq!(
            warning,
            Some(DirectiveWarning {
                found: "disble".into(),
                suggestion: "disable".into(),
            })
        );
    }

    #[test]
    fn enble_suggests_enable() {
        let warning = directives::check_directive_typo("enble");
        assert_eq!(
            warning,
            Some(DirectiveWarning {
                found: "enble".into(),
                suggestion: "enable".into(),
            })
        );
    }

    #[test]
    fn valid_directives_return_none() {
        assert!(directives::check_directive_typo("disable").is_none());
        assert!(directives::check_directive_typo("enable").is_none());
        assert!(directives::check_directive_typo("disable-line").is_none());
        assert!(directives::check_directive_typo("disable-next-line").is_none());
        assert!(directives::check_directive_typo("ignore").is_none());
        assert!(directives::check_directive_typo("ignoreWords").is_none());
        assert!(directives::check_directive_typo("words").is_none());
        assert!(directives::check_directive_typo("forbid").is_none());
        assert!(directives::check_directive_typo("flag").is_none());
        assert!(directives::check_directive_typo("enableCompoundWords").is_none());
        assert!(directives::check_directive_typo("disableCompoundWords").is_none());
        assert!(directives::check_directive_typo("enableCaseSensitive").is_none());
        assert!(directives::check_directive_typo("disableCaseSensitive").is_none());
        assert!(directives::check_directive_typo("ignoreRegExp").is_none());
        assert!(directives::check_directive_typo("includeRegExp").is_none());
        assert!(directives::check_directive_typo("dictionaries").is_none());
        assert!(directives::check_directive_typo("language").is_none());
        assert!(directives::check_directive_typo("locale").is_none());
    }

    #[test]
    fn valid_directives_case_insensitive() {
        assert!(directives::check_directive_typo("Disable").is_none());
        assert!(directives::check_directive_typo("ENABLE").is_none());
        assert!(directives::check_directive_typo("WORDS").is_none());
        assert!(directives::check_directive_typo("IgnoreRegExp").is_none());
    }

    #[test]
    fn completely_unrelated_returns_none() {
        // Strings too far from any valid directive should not produce suggestions
        assert!(directives::check_directive_typo("xyzzyplugh").is_none());
        assert!(directives::check_directive_typo("somethingelse").is_none());
        assert!(directives::check_directive_typo("qqzzxx").is_none());
        assert!(directives::check_directive_typo("abcdefghij").is_none());
    }

    #[test]
    fn single_char_off_typos() {
        // One character substitution
        let w = directives::check_directive_typo("flab").unwrap();
        assert_eq!(w.suggestion, "flag");

        // One character deletion
        let w = directives::check_directive_typo("nable").unwrap();
        assert_eq!(w.suggestion, "enable");
    }

    #[test]
    fn typo_preserves_found_string() {
        let w = directives::check_directive_typo("wrods").unwrap();
        assert_eq!(w.found, "wrods");
    }
}
