use anyhow::Result;
use std::path::PathBuf;

pub struct InitOptions {
    pub config: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub format: String,
    pub imports: Vec<String>,
    pub locale: Option<String>,
    pub dictionary: Vec<String>,
    pub no_comments: bool,
    pub stdout: bool,
}

/// cspell init: initialize a CSpell configuration file.
pub fn run(opts: InitOptions) -> Result<()> {
    let content = build_config(
        &opts.format,
        opts.locale.as_deref(),
        &opts.dictionary,
        &opts.imports,
        opts.no_comments,
    )?;

    if opts.stdout {
        println!("{content}");
        return Ok(());
    }

    let dest = opts
        .output
        .or(opts.config)
        .unwrap_or_else(|| default_filename(&opts.format));

    if dest.exists() {
        anyhow::bail!(
            "Configuration file already exists: {}. Use --output to specify a different path.",
            dest.display()
        );
    }

    std::fs::write(&dest, &content)?;
    eprintln!("Created {}", dest.display());

    Ok(())
}

fn default_filename(format: &str) -> PathBuf {
    match format {
        "json" | "jsonc" => PathBuf::from("cspell.json"),
        _ => PathBuf::from("cspell.yaml"),
    }
}

fn build_config(
    format: &str,
    locale: Option<&str>,
    dictionaries: &[String],
    imports: &[String],
    no_comments: bool,
) -> Result<String> {
    match format {
        "json" | "jsonc" => build_json(locale, dictionaries, imports, no_comments),
        "yaml" | "yml" => build_yaml(locale, dictionaries, imports, no_comments),
        _ => anyhow::bail!("unsupported format: {format}. Use yaml, yml, json, or jsonc."),
    }
}

fn build_json(
    locale: Option<&str>,
    dictionaries: &[String],
    imports: &[String],
    _no_comments: bool,
) -> Result<String> {
    let mut obj = serde_json::Map::new();
    obj.insert("version".into(), serde_json::Value::String("0.2".into()));

    if let Some(loc) = locale {
        obj.insert("language".into(), serde_json::Value::String(loc.into()));
    }

    if !imports.is_empty() {
        let arr: Vec<serde_json::Value> = imports
            .iter()
            .map(|i| serde_json::Value::String(i.clone()))
            .collect();
        obj.insert("import".into(), serde_json::Value::Array(arr));
    }

    if !dictionaries.is_empty() {
        let arr: Vec<serde_json::Value> = dictionaries
            .iter()
            .map(|d| serde_json::Value::String(d.clone()))
            .collect();
        obj.insert("dictionaries".into(), serde_json::Value::Array(arr));
    }

    obj.insert("words".into(), serde_json::Value::Array(Vec::new()));
    obj.insert(
        "ignorePaths".into(),
        serde_json::Value::Array(vec![
            serde_json::Value::String("node_modules".into()),
            serde_json::Value::String("target".into()),
        ]),
    );

    let json = serde_json::Value::Object(obj);
    Ok(serde_json::to_string_pretty(&json)? + "\n")
}

fn build_yaml(
    locale: Option<&str>,
    dictionaries: &[String],
    imports: &[String],
    no_comments: bool,
) -> Result<String> {
    let mut lines = Vec::new();

    if !no_comments {
        lines.push("# CSpell configuration".to_string());
        lines.push("# See https://cspell.org for documentation".to_string());
    }

    lines.push("version: \"0.2\"".to_string());

    if let Some(loc) = locale {
        lines.push(format!("language: {loc}"));
    }

    if !imports.is_empty() {
        lines.push("import:".into());
        for i in imports {
            lines.push(format!("  - {i}"));
        }
    }

    if !dictionaries.is_empty() {
        lines.push("dictionaries:".into());
        for d in dictionaries {
            lines.push(format!("  - {d}"));
        }
    }

    lines.push("words: []".into());
    lines.push("ignorePaths:".into());
    lines.push("  - node_modules".into());
    lines.push("  - target".into());

    Ok(lines.join("\n") + "\n")
}
