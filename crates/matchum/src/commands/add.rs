use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn run_add(words: &[String], forbidden: bool, config_path: Option<&Path>) -> Result<()> {
    if words.is_empty() {
        anyhow::bail!("No words specified");
    }

    let config_path = find_matchum_config(config_path)?;
    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    let config: matchum_config::matchum_config::MatchumConfig = toml::from_str(&content)
        .with_context(|| format!("failed to parse {}", config_path.display()))?;

    let config_dir = config_path.parent().unwrap_or(Path::new("."));
    let (key_label, word_file_path) = if forbidden {
        ("flag_words_file", config.flag_words_file.clone())
    } else {
        ("words_file", config.words_file.clone())
    };

    let file_path = match word_file_path {
        Some(p) => config_dir.join(&p),
        None => {
            // Create default word file path and update config
            let default_name = if forbidden {
                ".matchum/flagged.txt"
            } else {
                ".matchum/words.txt"
            };
            let full_path = config_dir.join(default_name);
            if let Some(parent) = full_path.parent()
                && !parent.exists()
            {
                std::fs::create_dir_all(parent)?;
            }
            // Add the field to the config file
            if !content.contains(&format!("{key_label} ="))
                && !content.contains(&format!("{key_label}="))
            {
                let new_line = format!("{key_label} = \"{default_name}\"\n");
                let updated = format!("{new_line}{content}");
                std::fs::write(&config_path, &updated)?;
            }
            full_path
        }
    };

    let word_refs: Vec<&str> = words.iter().map(|s| s.as_str()).collect();
    let added = matchum_config::words_file::append_words(&file_path, &word_refs)
        .with_context(|| format!("failed to write {}", file_path.display()))?;

    if added.is_empty() {
        eprintln!("All words already present");
        return Ok(());
    }

    eprintln!(
        "Added {} word{} to {}",
        added.len(),
        if added.len() == 1 { "" } else { "s" },
        file_path.display()
    );
    Ok(())
}

/// Find `matchum.toml` only. Does not fall back to cspell configs.
fn find_matchum_config(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p.to_path_buf());
    }
    let cwd = std::env::current_dir()?;
    match matchum_config::resolver::find_config_prioritized(&cwd) {
        matchum_config::resolver::ConfigFound::Matchum(p) => Ok(p),
        _ => anyhow::bail!("no matchum.toml found; run `matchum init` first"),
    }
}
