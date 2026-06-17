use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Collect canonical parent directories that should participate in local config search.
/// cspell searches upward from each file location and does not constrain the
/// walk to the explicit config file's directory.
pub fn collect_search_dirs(files: &[PathBuf]) -> HashSet<PathBuf> {
    let mut dirs: HashSet<PathBuf> = HashSet::new();
    for file in files {
        if let Some(parent) = file.parent() {
            dirs.insert(normalize_search_dir(parent));
        }
    }
    dirs
}

/// Find nearest cspell config from dir upward, stopping only at the requested
/// stop directories.
pub fn find_nearest_config(
    dir: &Path,
    stop_search_at: &[PathBuf],
    cache: &mut HashMap<PathBuf, Option<PathBuf>>,
) -> Option<PathBuf> {
    let stop_set: HashSet<PathBuf> = stop_search_at
        .iter()
        .map(|p| normalize_search_dir(p))
        .collect();
    find_nearest_config_inner(&normalize_search_dir(dir), &stop_set, cache)
}

fn find_nearest_config_inner(
    dir: &Path,
    stop_set: &HashSet<PathBuf>,
    cache: &mut HashMap<PathBuf, Option<PathBuf>>,
) -> Option<PathBuf> {
    let canonical = normalize_search_dir(dir);
    if let Some(cached) = cache.get(&canonical) {
        return cached.clone();
    }

    if stop_set.contains(&canonical) {
        cache.insert(canonical, None);
        return None;
    }
    for name in matchum_config::resolver::CONFIG_NAMES {
        let config_path = canonical.join(name);
        if config_path.exists() && is_searchable_local_config(&config_path) {
            let result = Some(config_path);
            cache.insert(canonical, result.clone());
            return result;
        }
    }
    let result = canonical
        .parent()
        .and_then(|parent| find_nearest_config_inner(parent, stop_set, cache));
    cache.insert(canonical, result.clone());
    result
}

/// Find the nearest per-directory value for a given directory.
pub fn find_nearest_dir_value<'a, T>(
    dir: &Path,
    per_dir: &'a HashMap<PathBuf, Arc<T>>,
) -> Option<&'a Arc<T>> {
    let canonical = normalize_search_dir(dir);
    let mut d = Some(canonical.as_path());
    while let Some(current) = d {
        if let Some(value) = per_dir.get(current) {
            return Some(value);
        }
        d = current.parent();
    }
    None
}

use matchum_config::resolver::is_searchable_cspell_config as is_searchable_local_config;

fn normalize_search_dir(dir: &Path) -> PathBuf {
    if dir.as_os_str().is_empty() {
        return std::env::current_dir().unwrap_or_default();
    }
    if dir.is_absolute() {
        return dir.to_path_buf();
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    let resolved = cwd.join(dir);
    resolved.canonicalize().unwrap_or(resolved)
}

#[cfg(test)]
mod tests {
    use super::{collect_search_dirs, find_nearest_config};
    use std::collections::HashMap;

    #[test]
    fn collect_search_dirs_is_not_limited_by_explicit_config_dir() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir
            .path()
            .join("repositories/temp/microsoft/TypeScript-Website");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        let file = repo.join("src/index.ts");
        std::fs::write(&file, "const value = 1;\n").unwrap();

        let dirs = collect_search_dirs(&[file]);
        assert!(dirs.iter().any(|dir| {
            dir.canonicalize().unwrap_or_else(|_| dir.clone())
                == repo.join("src").canonicalize().unwrap()
        }));
    }

    #[test]
    fn find_nearest_config_skips_package_json_without_cspell_section() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        let nested = root.join("src");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(root.join("package.json"), "{ \"name\": \"repo\" }\n").unwrap();
        std::fs::write(root.join("cspell.yaml"), "version: '0.2'\n").unwrap();

        let mut cache = HashMap::new();
        let found = find_nearest_config(&nested, &[], &mut cache);
        assert_eq!(
            found.map(|path| path.canonicalize().unwrap_or(path)),
            Some(root.join("cspell.yaml").canonicalize().unwrap())
        );
    }
}
