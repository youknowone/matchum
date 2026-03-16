use anyhow::{Context, Result};
use matchum_config::resolver;
use matchum_config::settings::CSpellSettings;
use std::path::{Path, PathBuf};

pub struct ResolvedConfig {
    pub settings: CSpellSettings,
    pub config_dir: Option<PathBuf>,
    pub is_cspell: bool,
    pub config_file: Option<PathBuf>,
}

/// Search for and load the nearest config file.
///
/// Returns `is_cspell = true` when the config was loaded from a cspell.json file,
/// `false` for matchum.toml. When no config file is found, settings are empty and
/// `is_cspell` is true (callers should apply their own fallback).
pub fn resolve_config(
    config_path: Option<&Path>,
    start_path: Option<&Path>,
    config_search: bool,
    stop_config_search_at: &[PathBuf],
) -> Result<ResolvedConfig> {
    if let Some(path) = config_path {
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        let path = path.canonicalize().unwrap_or(path);
        let config_dir = path.parent().map(|p| p.to_path_buf());
        let is_cspell = resolver::ConfigFound::classify(&path).is_cspell();
        let settings = resolver::load_config_auto(&path).context("failed to load config")?;
        return Ok(ResolvedConfig {
            settings,
            config_dir,
            is_cspell,
            config_file: Some(path),
        });
    }

    if !config_search {
        return Ok(ResolvedConfig {
            settings: CSpellSettings::default(),
            config_dir: None,
            is_cspell: true,
            config_file: None,
        });
    }

    let search_dir = start_path
        .and_then(|p| {
            if p.is_dir() {
                Some(p.to_path_buf())
            } else {
                p.parent().map(|pp| pp.to_path_buf())
            }
        })
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    match resolver::find_config_prioritized_with_stop(&search_dir, stop_config_search_at) {
        resolver::ConfigFound::Matchum(path) => {
            let config_dir = path.parent().map(|p| p.to_path_buf());
            let settings =
                resolver::load_matchum_config(&path).context("failed to load matchum.toml")?;
            Ok(ResolvedConfig {
                settings,
                config_dir,
                is_cspell: false,
                config_file: Some(path),
            })
        }
        resolver::ConfigFound::Cspell(path) => {
            eprintln!(
                "hint: using cspell config. Run `matchum init --from-cspell` to migrate to matchum.toml."
            );
            let config_dir = path.parent().map(|p| p.to_path_buf());
            let settings = resolver::load_config(&path).context("failed to load config file")?;
            Ok(ResolvedConfig {
                settings,
                config_dir,
                is_cspell: true,
                config_file: Some(path),
            })
        }
        resolver::ConfigFound::None => Ok(ResolvedConfig {
            settings: CSpellSettings::default(),
            config_dir: None,
            is_cspell: true,
            config_file: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CwdGuard(PathBuf);

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }

    #[test]
    fn explicit_relative_config_path_is_resolved_to_absolute_dir() {
        let _guard = CwdGuard(std::env::current_dir().unwrap());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let dir = dir.path().canonicalize().unwrap();

        std::fs::write(
            dir.join("cspell.json"),
            "{ \"version\": \"0.2\", \"import\": [\"cspell.yaml\"] }\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("cspell.yaml"),
            "version: '0.2'\nignorePaths:\n  - eng/**\n",
        )
        .unwrap();

        let resolved = resolve_config(Some(Path::new("cspell.json")), None, true, &[]).unwrap();

        assert_eq!(resolved.config_dir.as_deref(), Some(dir.as_path()));
        assert!(
            !resolved.settings.resolved_ignore_paths.is_empty(),
            "expected imported ignore paths to preserve resolved roots"
        );
        assert_eq!(
            resolved
                .settings
                .resolved_ignore_paths
                .iter()
                .next()
                .and_then(|g| g.root.as_deref()),
            Some(dir.to_string_lossy().as_ref())
        );
    }
}
