use super::LinkCommands;
use anyhow::Result;

/// cspell link: manage dictionaries and settings in the cspell global config.
pub fn run(action: Option<LinkCommands>) -> Result<()> {
    match action {
        None | Some(LinkCommands::List) => run_list(),
        Some(LinkCommands::Add { dictionaries }) => run_add(&dictionaries),
        Some(LinkCommands::Remove { paths }) => run_remove(&paths),
    }
}

fn run_list() -> Result<()> {
    let config_path = global_config_path();
    if !config_path.exists() {
        println!("No global configuration found at {}", config_path.display());
        println!("No linked configurations.");
        return Ok(());
    }

    let content = std::fs::read_to_string(&config_path)?;
    let settings: serde_json::Value = json5::from_str(&content)
        .map_err(|e| anyhow::anyhow!("failed to parse global config: {e}"))?;

    let imports = settings
        .get("import")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if imports.is_empty() {
        println!("No linked configurations.");
    } else {
        println!("Linked configurations:");
        for import in &imports {
            if let Some(s) = import.as_str() {
                println!("  {s}");
            }
        }
    }

    Ok(())
}

fn run_add(dictionaries: &[String]) -> Result<()> {
    let config_path = global_config_path();
    let mut settings = load_or_create_global_config(&config_path)?;

    let imports = settings
        .as_object_mut()
        .unwrap()
        .entry("import")
        .or_insert_with(|| serde_json::Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("import field is not an array"))?;

    for dict in dictionaries {
        let val = serde_json::Value::String(dict.clone());
        if !imports.contains(&val) {
            imports.push(val);
            eprintln!("Added: {dict}");
        } else {
            eprintln!("Already linked: {dict}");
        }
    }

    let json = serde_json::to_string_pretty(&settings)?;
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&config_path, json + "\n")?;
    eprintln!("Updated {}", config_path.display());

    Ok(())
}

fn run_remove(paths: &[String]) -> Result<()> {
    let config_path = global_config_path();
    if !config_path.exists() {
        println!("No global configuration found.");
        return Ok(());
    }

    let mut settings = load_or_create_global_config(&config_path)?;

    if let Some(imports) = settings
        .as_object_mut()
        .and_then(|obj| obj.get_mut("import"))
        .and_then(|v| v.as_array_mut())
    {
        let before = imports.len();
        imports.retain(|v| {
            if let Some(s) = v.as_str() {
                !paths.iter().any(|p| s.contains(p.as_str()))
            } else {
                true
            }
        });
        let removed = before - imports.len();
        eprintln!("Removed {removed} linked configuration(s).");
    }

    let json = serde_json::to_string_pretty(&settings)?;
    std::fs::write(&config_path, json + "\n")?;

    Ok(())
}

fn global_config_path() -> std::path::PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        std::path::PathBuf::from(home)
            .join(".config")
            .join("cspell")
            .join("cspell.json")
    } else {
        std::path::PathBuf::from(".cspell.json")
    }
}

fn load_or_create_global_config(
    path: &std::path::Path,
) -> Result<serde_json::Value> {
    if path.exists() {
        let content = std::fs::read_to_string(path)?;
        let val: serde_json::Value = json5::from_str(&content)
            .map_err(|e| anyhow::anyhow!("failed to parse global config: {e}"))?;
        Ok(val)
    } else {
        Ok(serde_json::json!({
            "version": "0.2",
            "import": []
        }))
    }
}
