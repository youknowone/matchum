pub mod issue;
pub mod random;
pub mod regex_pattern;
pub mod splitter;
pub mod validator;

/// JavaScript-compatible string length (UTF-16 code units).
pub fn js_string_len(s: &str) -> usize {
    s.encode_utf16().count()
}
