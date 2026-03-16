use anyhow::Result;
use matchum_dict::dictionary::Dictionary;
use std::collections::HashSet;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

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
    let (mut settings, config_dir) = super::load_settings(opts.config.as_deref())?;

    // Apply --locale
    if let Some(ref locale) = opts.locale {
        settings.language = Some(locale.clone());
    }

    let all_dicts = super::build_named_dictionaries(&settings, config_dir.as_deref())?;

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

fn run_repl(dictionaries: &[&dyn Dictionary], num_suggestions: usize, verbose: bool) -> Result<()> {
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
