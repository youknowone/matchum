//! Word splitting tests ported from cspell's wordSplitter.test.ts and text.test.ts.
//!
//! Sources:
//! - vendor/cspell/packages/cspell-lib/src/lib/util/wordSplitter.test.ts
//! - vendor/cspell/packages/cspell-lib/src/lib/util/text.test.ts

use matchum_core::splitter;

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

// ============================================================
// Additional camelCase edge cases from cspell's text.test.ts
// ============================================================

mod split_camel_case_additional {
    use super::*;

    fn split(word: &str) -> Vec<&str> {
        splitter::split_camel_case(word)
    }

    #[test]
    fn three_part_pascal() {
        assert_eq!(split("OneTwo Three"), vec!["One", "Two Three"]);
    }

    #[test]
    fn camel_with_trailing_s() {
        // "getURLs" should keep the trailing s with the acronym
        assert_eq!(split("getURLs"), vec!["get", "URLs"]);
    }

    #[test]
    fn camel_with_trailing_ed() {
        assert_eq!(split("WALKed"), vec!["WALKed"]);
    }

    #[test]
    fn camel_io_bound() {
        assert_eq!(split("IOBound"), vec!["IO", "Bound"]);
    }

    #[test]
    fn camel_get_id() {
        assert_eq!(split("getID"), vec!["get", "ID"]);
    }

    #[test]
    fn camel_set_html_content() {
        assert_eq!(split("setHTMLContent"), vec!["set", "HTML", "Content"]);
    }

    #[test]
    fn camel_is_nfa_state() {
        assert_eq!(split("isNFAState"), vec!["is", "NFA", "State"]);
    }

    #[test]
    fn camel_all_lower() {
        assert_eq!(split("lowercase"), vec!["lowercase"]);
    }

    #[test]
    fn camel_single_upper() {
        assert_eq!(split("A"), vec!["A"]);
    }

    #[test]
    fn camel_two_uppers() {
        assert_eq!(split("AB"), vec!["AB"]);
    }

    #[test]
    fn camel_three_uppers() {
        assert_eq!(split("ABC"), vec!["ABC"]);
    }

    #[test]
    fn camel_upper_lower() {
        assert_eq!(split("Ab"), vec!["Ab"]);
    }

    #[test]
    fn camel_upper_upper_lower() {
        // AB is an acronym followed by lowercase 'c' which is an English suffix
        assert_eq!(split("ABc"), vec!["A", "Bc"]);
    }

    #[test]
    fn camel_lower_upper() {
        assert_eq!(split("aB"), vec!["a", "B"]);
    }

    #[test]
    fn camel_lower_upper_lower() {
        assert_eq!(split("aBc"), vec!["a", "Bc"]);
    }

    #[test]
    fn digits_only() {
        assert_eq!(split("1234"), vec!["1234"]);
    }

    #[test]
    fn digit_then_upper() {
        // matchum does not split at digit-to-uppercase boundaries
        assert_eq!(split("int32Value"), vec!["int32Value"]);
    }

    #[test]
    fn upper_then_digit() {
        assert_eq!(split("UTF8"), vec!["UTF8"]);
    }

    #[test]
    fn upper_digit_upper() {
        // matchum does not split at digit-to-uppercase boundaries
        assert_eq!(split("UTF8Decoder"), vec!["UTF8Decoder"]);
    }

    #[test]
    fn screaming_snake() {
        // Pure camelCase splitter does not split on underscores
        assert_eq!(split("SCREAMING_SNAKE"), vec!["SCREAMING_SNAKE"]);
    }

    #[test]
    fn unicode_german_umlaut() {
        assert_eq!(split("überGreat"), vec!["über", "Great"]);
    }

    #[test]
    fn unicode_mixed_scripts() {
        // Accented letters are treated as lowercase
        assert_eq!(split("caféLatte"), vec!["café", "Latte"]);
    }

    #[test]
    fn x86_instruction() {
        assert_eq!(split("MOVDQA2PD"), vec!["MOVDQA2PD"]);
    }

    #[test]
    fn x86_instruction_mixed() {
        assert_eq!(split("movdqa2pd"), vec!["movdqa2pd"]);
    }
}

// ============================================================
// Additional extract_words edge cases
// ============================================================

mod extract_words_additional {
    use super::*;

    fn word_texts(text: &str) -> Vec<String> {
        splitter::extract_words(text)
            .into_iter()
            .map(|w| w.text.to_string())
            .collect()
    }

    #[test]
    fn multiple_underscores() {
        let ws = word_texts("__private__var__");
        assert!(ws.contains(&"private".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"var".to_string()), "got: {:?}", ws);
    }

    #[test]
    fn multiple_dots() {
        let ws = word_texts("a.b.c.d");
        assert_eq!(ws.len(), 4, "got: {:?}", ws);
    }

    #[test]
    fn curly_braces_separated() {
        let ws = word_texts("{hello} {world}");
        assert!(ws.contains(&"hello".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"world".to_string()), "got: {:?}", ws);
    }

    #[test]
    fn square_brackets() {
        let ws = word_texts("[item]");
        assert_eq!(ws, vec!["item"]);
    }

    #[test]
    fn angle_brackets_html_tag() {
        let ws = word_texts("<div class=\"main\">");
        assert!(ws.contains(&"div".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"class".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"main".to_string()), "got: {:?}", ws);
    }

    #[test]
    fn tab_separated() {
        let ws = word_texts("word1\tword2\tword3");
        assert_eq!(ws.len(), 3, "got: {:?}", ws);
    }

    #[test]
    fn newline_separated() {
        let ws = word_texts("line1\nline2\nline3");
        assert_eq!(ws.len(), 3, "got: {:?}", ws);
    }

    #[test]
    fn mixed_whitespace() {
        let ws = word_texts("  hello   world  ");
        assert_eq!(ws, vec!["hello", "world"]);
    }

    #[test]
    fn contraction_dont() {
        let ws = word_texts("don't");
        assert_eq!(ws, vec!["don't"]);
    }

    #[test]
    fn contraction_its() {
        let ws = word_texts("it's");
        assert_eq!(ws, vec!["it's"]);
    }

    #[test]
    fn possessive_johns() {
        let ws = word_texts("John's book");
        assert!(ws.contains(&"John's".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"book".to_string()), "got: {:?}", ws);
    }

    #[test]
    fn korean_characters_skipped() {
        let ws = word_texts("hello \u{D55C}\u{AE00} world");
        assert!(ws.contains(&"hello".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"world".to_string()), "got: {:?}", ws);
    }

    #[test]
    fn cyrillic_characters_kept() {
        let ws = word_texts("привет hello");
        assert!(ws.contains(&"hello".to_string()), "got: {:?}", ws);
        // Cyrillic should be extracted
        assert!(ws.contains(&"привет".to_string()), "got: {:?}", ws);
    }

    #[test]
    fn slash_separated_path() {
        let ws = word_texts("path/to/file");
        assert!(ws.contains(&"path".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"to".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"file".to_string()), "got: {:?}", ws);
    }

    #[test]
    fn backslash_separated() {
        let ws = word_texts(r"path\to\file");
        assert!(ws.contains(&"path".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"file".to_string()), "got: {:?}", ws);
    }

    #[test]
    fn at_sign_in_email() {
        let ws = word_texts("user@domain");
        assert!(ws.contains(&"user".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"domain".to_string()), "got: {:?}", ws);
    }

    #[test]
    fn hash_sign() {
        let ws = word_texts("#include");
        assert!(ws.contains(&"include".to_string()), "got: {:?}", ws);
    }
}

// ============================================================
// Additional extract_words_from_code edge cases
// ============================================================

mod extract_words_from_code_additional {
    use super::*;

    fn code_word_texts(text: &str) -> Vec<String> {
        splitter::extract_words_from_code(text)
            .into_iter()
            .map(|w| w.text.to_string())
            .collect()
    }

    #[test]
    fn pascal_case_splitting() {
        let ws = code_word_texts("HelloWorldComponent");
        assert_eq!(ws, vec!["Hello", "World", "Component"]);
    }

    #[test]
    fn screaming_snake_splitting() {
        let ws = code_word_texts("MAX_BUFFER_SIZE");
        assert!(ws.contains(&"MAX".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"BUFFER".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"SIZE".to_string()), "got: {:?}", ws);
    }

    #[test]
    fn kebab_case_splitting() {
        let ws = code_word_texts("my-component-name");
        assert!(ws.contains(&"my".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"component".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"name".to_string()), "got: {:?}", ws);
    }

    #[test]
    fn get_element_by_id() {
        let ws = code_word_texts("getElementById");
        assert_eq!(ws, vec!["get", "Element", "By", "Id"]);
    }

    #[test]
    fn xml_parser_factory() {
        let ws = code_word_texts("XMLParserFactory");
        assert_eq!(ws, vec!["XML", "Parser", "Factory"]);
    }

    #[test]
    fn url_in_code() {
        let ws = code_word_texts("parseURLString");
        assert_eq!(ws, vec!["parse", "URL", "String"]);
    }

    #[test]
    fn multiple_statements() {
        let ws = code_word_texts("const myVar = getValue();");
        assert!(ws.contains(&"const".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"my".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"Var".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"get".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"Value".to_string()), "got: {:?}", ws);
    }

    #[test]
    fn numbers_in_identifier() {
        let ws = code_word_texts("base64Encode");
        assert!(ws.contains(&"Encode".to_string()), "got: {:?}", ws);
    }

    #[test]
    fn single_letter_prefix() {
        let ws = code_word_texts("aValue");
        assert_eq!(ws, vec!["a", "Value"]);
    }

    #[test]
    fn double_letter_prefix() {
        let ws = code_word_texts("isValid");
        assert_eq!(ws, vec!["is", "Valid"]);
    }

    #[test]
    fn all_caps_with_trailing_s() {
        let ws = code_word_texts("APIs");
        assert_eq!(ws, vec!["APIs"]);
    }

    #[test]
    fn multiline_code() {
        let ws = code_word_texts("const myVar = 1;\nlet otherVar = 2;");
        assert!(ws.contains(&"my".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"Var".to_string()), "got: {:?}", ws);
        assert!(ws.contains(&"other".to_string()), "got: {:?}", ws);
    }
}

// ============================================================
// Word offset tracking - additional tests
// ============================================================

mod word_offsets_additional {
    use super::*;

    #[test]
    fn snake_case_offsets() {
        let words = splitter::extract_words("first_second_third");
        assert_eq!(words.len(), 3);
        assert_eq!(words[0].text, "first");
        assert_eq!(words[0].offset, 0);
        assert_eq!(words[1].text, "second");
        assert_eq!(words[1].offset, 6);
        assert_eq!(words[2].text, "third");
        assert_eq!(words[2].offset, 13);
    }

    #[test]
    fn leading_whitespace_offset() {
        let words = splitter::extract_words("   hello");
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "hello");
        assert_eq!(words[0].offset, 3);
    }

    #[test]
    fn code_pascal_offsets() {
        let words = splitter::extract_words_from_code("HelloWorld");
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "Hello");
        assert_eq!(words[0].offset, 0);
        assert_eq!(words[1].text, "World");
        assert_eq!(words[1].offset, 5);
    }

    #[test]
    fn code_acronym_offsets() {
        let words = splitter::extract_words_from_code("XMLParser");
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "XML");
        assert_eq!(words[0].offset, 0);
        assert_eq!(words[1].text, "Parser");
        assert_eq!(words[1].offset, 3);
    }

    #[test]
    fn multiline_offsets() {
        let text = "hello\nworld";
        let words = splitter::extract_words(text);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].offset, 0);
        assert_eq!(words[1].offset, 6);
    }
}
