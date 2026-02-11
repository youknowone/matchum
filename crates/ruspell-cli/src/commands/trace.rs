use anyhow::{Context, Result};
use ruspell_config::resolver;
use ruspell_config::settings::CSpellSettings;
use ruspell_dict::dictionary::Dictionary;
use ruspell_dict::loader;
use std::path::{Path, PathBuf};

/// Trace a word through all loaded dictionaries.
pub fn run_trace(word: &str, config_path: Option<&Path>) -> Result<()> {
    let (settings, config_dir) = load_settings(config_path)?;
    let dictionaries = build_named_dictionaries(&settings, config_dir.as_deref())?;

    println!("Tracing word: '{}'", word);
    println!();

    let mut found_in_any = false;

    for (name, dict) in &dictionaries {
        let result = dict.find(word);
        if result.found {
            found_in_any = true;
            let status = if result.forbidden {
                "forbidden"
            } else if result.no_suggest {
                "found (no-suggest)"
            } else {
                "found"
            };
            println!("  [{}] {} - {}", status, name, word);
        }
    }

    if settings.words.iter().any(|w| w.eq_ignore_ascii_case(word)) {
        found_in_any = true;
        println!("  [found] <inline words> - {}", word);
    }

    if settings
        .ignore_words
        .iter()
        .any(|w| w.eq_ignore_ascii_case(word))
    {
        found_in_any = true;
        println!("  [ignored] <ignoreWords> - {}", word);
    }

    if settings
        .flag_words
        .iter()
        .any(|w| w.eq_ignore_ascii_case(word))
    {
        println!("  [forbidden] <flagWords> - {}", word);
    }

    if !found_in_any {
        println!("  Word '{}' not found in any dictionary.", word);
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

fn build_named_dictionaries(
    settings: &CSpellSettings,
    config_dir: Option<&Path>,
) -> Result<Vec<(String, Box<dyn Dictionary>)>> {
    let mut dicts: Vec<(String, Box<dyn Dictionary>)> = Vec::new();

    for def in &settings.dictionary_definitions {
        if let Some(ref path_str) = def.path {
            let path = Path::new(path_str);
            let resolved = if path.is_absolute() {
                path.to_path_buf()
            } else if let Some(base) = config_dir {
                base.join(path)
            } else {
                path.to_path_buf()
            };
            if resolved.exists() {
                match loader::load_dictionary(&resolved) {
                    Ok(dict) => dicts.push((def.name.clone(), Box::new(dict))),
                    Err(e) => {
                        eprintln!("Warning: failed to load dictionary {}: {}", path_str, e)
                    }
                }
            }
        }
    }

    Ok(dicts)
}
