//! Shared glob/path matching utilities used by overrides and file collection.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

pub fn is_global_pattern(pattern: &str) -> bool {
    pattern.trim_start_matches('!').starts_with("**")
}

pub fn normalized_match_path(path: &Path) -> Option<Cow<'_, str>> {
    match path.to_str() {
        Some(path_str) if !path_str.contains('\\') => None,
        Some(path_str) => Some(Cow::Owned(path_str.replace('\\', "/"))),
        None => Some(Cow::Owned(path.to_string_lossy().replace('\\', "/"))),
    }
}

pub fn global_match_path(path: &Path) -> PathBuf {
    let absolute = absolute_match_path(path);
    let normalized = absolute.to_string_lossy().replace('\\', "/");
    let normalized = normalized.trim_start_matches('/').to_string();
    PathBuf::from(normalized)
}

pub fn resolve_match_root(root: &str) -> PathBuf {
    let path = Path::new(root);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    }
}

pub fn absolute_match_path<'a>(path: &'a Path) -> Cow<'a, Path> {
    if path.is_absolute() {
        Cow::Borrowed(path)
    } else {
        Cow::Owned(std::env::current_dir().unwrap_or_default().join(path))
    }
}

pub fn root_relative_match_path<'a>(
    file_path: &'a Path,
    root: Option<&Path>,
) -> Option<Cow<'a, Path>> {
    match root {
        Some(root) => {
            let absolute = absolute_match_path(file_path);
            Some(Cow::Owned(absolute.strip_prefix(root).ok()?.to_path_buf()))
        }
        None => Some(Cow::Borrowed(file_path)),
    }
}
