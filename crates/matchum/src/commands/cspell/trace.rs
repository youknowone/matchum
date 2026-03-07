use anyhow::{Context, Result};
use matchum_config::resolver;
use matchum_config::settings::CSpellSettings;
use matchum_dict::dictionary::Dictionary;
use matchum_dict::loader;
use std::collections::HashSet;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};

#[allow(dead_code)]
pub struct TraceOptions {
    pub words: Vec<String>,
    pub config: Option<PathBuf>,
    pub locale: Option<String>,
    pub language_id: Option<String>,
    pub allow_compound_words: Option<bool>,
    pub ignore_case: bool,
    pub dictionary: Vec<String>,
    pub stdin: bool,
    pub show_all: bool,
    pub only_found: bool,
    pub no_default_configuration: bool,
}

/// cspell trace: search for words in the configuration and dictionaries.
pub fn run(opts: TraceOptions) -> Result<()> {
    let mut all_words = opts.words;

    if opts.stdin || all_words.is_empty() {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let line = line?;
            for word in line.split_whitespace() {
                if !word.is_empty() {
                    all_words.push(word.to_string());
                }
            }
        }
    }

    if all_words.is_empty() {
        anyhow::bail!("No words to trace. Provide words as arguments or via --stdin.");
    }

    let (mut settings, config_dir) = if opts.no_default_configuration {
        (CSpellSettings::default(), None)
    } else {
        load_settings(opts.config.as_deref())?
    };

    // Apply --locale
    if let Some(ref locale) = opts.locale {
        settings.language = Some(locale.clone());
    }

    // Apply --allow-compound-words
    if let Some(acw) = opts.allow_compound_words {
        settings.allow_compound_words = Some(acw);
    }

    let dictionaries = build_named_dictionaries(&settings, config_dir.as_deref())?;
    let filter_set: HashSet<String> = opts.dictionary.iter().map(|d| d.to_lowercase()).collect();

    for word in &all_words {
        let lookup_word = if opts.ignore_case {
            word.to_lowercase()
        } else {
            word.clone()
        };

        println!("Word: {word}");

        for (name, dict) in &dictionaries {
            if !filter_set.is_empty() && !filter_set.contains(&name.to_lowercase()) {
                continue;
            }

            let result = dict.find(&lookup_word);
            let marker = if result.found { "*" } else { "-" };

            if opts.only_found && !result.found {
                continue;
            }
            if !opts.show_all && !result.found {
                continue;
            }

            let status = if result.forbidden {
                "forbidden"
            } else if result.no_suggest {
                "no-suggest"
            } else {
                ""
            };

            if status.is_empty() {
                println!("  {word} {marker} {name}");
            } else {
                println!("  {word} {marker} {name} ({status})");
            }
        }

        // Check inline word lists
        let matches_word = |w: &str| {
            if opts.ignore_case {
                w.eq_ignore_ascii_case(word)
            } else {
                w == word.as_str()
            }
        };

        let in_words = settings.words.iter().any(|w| matches_word(w));
        let in_ignore = settings.ignore_words.iter().any(|w| matches_word(w));
        let in_flag = settings.flag_words.iter().any(|w| matches_word(w));

        if in_words {
            println!("  {word} * [words]");
        } else if opts.show_all {
            println!("  {word} - [words]");
        }

        if in_ignore {
            println!("  {word} * [ignoreWords]");
        } else if opts.show_all {
            println!("  {word} - [ignoreWords]");
        }

        if in_flag {
            println!("  {word} ! [flagWords]");
        } else if opts.show_all {
            println!("  {word} - [flagWords]");
        }

        println!();
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
