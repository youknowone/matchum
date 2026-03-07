//! Built-in pattern matching tests ported from cspell's RegExpPatterns.test.ts
//!
//! Sources:
//! - vendor/cspell/packages/cspell-lib/src/lib/Settings/RegExpPatterns.test.ts
//! - vendor/cspell/packages/cspell-lib/src/lib/Settings/RegExpPatterns.ts

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

fn issue_words(issues: &[ValidationIssue]) -> Vec<&str> {
    issues.iter().map(|i| i.word.as_str()).collect()
}

// ============================================================
// 1. URL pattern skipping
// ============================================================

mod url_patterns {
    use super::*;

    #[test]
    fn http_url_skipped() {
        let dict = make_dict(&["link", "to", "example", "the"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());
        let text = r#"<a href="http://example.org">link to example</a>"#;
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        // "org" inside URL should not be checked
        assert!(!words.contains(&"org"), "URL words skipped: {:?}", words);
    }

    #[test]
    fn https_url_skipped() {
        let dict = make_dict(&["visit", "the"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());
        let text = "visit https://www.google.com?q=typescript";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        assert!(!words.contains(&"google"), "HTTPS URL: {:?}", words);
        assert!(!words.contains(&"typescript"), "URL query: {:?}", words);
    }

    #[test]
    fn ftp_url_skipped() {
        let dict = make_dict(&["download", "from"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());
        let text = "download from ftp://files.example.com/data";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        assert!(!words.contains(&"files"), "FTP URL: {:?}", words);
    }

    #[test]
    fn relative_url_not_skipped() {
        let dict = make_dict(&["link", "to"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());
        // relative URLs should NOT be matched by URL pattern
        let text = "link to /example/page";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        assert!(
            words.contains(&"example") || words.contains(&"page"),
            "relative URL not skipped: {:?}",
            words
        );
    }
}

// ============================================================
// 2. Hex value patterns
// ============================================================

mod hex_patterns {
    use super::*;

    #[test]
    fn hex_0x_prefix_skipped() {
        let dict = make_dict(&["const", "value"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());
        let text = "const value = 0xDEADBEEF;";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        assert!(!words.contains(&"DEADBEEF"), "0x hex: {:?}", words);
    }

    #[test]
    fn hex_0x_lowercase() {
        let dict = make_dict(&["commit"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());
        let text = "commit 0x60975ea";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        assert!(
            !words.iter().any(|w| w.contains("60975")),
            "hex value: {:?}",
            words
        );
    }

    #[test]
    fn hex_single_digit() {
        let dict = make_dict(&["value"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());
        let text = "value = 0xf";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        // 0xf is short, should be skipped
        assert!(!words.contains(&"xf"), "short hex: {:?}", words);
    }
}

// ============================================================
// 3. Email pattern skipping
// ============================================================

mod email_patterns {
    use super::*;

    #[test]
    fn email_address_skipped() {
        let dict = make_dict(&["send", "email", "to"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());
        let text = "send email to user@example.com";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        assert!(!words.contains(&"user"), "email user: {:?}", words);
        assert!(!words.contains(&"example"), "email domain: {:?}", words);
    }

    #[test]
    fn email_with_dots() {
        let dict = make_dict(&["contact"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());
        let text = "contact first.last@subdomain.example.org";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        assert!(!words.contains(&"subdomain"), "email domain: {:?}", words);
    }
}

// ============================================================
// 4. SHA/hash pattern skipping
// ============================================================

mod hash_patterns {
    use super::*;

    #[test]
    fn sha_hash_40_chars() {
        let dict = make_dict(&["commit", "hash"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());
        let text = "commit hash a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        assert!(
            !words.iter().any(|w| w.len() >= 20),
            "SHA hash: {:?}",
            words
        );
    }

    #[test]
    fn sha256_hash() {
        let dict = make_dict(&["hash"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());
        let text = "hash e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        assert!(
            !words.iter().any(|w| w.len() >= 40),
            "SHA-256 hash: {:?}",
            words
        );
    }
}

// ============================================================
// 5. Escape sequence patterns
// ============================================================

mod escape_patterns {
    use super::*;

    #[test]
    fn unicode_escape_not_skipped_by_default() {
        // cspell's EscapeCharacters pattern is defined but NOT in the default exclude list.
        // So \uBABC is NOT skipped by default — BABC is flagged as unknown.
        let dict = make_dict(&["const", "value"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());
        let text = r"const value = '\uBABC'";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        assert!(
            words.contains(&"BABC"),
            "unicode escape should not be skipped by default: {:?}",
            words
        );
    }

    #[test]
    fn hex_escape_skipped() {
        let dict = make_dict(&["const", "char"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());
        let text = r"const char = '\x0A'";
        let issues = v.validate_text(text);
        // \x0A is a control character escape, should be skipped
        assert!(
            !issues.iter().any(|i| i.word.contains("0A")),
            "hex escape: {:?}",
            issue_words(&issues)
        );
    }
}

// ============================================================
// 6. Guard block directives (multi-line disable/enable)
// ============================================================

mod guard_blocks {
    use super::*;

    #[test]
    fn disable_enable_block() {
        let dict = make_dict(&["hello", "world"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "hello\n\
                     // spell-checker:disable\n\
                     badspelling1\n\
                     badspelling2\n\
                     // spell-checker:enable\n\
                     world";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        assert!(!words.contains(&"badspelling1"), "disabled: {:?}", words);
        assert!(!words.contains(&"badspelling2"), "disabled: {:?}", words);
    }

    #[test]
    fn disable_to_eof() {
        let dict = make_dict(&["hello"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "hello\n\
                     // cspell:disable\n\
                     badspelling1\n\
                     badspelling2";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        assert!(words.is_empty(), "disabled to EOF: {:?}", words);
    }

    #[test]
    fn disable_line_only_affects_current() {
        let dict = make_dict(&["hello"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "badspelling // cspell:disable-line\nbadspelling";
        let issues = v.validate_text(text);
        assert_eq!(issues.len(), 1, "issues: {:?}", issue_words(&issues));
        assert_eq!(issues[0].line, 2);
    }

    #[test]
    fn disable_next_line_only_affects_next() {
        let dict = make_dict(&["hello"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "// cspell:disable-next-line\nbadspelling\nmisspeled";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        assert!(
            !words.contains(&"badspelling"),
            "next line disabled: {:?}",
            words
        );
        assert!(
            words.contains(&"misspeled"),
            "2 lines after not disabled: {:?}",
            words
        );
    }

    #[test]
    fn multiple_disable_enable_blocks() {
        let dict = make_dict(&["hello", "world"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "hello\n\
                     // cspell:disable\n\
                     xyzzy1\n\
                     // cspell:enable\n\
                     world\n\
                     // cspell:disable\n\
                     xyzzy2\n\
                     // cspell:enable\n\
                     hello";
        let issues = v.validate_text(text);
        let words = issue_words(&issues);
        assert!(!words.contains(&"xyzzy1"), "block 1: {:?}", words);
        assert!(!words.contains(&"xyzzy2"), "block 2: {:?}", words);
    }

    #[test]
    fn cspell_prefix_variations() {
        let dict = make_dict(&["hello"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        // All prefix forms should work
        for prefix in &["cspell", "cSpell", "spell-checker", "spellchecker"] {
            let text = format!("hello\n// {prefix}:disable\nbadword\n// {prefix}:enable\nhello");
            let issues = v.validate_text(&text);
            let words = issue_words(&issues);
            assert!(!words.contains(&"badword"), "{prefix}: {:?}", words);
        }
    }
}

// ============================================================
// 7. Compound pattern: URL + directives combined
// ============================================================

mod combined_patterns {
    use super::*;

    #[test]
    fn url_inside_disabled_block() {
        let dict = make_dict(&["hello"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "// cspell:disable\n\
                     https://www.example.com/badpath\n\
                     badword\n\
                     // cspell:enable\n\
                     hello";
        let issues = v.validate_text(text);
        assert!(
            issues.is_empty(),
            "disabled block + URL: {:?}",
            issue_words(&issues)
        );
    }

    #[test]
    fn ignore_directive_with_url() {
        let dict = make_dict(&["visit"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());

        let text = "// cspell:ignore customterm\n\
                     visit https://example.com customterm";
        let issues = v.validate_text(text);
        assert!(
            issues.is_empty(),
            "ignore + URL: {:?}",
            issue_words(&issues)
        );
    }
}

// ============================================================
// 8. Windows path patterns
// ============================================================

mod windows_paths {
    use super::*;

    #[test]
    fn windows_path_not_skipped() {
        // cspell has no built-in Windows path skip pattern.
        // Path components are checked as regular words.
        let dict = make_dict(&["file", "located", "at", "Users", "Documents", "txt"]);
        let v = Validator::new(vec![dict], ValidatorConfig::default());
        let text = r"file located at C:\Users\admin\Documents\file.txt";
        let issues = v.validate_text(text);
        assert!(
            issues.iter().any(|i| i.word == "admin"),
            "Windows path components should be checked: {:?}",
            issue_words(&issues)
        );
    }
}
