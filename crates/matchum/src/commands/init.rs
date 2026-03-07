use anyhow::{Context, Result};
use matchum_config::convert;
use matchum_config::resolver;
use std::path::Path;

const DEFAULT_MATCHUM_TOML: &str = r#"language = "en"

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

    let path = Path::new("matchum.toml");
    if path.exists() {
        anyhow::bail!("matchum.toml already exists");
    }

    std::fs::write(path, DEFAULT_MATCHUM_TOML)?;
    eprintln!("Created matchum.toml");
    Ok(())
}

fn migrate_from_cspell(config_path: Option<&Path>) -> Result<()> {
    let output = Path::new("matchum.toml");
    if output.exists() {
        anyhow::bail!("matchum.toml already exists");
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
        let matchum_dir = Path::new(".matchum");
        if !matchum_dir.exists() {
            std::fs::create_dir_all(matchum_dir)?;
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
        "Created matchum.toml (migrated from {})",
        cspell_path.display()
    );
    Ok(())
}
