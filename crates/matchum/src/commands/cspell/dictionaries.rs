use anyhow::{Context, Result};
use matchum_config::resolver;
use matchum_config::settings::CSpellSettings;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[allow(dead_code)]
pub struct DictListOptions {
    pub config: Option<PathBuf>,
    pub path_format: Option<String>,
    pub enabled: bool,
    pub locale: Option<String>,
    pub file_type: Option<String>,
    pub no_show_location: bool,
    pub no_default_configuration: bool,
}

/// cspell dictionaries: list available dictionaries.
pub fn run(opts: DictListOptions) -> Result<()> {
    let (mut settings, _config_dir) = if opts.no_default_configuration {
        (CSpellSettings::default(), None)
    } else {
        load_settings(opts.config.as_deref())?
    };

    // Apply --locale override
    if let Some(ref locale) = opts.locale {
        settings.language = Some(locale.clone());
    }

    let enabled_set: HashSet<String> = settings
        .dictionaries
        .iter()
        .map(|d| d.to_lowercase())
        .collect();

    let use_short_path = opts.path_format.as_deref() == Some("short");

    let mut count = 0;
    for def in &settings.dictionary_definitions {
        let is_enabled = enabled_set.contains(&def.name.to_lowercase());

        if opts.enabled && !is_enabled {
            continue;
        }

        let marker = if is_enabled { "*" } else { " " };
        let path_display = if opts.no_show_location {
            String::new()
        } else {
            def.path
                .as_deref()
                .map(|p| {
                    if use_short_path {
                        // Show only the filename
                        Path::new(p)
                            .file_name()
                            .map(|f| format!("  {}", f.to_string_lossy()))
                            .unwrap_or_else(|| format!("  {p}"))
                    } else {
                        format!("  {p}")
                    }
                })
                .unwrap_or_default()
        };

        println!(" {marker} {}{}", def.name, path_display);
        count += 1;
    }

    if count == 0 {
        println!("No dictionaries found.");
    } else {
        eprintln!("\n{count} dictionary(ies) found. (* = enabled)");
    }

    Ok(())
}

fn load_settings(config_path: Option<&Path>) -> Result<(CSpellSettings, Option<PathBuf>)> {
    if let Some(path) = config_path {
        let config_dir = path.parent().map(|p| p.to_path_buf());
        let settings = resolver::load_config(path).context("failed to load config file")?;
        Ok((settings, config_dir))
    } else {
        let cwd = std::env::current_dir()?;
        match resolver::find_config(&cwd) {
            Some(path) => {
                let config_dir = path.parent().map(|p| p.to_path_buf());
                let settings =
                    resolver::load_config(&path).context("failed to load config file")?;
                Ok((settings, config_dir))
            }
            None => Ok((CSpellSettings::default(), None)),
        }
    }
}
