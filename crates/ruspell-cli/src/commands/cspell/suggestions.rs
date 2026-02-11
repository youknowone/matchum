use anyhow::{Context, Result};
use ruspell_config::resolver;
use ruspell_config::settings::CSpellSettings;
use ruspell_dict::dictionary::Dictionary;
use ruspell_dict::loader;
use std::collections::HashSet;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

#[allow(dead_code)]
pub struct SuggestionsOptions {
    pub words: Vec<String>,
    pub config: Option<PathBuf>,
    pub locale: Option<String>,
    pub language_id: Option<String>,
    pub ignore_case: bool,
    pub num_changes: usize,
    pub num_suggestions: usize,
    pub stdin: bool,
    pub repl: bool,
    pub verbose: bool,
    pub filter_dicts: Vec<String>,
}

/// cspell suggestions: provide spelling suggestions for words.
pub fn run(opts: SuggestionsOptions) -> Result<()> {
    let (mut settings, config_dir) = load_settings(opts.config.as_deref())?;

    // Apply --locale
    if let Some(ref locale) = opts.locale {
        settings.language = Some(locale.clone());
    }

    let all_dicts = build_dictionaries(&settings, config_dir.as_deref())?;

    // Filter by --dictionary/--dictionaries if specified
    let filter_set: HashSet<String> = opts.filter_dicts.iter().map(|d| d.to_lowercase()).collect();
    let dictionaries: Vec<(String, Box<dyn Dictionary>)> = if filter_set.is_empty() {
        all_dicts
    } else {
        all_dicts
            .into_iter()
            .filter(|(name, _)| filter_set.contains(&name.to_lowercase()))
            .collect()
    };

    let dict_refs: Vec<&dyn Dictionary> = dictionaries.iter().map(|(_, d)| d.as_ref()).collect();

    if opts.repl {
        run_repl(&dict_refs, opts.num_suggestions, opts.verbose)?;
        return Ok(());
    }

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

    for word in &all_words {
        print_suggestions(word, &dict_refs, opts.num_suggestions, opts.verbose);
    }

    Ok(())
}

fn run_repl(
    dictionaries: &[&dyn Dictionary],
    num_suggestions: usize,
    verbose: bool,
) -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("> ");
        stdout.flush()?;

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            break;
        }

        let word = line.trim();
        if word.is_empty() {
            continue;
        }
        if word == "exit" || word == "quit" || word == "q" {
            break;
        }

        print_suggestions(word, dictionaries, num_suggestions, verbose);
    }

    Ok(())
}

fn print_suggestions(word: &str, dictionaries: &[&dyn Dictionary], limit: usize, verbose: bool) {
    let mut all_suggestions: Vec<String> = Vec::new();

    for dict in dictionaries {
        let suggestions = dict.suggest(word, limit);
        for s in suggestions {
            if !all_suggestions.contains(&s) {
                all_suggestions.push(s);
            }
        }
    }

    all_suggestions.truncate(limit);

    if all_suggestions.is_empty() {
        println!("{word}: (no suggestions)");
    } else if verbose {
        println!("{word}:");
        for (i, s) in all_suggestions.iter().enumerate() {
            println!("  {}: {s}", i + 1);
        }
    } else {
        println!("{word}: {}", all_suggestions.join(", "));
    }
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

fn build_dictionaries(
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
