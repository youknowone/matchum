// spell-checker:ignore ccept
use anyhow::{Context, Result};
use matchum_config::resolver;
use matchum_core::issue::ValidationIssue;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use crate::commands::check::{self, CheckOptions};

struct WritableDict {
    name: String,
    path: PathBuf,
}

struct Occurrence {
    file: PathBuf,
    line: usize,
    column: usize,
    context_line: String,
}

struct WordGroup {
    word: String,
    occurrences: Vec<Occurrence>,
    suggestions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DispatchResult {
    Accepted,
    Ignored,
    Fixed,
    Skipped,
    Quit,
}

pub fn run_review(
    paths: &[PathBuf],
    config_path: Option<&Path>,
    options: CheckOptions,
) -> Result<()> {
    // 1. Load matchum config for writable dicts
    let (matchum_config, matchum_config_path, config_dir) = load_matchum_config(config_path)?;
    let mut writable_dicts = collect_writable_dicts(&matchum_config, &config_dir);

    // 2. Collect all issues
    eprintln!("Checking files...");
    let results = check::collect_all_issues(paths, config_path, options)?;

    if results.is_empty() {
        eprintln!("No spelling issues found.");
        return Ok(());
    }

    // 3. Group by word (case-insensitive)
    let groups = group_by_word(&results);

    let total_issues: usize = groups.iter().map(|g| g.occurrences.len()).sum();
    eprintln!(
        "Found {} issues ({} unique words)\n",
        total_issues,
        groups.len()
    );

    // 4. Load _typos.toml (typos-cli compatible)
    let typos_toml = config_dir.join("_typos.toml");
    let mut fix_memory: HashMap<String, String> = load_typos_toml(&typos_toml);

    // 5. Interactive loop
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut accepted = 0usize;
    let mut ignored = 0usize;
    let mut fixed = 0usize;
    let mut skipped = 0usize;
    let mut disabled_files: HashSet<PathBuf> = HashSet::new();

    let base_dir = std::env::current_dir().ok();

    for (idx, group) in groups.iter().enumerate() {
        // Filter out occurrences in already-disabled files
        let live_occurrences: Vec<&Occurrence> = group
            .occurrences
            .iter()
            .filter(|occ| !disabled_files.contains(&occ.file))
            .collect();

        if live_occurrences.is_empty() {
            continue;
        }

        println!(
            "[{}/{}] \"{}\" ({} occurrence{})",
            idx + 1,
            groups.len(),
            group.word,
            live_occurrences.len(),
            if live_occurrences.len() == 1 {
                ""
            } else {
                "s"
            },
        );

        // Show occurrences with context
        for occ in &live_occurrences {
            let display_path = match &base_dir {
                Some(base) => occ
                    .file
                    .strip_prefix(base)
                    .unwrap_or(&occ.file)
                    .display()
                    .to_string(),
                None => occ.file.display().to_string(),
            };
            println!("  {}:{}:{}", display_path, occ.line, occ.column);
            if !occ.context_line.is_empty() {
                println!("    {}", occ.context_line.trim_end());
            }
        }

        // Show known fix (from _typos.toml or earlier in session)
        if let Some(prev) = fix_memory.get(&group.word.to_lowercase()) {
            println!();
            println!(
                "  Known fix: \"{}\" -> \"{}\" (press f to apply)",
                group.word, prev
            );
        }

        // Show suggestions
        if !group.suggestions.is_empty() {
            println!();
            print!("  Suggestions:");
            for (i, s) in group.suggestions.iter().enumerate() {
                print!(" [{}]{}", i + 1, s);
            }
            println!();
        }

        // Show writable dicts
        println!();
        println!("  Writable dictionaries:");
        println!("    [0] create new dictionary");
        for (i, d) in writable_dicts.iter().enumerate() {
            let rel = match &base_dir {
                Some(base) => d
                    .path
                    .strip_prefix(base)
                    .unwrap_or(&d.path)
                    .display()
                    .to_string(),
                None => d.path.display().to_string(),
            };
            println!("    [{}] {:<12} ({})", i + 1, d.name, rel);
        }

        // Prompt
        println!();
        print!("  [a]ccept N | [i]gnore | [d]isable [l]ine/[f]ile | [f]ix | [e]dit | [s]kip | [q]uit > ");
        io::stdout().flush()?;

        let mut input = String::new();
        reader.read_line(&mut input)?;
        let input = input.trim();

        let result = dispatch_command(
            &mut reader,
            input,
            group,
            &mut writable_dicts,
            &config_dir,
            matchum_config_path.as_deref(),
            &typos_toml,
            &mut fix_memory,
            &mut disabled_files,
        )?;

        match result {
            DispatchResult::Accepted => accepted += 1,
            DispatchResult::Ignored => ignored += 1,
            DispatchResult::Fixed => fixed += 1,
            DispatchResult::Skipped => skipped += 1,
            DispatchResult::Quit => {
                eprintln!("Review aborted.");
                break;
            }
        }
        println!();
    }

    // Summary
    eprintln!("--- Review complete ---");
    if accepted > 0 {
        eprintln!("  Accepted: {}", accepted);
    }
    if ignored > 0 {
        eprintln!("  Ignored:  {}", ignored);
    }
    if fixed > 0 {
        eprintln!("  Fixed:    {}", fixed);
    }
    if skipped > 0 {
        eprintln!("  Skipped:  {}", skipped);
    }

    Ok(())
}

fn dispatch_command(
    reader: &mut impl BufRead,
    input: &str,
    group: &WordGroup,
    writable_dicts: &mut Vec<WritableDict>,
    config_dir: &Path,
    matchum_config_path: Option<&Path>,
    typos_toml: &Path,
    fix_memory: &mut HashMap<String, String>,
    disabled_files: &mut HashSet<PathBuf>,
) -> Result<DispatchResult> {
    if input.is_empty() || input == "s" || input == "skip" {
        return Ok(DispatchResult::Skipped);
    }
    if input == "q" || input == "quit" {
        return Ok(DispatchResult::Quit);
    }

    let parts: Vec<&str> = input.splitn(2, ' ').collect();
    let cmd = parts[0];
    let arg = parts.get(1).map(|s| s.trim());

    match cmd {
        "a" | "accept" => {
            if arg == Some("0") {
                let new_dict = create_new_dict(reader, config_dir, matchum_config_path)?;
                eprintln!(
                    "  Created dictionary \"{}\" at {}",
                    new_dict.name,
                    new_dict.path.display()
                );
                append_word_to_file(&new_dict.path, &group.word)?;
                eprintln!("  Added \"{}\" to {}", group.word, new_dict.path.display());
                writable_dicts.push(new_dict);
                Ok(DispatchResult::Accepted)
            } else if writable_dicts.is_empty() {
                eprintln!("  No dictionaries yet. Use `a 0` to create one.");
                Ok(DispatchResult::Skipped)
            } else {
                let dict = resolve_dict(writable_dicts, arg)?;
                append_word_to_file(&dict.path, &group.word)?;
                eprintln!("  Added \"{}\" to {}", group.word, dict.path.display());
                Ok(DispatchResult::Accepted)
            }
        }
        "i" | "ignore" => {
            let count = ignore_in_files(&group.occurrences, &group.word)?;
            eprintln!(
                "  Added spell-checker:ignore \"{}\" to {} file{}",
                group.word,
                count,
                if count == 1 { "" } else { "s" },
            );
            Ok(DispatchResult::Ignored)
        }
        "f" | "fix" => {
            let replacement = if let Some(inline) = arg {
                if let Ok(n) = inline.parse::<usize>() {
                    if n >= 1 && n <= group.suggestions.len() {
                        Some(group.suggestions[n - 1].clone())
                    } else {
                        eprintln!("  Invalid suggestion number. Skipping.");
                        None
                    }
                } else {
                    Some(inline.to_string())
                }
            } else if let Some(known) = fix_memory.get(&group.word.to_lowercase()) {
                Some(known.clone())
            } else if !group.suggestions.is_empty() {
                println!("  Pick suggestion number (1-{}):", group.suggestions.len());
                print!("  > ");
                io::stdout().flush()?;
                let mut fix_input = String::new();
                reader.read_line(&mut fix_input)?;
                let fix_input = fix_input.trim();
                if let Ok(n) = fix_input.parse::<usize>() {
                    if n >= 1 && n <= group.suggestions.len() {
                        Some(group.suggestions[n - 1].clone())
                    } else {
                        eprintln!("  Invalid number. Skipping.");
                        None
                    }
                } else {
                    eprintln!("  Invalid input. Skipping.");
                    None
                }
            } else {
                eprintln!(
                    "  No suggestions available. Use `f <word>` or [e]dit to enter replacement."
                );
                None
            };

            if let Some(replacement) = replacement {
                let count = replace_word_in_files(&group.occurrences, &group.word, &replacement)?;
                eprintln!(
                    "  Replaced \"{}\" -> \"{}\" in {} location{}",
                    group.word,
                    replacement,
                    count,
                    if count == 1 { "" } else { "s" },
                );
                append_typo(typos_toml, &group.word, &replacement)?;
                fix_memory.insert(group.word.to_lowercase(), replacement);
                Ok(DispatchResult::Fixed)
            } else {
                Ok(DispatchResult::Skipped)
            }
        }
        "e" | "edit" => {
            let replacement = if let Some(inline) = arg {
                inline.to_string()
            } else {
                print!("  Enter replacement: ");
                io::stdout().flush()?;
                let mut edit_input = String::new();
                reader.read_line(&mut edit_input)?;
                edit_input.trim().to_string()
            };
            if replacement.is_empty() {
                eprintln!("  Empty input. Skipping.");
                Ok(DispatchResult::Skipped)
            } else {
                let count = replace_word_in_files(&group.occurrences, &group.word, &replacement)?;
                eprintln!(
                    "  Replaced \"{}\" -> \"{}\" in {} location{}",
                    group.word,
                    replacement,
                    count,
                    if count == 1 { "" } else { "s" },
                );
                append_typo(typos_toml, &group.word, &replacement)?;
                fix_memory.insert(group.word.to_lowercase(), replacement);
                Ok(DispatchResult::Fixed)
            }
        }
        "dl" => {
            let count = disable_in_lines(&group.occurrences)?;
            eprintln!(
                "  Disabled spell-checking for {} line{} (all words on those lines)",
                count,
                if count == 1 { "" } else { "s" },
            );
            Ok(DispatchResult::Ignored)
        }
        "df" => {
            let count = disable_in_files(&group.occurrences)?;
            for occ in &group.occurrences {
                disabled_files.insert(occ.file.clone());
            }
            eprintln!(
                "  Disabled spell-checking for {} file{} (entire file)",
                count,
                if count == 1 { "" } else { "s" },
            );
            Ok(DispatchResult::Ignored)
        }
        _ => {
            eprintln!("  Unknown command. Skipping.");
            Ok(DispatchResult::Skipped)
        }
    }
}

/// Returns (config, config_path_if_exists, config_dir).
fn load_matchum_config(
    config_path: Option<&Path>,
) -> Result<(
    matchum_config::matchum_config::MatchumConfig,
    Option<PathBuf>,
    PathBuf,
)> {
    let cwd = std::env::current_dir()?;

    let path = if let Some(p) = config_path {
        p.to_path_buf()
    } else {
        match resolver::find_config_prioritized(&cwd) {
            resolver::ConfigFound::Matchum(p) => p,
            _ => {
                return Ok((
                    matchum_config::matchum_config::MatchumConfig::default(),
                    None,
                    cwd,
                ));
            }
        }
    };

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext != "toml" {
        // cspell JSON — no MatchumConfig to extract, return defaults
        let config_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        return Ok((
            matchum_config::matchum_config::MatchumConfig::default(),
            None,
            config_dir,
        ));
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let config: matchum_config::matchum_config::MatchumConfig =
        toml::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))?;
    let config_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    Ok((config, Some(path), config_dir))
}

fn collect_writable_dicts(
    config: &matchum_config::matchum_config::MatchumConfig,
    config_dir: &Path,
) -> Vec<WritableDict> {
    let mut dicts = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Auto-discover .matchum/dict/*.txt
    let dict_dir = config_dir.join(".matchum/dict");
    if let Ok(entries) = std::fs::read_dir(&dict_dir) {
        let mut paths: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        paths.sort_by_key(|e| e.file_name());
        for entry in paths {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("txt") && seen.insert(path.clone())
            {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("words")
                    .to_string();
                dicts.push(WritableDict { name, path });
            }
        }
    }

    // words_file — only if configured AND file exists
    if let Some(ref words_file) = config.words_file {
        let path = config_dir.join(words_file);
        if path.exists() && seen.insert(path.clone()) {
            let name = Path::new(words_file)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("words")
                .to_string();
            dicts.push(WritableDict { name, path });
        }
    }

    // Custom dictionaries with add_words=true — only if file exists
    for def in &config.dictionary_definitions {
        if def.add_words {
            if let Some(ref p) = def.path {
                let path = config_dir.join(p);
                if path.exists() && seen.insert(path.clone()) {
                    dicts.push(WritableDict {
                        name: def.name.clone(),
                        path,
                    });
                }
            }
        }
    }

    dicts
}

fn group_by_word(results: &[(PathBuf, String, Vec<ValidationIssue>)]) -> Vec<WordGroup> {
    let mut map: BTreeMap<String, WordGroup> = BTreeMap::new();

    for (file, content, issues) in results {
        for issue in issues {
            let key = issue.word.to_lowercase();
            let context_line = content
                .lines()
                .nth(issue.line.saturating_sub(1))
                .unwrap_or("")
                .to_string();

            let group = map.entry(key).or_insert_with(|| WordGroup {
                word: issue.word.clone(),
                occurrences: Vec::new(),
                suggestions: issue.suggestions.clone(),
            });

            group.occurrences.push(Occurrence {
                file: file.clone(),
                line: issue.line,
                column: issue.column,
                context_line,
            });

            // Merge suggestions
            if group.suggestions.is_empty() && !issue.suggestions.is_empty() {
                group.suggestions = issue.suggestions.clone();
            }
        }
    }

    map.into_values().collect()
}

fn resolve_dict<'a>(dicts: &'a [WritableDict], arg: Option<&str>) -> Result<&'a WritableDict> {
    if dicts.is_empty() {
        anyhow::bail!("no writable dictionaries available");
    }

    let arg = match arg {
        Some(a) => a,
        None => {
            if dicts.len() == 1 {
                return Ok(&dicts[0]);
            }
            return Ok(&dicts[0]);
        }
    };

    // Try as number first
    if let Ok(n) = arg.parse::<usize>() {
        if n >= 1 && n <= dicts.len() {
            return Ok(&dicts[n - 1]);
        }
        anyhow::bail!("dictionary number {} out of range (1-{})", n, dicts.len());
    }

    // Try as name
    for d in dicts {
        if d.name.eq_ignore_ascii_case(arg) {
            return Ok(d);
        }
    }

    anyhow::bail!(
        "unknown dictionary '{}'. Available: {}",
        arg,
        dicts
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

fn append_word_to_file(path: &Path, word: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }

    // Check if word already exists
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let already: HashSet<String> = existing.lines().map(|l| l.trim().to_lowercase()).collect();
    if already.contains(&word.to_lowercase()) {
        return Ok(());
    }

    // Append
    use std::fs::OpenOptions;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;

    // Ensure newline before appending if file doesn't end with one
    if !existing.is_empty() && !existing.ends_with('\n') {
        writeln!(file)?;
    }
    writeln!(file, "{}", word)?;
    Ok(())
}

/// Interactively create a new word-list file and register it in matchum.toml.
fn create_new_dict(
    reader: &mut impl BufRead,
    config_dir: &Path,
    matchum_config_path: Option<&Path>,
) -> Result<WritableDict> {
    print!("  Dictionary name (e.g. matchum): ");
    io::stdout().flush()?;
    let mut name_input = String::new();
    reader.read_line(&mut name_input)?;
    let name_input = name_input.trim();
    if name_input.is_empty() {
        anyhow::bail!("empty name");
    }

    // Bare name → .matchum/dict/<name>.txt, otherwise use as-is
    let rel_path = if name_input.contains('/') || name_input.contains('.') {
        name_input.to_string()
    } else {
        format!(".matchum/dict/{}.txt", name_input)
    };

    let full_path = config_dir.join(&rel_path);
    if let Some(parent) = full_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    // Create the file if it doesn't exist
    if !full_path.exists() {
        std::fs::write(&full_path, "")?;
    }

    let name = Path::new(&rel_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("words")
        .to_string();

    // Update matchum.toml with words_file if it exists
    if let Some(config_path) = matchum_config_path {
        let content = std::fs::read_to_string(config_path).unwrap_or_default();
        if !content.contains("words_file") {
            let new_line = format!("words_file = \"{}\"\n", rel_path);
            let updated = format!("{new_line}{content}");
            std::fs::write(config_path, updated)?;
        }
    }

    Ok(WritableDict {
        name,
        path: full_path,
    })
}

fn replace_word_in_files(
    occurrences: &[Occurrence],
    old_word: &str,
    new_word: &str,
) -> Result<usize> {
    // Group occurrences by file
    let mut by_file: BTreeMap<&Path, Vec<&Occurrence>> = BTreeMap::new();
    for occ in occurrences {
        by_file.entry(occ.file.as_path()).or_default().push(occ);
    }

    let mut total_replaced = 0;

    for (file_path, mut occs) in by_file {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("failed to read {}", file_path.display()))?;

        // Sort by line DESC, then column DESC so replacements don't shift
        // earlier positions. Using line/column instead of byte offset because
        // previous fixes in the same session may have shifted byte offsets.
        occs.sort_by(|a, b| b.line.cmp(&a.line).then(b.column.cmp(&a.column)));

        // Precompute line start byte offsets from the current file content.
        let line_starts = compute_line_starts(content.as_bytes());

        let mut bytes = content.into_bytes();
        for occ in &occs {
            let line_idx = occ.line.saturating_sub(1);
            let col_offset = occ.column.saturating_sub(1);
            if line_idx >= line_starts.len() {
                continue;
            }
            let start = line_starts[line_idx] + col_offset;
            let end = start + old_word.len();
            if end <= bytes.len() {
                // Verify the word at this position matches
                let slice = &bytes[start..end];
                if slice.eq_ignore_ascii_case(old_word.as_bytes()) {
                    bytes.splice(start..end, new_word.bytes());
                    total_replaced += 1;
                }
            }
        }

        let new_content = String::from_utf8(bytes).with_context(|| {
            format!("invalid UTF-8 after replacement in {}", file_path.display())
        })?;
        std::fs::write(file_path, &new_content)
            .with_context(|| format!("failed to write {}", file_path.display()))?;
    }

    Ok(total_replaced)
}

/// Compute byte offset of the start of each line (0-indexed line → byte offset).
fn compute_line_starts(bytes: &[u8]) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

fn comment_prefix(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("py" | "rb" | "sh" | "bash" | "zsh" | "yaml" | "yml" | "toml" | "pl" | "pm") => "#",
        _ => "//",
    }
}

/// Insert `spell-checker:disable-next-line` above each occurrence's line.
fn disable_in_lines(occurrences: &[Occurrence]) -> Result<usize> {
    let mut by_file: BTreeMap<&Path, Vec<&Occurrence>> = BTreeMap::new();
    for occ in occurrences {
        by_file.entry(occ.file.as_path()).or_default().push(occ);
    }

    let mut total = 0;

    for (file_path, mut occs) in by_file {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("failed to read {}", file_path.display()))?;
        let prefix = comment_prefix(file_path);

        // Sort by line descending so insertions don't shift earlier indices.
        // Deduplicate by line number.
        occs.sort_by(|a, b| b.line.cmp(&a.line));
        occs.dedup_by(|a, b| a.line == b.line);

        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();

        for occ in &occs {
            let line_idx = occ.line.saturating_sub(1);
            if line_idx < lines.len() {
                let indent: String = lines[line_idx]
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .collect();
                let directive = format!("{indent}{prefix} spell-checker:disable-next-line");
                lines.insert(line_idx, directive);
                total += 1;
            }
        }

        let mut new_content = lines.join("\n");
        if content.ends_with('\n') {
            new_content.push('\n');
        }
        std::fs::write(file_path, &new_content)
            .with_context(|| format!("failed to write {}", file_path.display()))?;
    }

    Ok(total)
}

/// Insert `spell-checker:disable` at the top of each file.
fn disable_in_files(occurrences: &[Occurrence]) -> Result<usize> {
    let mut files: Vec<&Path> = occurrences.iter().map(|o| o.file.as_path()).collect();
    files.sort();
    files.dedup();

    let mut total = 0;

    for file_path in files {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("failed to read {}", file_path.display()))?;
        let prefix = comment_prefix(file_path);
        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();

        // Check if already disabled at the top
        let already = lines.iter().take(5).any(|l| {
            matches!(
                matchum_config::directives::parse_directive(l),
                Some(matchum_config::directives::Directive::Disable)
            )
        });

        if !already {
            let directive = format!("{prefix} spell-checker:disable");
            lines.insert(0, directive);

            let mut new_content = lines.join("\n");
            if content.ends_with('\n') {
                new_content.push('\n');
            }
            std::fs::write(file_path, &new_content)
                .with_context(|| format!("failed to write {}", file_path.display()))?;
        }
        total += 1;
    }

    Ok(total)
}

/// Add `spell-checker:ignore <word>` at the top of each file containing the word.
/// If an existing ignore directive is found near the top, appends to it.
fn ignore_in_files(occurrences: &[Occurrence], word: &str) -> Result<usize> {
    let mut files: Vec<&Path> = occurrences.iter().map(|o| o.file.as_path()).collect();
    files.sort();
    files.dedup();

    let mut total = 0;

    for file_path in files {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("failed to read {}", file_path.display()))?;
        let prefix = comment_prefix(file_path);
        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();

        // Search first 20 lines for an existing ignore directive
        let mut found = false;
        for line in lines.iter_mut().take(20) {
            if let Some(matchum_config::directives::Directive::Ignore(ref words)) =
                matchum_config::directives::parse_directive(line)
            {
                if !words.iter().any(|w| w.eq_ignore_ascii_case(word)) {
                    line.push(' ');
                    line.push_str(word);
                }
                found = true;
                break;
            }
        }

        if !found {
            let directive = format!("{prefix} spell-checker:ignore {word}");
            lines.insert(0, directive);
        }

        let mut new_content = lines.join("\n");
        if content.ends_with('\n') {
            new_content.push('\n');
        }
        std::fs::write(file_path, &new_content)
            .with_context(|| format!("failed to write {}", file_path.display()))?;
        total += 1;
    }

    Ok(total)
}

/// Append a typo correction to `_typos.toml` under `[default.extend-words]`.
fn append_typo(path: &Path, wrong: &str, right: &str) -> Result<()> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let mut doc: toml::Table = if existing.is_empty() {
        toml::Table::new()
    } else {
        existing.parse().unwrap_or_else(|_| toml::Table::new())
    };

    // Navigate to default.extend-words, creating sections as needed
    let default = doc
        .entry("default")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
        .as_table_mut()
        .context("default is not a table")?;
    let extend_words = default
        .entry("extend-words")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
        .as_table_mut()
        .context("extend-words is not a table")?;

    let key = wrong.to_lowercase();
    if extend_words.contains_key(&key) {
        return Ok(()); // already recorded
    }
    extend_words.insert(key, toml::Value::String(right.to_string()));

    std::fs::write(path, toml::to_string_pretty(&doc)?)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Load `[default.extend-words]` from `_typos.toml` into a HashMap.
fn load_typos_toml(path: &Path) -> HashMap<String, String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let doc: toml::Table = match content.parse() {
        Ok(t) => t,
        Err(_) => return HashMap::new(),
    };
    let mut map = HashMap::new();
    if let Some(default) = doc.get("default").and_then(|v| v.as_table()) {
        if let Some(words) = default.get("extend-words").and_then(|v| v.as_table()) {
            for (k, v) in words {
                if let Some(s) = v.as_str() {
                    map.insert(k.to_lowercase(), s.to_string());
                }
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn make_occurrence(
        dir: &Path,
        filename: &str,
        line: usize,
        col: usize,
        context: &str,
    ) -> Occurrence {
        Occurrence {
            file: dir.join(filename),
            line,
            column: col,
            context_line: context.to_string(),
        }
    }

    fn make_group(word: &str, occurrences: Vec<Occurrence>) -> WordGroup {
        WordGroup {
            word: word.to_string(),
            occurrences,
            suggestions: Vec::new(),
        }
    }

    fn make_group_with_suggestions(
        word: &str,
        occs: Vec<Occurrence>,
        sugs: Vec<&str>,
    ) -> WordGroup {
        WordGroup {
            word: word.to_string(),
            occurrences: occs,
            suggestions: sugs.iter().map(|s| s.to_string()).collect(),
        }
    }

    // ── comment_prefix ──

    #[test]
    fn comment_prefix_python() {
        assert_eq!(comment_prefix(Path::new("foo.py")), "#");
    }

    #[test]
    fn comment_prefix_rust() {
        assert_eq!(comment_prefix(Path::new("foo.rs")), "//");
    }

    #[test]
    fn comment_prefix_toml() {
        assert_eq!(comment_prefix(Path::new("foo.toml")), "#");
    }

    #[test]
    fn comment_prefix_yaml() {
        assert_eq!(comment_prefix(Path::new("foo.yml")), "#");
    }

    #[test]
    fn comment_prefix_no_extension() {
        assert_eq!(comment_prefix(Path::new("Makefile")), "//");
    }

    // ── group_by_word ──

    #[test]
    fn group_by_word_empty() {
        let groups = group_by_word(&[]);
        assert!(groups.is_empty());
    }

    #[test]
    fn group_by_word_single() {
        use matchum_core::issue::ValidationIssue;
        let file = PathBuf::from("/tmp/test.rs");
        let content = "let teh = 1;".to_string();
        let issues = vec![ValidationIssue {
            word: "teh".to_string(),
            line: 1,
            column: 5,
            offset: 4,
            is_forbidden: false,
            is_known_typo: false,
            suggestions: vec!["the".to_string()],
        }];
        let groups = group_by_word(&[(file, content, issues)]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].word, "teh");
        assert_eq!(groups[0].occurrences.len(), 1);
        assert_eq!(groups[0].suggestions, vec!["the"]);
    }

    #[test]
    fn group_by_word_case_insensitive() {
        use matchum_core::issue::ValidationIssue;
        let file = PathBuf::from("/tmp/test.rs");
        let content = "Teh teh".to_string();
        let issues = vec![
            ValidationIssue {
                word: "Teh".to_string(),
                line: 1,
                column: 1,
                offset: 0,
                is_forbidden: false,
                is_known_typo: false,
                suggestions: vec!["The".to_string()],
            },
            ValidationIssue {
                word: "teh".to_string(),
                line: 1,
                column: 5,
                offset: 4,
                is_forbidden: false,
                is_known_typo: false,
                suggestions: vec![],
            },
        ];
        let groups = group_by_word(&[(file, content, issues)]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].occurrences.len(), 2);
        assert_eq!(groups[0].word, "Teh");
    }

    #[test]
    fn group_by_word_sorted_by_key() {
        use matchum_core::issue::ValidationIssue;
        let file = PathBuf::from("/tmp/test.rs");
        let content = "zzz aaa".to_string();
        let issues = vec![
            ValidationIssue {
                word: "zzz".to_string(),
                line: 1,
                column: 1,
                offset: 0,
                is_forbidden: false,
                is_known_typo: false,
                suggestions: vec![],
            },
            ValidationIssue {
                word: "aaa".to_string(),
                line: 1,
                column: 5,
                offset: 4,
                is_forbidden: false,
                is_known_typo: false,
                suggestions: vec![],
            },
        ];
        let groups = group_by_word(&[(file, content, issues)]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].word, "aaa");
        assert_eq!(groups[1].word, "zzz");
    }

    #[test]
    fn group_by_word_context_line() {
        use matchum_core::issue::ValidationIssue;
        let file = PathBuf::from("/tmp/test.rs");
        let content = "line one\nlet teh = 1;\nline three".to_string();
        let issues = vec![ValidationIssue {
            word: "teh".to_string(),
            line: 2,
            column: 5,
            offset: 13,
            is_forbidden: false,
            is_known_typo: false,
            suggestions: vec![],
        }];
        let groups = group_by_word(&[(file, content, issues)]);
        assert_eq!(groups[0].occurrences[0].context_line, "let teh = 1;");
    }

    // ── resolve_dict ──

    #[test]
    fn resolve_dict_no_arg_returns_first() {
        let dicts = vec![
            WritableDict {
                name: "alpha".into(),
                path: PathBuf::from("/a.txt"),
            },
            WritableDict {
                name: "beta".into(),
                path: PathBuf::from("/b.txt"),
            },
        ];
        let d = resolve_dict(&dicts, None).unwrap();
        assert_eq!(d.name, "alpha");
    }

    #[test]
    fn resolve_dict_by_number() {
        let dicts = vec![
            WritableDict {
                name: "alpha".into(),
                path: PathBuf::from("/a.txt"),
            },
            WritableDict {
                name: "beta".into(),
                path: PathBuf::from("/b.txt"),
            },
        ];
        let d = resolve_dict(&dicts, Some("2")).unwrap();
        assert_eq!(d.name, "beta");
    }

    #[test]
    fn resolve_dict_by_name() {
        let dicts = vec![
            WritableDict {
                name: "alpha".into(),
                path: PathBuf::from("/a.txt"),
            },
            WritableDict {
                name: "beta".into(),
                path: PathBuf::from("/b.txt"),
            },
        ];
        let d = resolve_dict(&dicts, Some("beta")).unwrap();
        assert_eq!(d.name, "beta");
    }

    #[test]
    fn resolve_dict_by_name_case_insensitive() {
        let dicts = vec![WritableDict {
            name: "Alpha".into(),
            path: PathBuf::from("/a.txt"),
        }];
        let d = resolve_dict(&dicts, Some("alpha")).unwrap();
        assert_eq!(d.name, "Alpha");
    }

    #[test]
    fn resolve_dict_out_of_range() {
        let dicts = vec![WritableDict {
            name: "alpha".into(),
            path: PathBuf::from("/a.txt"),
        }];
        assert!(resolve_dict(&dicts, Some("5")).is_err());
    }

    #[test]
    fn resolve_dict_empty_list() {
        let dicts: Vec<WritableDict> = vec![];
        assert!(resolve_dict(&dicts, None).is_err());
    }

    // ── append_word_to_file ──

    #[test]
    fn append_word_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("words.txt");
        append_word_to_file(&path, "hello").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "hello\n");
    }

    #[test]
    fn append_word_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("words.txt");
        std::fs::write(&path, "existing\n").unwrap();
        append_word_to_file(&path, "hello").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "existing\nhello\n");
    }

    #[test]
    fn append_word_no_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("words.txt");
        std::fs::write(&path, "existing").unwrap();
        append_word_to_file(&path, "hello").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "existing\nhello\n");
    }

    #[test]
    fn append_word_duplicate_prevention() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("words.txt");
        std::fs::write(&path, "hello\n").unwrap();
        append_word_to_file(&path, "hello").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "hello\n");
    }

    #[test]
    fn append_word_case_insensitive_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("words.txt");
        std::fs::write(&path, "Hello\n").unwrap();
        append_word_to_file(&path, "hello").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "Hello\n");
    }

    #[test]
    fn append_word_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/dir/words.txt");
        append_word_to_file(&path, "hello").unwrap();
        assert!(path.exists());
    }

    // ── replace_word_in_files ──

    #[test]
    fn replace_word_single() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "let teh = 1;").unwrap();
        let occs = vec![make_occurrence(dir.path(), "test.rs", 1, 5, "let teh = 1;")];
        let count = replace_word_in_files(&occs, "teh", "the").unwrap();
        assert_eq!(count, 1);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "let the = 1;");
    }

    #[test]
    fn replace_word_multiple_same_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "teh teh").unwrap();
        let occs = vec![
            make_occurrence(dir.path(), "test.rs", 1, 1, "teh teh"),
            make_occurrence(dir.path(), "test.rs", 1, 5, "teh teh"),
        ];
        let count = replace_word_in_files(&occs, "teh", "the").unwrap();
        assert_eq!(count, 2);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "the the");
    }

    #[test]
    fn replace_word_different_files() {
        let dir = tempfile::tempdir().unwrap();
        let file1 = dir.path().join("a.rs");
        let file2 = dir.path().join("b.rs");
        std::fs::write(&file1, "teh").unwrap();
        std::fs::write(&file2, "teh").unwrap();
        let occs = vec![
            make_occurrence(dir.path(), "a.rs", 1, 1, "teh"),
            make_occurrence(dir.path(), "b.rs", 1, 1, "teh"),
        ];
        let count = replace_word_in_files(&occs, "teh", "the").unwrap();
        assert_eq!(count, 2);
        assert_eq!(std::fs::read_to_string(&file1).unwrap(), "the");
        assert_eq!(std::fs::read_to_string(&file2).unwrap(), "the");
    }

    #[test]
    fn replace_word_different_length() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "abcde fgh").unwrap();
        let occs = vec![make_occurrence(dir.path(), "test.rs", 1, 1, "abcde fgh")];
        let count = replace_word_in_files(&occs, "abcde", "xy").unwrap();
        assert_eq!(count, 1);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "xy fgh");
    }

    #[test]
    fn replace_word_multiline() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "let teh = 1;\nlet teh = 2;\n").unwrap();
        let occs = vec![
            make_occurrence(dir.path(), "test.rs", 1, 5, "let teh = 1;"),
            make_occurrence(dir.path(), "test.rs", 2, 5, "let teh = 2;"),
        ];
        let count = replace_word_in_files(&occs, "teh", "the").unwrap();
        assert_eq!(count, 2);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "let the = 1;\nlet the = 2;\n"
        );
    }

    /// After a previous fix changes byte lengths, cached offsets become stale.
    /// line/column based replacement still works.
    #[test]
    fn replace_word_after_length_change() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        // Original: "ab allcaps cd\nef allcaps gh\n"
        // First fix group changes "ab" → "abc" (adds 1 byte), simulating a prior fix.
        // We write the post-fix content but use original line/col for allcaps.
        std::fs::write(&file, "abc allcaps cd\nef allcaps gh\n").unwrap();
        // line/col of "allcaps" in the NEW content:
        // line 1, col 5 (byte 4) — "allcaps" starts at "abc " (4 bytes)
        // line 2, col 4 (byte 3) — "allcaps" starts at "ef " (3 bytes)
        let occs = vec![
            make_occurrence(dir.path(), "test.rs", 1, 5, "abc allcaps cd"),
            make_occurrence(dir.path(), "test.rs", 2, 4, "ef allcaps gh"),
        ];
        let count = replace_word_in_files(&occs, "allcaps", "all_caps").unwrap();
        assert_eq!(count, 2);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "abc all_caps cd\nef all_caps gh\n"
        );
    }

    // ── append_typo with corrupt file ──

    #[test]
    fn append_typo_corrupt_file_recovers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("_typos.toml");
        // Write a corrupt TOML file with duplicate keys
        std::fs::write(
            &path,
            "[default.extend-words]\nfoo = \"bar\"\nfoo = \"baz\"\n",
        )
        .unwrap();
        // Should not crash — recovers by starting fresh
        append_typo(&path, "teh", "the").unwrap();
        let map = load_typos_toml(&path);
        assert_eq!(map.get("teh").map(|s| s.as_str()), Some("the"));
    }

    // ── disable_in_lines ──

    #[test]
    fn disable_in_lines_inserts_directive() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "let teh = 1;\n").unwrap();
        let occs = vec![make_occurrence(dir.path(), "test.rs", 1, 5, "let teh = 1;")];
        let count = disable_in_lines(&occs).unwrap();
        assert_eq!(count, 1);
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.starts_with("// spell-checker:disable-next-line\n"));
    }

    #[test]
    fn disable_in_lines_preserves_indent() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "    let teh = 1;\n").unwrap();
        let occs = vec![make_occurrence(
            dir.path(),
            "test.rs",
            1,
            9,
            "    let teh = 1;",
        )];
        let count = disable_in_lines(&occs).unwrap();
        assert_eq!(count, 1);
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.starts_with("    // spell-checker:disable-next-line\n"));
    }

    #[test]
    fn disable_in_lines_python_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.py");
        std::fs::write(&file, "teh = 1\n").unwrap();
        let occs = vec![make_occurrence(dir.path(), "test.py", 1, 1, "teh = 1")];
        disable_in_lines(&occs).unwrap();
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.starts_with("# spell-checker:disable-next-line\n"));
    }

    #[test]
    fn disable_in_lines_dedup_same_line() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "teh teh\n").unwrap();
        let occs = vec![
            make_occurrence(dir.path(), "test.rs", 1, 1, "teh teh"),
            make_occurrence(dir.path(), "test.rs", 1, 5, "teh teh"),
        ];
        let count = disable_in_lines(&occs).unwrap();
        assert_eq!(count, 1);
    }

    // ── disable_in_files ──

    #[test]
    fn disable_in_files_top_insertion() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "let teh = 1;\n").unwrap();
        let occs = vec![make_occurrence(dir.path(), "test.rs", 1, 5, "let teh = 1;")];
        let count = disable_in_files(&occs).unwrap();
        assert_eq!(count, 1);
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.starts_with("// spell-checker:disable\n"));
    }

    #[test]
    fn disable_in_files_already_disabled_skips() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "// spell-checker:disable\nlet teh = 1;\n").unwrap();
        let occs = vec![make_occurrence(dir.path(), "test.rs", 2, 5, "let teh = 1;")];
        disable_in_files(&occs).unwrap();
        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content.matches("spell-checker:disable").count(), 1);
    }

    #[test]
    fn disable_in_files_dedup_files() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "teh teh\n").unwrap();
        let occs = vec![
            make_occurrence(dir.path(), "test.rs", 1, 1, "teh teh"),
            make_occurrence(dir.path(), "test.rs", 1, 5, "teh teh"),
        ];
        let count = disable_in_files(&occs).unwrap();
        assert_eq!(count, 1);
    }

    // ── ignore_in_files ──

    #[test]
    fn ignore_in_files_top_insertion() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "let teh = 1;\n").unwrap();
        let occs = vec![make_occurrence(dir.path(), "test.rs", 1, 5, "let teh = 1;")];
        let count = ignore_in_files(&occs, "teh").unwrap();
        assert_eq!(count, 1);
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.starts_with("// spell-checker:ignore teh\n"));
    }

    #[test]
    fn ignore_in_files_appends_to_existing() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "// spell-checker:ignore foo\nlet teh = 1;\n").unwrap();
        let occs = vec![make_occurrence(dir.path(), "test.rs", 2, 5, "let teh = 1;")];
        ignore_in_files(&occs, "teh").unwrap();
        let content = std::fs::read_to_string(&file).unwrap();
        let first_line = content.lines().next().unwrap();
        assert!(first_line.contains("foo"));
        assert!(first_line.contains("teh"));
    }

    #[test]
    fn ignore_in_files_no_duplicate_word() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "// spell-checker:ignore teh\nlet teh = 1;\n").unwrap();
        let occs = vec![make_occurrence(dir.path(), "test.rs", 2, 5, "let teh = 1;")];
        ignore_in_files(&occs, "teh").unwrap();
        let content = std::fs::read_to_string(&file).unwrap();
        let first_line = content.lines().next().unwrap();
        assert_eq!(first_line.matches("teh").count(), 1);
    }

    #[test]
    fn ignore_in_files_python_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.py");
        std::fs::write(&file, "teh = 1\n").unwrap();
        let occs = vec![make_occurrence(dir.path(), "test.py", 1, 1, "teh = 1")];
        ignore_in_files(&occs, "teh").unwrap();
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.starts_with("# spell-checker:ignore teh\n"));
    }

    // ── append_typo + load_typos_toml ──

    #[test]
    fn append_typo_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("_typos.toml");
        append_typo(&path, "teh", "the").unwrap();
        assert!(path.exists());
        let map = load_typos_toml(&path);
        assert_eq!(map.get("teh").map(|s| s.as_str()), Some("the"));
    }

    #[test]
    fn append_typo_accumulates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("_typos.toml");
        append_typo(&path, "teh", "the").unwrap();
        append_typo(&path, "writen", "written").unwrap();
        let map = load_typos_toml(&path);
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("teh").map(|s| s.as_str()), Some("the"));
        assert_eq!(map.get("writen").map(|s| s.as_str()), Some("written"));
    }

    #[test]
    fn append_typo_duplicate_prevention() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("_typos.toml");
        append_typo(&path, "teh", "the").unwrap();
        append_typo(&path, "teh", "thee").unwrap();
        let map = load_typos_toml(&path);
        assert_eq!(map.get("teh").map(|s| s.as_str()), Some("the"));
    }

    #[test]
    fn load_typos_toml_missing_file() {
        let map = load_typos_toml(Path::new("/nonexistent/_typos.toml"));
        assert!(map.is_empty());
    }

    #[test]
    fn load_typos_toml_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("_typos.toml");
        std::fs::write(&path, "").unwrap();
        let map = load_typos_toml(&path);
        assert!(map.is_empty());
    }

    #[test]
    fn typos_toml_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("_typos.toml");
        append_typo(&path, "Teh", "the").unwrap();
        let map = load_typos_toml(&path);
        assert!(map.contains_key("teh"));
        assert!(!map.contains_key("Teh"));
    }

    // ── collect_writable_dicts ──

    #[test]
    fn collect_writable_dicts_auto_discover() {
        let dir = tempfile::tempdir().unwrap();
        let dict_dir = dir.path().join(".matchum/dict");
        std::fs::create_dir_all(&dict_dir).unwrap();
        std::fs::write(dict_dir.join("project.txt"), "").unwrap();
        std::fs::write(dict_dir.join("names.txt"), "").unwrap();
        let config = matchum_config::matchum_config::MatchumConfig::default();
        let dicts = collect_writable_dicts(&config, dir.path());
        assert_eq!(dicts.len(), 2);
        assert_eq!(dicts[0].name, "names");
        assert_eq!(dicts[1].name, "project");
    }

    #[test]
    fn collect_writable_dicts_missing_dir() {
        let dir = tempfile::tempdir().unwrap();
        let config = matchum_config::matchum_config::MatchumConfig::default();
        let dicts = collect_writable_dicts(&config, dir.path());
        assert!(dicts.is_empty());
    }

    #[test]
    fn collect_writable_dicts_words_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".matchum")).unwrap();
        std::fs::write(dir.path().join(".matchum/words.txt"), "hello\n").unwrap();
        let config = matchum_config::matchum_config::MatchumConfig {
            words_file: Some(".matchum/words.txt".to_string()),
            ..Default::default()
        };
        let dicts = collect_writable_dicts(&config, dir.path());
        assert_eq!(dicts.len(), 1);
        assert_eq!(dicts[0].name, "words");
    }

    #[test]
    fn collect_writable_dicts_missing_file_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let config = matchum_config::matchum_config::MatchumConfig {
            words_file: Some(".matchum/nonexistent.txt".to_string()),
            ..Default::default()
        };
        let dicts = collect_writable_dicts(&config, dir.path());
        assert!(dicts.is_empty());
    }

    #[test]
    fn collect_writable_dicts_dedup() {
        let dir = tempfile::tempdir().unwrap();
        let dict_dir = dir.path().join(".matchum/dict");
        std::fs::create_dir_all(&dict_dir).unwrap();
        std::fs::write(dict_dir.join("words.txt"), "").unwrap();
        let config = matchum_config::matchum_config::MatchumConfig {
            words_file: Some(".matchum/dict/words.txt".to_string()),
            ..Default::default()
        };
        let dicts = collect_writable_dicts(&config, dir.path());
        assert_eq!(dicts.len(), 1);
    }

    // ── create_new_dict ──

    #[test]
    fn create_new_dict_bare_name() {
        let dir = tempfile::tempdir().unwrap();
        let mut reader = Cursor::new(b"matchum\n".to_vec());
        let dict = create_new_dict(&mut reader, dir.path(), None).unwrap();
        assert_eq!(dict.name, "matchum");
        assert!(dict.path.ends_with(".matchum/dict/matchum.txt"));
        assert!(dict.path.exists());
    }

    #[test]
    fn create_new_dict_with_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut reader = Cursor::new(b"custom/dict.txt\n".to_vec());
        let dict = create_new_dict(&mut reader, dir.path(), None).unwrap();
        assert_eq!(dict.name, "dict");
        assert!(dict.path.ends_with("custom/dict.txt"));
    }

    #[test]
    fn create_new_dict_empty_name_error() {
        let dir = tempfile::tempdir().unwrap();
        let mut reader = Cursor::new(b"\n".to_vec());
        assert!(create_new_dict(&mut reader, dir.path(), None).is_err());
    }

    #[test]
    fn create_new_dict_updates_matchum_toml() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("matchum.toml");
        std::fs::write(&config_path, "language = \"en\"\n").unwrap();
        let mut reader = Cursor::new(b"mydict\n".to_vec());
        create_new_dict(&mut reader, dir.path(), Some(&config_path)).unwrap();
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("words_file"));
    }

    #[test]
    fn create_new_dict_preserves_existing_words_file() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("matchum.toml");
        std::fs::write(&config_path, "words_file = \"existing.txt\"\n").unwrap();
        let mut reader = Cursor::new(b"newdict\n".to_vec());
        create_new_dict(&mut reader, dir.path(), Some(&config_path)).unwrap();
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("existing.txt"));
        assert_eq!(content.matches("words_file").count(), 1);
    }

    // ── dispatch_command ──

    #[test]
    fn dispatch_skip_empty() {
        let dir = tempfile::tempdir().unwrap();
        let group = make_group("teh", vec![]);
        let mut dicts = vec![];
        let typos = dir.path().join("_typos.toml");
        let mut fix_memory = HashMap::new();
        let mut reader = Cursor::new(b"".to_vec());
        let result = dispatch_command(
            &mut reader,
            "",
            &group,
            &mut dicts,
            dir.path(),
            None,
            &typos,
            &mut fix_memory,
            &mut HashSet::new(),
        )
        .unwrap();
        assert_eq!(result, DispatchResult::Skipped);
    }

    #[test]
    fn dispatch_skip_s() {
        let dir = tempfile::tempdir().unwrap();
        let group = make_group("teh", vec![]);
        let mut dicts = vec![];
        let typos = dir.path().join("_typos.toml");
        let mut fix_memory = HashMap::new();
        let mut reader = Cursor::new(b"".to_vec());
        let result = dispatch_command(
            &mut reader,
            "s",
            &group,
            &mut dicts,
            dir.path(),
            None,
            &typos,
            &mut fix_memory,
            &mut HashSet::new(),
        )
        .unwrap();
        assert_eq!(result, DispatchResult::Skipped);
    }

    #[test]
    fn dispatch_quit() {
        let dir = tempfile::tempdir().unwrap();
        let group = make_group("teh", vec![]);
        let mut dicts = vec![];
        let typos = dir.path().join("_typos.toml");
        let mut fix_memory = HashMap::new();
        let mut reader = Cursor::new(b"".to_vec());
        let result = dispatch_command(
            &mut reader,
            "q",
            &group,
            &mut dicts,
            dir.path(),
            None,
            &typos,
            &mut fix_memory,
            &mut HashSet::new(),
        )
        .unwrap();
        assert_eq!(result, DispatchResult::Quit);
    }

    #[test]
    fn dispatch_accept_default_dict() {
        let dir = tempfile::tempdir().unwrap();
        let dict_path = dir.path().join("dict.txt");
        std::fs::write(&dict_path, "").unwrap();
        let group = make_group(
            "teh",
            vec![make_occurrence(dir.path(), "test.rs", 1, 5, "let teh = 1;")],
        );
        let mut dicts = vec![WritableDict {
            name: "words".into(),
            path: dict_path.clone(),
        }];
        let typos = dir.path().join("_typos.toml");
        let mut fix_memory = HashMap::new();
        let mut reader = Cursor::new(b"".to_vec());
        let result = dispatch_command(
            &mut reader,
            "a",
            &group,
            &mut dicts,
            dir.path(),
            None,
            &typos,
            &mut fix_memory,
            &mut HashSet::new(),
        )
        .unwrap();
        assert_eq!(result, DispatchResult::Accepted);
        let content = std::fs::read_to_string(&dict_path).unwrap();
        assert!(content.contains("teh"));
    }

    #[test]
    fn dispatch_accept_numbered_dict() {
        let dir = tempfile::tempdir().unwrap();
        let dict1 = dir.path().join("first.txt");
        let dict2 = dir.path().join("second.txt");
        std::fs::write(&dict1, "").unwrap();
        std::fs::write(&dict2, "").unwrap();
        let group = make_group("teh", vec![]);
        let mut dicts = vec![
            WritableDict {
                name: "first".into(),
                path: dict1,
            },
            WritableDict {
                name: "second".into(),
                path: dict2.clone(),
            },
        ];
        let typos = dir.path().join("_typos.toml");
        let mut fix_memory = HashMap::new();
        let mut reader = Cursor::new(b"".to_vec());
        let result = dispatch_command(
            &mut reader,
            "a 2",
            &group,
            &mut dicts,
            dir.path(),
            None,
            &typos,
            &mut fix_memory,
            &mut HashSet::new(),
        )
        .unwrap();
        assert_eq!(result, DispatchResult::Accepted);
        let content = std::fs::read_to_string(&dict2).unwrap();
        assert!(content.contains("teh"));
    }

    #[test]
    fn dispatch_accept_create_new() {
        let dir = tempfile::tempdir().unwrap();
        let group = make_group("teh", vec![]);
        let mut dicts = vec![];
        let typos = dir.path().join("_typos.toml");
        let mut fix_memory = HashMap::new();
        let mut reader = Cursor::new(b"mydict\n".to_vec());
        let result = dispatch_command(
            &mut reader,
            "a 0",
            &group,
            &mut dicts,
            dir.path(),
            None,
            &typos,
            &mut fix_memory,
            &mut HashSet::new(),
        )
        .unwrap();
        assert_eq!(result, DispatchResult::Accepted);
        assert_eq!(dicts.len(), 1);
        assert_eq!(dicts[0].name, "mydict");
        let content = std::fs::read_to_string(&dicts[0].path).unwrap();
        assert!(content.contains("teh"));
    }

    #[test]
    fn dispatch_fix_inline() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "let teh = 1;").unwrap();
        let group = make_group(
            "teh",
            vec![make_occurrence(dir.path(), "test.rs", 1, 5, "let teh = 1;")],
        );
        let mut dicts = vec![];
        let typos = dir.path().join("_typos.toml");
        let mut fix_memory = HashMap::new();
        let mut reader = Cursor::new(b"".to_vec());
        let result = dispatch_command(
            &mut reader,
            "f the",
            &group,
            &mut dicts,
            dir.path(),
            None,
            &typos,
            &mut fix_memory,
            &mut HashSet::new(),
        )
        .unwrap();
        assert_eq!(result, DispatchResult::Fixed);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "let the = 1;");
        assert_eq!(fix_memory.get("teh").map(|s| s.as_str()), Some("the"));
    }

    #[test]
    fn dispatch_fix_suggestion_number() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "let teh = 1;").unwrap();
        let occs = vec![make_occurrence(dir.path(), "test.rs", 1, 5, "let teh = 1;")];
        let group = make_group_with_suggestions("teh", occs, vec!["the", "tea"]);
        let mut dicts = vec![];
        let typos = dir.path().join("_typos.toml");
        let mut fix_memory = HashMap::new();
        let mut reader = Cursor::new(b"".to_vec());
        let result = dispatch_command(
            &mut reader,
            "f 2",
            &group,
            &mut dicts,
            dir.path(),
            None,
            &typos,
            &mut fix_memory,
            &mut HashSet::new(),
        )
        .unwrap();
        assert_eq!(result, DispatchResult::Fixed);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "let tea = 1;");
    }

    #[test]
    fn dispatch_fix_from_memory() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "let teh = 1;").unwrap();
        let group = make_group(
            "teh",
            vec![make_occurrence(dir.path(), "test.rs", 1, 5, "let teh = 1;")],
        );
        let mut dicts = vec![];
        let typos = dir.path().join("_typos.toml");
        let mut fix_memory = HashMap::new();
        fix_memory.insert("teh".to_string(), "the".to_string());
        let mut reader = Cursor::new(b"".to_vec());
        let result = dispatch_command(
            &mut reader,
            "f",
            &group,
            &mut dicts,
            dir.path(),
            None,
            &typos,
            &mut fix_memory,
            &mut HashSet::new(),
        )
        .unwrap();
        assert_eq!(result, DispatchResult::Fixed);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "let the = 1;");
    }

    #[test]
    fn dispatch_edit_inline() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "let teh = 1;").unwrap();
        let group = make_group(
            "teh",
            vec![make_occurrence(dir.path(), "test.rs", 1, 5, "let teh = 1;")],
        );
        let mut dicts = vec![];
        let typos = dir.path().join("_typos.toml");
        let mut fix_memory = HashMap::new();
        let mut reader = Cursor::new(b"".to_vec());
        let result = dispatch_command(
            &mut reader,
            "e the",
            &group,
            &mut dicts,
            dir.path(),
            None,
            &typos,
            &mut fix_memory,
            &mut HashSet::new(),
        )
        .unwrap();
        assert_eq!(result, DispatchResult::Fixed);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "let the = 1;");
    }

    #[test]
    fn dispatch_edit_interactive() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "let teh = 1;").unwrap();
        let group = make_group(
            "teh",
            vec![make_occurrence(dir.path(), "test.rs", 1, 5, "let teh = 1;")],
        );
        let mut dicts = vec![];
        let typos = dir.path().join("_typos.toml");
        let mut fix_memory = HashMap::new();
        let mut reader = Cursor::new(b"the\n".to_vec());
        let result = dispatch_command(
            &mut reader,
            "e",
            &group,
            &mut dicts,
            dir.path(),
            None,
            &typos,
            &mut fix_memory,
            &mut HashSet::new(),
        )
        .unwrap();
        assert_eq!(result, DispatchResult::Fixed);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "let the = 1;");
    }

    #[test]
    fn dispatch_ignore() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "let teh = 1;\n").unwrap();
        let group = make_group(
            "teh",
            vec![make_occurrence(dir.path(), "test.rs", 1, 5, "let teh = 1;")],
        );
        let mut dicts = vec![];
        let typos = dir.path().join("_typos.toml");
        let mut fix_memory = HashMap::new();
        let mut reader = Cursor::new(b"".to_vec());
        let result = dispatch_command(
            &mut reader,
            "i",
            &group,
            &mut dicts,
            dir.path(),
            None,
            &typos,
            &mut fix_memory,
            &mut HashSet::new(),
        )
        .unwrap();
        assert_eq!(result, DispatchResult::Ignored);
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.contains("spell-checker:ignore teh"));
    }

    #[test]
    fn dispatch_disable_line() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "let teh = 1;\n").unwrap();
        let group = make_group(
            "teh",
            vec![make_occurrence(dir.path(), "test.rs", 1, 5, "let teh = 1;")],
        );
        let mut dicts = vec![];
        let typos = dir.path().join("_typos.toml");
        let mut fix_memory = HashMap::new();
        let mut reader = Cursor::new(b"".to_vec());
        let result = dispatch_command(
            &mut reader,
            "dl",
            &group,
            &mut dicts,
            dir.path(),
            None,
            &typos,
            &mut fix_memory,
            &mut HashSet::new(),
        )
        .unwrap();
        assert_eq!(result, DispatchResult::Ignored);
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.contains("spell-checker:disable-next-line"));
    }

    #[test]
    fn dispatch_disable_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "let teh = 1;\n").unwrap();
        let group = make_group(
            "teh",
            vec![make_occurrence(dir.path(), "test.rs", 1, 5, "let teh = 1;")],
        );
        let mut dicts = vec![];
        let typos = dir.path().join("_typos.toml");
        let mut fix_memory = HashMap::new();
        let mut reader = Cursor::new(b"".to_vec());
        let result = dispatch_command(
            &mut reader,
            "df",
            &group,
            &mut dicts,
            dir.path(),
            None,
            &typos,
            &mut fix_memory,
            &mut HashSet::new(),
        )
        .unwrap();
        assert_eq!(result, DispatchResult::Ignored);
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.contains("spell-checker:disable"));
    }

    #[test]
    fn dispatch_unknown_command() {
        let dir = tempfile::tempdir().unwrap();
        let group = make_group("teh", vec![]);
        let mut dicts = vec![];
        let typos = dir.path().join("_typos.toml");
        let mut fix_memory = HashMap::new();
        let mut reader = Cursor::new(b"".to_vec());
        let result = dispatch_command(
            &mut reader,
            "xyz",
            &group,
            &mut dicts,
            dir.path(),
            None,
            &typos,
            &mut fix_memory,
            &mut HashSet::new(),
        )
        .unwrap();
        assert_eq!(result, DispatchResult::Skipped);
    }
}
