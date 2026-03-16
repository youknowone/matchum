use std::path::{Path, PathBuf};

use crate::commands::cspell::defaults;

/// Try to resolve an npm package import by walking up node_modules or auto-fetching.
pub fn resolve_npm_import(import: &str, base_dir: &Path) -> Option<PathBuf> {
    use matchum_config::npm_fetch;

    let cache_dir = npm_fetch::default_cache_dir();

    // Walk up looking for node_modules/ (project-local dictionaries).
    // Skip the cache dir itself — it's handled below as a fallback.
    let mut search_dir = Some(base_dir);
    while let Some(dir) = search_dir {
        if dir != cache_dir {
            let candidate = dir.join("node_modules").join(import);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        search_dir = dir.parent();
    }

    // Auto-download into cache directory (not project directory)
    let package_name = npm_fetch::extract_package_name(import);
    let sub_path = npm_fetch::extract_sub_path(import);
    let exact_version = defaults::bundled_package_version(package_name);
    if let Ok(pkg_dir) = npm_fetch::ensure_package(package_name, exact_version, &cache_dir) {
        let resolved = match sub_path {
            Some(sub) => pkg_dir.join(sub),
            None => pkg_dir.join("cspell-ext.json"),
        };
        if resolved.exists() {
            return Some(resolved);
        }
    }

    None
}
