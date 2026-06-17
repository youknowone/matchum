pub mod hunspell;
pub mod trie_v3;
pub mod txt;

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::hashdict::HashDictionary;

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Format error: {0}")]
    Format(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDictionaryFormat {
    Simple,
    Legacy,
    WordsPerLine,
}

/// Load a dictionary file with binary cache.
///
/// On first load, parses the source file (trie/txt) and writes a binary cache.
/// On subsequent loads, reads the cache directly (skipping gz decompression,
/// trie parsing, and normalization). Cache is invalidated when source file's
/// mtime or size changes.
pub fn load_dictionary(path: &Path) -> Result<HashDictionary, LoadError> {
    // Try loading from cache
    if let Some(dict) = try_load_cache(path) {
        return Ok(dict);
    }

    // Parse from source
    let dict = load_dictionary_inner(path)?;

    // Write cache (best-effort)
    let _ = write_cache(path, &dict);

    Ok(dict)
}

pub fn load_dictionary_with_options(
    path: &Path,
    type_hint: Option<&str>,
) -> Result<HashDictionary, LoadError> {
    let explicit = type_hint.and_then(parse_type_hint);
    if explicit.is_none() {
        return load_dictionary(path);
    }
    load_dictionary_inner_with_options(path, explicit)
}

/// Load dictionary from source file without cache.
fn load_dictionary_inner(path: &Path) -> Result<HashDictionary, LoadError> {
    load_dictionary_inner_with_options(path, None)
}

fn load_dictionary_inner_with_options(
    path: &Path,
    type_hint: Option<TextDictionaryFormat>,
) -> Result<HashDictionary, LoadError> {
    let name = path.to_string_lossy();
    if matches!(type_hint, Some(TextDictionaryFormat::Simple))
        && (name.ends_with(".trie") || name.ends_with(".trie.gz"))
    {
        trie_v3::load_trie_v3(path)
    } else if matches!(
        type_hint,
        Some(TextDictionaryFormat::Legacy | TextDictionaryFormat::WordsPerLine)
    ) {
        txt::load_txt_with_format(path, type_hint.expect("checked above"))
    } else if name.ends_with(".trie") || name.ends_with(".trie.gz") {
        trie_v3::load_trie_v3(path)
    } else if name.ends_with(".dic") {
        let aff_path = path.with_extension("aff");
        hunspell::load_hunspell(path, &aff_path)
    } else {
        txt::load_txt_with_format(path, type_hint.unwrap_or(TextDictionaryFormat::Simple))
    }
}

/// Dictionary format for loading from bytes.
pub enum DictFormat {
    /// Plain text (.txt)
    Txt,
    /// Plain text using cspell's legacy `C` parser.
    TxtLegacy,
    /// Plain text using cspell's whitespace-delimited `W` parser.
    TxtWordsPerLine,
    /// Gzip-compressed text (.txt.gz)
    TxtGz,
    /// TrieXv3 format (.trie)
    TrieV3,
    /// Gzip-compressed TrieXv3 (.trie.gz)
    TrieV3Gz,
}

/// Load a dictionary from raw bytes without filesystem access.
pub fn load_dictionary_from_bytes(
    data: &[u8],
    format: DictFormat,
) -> Result<HashDictionary, LoadError> {
    match format {
        DictFormat::Txt => txt::load_txt_from_reader_with_format(
            std::io::BufReader::new(data),
            TextDictionaryFormat::Simple,
        ),
        DictFormat::TxtLegacy => txt::load_txt_from_reader_with_format(
            std::io::BufReader::new(data),
            TextDictionaryFormat::Legacy,
        ),
        DictFormat::TxtWordsPerLine => txt::load_txt_from_reader_with_format(
            std::io::BufReader::new(data),
            TextDictionaryFormat::WordsPerLine,
        ),
        DictFormat::TxtGz => {
            let decoder = flate2::read::GzDecoder::new(data);
            txt::load_txt_from_reader_with_format(
                std::io::BufReader::new(decoder),
                TextDictionaryFormat::Simple,
            )
        }
        DictFormat::TrieV3 => trie_v3::load_trie_v3_from_reader(data),
        DictFormat::TrieV3Gz => {
            let decoder = flate2::read::GzDecoder::new(data);
            trie_v3::load_trie_v3_from_reader(decoder)
        }
    }
}

fn parse_type_hint(type_hint: &str) -> Option<TextDictionaryFormat> {
    match type_hint.trim().to_ascii_uppercase().as_str() {
        "S" => Some(TextDictionaryFormat::Simple),
        "C" => Some(TextDictionaryFormat::Legacy),
        "W" => Some(TextDictionaryFormat::WordsPerLine),
        "T" => Some(TextDictionaryFormat::Simple),
        _ => None,
    }
}

fn cache_path(source: &Path) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let abs = std::fs::canonicalize(source).ok()?;
    let hash = {
        let bytes = abs.as_os_str().as_encoded_bytes();
        // FNV-1a 64-bit
        let mut h: u64 = 0xcbf29ce484222325;
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    };
    Some(
        PathBuf::from(home)
            .join(".matchum_cache")
            .join("dicts")
            .join(format!("{:016x}.bin", hash)),
    )
}

fn source_metadata(source: &Path) -> Option<(u64, u32)> {
    let meta = std::fs::metadata(source).ok()?;
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();
    let size = meta.len() as u32;
    Some((mtime, size))
}

fn try_load_cache(source: &Path) -> Option<HashDictionary> {
    let (mtime, size) = source_metadata(source)?;
    let cp = cache_path(source)?;
    let file = std::fs::File::open(&cp).ok()?;
    let mmap = unsafe { memmap2::Mmap::map(&file) }.ok()?;
    HashDictionary::from_cache_bytes(&mmap, mtime, size)
}

fn write_cache(source: &Path, dict: &HashDictionary) -> std::io::Result<()> {
    let (mtime, size) = source_metadata(source)
        .ok_or_else(|| std::io::Error::other("cannot read source metadata"))?;
    let cp =
        cache_path(source).ok_or_else(|| std::io::Error::other("cannot determine cache path"))?;
    if let Some(parent) = cp.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&cp, dict.to_cache_bytes(mtime, size))
}
