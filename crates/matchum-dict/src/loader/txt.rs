use crate::hashdict::HashDictionary;
use regex::Regex;
use std::io::BufRead;
use std::path::Path;
use std::sync::LazyLock;

use super::LoadError;
use super::TextDictionaryFormat;

static LEGACY_SPLIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"[^\w\p{L}\p{M}'’]+"#).expect("valid legacy dictionary split regex")
});

/// Load a plain text dictionary (.txt or .txt.gz) from a file path.
pub fn load_txt(path: &Path) -> Result<HashDictionary, LoadError> {
    load_txt_with_format(path, TextDictionaryFormat::Simple)
}

pub fn load_txt_with_format(
    path: &Path,
    format: TextDictionaryFormat,
) -> Result<HashDictionary, LoadError> {
    let file = std::fs::File::open(path)?;
    let reader: Box<dyn std::io::Read> = if path.extension().is_some_and(|ext| ext == "gz") {
        Box::new(flate2::read::GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    load_txt_from_reader_with_format(std::io::BufReader::new(reader), format)
}

/// Load a plain text dictionary from a buffered reader.
pub fn load_txt_from_reader(reader: impl BufRead) -> Result<HashDictionary, LoadError> {
    load_txt_from_reader_with_format(reader, TextDictionaryFormat::Simple)
}

pub fn load_txt_from_reader_with_format(
    reader: impl BufRead,
    format: TextDictionaryFormat,
) -> Result<HashDictionary, LoadError> {
    match format {
        TextDictionaryFormat::Simple => load_simple_txt_from_reader(reader),
        TextDictionaryFormat::Legacy => load_legacy_txt_from_reader(reader),
        TextDictionaryFormat::WordsPerLine => load_words_per_line_txt_from_reader(reader),
    }
}

fn load_simple_txt_from_reader(reader: impl BufRead) -> Result<HashDictionary, LoadError> {
    let mut dict = HashDictionary::new(false);
    let mut split_mode = false;
    for line in reader.lines() {
        let line = line?;
        let word = line.trim();
        if word.is_empty() {
            continue;
        }
        // Handle comments, but check for cspell-dictionary: / cspell-tools: directives
        if word.starts_with('#') {
            // Both `cspell-dictionary:` and `cspell-tools:` carry the same flags
            let flags_str = word
                .find("cspell-dictionary:")
                .map(|pos| &word[pos + "cspell-dictionary:".len()..])
                .or_else(|| {
                    word.find("cspell-tools:")
                        .map(|pos| &word[pos + "cspell-tools:".len()..])
                });
            if let Some(flags) = flags_str {
                for flag in flags
                    .split([' ', ',', ';'])
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                {
                    match flag {
                        "split" => split_mode = true,
                        "no-split" => split_mode = false,
                        // "keep-case" affects suggestion display, not lookup behavior.
                        // Lookup remains case-insensitive (cspell default).
                        _ => {}
                    }
                }
            }
            continue;
        }
        if split_mode {
            for sub in word
                .split([' ', '\t'])
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                add_word_entry(&mut dict, sub);
            }
        } else {
            add_word_entry(&mut dict, word);
        }
    }
    Ok(dict)
}

fn load_legacy_txt_from_reader(reader: impl BufRead) -> Result<HashDictionary, LoadError> {
    let mut dict = HashDictionary::new(false);
    for line in reader.lines() {
        let line = line?;
        let content = line.split('#').next().unwrap_or("").trim();
        if content.is_empty() {
            continue;
        }
        for sub in LEGACY_SPLIT_RE
            .split(content)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            add_word_entry(&mut dict, sub);
        }
    }
    Ok(dict)
}

fn load_words_per_line_txt_from_reader(reader: impl BufRead) -> Result<HashDictionary, LoadError> {
    let mut dict = HashDictionary::new(false);
    for line in reader.lines() {
        let line = line?;
        let content = line.split('#').next().unwrap_or("").trim();
        if content.is_empty() {
            continue;
        }
        for sub in content.split_whitespace() {
            add_word_entry(&mut dict, sub.trim());
        }
    }
    Ok(dict)
}

/// Parse a single dictionary entry with prefix/suffix markers.
fn add_word_entry(dict: &mut HashDictionary, word: &str) {
    if word.is_empty() {
        return;
    }

    // Suggestion-only prefix ':'  (e.g. :word:suggestion, :word->suggestion)
    if let Some(rest) = word.strip_prefix(':') {
        let base = extract_base_before_suggestion(rest);
        if !base.is_empty() {
            dict.add_word(base);
        }
        return;
    }

    // Preferred suggestion prefix '!*'
    if let Some(rest) = word.strip_prefix("!*") {
        let base = extract_base_before_suggestion(rest);
        if !base.is_empty() {
            dict.add_preferred(base);
        }
        return;
    }

    // Forbidden prefix '!'
    if let Some(rest) = word.strip_prefix('!') {
        let base = extract_base_before_suggestion(rest);
        if !base.is_empty() {
            dict.add_forbidden(base);
        }
        return;
    }

    // No-suggest prefix '~'
    if let Some(rest) = word.strip_prefix('~') {
        dict.add_no_suggest(rest);
        return;
    }

    // Identity prefix '=' (keep exact case, no normalization)
    if let Some(rest) = word.strip_prefix('=') {
        dict.add_identity_word(rest);
        return;
    }

    // Compound markers '*' and '+'
    // '*' at start/end = optional compound (also valid standalone)
    // '+' at start/end = required compound only (not valid standalone)
    // Mixed: *word+ = compound prefix/middle, not standalone
    // AES* = AES can begin a compound (prefix position)
    // *s = s can end a compound (suffix position)
    // *word* = any position in compound, also standalone
    // base*ball = internal * means compound join (both parts are middle)
    if word.contains('*') || word.starts_with('+') || word.ends_with('+') {
        let star_start = word.starts_with('*');
        let star_end = word.ends_with('*');
        let plus_start = word.starts_with('+');
        let plus_end = word.ends_with('+');

        // Strip all leading/trailing * and + markers to get base word
        let base = word.trim_matches(|c| c == '*' || c == '+');

        if base.is_empty() {
            return;
        }

        // Internal * (e.g., `base*ball`) — treat as compound word
        if !star_start && !star_end && base.contains('*') {
            let clean = base.replace('*', "");
            dict.add_word(&clean);
            for segment in base.split('*') {
                if !segment.is_empty() {
                    dict.add_compound_part_with_pos(segment, crate::hashdict::CompoundPos::Middle);
                }
            }
            return;
        }

        let has_start_marker = star_start || plus_start;
        let has_end_marker = star_end || plus_end;

        // In cspell's trie model:
        // - `word+` in main trie (first part) requires `*` at end AND no `+` at start
        //   (`word*` or `*word*`, but NOT `+word*`)
        // - `+word` in compound trie (continuation/last) requires any start marker
        // - `+word+` in compound trie (middle part) requires both start AND end markers
        let can_first = has_end_marker && !plus_start;
        // can_last requires a start marker (creates `+word` in the trie) AND
        // no trailing `+` (trailing `+` creates `+word+` not `+word`, so the
        // word can't be the final compound part).
        let can_last = has_start_marker && !plus_end;
        let can_middle = has_start_marker && has_end_marker;

        // In cspell's trie, a form like `word+` (no `+` prefix) marks `word` as
        // a standalone word AND a compound prefix. So `*` at the start always
        // creates standalone (the `*` expands to empty, yielding `word...`).
        // `*` at the end creates standalone only if there's no `+` at the start
        // (otherwise all forms have a `+` prefix = compound-only).
        let standalone = star_start || (star_end && !plus_start);
        if standalone {
            dict.add_word(base);
        }
        dict.add_compound_part_explicit(base, can_first, can_last, can_middle);
        return;
    }

    // Normal word, possibly with suggestion syntax (word:suggestion or word->suggestion)
    let base = extract_base_before_suggestion(word);
    if !base.is_empty() {
        dict.add_word(base);
    }
}

/// Extract the base word before a suggestion separator (`:` or `->`).
fn extract_base_before_suggestion(word: &str) -> &str {
    // Check for -> first
    if let Some(pos) = word.find("->") {
        return word[..pos].trim();
    }
    // Check for : separator (but not at position 0, which is the suggestion prefix)
    if let Some(pos) = word.find(':') {
        return word[..pos].trim();
    }
    word.trim()
}

#[cfg(test)]
mod tests {
    use super::load_txt_from_reader;
    use crate::dictionary::Dictionary;

    #[test]
    fn coding_compound_terms_are_valid_without_global_compound_mode() {
        let data = br#"
# cspell-tools: keep-case no-split
*Cache*
*Data*
*Domain*
My*
O+
"#;

        let dict = load_txt_from_reader(std::io::BufReader::new(&data[..])).unwrap();

        assert!(dict.has("mydomain"));
        assert!(dict.has("mydata"));
        assert!(dict.has("odata"));
        assert!(dict.has("MYCACHE"));
    }
}
