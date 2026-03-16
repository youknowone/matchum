use std::collections::HashSet;
use std::path::Path;

/// Append words to a word-list file, skipping duplicates (case-insensitive).
/// Creates parent directories and the file itself if they don't exist.
/// Returns the list of words that were actually added.
pub fn append_words(path: &Path, words: &[&str]) -> std::io::Result<Vec<String>> {
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)?;
    }

    let existing_content = std::fs::read_to_string(path).unwrap_or_default();
    let existing: HashSet<String> = existing_content
        .lines()
        .map(|l| l.trim().to_lowercase())
        .collect();

    let mut added = Vec::new();
    let mut buf = existing_content;
    for &word in words {
        if !existing.contains(&word.to_lowercase()) {
            if !buf.is_empty() && !buf.ends_with('\n') {
                buf.push('\n');
            }
            buf.push_str(word);
            buf.push('\n');
            added.push(word.to_string());
        }
    }

    if !added.is_empty() {
        std::fs::write(path, &buf)?;
    }

    Ok(added)
}

/// Append a single word to a word-list file, skipping if already present.
pub fn append_word(path: &Path, word: &str) -> std::io::Result<bool> {
    let result = append_words(path, &[word])?;
    Ok(!result.is_empty())
}
