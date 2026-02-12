//! Word splitting tests ported from cspell's wordSplitter.test.ts and text.test.ts.
//!
//! Sources:
//! - vendor/cspell/packages/cspell-lib/src/lib/util/wordSplitter.test.ts
//! - vendor/cspell/packages/cspell-lib/src/lib/util/text.test.ts

use ruspell_core::splitter;

// ============================================================
// splitCamelCaseWord — from text.test.ts
// ============================================================

mod split_camel_case {
    use super::*;

    fn split(word: &str) -> Vec<&str> {
        splitter::split_camel_case(word)
    }

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

    #[test]
    fn error_code() {
        assert_eq!(split("ERRORCode"), vec!["ERROR", "Code"]);
    }

    #[test]
    fn error_codes_two() {
        assert_eq!(split("ERRORCodesTwo"), vec!["ERROR", "Codes", "Two"]);
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

    #[test]
    fn cvtpd2ps_x_xm() {
        assert_eq!(split("CVTPD2PS"), vec!["CVTPD2PS"]);
    }

    #[test]
    fn cvtsi2sd() {
        // CVTSI2SD: uppercase block with digit, then uppercase
        assert_eq!(split("CVTSI2SD"), vec!["CVTSI2SD"]);
    }
}

// ============================================================
// extract_words — from text.test.ts + wordSplitter.test.ts
// ============================================================

mod extract_words {
    use super::*;

    fn word_texts(text: &str) -> Vec<String> {
        splitter::extract_words(text)
            .into_iter()
            .map(|w| w.text.to_string())
            .collect()
    }

    // Contractions from text.test.ts
    #[test]
    fn contractions() {
        let text = "could've would've couldn't've wasn't y'all 'twas shouldn\u{2019}t";
        let ws = word_texts(text);
        assert!(ws.contains(&"could've".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"couldn't've".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"wasn't".to_string()), "got: {:?}", ws);
    }

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

    #[test]
    fn skip_chinese_characters() {
        let text = r#"<a href="http://www.ctrip.com" title="携程旅行网">携程旅行网</a>"#;
        let ws = word_texts(text);
        assert!(ws.contains(&"href".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"ctrip".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"title".to_string()), "got: {:?}", ws);
        assert!(
            !ws.iter().any(|w| w.contains('携')),
            "should not contain Chinese chars"
        );
    }

    #[test]
    fn skip_japanese_characters() {
        let text = "Example text: gitのpackageのみ際インストール";
        let ws = word_texts(text);
        assert!(ws.contains(&"Example".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"text".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"git".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"package".to_string()), "got: {:?}", ws);
    }

    #[test]
    fn keep_greek_characters() {
        let text = "Γ γ\tgamma γάμμα";
        let ws = word_texts(text);
        assert!(ws.contains(&"Γ".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"γ".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"gamma".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"γάμμα".to_string()), "got: {:?}", ws);
    }

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

    #[test]
    fn hyphenated() {
        let ws = word_texts("well-educated");
        assert_eq!(ws, vec!["well", "educated"]);
    }

    #[test]
    fn numbers_only_skipped() {
        let ws = word_texts("12345 hello 67890");
        assert_eq!(ws, vec!["hello"]);
    }

    #[test]
    fn mixed_number_word() {
        // From wordSplitter: 32bit-checksum
        let ws = word_texts("32bit-checksum");
        // extract_words gets tokens, doesn't split by number boundaries
        assert!(ws.contains(&"checksum".to_string()), "got: {:?}", ws);
    }

    #[test]
    fn geschaeft_unicode() {
        let ws = word_texts("Geschäft");
        assert_eq!(ws, vec!["Geschäft"]);
    }

    #[test]
    fn iphone_with_circumflex() {
        let ws = word_texts("îphoneStatic");
        assert_eq!(ws, vec!["îphoneStatic"]);
    }

    #[test]
    fn ephone_with_circumflex() {
        let ws = word_texts("êphoneStatic");
        assert_eq!(ws, vec!["êphoneStatic"]);
    }

    #[test]
    fn toms_hardware() {
        let ws = word_texts("Tom's hardware");
        assert!(ws.contains(&"Tom's".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"hardware".to_string()), "got: {:?}", ws);
    }
}

// ============================================================
// extract_words_from_code — camelCase splitting
// ============================================================

mod extract_words_from_code {
    use super::*;

    fn code_word_texts(text: &str) -> Vec<String> {
        splitter::extract_words_from_code(text)
            .into_iter()
            .map(|w| w.text.to_string())
            .collect()
    }

    #[test]
    fn split_camel_case_word() {
        let text = "expect(splitCamelCaseWord('hello')).to.deep.equal(['hello']);";
        let ws = code_word_texts(text);
        assert!(ws.contains(&"expect".to_string()));
        assert!(ws.contains(&"split".to_string()));
        assert!(ws.contains(&"Camel".to_string()));
        assert!(ws.contains(&"Case".to_string()));
        assert!(ws.contains(&"Word".to_string()));
    }

    #[test]
    fn reg_exp_match() {
        let text = "expect(regExp.match(first_line));";
        let ws = code_word_texts(text);
        assert_eq!(ws, vec!["expect", "reg", "Exp", "match", "first", "line"]);
    }

    #[test]
    fn a_hello() {
        let text = "expect(aHELLO);";
        let ws = code_word_texts(text);
        assert_eq!(ws, vec!["expect", "a", "HELLO"]);
    }

    #[test]
    fn html_input_value() {
        let text = "var value = HTMLInput.value;";
        let ws = code_word_texts(text);
        assert_eq!(ws, vec!["var", "value", "HTML", "Input", "value"]);
    }

    #[test]
    fn error_code() {
        let ws = code_word_texts("ERRORCode");
        assert_eq!(ws, vec!["ERROR", "Code"]);
    }

    #[test]
    fn error_codes_two() {
        let ws = code_word_texts("ERRORCodesTwo");
        assert_eq!(ws, vec!["ERROR", "Codes", "Two"]);
    }

    #[test]
    fn iphone_static() {
        let ws = code_word_texts("îphoneStatic");
        assert_eq!(ws, vec!["îphone", "Static"]);
    }

    #[test]
    fn ephone_static() {
        let ws = code_word_texts("êphoneStatic");
        assert_eq!(ws, vec!["êphone", "Static"]);
    }

    #[test]
    fn geschaeft() {
        let ws = code_word_texts("geschäft");
        assert_eq!(ws, vec!["geschäft"]);
    }

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

    // From wordSplitter.test.ts: snake_case with numbers
    #[test]
    fn error_code42_one_two() {
        let ws = code_word_texts("error_code42_one_two");
        assert!(ws.contains(&"error".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"one".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"two".to_string()), "got: {:?}", ws);
    }
}

// ============================================================
// Word offset tracking
// ============================================================

mod word_offsets {
    use super::*;

    #[test]
    fn extract_words_offsets() {
        let words = splitter::extract_words("hello world");
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "hello");
        assert_eq!(words[0].offset, 0);
        assert_eq!(words[1].text, "world");
        assert_eq!(words[1].offset, 6);
    }

    #[test]
    fn code_words_offsets() {
        let words = splitter::extract_words_from_code("camelCase");
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "camel");
        assert_eq!(words[0].offset, 0);
        assert_eq!(words[1].text, "Case");
        assert_eq!(words[1].offset, 5);
    }

    #[test]
    fn code_words_offsets_with_prefix() {
        let words = splitter::extract_words_from_code("var splitCamelCaseWord = 1;");
        let texts: Vec<(&str, usize)> = words.iter().map(|w| (w.text, w.offset)).collect();
        assert!(texts.contains(&("split", 4)), "got: {:?}", texts);
        assert!(texts.contains(&("Camel", 9)), "got: {:?}", texts);
        assert!(texts.contains(&("Case", 14)), "got: {:?}", texts);
        assert!(texts.contains(&("Word", 18)), "got: {:?}", texts);
    }
}
