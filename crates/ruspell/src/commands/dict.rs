use anyhow::{Context, Result};
use ruspell_config::{npm_fetch, resolver};
use std::path::{Path, PathBuf};

/// Fetch all @cspell/dict-* imports from the config, or specific packages.
pub fn run_fetch(
    packages: &[String],
    config_path: Option<&Path>,
    project_dir: &Path,
) -> Result<()> {
    if packages.is_empty() {
        // Fetch all imports from config
        fetch_from_config(config_path, project_dir)?;
    } else {
        // Fetch specific packages
        for spec in packages {
            let (name, version) =
                npm_fetch::parse_package_spec(spec).context("invalid package spec")?;
            npm_fetch::ensure_package(name, version, project_dir)
                .with_context(|| format!("failed to fetch {spec}"))?;
        }
    }
    Ok(())
}

/// List installed dictionary packages under node_modules/@cspell/.
pub fn run_list(project_dir: &Path) -> Result<()> {
    let cspell_dir = project_dir.join("node_modules/@cspell");
    if !cspell_dir.exists() {
        println!("No @cspell dictionary packages installed.");
        return Ok(());
    }

    let mut count = 0;
    for entry in std::fs::read_dir(&cspell_dir)? {
        let entry = entry?;
        let pkg_dir = entry.path();
        if !pkg_dir.is_dir() {
            continue;
        }
        let pkg_json = pkg_dir.join("package.json");
        if !pkg_json.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&pkg_json)?;
        let parsed: serde_json::Value = serde_json::from_str(&content)?;
        let name = parsed.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let version = parsed
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        println!("  {name}@{version}");
        count += 1;
    }

    if count == 0 {
        println!("No @cspell dictionary packages installed.");
    } else {
        println!("\n{count} package(s) installed.");
    }
    Ok(())
}

fn fetch_from_config(config_path: Option<&Path>, project_dir: &Path) -> Result<()> {
    let config_file = if let Some(p) = config_path {
        p.to_path_buf()
    } else {
        resolver::find_config(project_dir).context("no cspell config found")?
    };

    let settings = resolver::load_config(&config_file).context("failed to parse config")?;

    let mut fetched = 0;
    for import in &settings.import {
        let pkg_name = npm_fetch::extract_package_name(import);
        // Only auto-fetch scoped packages (not relative paths)
        if !pkg_name.starts_with('@') {
            continue;
        }
        match npm_fetch::ensure_package(pkg_name, None, project_dir) {
            Ok(_) => fetched += 1,
            Err(e) => eprintln!("Warning: failed to fetch {pkg_name}: {e}"),
        }
    }

    if fetched == 0 {
        println!("No packages to fetch.");
    } else {
        println!("\nFetched {fetched} package(s).");
    }
    Ok(())
}

/// Find the project root by looking for cspell config or Cargo.toml.
pub fn find_project_dir(hint: Option<&Path>) -> PathBuf {
    if let Some(h) = hint {
        if h.is_dir() {
            return h.to_path_buf();
        }
        if let Some(parent) = h.parent() {
            return parent.to_path_buf();
        }
    }
    std::env::current_dir().unwrap_or_default()
}
