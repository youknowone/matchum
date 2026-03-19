use flate2::read::GzDecoder;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tar::Archive;

fn tls_config() -> ureq::tls::TlsConfig {
    ureq::tls::TlsConfig::builder()
        .unversioned_rustls_crypto_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .build()
}

const NPM_REGISTRY: &str = "https://registry.npmjs.org";

/// Default cache directory for downloaded npm packages.
/// Packages are stored here instead of creating `node_modules/` in the project directory.
pub fn default_cache_dir() -> PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_default();
    PathBuf::from(home).join(".matchum_cache").join("packages")
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("package not found: {0}")]
    NotFound(String),
    #[error("invalid package name: {0}")]
    InvalidPackage(String),
}

#[derive(Debug, Deserialize)]
struct PackageMeta {
    version: String,
    dist: Dist,
}

#[derive(Debug, Deserialize)]
struct Dist {
    tarball: String,
}

/// Parse `@scope/name@version` into (package_name, version).
///
/// Examples:
/// - `@cspell/dict-en_us` → `("@cspell/dict-en_us", None)`
/// - `@cspell/dict-en_us@4.4.29` → `("@cspell/dict-en_us", Some("4.4.29"))`
pub fn parse_package_spec(spec: &str) -> Result<(&str, Option<&str>), FetchError> {
    // Handle scoped packages: @scope/name[@version]
    if let Some(rest) = spec.strip_prefix('@') {
        // Find scope/name boundary
        if let Some(slash_pos) = rest.find('/') {
            let after_slash = &rest[slash_pos + 1..];
            // Check for @version after the name
            if let Some(at_pos) = after_slash.find('@') {
                let name_end = 1 + slash_pos + 1 + at_pos; // +1 for leading @
                let name = &spec[..name_end];
                let version = &spec[name_end + 1..];
                if version.is_empty() {
                    return Err(FetchError::InvalidPackage(spec.to_string()));
                }
                return Ok((name, Some(version)));
            }
            return Ok((spec, None));
        }
    }
    // Non-scoped: name[@version]
    if let Some(at_pos) = spec.find('@') {
        let name = &spec[..at_pos];
        let version = &spec[at_pos + 1..];
        if name.is_empty() || version.is_empty() {
            return Err(FetchError::InvalidPackage(spec.to_string()));
        }
        Ok((name, Some(version)))
    } else {
        Ok((spec, None))
    }
}

/// Extract the package name from an import path.
///
/// `@cspell/dict-en_us/cspell-ext.json` → `@cspell/dict-en_us`
pub fn extract_package_name(import: &str) -> &str {
    if let Some(rest) = import.strip_prefix('@') {
        // Scoped package: @scope/name/sub/path
        if let Some(slash_pos) = rest.find('/') {
            let after_scope = &rest[slash_pos + 1..];
            if let Some(next_slash) = after_scope.find('/') {
                // @scope/name + /sub/path
                return &import[..1 + slash_pos + 1 + next_slash];
            }
            // @scope/name (no sub-path)
            return import;
        }
    }
    // Non-scoped: split at first /
    import.split('/').next().unwrap_or(import)
}

/// Extract the sub-path from an import path (after the package name).
///
/// `@cspell/dict-en_us/cspell-ext.json` → `cspell-ext.json`
pub fn extract_sub_path(import: &str) -> Option<&str> {
    let pkg_name = extract_package_name(import);
    let rest = import.strip_prefix(pkg_name)?;
    let rest = rest.strip_prefix('/')?;
    if rest.is_empty() { None } else { Some(rest) }
}

/// Fetch package metadata from the npm registry.
fn fetch_package_meta(
    package_name: &str,
    version: Option<&str>,
) -> Result<PackageMeta, FetchError> {
    let url = match version {
        Some(v) => format!("{NPM_REGISTRY}/{package_name}/{v}"),
        None => format!("{NPM_REGISTRY}/{package_name}/latest"),
    };

    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .tls_config(tls_config())
            .timeout_global(Some(std::time::Duration::from_secs(30)))
            .build(),
    );
    let mut response = agent.get(&url).call().map_err(|e| {
        if e.to_string().contains("404") || e.to_string().contains("Not Found") {
            FetchError::NotFound(package_name.to_string())
        } else {
            FetchError::Http(e.to_string())
        }
    })?;

    let status = response.status();
    if status == 404 {
        return Err(FetchError::NotFound(package_name.to_string()));
    }

    let body: Vec<u8> = response
        .body_mut()
        .read_to_vec()
        .map_err(|e| FetchError::Http(e.to_string()))?;

    let meta: PackageMeta = serde_json::from_slice(&body)?;
    Ok(meta)
}

/// Download a tarball and extract its contents to target_dir.
fn download_and_extract(tarball_url: &str, target_dir: &Path) -> Result<(), FetchError> {
    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .tls_config(tls_config())
            .timeout_global(Some(std::time::Duration::from_secs(30)))
            .build(),
    );
    let body: Vec<u8> = agent
        .get(tarball_url)
        .call()
        .map_err(|e| FetchError::Http(e.to_string()))?
        .body_mut()
        .read_to_vec()
        .map_err(|e| FetchError::Http(e.to_string()))?;

    let gz = GzDecoder::new(&body[..]);
    let mut archive = Archive::new(gz);

    // Ensure target directory exists
    fs::create_dir_all(target_dir)?;

    // Extract, stripping the `package/` prefix
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();

        // Strip "package/" prefix from tar entries
        let stripped = path.strip_prefix("package").unwrap_or(&path).to_path_buf();

        if stripped.as_os_str().is_empty() {
            continue;
        }

        let dest = target_dir.join(&stripped);

        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&dest)?;
        } else {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut file = fs::File::create(&dest)?;
            std::io::copy(&mut entry, &mut file)?;
        }
    }

    Ok(())
}

/// Check if a package is already installed with the expected version.
fn is_installed(pkg_dir: &Path, expected_version: Option<&str>) -> bool {
    let pkg_json = pkg_dir.join("package.json");
    if !pkg_json.exists() {
        return false;
    }
    let Some(version) = expected_version else {
        return true; // No specific version required, any installed version is fine
    };

    // Read package.json and check version
    let Ok(content) = fs::read_to_string(&pkg_json) else {
        return false;
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) else {
        return false;
    };
    parsed
        .get("version")
        .and_then(|v| v.as_str())
        .is_some_and(|v| v == version)
}

/// Ensure an npm package is available locally under `project_dir/node_modules/`.
///
/// If the package is already installed (and matches the version if specified),
/// returns the existing path. Otherwise downloads from the npm registry.
/// A negative cache prevents repeated 404 requests for non-existent packages.
pub fn ensure_package(
    package_name: &str,
    version: Option<&str>,
    project_dir: &Path,
) -> Result<PathBuf, FetchError> {
    let pkg_dir = project_dir.join("node_modules").join(package_name);

    if is_installed(&pkg_dir, version) {
        return Ok(pkg_dir);
    }

    // Check negative cache: skip packages previously confirmed as non-existent.
    let neg_marker = pkg_dir.with_extension("notfound");
    if neg_marker.exists() {
        return Err(FetchError::NotFound(package_name.to_string()));
    }

    eprintln!(
        "Fetching {package_name}{}...",
        version.map_or(String::new(), |v| format!("@{v}"))
    );

    let meta = match fetch_package_meta(package_name, version) {
        Ok(m) => m,
        Err(FetchError::NotFound(_)) => {
            // Write negative cache marker so we don't retry.
            if let Some(parent) = neg_marker.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&neg_marker, b"");
            return Err(FetchError::NotFound(package_name.to_string()));
        }
        Err(e) => return Err(e),
    };

    // Remove old version if exists
    if pkg_dir.exists() {
        fs::remove_dir_all(&pkg_dir)?;
    }

    download_and_extract(&meta.dist.tarball, &pkg_dir)?;

    eprintln!(
        "Installed {package_name}@{} → {}",
        meta.version,
        pkg_dir.display()
    );

    Ok(pkg_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_scoped_package() {
        let (name, ver) = parse_package_spec("@cspell/dict-en_us").unwrap();
        assert_eq!(name, "@cspell/dict-en_us");
        assert_eq!(ver, None);
    }

    #[test]
    fn parse_scoped_package_with_version() {
        let (name, ver) = parse_package_spec("@cspell/dict-en_us@4.4.29").unwrap();
        assert_eq!(name, "@cspell/dict-en_us");
        assert_eq!(ver, Some("4.4.29"));
    }

    #[test]
    fn parse_unscoped_package() {
        let (name, ver) = parse_package_spec("lodash").unwrap();
        assert_eq!(name, "lodash");
        assert_eq!(ver, None);
    }

    #[test]
    fn parse_unscoped_package_with_version() {
        let (name, ver) = parse_package_spec("lodash@4.17.21").unwrap();
        assert_eq!(name, "lodash");
        assert_eq!(ver, Some("4.17.21"));
    }

    #[test]
    fn extract_pkg_name_scoped() {
        assert_eq!(
            extract_package_name("@cspell/dict-en_us/cspell-ext.json"),
            "@cspell/dict-en_us"
        );
    }

    #[test]
    fn extract_pkg_name_scoped_no_sub() {
        assert_eq!(
            extract_package_name("@cspell/dict-en_us"),
            "@cspell/dict-en_us"
        );
    }

    #[test]
    fn extract_sub_path_exists() {
        assert_eq!(
            extract_sub_path("@cspell/dict-en_us/cspell-ext.json"),
            Some("cspell-ext.json")
        );
    }

    #[test]
    fn extract_sub_path_none() {
        assert_eq!(extract_sub_path("@cspell/dict-en_us"), None);
    }

    #[test]
    fn extract_sub_path_nested() {
        assert_eq!(
            extract_sub_path("@cspell/dict-en_us/dict/en_US.trie.gz"),
            Some("dict/en_US.trie.gz")
        );
    }
}
