use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn run_add(words: &[String], forbidden: bool, config_path: Option<&Path>) -> Result<()> {
    if words.is_empty() {
        anyhow::bail!("No words specified");
    }

    let config_path = find_ruspell_config(config_path)?;
    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    let config: ruspell_config::ruspell_config::RuspellConfig = toml::from_str(&content)
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
                ".ruspell/flagged.txt"
            } else {
                ".ruspell/words.txt"
            };
            let full_path = config_dir.join(default_name);
            if let Some(parent) = full_path.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)?;
                }
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

    // Read existing words
    let existing_content = std::fs::read_to_string(&file_path).unwrap_or_default();
    let existing: std::collections::HashSet<String> = existing_content
        .lines()
        .map(|l| l.trim().to_lowercase())
        .collect();

    let mut added = Vec::new();
    let mut new_content = existing_content;
    for word in words {
        if !existing.contains(&word.to_lowercase()) {
            if !new_content.is_empty() && !new_content.ends_with('\n') {
                new_content.push('\n');
            }
            new_content.push_str(word);
            new_content.push('\n');
            added.push(word.as_str());
        }
    }

    if added.is_empty() {
        eprintln!("All words already present");
        return Ok(());
    }

    if let Some(parent) = file_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(&file_path, &new_content)
        .with_context(|| format!("failed to write {}", file_path.display()))?;

    eprintln!(
        "Added {} word{} to {}",
        added.len(),
        if added.len() == 1 { "" } else { "s" },
        file_path.display()
    );
    Ok(())
}

/// Find `ruspell.toml` only. Does not fall back to cspell configs.
fn find_ruspell_config(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p.to_path_buf());
    }
    let cwd = std::env::current_dir()?;
    let mut dir = cwd.as_path();
    loop {
        for name in &["ruspell.toml", ".ruspell.toml"] {
            let p = dir.join(name);
            if p.exists() {
                return Ok(p);
            }
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => anyhow::bail!("no ruspell.toml found; run `ruspell init` first"),
        }
    }
}
