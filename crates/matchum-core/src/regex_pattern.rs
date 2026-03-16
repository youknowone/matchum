use regex::Regex;

/// Parse a `/pattern/flags` string into a compiled regex.
///
/// Handles flags: `i` (case-insensitive), `m` (multiline), `s` (dotall),
/// `x` (verbose/whitespace-stripped), `u` (ignored, Rust is Unicode by default),
/// `g` (ignored, handled by `find_iter`).
///
/// If the input is not slash-delimited, compiles it as a plain regex.
pub fn parse_regex_pattern(value: &str) -> Option<Regex> {
    let pat = match parse_slash_regex(value) {
        Some(p) => p,
        None => value.trim().to_string(),
    };
    Regex::new(&pat).ok()
}

/// Parse a `/pattern/flags` string into a regex pattern string with flags embedded.
///
/// Returns `Some(pattern)` if the input is in `/pattern/flags` format,
/// `None` if it's a plain (non-delimited) pattern.
pub fn parse_slash_regex(value: &str) -> Option<String> {
    let s = value.trim();
    if s.starts_with('/') && s.len() > 1
        && let Some(last_slash) = s.rfind('/')
            && last_slash > 0 {
                let body = &s[1..last_slash];
                let flags = &s[last_slash + 1..];
                let mut prefix = String::new();
                if flags.contains('i') {
                    prefix.push('i');
                }
                if flags.contains('m') {
                    prefix.push('m');
                }
                if flags.contains('s') {
                    prefix.push('s');
                }
                let body = if flags.contains('x') {
                    strip_verbose_whitespace(body)
                } else {
                    body.to_string()
                };
                let pat = if prefix.is_empty() {
                    body
                } else {
                    format!("(?{}){}", prefix, body)
                };
                return Some(pat);
            }
    None
}

/// Strip unescaped whitespace and `#` line comments for verbose (`x`) mode.
pub fn strip_verbose_whitespace(pattern: &str) -> String {
    let mut result = String::with_capacity(pattern.len());
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            result.push(ch);
            if let Some(next) = chars.next() {
                result.push(next);
            }
        } else if ch == '#' {
            for c in chars.by_ref() {
                if c == '\n' {
                    break;
                }
            }
        } else if ch.is_whitespace() {
            // Skip unescaped whitespace
        } else {
            result.push(ch);
        }
    }
    result
}
