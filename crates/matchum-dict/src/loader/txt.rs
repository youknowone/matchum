use crate::hashdict::HashDictionary;
use std::path::Path;

use super::LoadError;

/// Load a plain text dictionary (.txt or .txt.gz).
pub fn load_txt(path: &Path) -> Result<HashDictionary, LoadError> {
    let file = std::fs::File::open(path)?;
    let reader: Box<dyn std::io::Read> = if path.extension().is_some_and(|ext| ext == "gz") {
        Box::new(flate2::read::GzDecoder::new(file))
    } else {
        Box::new(file)
    };

    let mut dict = HashDictionary::new(false);
    let mut split_mode = false;
    let buf = std::io::BufReader::new(reader);
    for line in std::io::BufRead::lines(buf) {
        let line = line?;
        let word = line.trim();
        if word.is_empty() {
            continue;
        }
        // Handle comments, but check for cspell-dictionary: directives
        if word.starts_with('#') {
            if let Some(pos) = word.find("cspell-dictionary:") {
                let flags = &word[pos + "cspell-dictionary:".len()..];
                for flag in flags
                    .split([' ', ',', ';'])
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                {
                    match flag {
                        "split" => split_mode = true,
                        "no-split" => split_mode = false,
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

/// Parse a single dictionary entry with prefix/suffix markers.
fn add_word_entry(dict: &mut HashDictionary, word: &str) {
    if word.is_empty() {
        return;
    }

    // Suggestion-only prefix ':'  (e.g. :word:suggestion, :word->suggestion)
    if word.starts_with(':') {
        let rest = &word[1..];
        let base = extract_base_before_suggestion(rest);
        if !base.is_empty() {
            dict.add_word(base);
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
    if word.contains('*') {
        // '*' = optional compound: generates both plain and compound forms
        let base = word.replace('*', "");
        if !base.is_empty() {
            dict.add_word(&base);
            dict.add_compound_part(&base);
        }
        return;
    }
    if word.starts_with('+') || word.ends_with('+') {
        // '+' = required compound only
        let base = word.trim_matches('+');
        if !base.is_empty() {
            dict.add_compound_part(base);
        }
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
