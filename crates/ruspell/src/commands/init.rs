use anyhow::{Context, Result};
use ruspell_config::convert;
use ruspell_config::resolver;
use std::path::Path;

const DEFAULT_RUSPELL_TOML: &str = r#"language = "en"

dictionaries = ["en_us"]

ignore_paths = [
    "node_modules",
    "target",
    "dist",
    "build",
    "*.lock",
]
"#;

pub fn run_init(from_cspell: bool, config_path: Option<&Path>) -> Result<()> {
    if from_cspell {
        return migrate_from_cspell(config_path);
    }

    let path = Path::new("ruspell.toml");
    if path.exists() {
        anyhow::bail!("ruspell.toml already exists");
    }

    std::fs::write(path, DEFAULT_RUSPELL_TOML)?;
    eprintln!("Created ruspell.toml");
    Ok(())
}

fn migrate_from_cspell(config_path: Option<&Path>) -> Result<()> {
    let output = Path::new("ruspell.toml");
    if output.exists() {
        anyhow::bail!("ruspell.toml already exists");
    }

    let cspell_path = match config_path {
        Some(p) => p.to_path_buf(),
        None => {
            let cwd = std::env::current_dir()?;
            resolver::find_config(&cwd).context("no cspell config found to migrate")?
        }
    };

    let settings = resolver::load_config(&cspell_path).context("failed to load cspell config")?;
    let (config, word_files) = convert::from_cspell_settings(&settings);

    // Create word files
    if !word_files.is_empty() {
        let ruspell_dir = Path::new(".ruspell");
        if !ruspell_dir.exists() {
            std::fs::create_dir_all(ruspell_dir)?;
        }
        for (path, content) in &word_files {
            let file_path = Path::new(path);
            if let Some(parent) = file_path.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            std::fs::write(file_path, content)
                .with_context(|| format!("failed to write {}", path))?;
            eprintln!("  Created {}", path);
        }
    }

    let toml_str = toml::to_string_pretty(&config)?;
    std::fs::write(output, &toml_str)?;
    eprintln!(
        "Created ruspell.toml (migrated from {})",
        cspell_path.display()
    );
    Ok(())
}
