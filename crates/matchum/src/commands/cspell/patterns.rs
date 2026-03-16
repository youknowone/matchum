use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use matchum_core::regex_pattern::parse_slash_regex;
use matchum_core::validator::CustomIgnorePatternMask;

#[derive(Clone)]
pub enum CompiledRegex {
    Rust(regex::Regex),
    Fancy(fancy_regex::Regex),
}

/// Global cache for compiled regex patterns, keyed by the original pattern string.
/// Avoids recompiling identical patterns across validator templates.
static REGEX_CACHE: LazyLock<Mutex<HashMap<String, Option<CompiledRegex>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

impl CompiledRegex {
    pub fn is_match(&self, text: &str) -> bool {
        match self {
            Self::Rust(re) => re.is_match(text),
            Self::Fancy(re) => re.is_match(text).unwrap_or(false),
        }
    }

    pub fn find_ranges(&self, text: &str) -> Vec<(usize, usize)> {
        match self {
            Self::Rust(re) => re.find_iter(text).map(|m| (m.start(), m.end())).collect(),
            Self::Fancy(re) => re
                .find_iter(text)
                .filter_map(Result::ok)
                .map(|m| (m.start(), m.end()))
                .collect(),
        }
    }
}

const CSPELL_REGEX_BASE64: &str = r"/(?<![A-Za-z0-9/+])(?:[A-Za-z0-9/+]{40,})(?:\s^\s*[A-Za-z0-9/+]{40,})*(?:\s^\s*[A-Za-z0-9/+]+=*)?(?![A-Za-z0-9/+=])/gm";
const CSPELL_REGEX_HASH_STRINGS: &str = r#"/(?:\b(?:sha\d+|md5|base64|crypt|bcrypt|scrypt|security-token|assertion)[-,:$=]|#code[/])[-\w/+%.]{25,}={0,3}(?:(['"])\s*\+?\s*\1?[-\w/+%.]+={0,3})*(?![-\w/+=%.])/gi"#;
const LATEX_MACRO_FUNCTION_NAMES: &str = r"/(?<!\\)\\(?:\\\\)*\w+/g";
const LATEX_MACROS_MULTILINE: &str = r"/(?<!\\)\\(?:\\\\)*(?!(?:title|color|section|subsection|footnote|chapter|part|caption|emph|enquote|text|in\b))\w+(?:\[[^]*?\]|\{[^]*?\})*/gi";
const LATEX_MATH: &str = r"/(?<!(?<!\\)(?:\\\\)*%.*)(?<!\\)(?:\\\\)*[$]+(.|\r|\n)+?(?<!(?<!\\)(?:\\\\)*%.*)(?<!\\)(?:\\\\)*[$]+/g";
const ADA_WORD_BREAK: &str = r"/(?<=\w)['](?=\w)(?!((?<=n')t|ve|d|ll|m|s|re)\b)/g";

/// Parse a regex pattern using cspell's conventions.
///
/// Differs from the base `parse_regex_pattern` in two ways:
/// 1. Non-delimited patterns get default `(?im)` flags (cspell's `stringToRegExp` behavior)
/// 2. Backreferences (`\1`-`\9`) are resolved by substituting capture group patterns
///
/// Results are cached globally so identical patterns are compiled only once.
pub fn parse_cspell_regex_pattern(value: &str) -> Option<CompiledRegex> {
    {
        let cache = REGEX_CACHE.lock().unwrap();
        if let Some(cached) = cache.get(value) {
            return cached.clone();
        }
    }

    let result = parse_cspell_regex_pattern_uncached(value);

    {
        let mut cache = REGEX_CACHE.lock().unwrap();
        cache.insert(value.to_string(), result.clone());
    }

    result
}

fn parse_cspell_regex_pattern_uncached(value: &str) -> Option<CompiledRegex> {
    let pat = match parse_slash_regex(value) {
        Some(p) => p,
        None => format!("(?im){}", value.trim()),
    };
    let pat = translate_leading_positive_lookbehind(&pat);
    let pat = translate_trailing_positive_lookahead(pat.as_ref());
    let pat = translate_js_regex_shorthands(&pat);
    try_compile_regex(pat.as_ref())
}

/// Try to compile a regex, falling back to resolving backreferences (\1, \2, etc.)
/// by substituting them with the pattern from the corresponding capture group.
pub fn try_compile_regex(pat: &str) -> Option<CompiledRegex> {
    if let Ok(re) = regex::Regex::new(pat) {
        return Some(CompiledRegex::Rust(re));
    }
    static BACKREF_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\\[1-9]").unwrap());
    if BACKREF_RE.is_match(pat) {
        let groups = extract_capture_groups(pat);
        let referenced_groups: Vec<usize> = BACKREF_RE
            .find_iter(pat)
            .filter_map(|m| {
                pat[m.start() + 1..m.end()]
                    .parse::<usize>()
                    .ok()
                    .and_then(|idx| idx.checked_sub(1))
            })
            .collect();
        let has_required_backref = referenced_groups.iter().any(|&idx| {
            groups
                .get(idx)
                .map(|(_, optional)| !optional)
                .unwrap_or(true)
        });

        if has_required_backref
            && let Ok(re) = fancy_regex::Regex::new(pat) {
                return Some(CompiledRegex::Fancy(re));
            }

        if let Some(expanded) = expand_single_optional_backreference_pattern(pat)
            && let Ok(re) = regex::Regex::new(&expanded) {
                return Some(CompiledRegex::Rust(re));
            }

        if !groups.is_empty() {
            let resolved = BACKREF_RE.replace_all(pat, |caps: &regex::Captures| {
                let digit = caps[0].as_bytes()[1] - b'0';
                let idx = digit as usize - 1;
                if let Some((group_pat, optional)) = groups.get(idx) {
                    if *optional {
                        format!("(?:{})?", group_pat)
                    } else {
                        format!("(?:{})", group_pat)
                    }
                } else {
                    r"[\s\S]+?".to_string()
                }
            });
            if let Ok(re) = regex::Regex::new(&resolved) {
                return Some(CompiledRegex::Rust(re));
            }
        }
        let simplified = BACKREF_RE.replace_all(pat, r"[\s\S]+?");
        return regex::Regex::new(&simplified).ok().map(CompiledRegex::Rust);
    }
    None
}

fn expand_single_optional_backreference_pattern(pat: &str) -> Option<String> {
    let groups = extract_capture_groups(pat);
    if groups.len() != 1 || !groups[0].1 {
        return None;
    }

    let refs: Vec<_> = regex::Regex::new(r"\\1").ok()?.find_iter(pat).collect();
    if refs.is_empty() {
        return None;
    }
    if regex::Regex::new(r"\\[2-9]").ok()?.is_match(pat) {
        return None;
    }

    let (group_start, group_end, group_pat) = find_first_optional_capture_group(pat)?;
    let before = &pat[..group_start];
    let after_group = &pat[group_end..];
    let backref_re = regex::Regex::new(r"\\1").ok()?;

    let present_tail = backref_re.replace_all(after_group, format!("(?:{group_pat})"));
    let absent_tail = backref_re.replace_all(after_group, "");

    Some(format!(
        "(?:{}(?:{}){}|{}{})",
        before, group_pat, present_tail, before, absent_tail
    ))
}

pub fn classify_custom_ignore_pattern(value: &str) -> CustomIgnorePatternMask {
    let mut mask = CustomIgnorePatternMask::default();
    let trimmed = value.trim();
    let compact: String = trimmed
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect();
    let normalized = parse_slash_regex(value);

    if trimmed == CSPELL_REGEX_BASE64
        || normalized.as_deref()
            == Some(
                r"(?m)(?<![A-Za-z0-9/+])(?:[A-Za-z0-9/+]{40,})(?:\s^\s*[A-Za-z0-9/+]{40,})*(?:\s^\s*[A-Za-z0-9/+]+=*)?(?![A-Za-z0-9/+=])",
            )
    {
        mask.enable_base64();
    }

    if trimmed == CSPELL_REGEX_HASH_STRINGS {
        mask.enable_hash_strings();
    }

    if trimmed == ADA_WORD_BREAK
        || normalized.as_deref() == Some(r"(?<=\w)['](?=\w)(?!((?<=n')t|ve|d|ll|m|s|re)\b)")
    {
        mask.enable_ada_word_break();
    }

    if trimmed == LATEX_MACRO_FUNCTION_NAMES || compact == LATEX_MACRO_FUNCTION_NAMES {
        mask.enable_latex_macro_function_names();
    }

    if trimmed == LATEX_MACROS_MULTILINE || compact == LATEX_MACROS_MULTILINE {
        mask.enable_latex_macros_multiline();
    }

    if trimmed == LATEX_MATH || compact == LATEX_MATH {
        mask.enable_latex_math();
    }

    mask
}

/// Extract the pattern text of each capturing group from a regex string.
fn extract_capture_groups(pat: &str) -> Vec<(String, bool)> {
    let bytes = pat.as_bytes();
    let len = bytes.len();
    let mut groups: Vec<(String, bool)> = Vec::new();
    let mut stack: Vec<(usize, bool)> = Vec::new();
    let mut i = 0;
    while i < len {
        match bytes[i] {
            b'\\' => {
                i += 2;
            }
            b'[' => {
                i += 1;
                if i < len && bytes[i] == b'^' {
                    i += 1;
                }
                if i < len && bytes[i] == b']' {
                    i += 1;
                }
                while i < len && bytes[i] != b']' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'(' => {
                let group_start = i + 1;
                let is_capturing = !(i + 1 < len && bytes[i + 1] == b'?');
                stack.push((group_start, is_capturing));
                i += 1;
            }
            b')' => {
                if let Some((start, is_capturing)) = stack.pop()
                    && is_capturing {
                        let content = &pat[start..i];
                        let optional =
                            i + 1 < len && (bytes[i + 1] == b'?' || bytes[i + 1] == b'*');
                        groups.push((content.to_string(), optional));
                    }
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    groups
}

fn find_first_optional_capture_group(pat: &str) -> Option<(usize, usize, String)> {
    let bytes = pat.as_bytes();
    let mut i = 0usize;
    let mut in_class = false;

    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'[' if !in_class => {
                in_class = true;
                i += 1;
            }
            b']' if in_class => {
                in_class = false;
                i += 1;
            }
            b'(' if !in_class => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'?' {
                    i += 1;
                    continue;
                }
                let end = find_group_end(pat, i + 1)?;
                if end + 1 >= bytes.len() || bytes[end + 1] != b'?' {
                    return None;
                }
                return Some((i, end + 2, pat[i + 1..end].to_string()));
            }
            _ => i += 1,
        }
    }

    None
}

/// Normalize an exclude pattern following cspell's `normalizePatternNested` logic.
pub fn normalize_pattern_nested(pattern: &str) -> Vec<String> {
    if pattern.is_empty() {
        return vec![];
    }
    let (neg, pat) = if let Some(stripped) = pattern.strip_prefix('!') {
        ("!", stripped)
    } else {
        ("", pattern)
    };

    if !pat.contains('/') {
        if pat == "**" {
            return vec![format!("{neg}**")];
        }
        return vec![format!("{neg}**/{pat}"), format!("{neg}**/{pat}/**")];
    }

    let has_leading_slash = pat.starts_with('/');
    let pat = if has_leading_slash { &pat[1..] } else { pat };

    if let Some(inner) = pat.strip_suffix('/') {
        if has_leading_slash || inner.contains('/') {
            return vec![format!("{neg}{pat}**/*")];
        }
        return vec![format!("{neg}**/{pat}**/*")];
    }

    if pat.ends_with("**") {
        return vec![format!("{neg}{pat}")];
    }

    vec![format!("{neg}{pat}"), format!("{neg}{pat}/**")]
}

/// Expand bash-style `!(...)` extglob negation into positive + negation patterns.
pub fn expand_extglobs(pattern: &str) -> Vec<String> {
    if let Some(start) = pattern.find("!(")
        && let Some(close_offset) = pattern[start + 2..].find(')') {
            let close = start + 2 + close_offset;
            let prefix = &pattern[..start];
            let inner = &pattern[start + 2..close];
            let suffix = &pattern[close + 1..];

            let positive = format!("{prefix}*{suffix}");
            let mut result = vec![positive];
            for alt in inner.split('|') {
                result.push(format!("!{prefix}{alt}{suffix}"));
            }
            return result;
        }
    vec![pattern.to_string()]
}

fn translate_js_regex_shorthands(pattern: &str) -> Cow<'_, str> {
    if !pattern.as_bytes().contains(&b'\\') {
        return Cow::Borrowed(pattern);
    }

    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars();
    let mut in_char_class = false;

    while let Some(ch) = chars.next() {
        match ch {
            '[' if !in_char_class => {
                in_char_class = true;
                out.push(ch);
            }
            ']' if in_char_class => {
                in_char_class = false;
                out.push(ch);
            }
            '\\' => {
                let Some(next) = chars.next() else {
                    out.push('\\');
                    break;
                };

                match (in_char_class, next) {
                    (true, 'w') => out.push_str("A-Za-z0-9_"),
                    (false, 'w') => out.push_str("[A-Za-z0-9_]"),
                    (true, 'd') => out.push_str("0-9"),
                    (false, 'd') => out.push_str("[0-9]"),
                    (false, 'W') => out.push_str("[^A-Za-z0-9_]"),
                    (false, 'D') => out.push_str("[^0-9]"),
                    (false, 'b') => out.push_str("(?-u:\\b)"),
                    (false, 'B') => out.push_str("(?-u:\\B)"),
                    _ => {
                        out.push('\\');
                        out.push(next);
                    }
                }
            }
            _ => out.push(ch),
        }
    }

    if out == pattern {
        Cow::Borrowed(pattern)
    } else {
        Cow::Owned(out)
    }
}

fn translate_leading_positive_lookbehind(pattern: &str) -> Cow<'_, str> {
    let Some((prefix_end, lookbehind_start)) = split_inline_flag_prefix(pattern) else {
        return Cow::Borrowed(pattern);
    };

    let rest = &pattern[lookbehind_start..];
    if !rest.starts_with("(?<=") {
        return Cow::Borrowed(pattern);
    }

    let Some(lookbehind_end) = find_group_end(rest, 4) else {
        return Cow::Borrowed(pattern);
    };

    let lookbehind = &rest[4..lookbehind_end];
    let mut rewritten = String::with_capacity(pattern.len());
    rewritten.push_str(&pattern[..prefix_end]);
    rewritten.push_str(lookbehind);
    rewritten.push_str(&rest[lookbehind_end + 1..]);
    Cow::Owned(rewritten)
}

fn translate_trailing_positive_lookahead(pattern: &str) -> Cow<'_, str> {
    let Some((prefix_end, _)) = split_inline_flag_prefix(pattern) else {
        return Cow::Borrowed(pattern);
    };

    let mut rewritten = pattern.to_string();
    let mut changed = false;

    loop {
        let rest = &rewritten[prefix_end..];
        let Some(lookahead_rel) = rest.rfind("(?=") else {
            break;
        };
        let Some(lookahead_end) = find_group_end(rest, lookahead_rel + 3) else {
            break;
        };
        if lookahead_end + 1 != rest.len() {
            break;
        }

        let inner_start = prefix_end + lookahead_rel + 3;
        let inner_end = prefix_end + lookahead_end;
        let inner = rewritten[inner_start..inner_end].to_string();
        rewritten.replace_range(
            prefix_end + lookahead_rel..=inner_end,
            &format!("(?:{inner})"),
        );
        changed = true;
    }

    if changed {
        Cow::Owned(rewritten)
    } else {
        Cow::Borrowed(pattern)
    }
}

fn split_inline_flag_prefix(pattern: &str) -> Option<(usize, usize)> {
    let mut offset = 0usize;
    loop {
        let Some(rest) = pattern.get(offset..) else {
            return Some((offset, offset));
        };
        if !rest.starts_with("(?") {
            return Some((offset, offset));
        }
        if rest.starts_with("(?<") || rest.starts_with("(?=") || rest.starts_with("(?!") {
            return Some((offset, offset));
        }

        let Some(close) = rest.find(')') else {
            return None;
        };
        let flags = &rest[2..close];
        if flags.is_empty()
            || flags
                .chars()
                .any(|ch| !matches!(ch, 'i' | 'm' | 's' | 'x' | 'u' | 'U' | '-'))
        {
            return Some((offset, offset));
        }
        offset += close + 1;
    }
}

fn find_group_end(pattern: &str, start: usize) -> Option<usize> {
    let bytes = pattern.as_bytes();
    let mut idx = start;
    let mut depth = 1usize;
    let mut in_class = false;

    while idx < bytes.len() {
        match bytes[idx] {
            b'\\' => {
                idx += 2;
            }
            b'[' if !in_class => {
                in_class = true;
                idx += 1;
            }
            b']' if in_class => {
                in_class = false;
                idx += 1;
            }
            b'(' if !in_class => {
                depth += 1;
                idx += 1;
            }
            b')' if !in_class => {
                depth -= 1;
                if depth == 0 {
                    return Some(idx);
                }
                idx += 1;
            }
            _ => idx += 1,
        }
    }

    None
}

// cspell:disable
#[cfg(test)]
mod tests {
    use super::parse_cspell_regex_pattern;

    #[test]
    fn cspell_regex_uses_ascii_word_classes_like_js() {
        let re =
            parse_cspell_regex_pattern(r"/<?\b[\w.\-+]{1,128}@\w{1,63}(\.\w{1,63}){1,4}\b>?/gi")
                .unwrap();

        assert!(re.is_match("user@example.com"));
        assert!(!re.is_match("mike@ıxample.org"));
    }

    #[test]
    fn cspell_regex_translates_ascii_digit_classes() {
        let re = parse_cspell_regex_pattern(r"/\bsha\d+-[a-z0-9+/]{25,}={0,3}/gi").unwrap();

        assert!(re.is_match("sha256-abcdefghijklmnopqrstuvwxy12345=="));
        assert!(!re.is_match("sha٢٥٦-abcdefghijklmnopqrstuvwxy12345=="));
    }

    #[test]
    fn cspell_regex_rewrites_leading_positive_lookbehind_for_markdown_links() {
        let link = parse_cspell_regex_pattern(r"/(?<=\]\()[^)\s]+/g").unwrap();
        let reference = parse_cspell_regex_pattern(r"/(?<=\])\[[^\]]+\]/").unwrap();
        let definition =
            parse_cspell_regex_pattern(r"/(?<=\]:\s)(\s*((https?:)?|\/|\.{1,2}))(\/\S+)/").unwrap();

        assert!(link.is_match("[about](./media/ps-msupdate-msi.png)"));
        assert!(reference.is_match("[foo][about_pssessions]"));
        assert!(definition.is_match(
            "[foo]: /powershell/module/microsoft.powershell.core/about/about_pssessions"
        ));
    }

    #[test]
    fn cspell_regex_rewrites_trailing_positive_lookahead_for_java_member_functions() {
        let re = parse_cspell_regex_pattern(r"/(\.\w+)+(?=\()/g").unwrap();

        assert!(re.is_match("System.getenv("));
        assert!(re.is_match(".getenv("));
    }

    #[test]
    fn cspell_regex_keeps_backreference_semantics_for_markdown_handles() {
        let re = parse_cspell_regex_pattern(r"\[(\*{2})?@[-\w]+?\1\]").unwrap();

        assert!(re.is_match("[@porada]"));
        assert!(re.is_match("[**@porada**]"));
        assert!(!re.is_match("[@porada**]"));
    }

    #[test]
    fn classify_custom_ignore_pattern_detects_hash_strings() {
        let hash_strings = super::classify_custom_ignore_pattern(super::CSPELL_REGEX_HASH_STRINGS);
        assert!(hash_strings.has_hash_strings());
    }

    #[test]
    fn classify_custom_ignore_pattern_detects_latex_patterns() {
        let macro_names = super::classify_custom_ignore_pattern(super::LATEX_MACRO_FUNCTION_NAMES);
        assert!(macro_names.has_latex_macro_function_names());

        let multiline = super::classify_custom_ignore_pattern(super::LATEX_MACROS_MULTILINE);
        assert!(multiline.has_latex_macros_multiline());

        let math = super::classify_custom_ignore_pattern(super::LATEX_MATH);
        assert!(math.has_latex_math());
    }

    #[test]
    fn classify_custom_ignore_pattern_detects_ada_word_break() {
        let ada_word_break = super::classify_custom_ignore_pattern(super::ADA_WORD_BREAK);
        assert!(ada_word_break.has_ada_word_break());
    }
}
