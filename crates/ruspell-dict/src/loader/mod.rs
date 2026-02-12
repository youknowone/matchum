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

/// Load dictionary from source file without cache.
fn load_dictionary_inner(path: &Path) -> Result<HashDictionary, LoadError> {
    let name = path.to_string_lossy();
    if name.ends_with(".trie") || name.ends_with(".trie.gz") {
        trie_v3::load_trie_v3(path)
    } else {
        txt::load_txt(path)
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
            .join(".ruspell_cache")
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
    let cache_bytes = std::fs::read(&cp).ok()?;
    HashDictionary::from_cache_bytes(&cache_bytes, mtime, size)
}

fn write_cache(source: &Path, dict: &HashDictionary) -> std::io::Result<()> {
    let (mtime, size) = source_metadata(source).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::Other, "cannot read source metadata")
    })?;
    let cp = cache_path(source).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::Other, "cannot determine cache path")
    })?;
    if let Some(parent) = cp.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&cp, dict.to_cache_bytes(mtime, size))
}
