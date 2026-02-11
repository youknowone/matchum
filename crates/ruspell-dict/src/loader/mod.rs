pub mod trie_v3;
pub mod txt;

use std::path::Path;

use crate::hashdict::HashDictionary;

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Format error: {0}")]
    Format(String),
}

/// Load a dictionary file, auto-detecting format from extension.
///
/// Supported formats:
/// - `.txt`, `.txt.gz` — plain text word list
/// - `.trie`, `.trie.gz` — TrieXv3 format
pub fn load_dictionary(path: &Path) -> Result<HashDictionary, LoadError> {
    let name = path.to_string_lossy();
    if name.ends_with(".trie") || name.ends_with(".trie.gz") {
        trie_v3::load_trie_v3(path)
    } else {
        txt::load_txt(path)
    }
}
