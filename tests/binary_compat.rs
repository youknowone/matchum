//! Binary compatibility tests: run both cspell and matchum on the same files
//! and compare results.
//!
//! These tests require `npx cspell` to be available and are ignored by default.
//! Run with: `cargo test --test binary_compat -- --ignored`

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A detected spelling issue (word + location).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Issue {
    line: usize,
    column: usize,
    word: String,
}

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn config_path() -> PathBuf {
    project_root().join("tests/fixtures/cspell-full.json")
}

/// Parse cspell output lines like:
///   /path/to/file.txt:1:14 - Unknown word (fulll)
///   /path/to/file.txt:1:14 - Unknown word (hass) fix: (hash)
///   /path/to/file.txt:1:14 - Forbidden word (fulll)
fn parse_cspell_output(output: &str) -> BTreeSet<Issue> {
    let mut issues = BTreeSet::new();
    for line in output.lines() {
        // Format: path:line:col - Unknown word (word)
        // or:     path:line:col - Unknown word (word) fix: (suggestion)
        let marker = if line.contains(" - Unknown word (") {
            " - Unknown word ("
        } else if line.contains(" - Forbidden word (") {
            " - Forbidden word ("
        } else {
            continue;
        };

        if let Some((loc, rest)) = line.split_once(marker) {
            // Extract word from "(word)" or "(word) fix: (...)"
            let word = if let Some(paren_end) = rest.find(')') {
                &rest[..paren_end]
            } else {
                continue;
            };

            let parts: Vec<&str> = loc.rsplitn(3, ':').collect();
            if parts.len() >= 2 {
                if let (Ok(col), Ok(ln)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                    issues.insert(Issue {
                        line: ln,
                        column: col,
                        word: word.to_string(),
                    });
                }
            }
        }
    }
    issues
}

/// Parse matchum output lines like:
///   /path/to/file.txt:1:14 - Unknown word: 'fulll'
///   /path/to/file.txt:1:14 - Forbidden word: 'fulll'
fn parse_matchum_output(output: &str) -> BTreeSet<Issue> {
    let mut issues = BTreeSet::new();
    for line in output.lines() {
        let marker = if line.contains(" - Unknown word: '") {
            " - Unknown word: '"
        } else if line.contains(" - Forbidden word: '") {
            " - Forbidden word: '"
        } else {
            continue;
        };

        if let Some((loc, rest)) = line.split_once(marker) {
            let word = if let Some(quote_end) = rest.find('\'') {
                &rest[..quote_end]
            } else {
                continue;
            };

            let parts: Vec<&str> = loc.rsplitn(3, ':').collect();
            if parts.len() >= 2 {
                if let (Ok(col), Ok(ln)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                    issues.insert(Issue {
                        line: ln,
                        column: col,
                        word: word.to_string(),
                    });
                }
            }
        }
    }
    issues
}

fn run_cspell(file: &Path, config: &Path) -> Option<String> {
    let output = Command::new("npx")
        .args([
            "cspell",
            "lint",
            "--no-progress",
            "--no-summary",
            "--config",
        ])
        .arg(config)
        .arg(file)
        .current_dir(project_root())
        .output()
        .ok()?;

    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

fn run_matchum(file: &Path, config: &Path) -> Option<String> {
    let binary = project_root().join("target/debug/matchum");
    if !binary.exists() {
        return None;
    }

    let output = Command::new(&binary)
        .args(["check", "--config"])
        .arg(config)
        .arg(file)
        .current_dir(project_root())
        .output()
        .ok()?;

    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

fn compare_file(relative_path: &str) {
    let file = project_root().join(relative_path);
    if !file.exists() {
        eprintln!("Skipping: {} not found", relative_path);
        return;
    }

    let config = config_path();

    let cspell_out = run_cspell(&file, &config).expect("failed to run cspell");
    let matchum_out = run_matchum(&file, &config).expect("failed to run matchum");

    let cspell_issues = parse_cspell_output(&cspell_out);
    let matchum_issues = parse_matchum_output(&matchum_out);

    // Find differences
    let only_cspell: Vec<_> = cspell_issues.difference(&matchum_issues).collect();
    let only_matchum: Vec<_> = matchum_issues.difference(&cspell_issues).collect();

    if !only_cspell.is_empty() || !only_matchum.is_empty() {
        eprintln!("\n=== Differences for {} ===", relative_path);
        if !only_cspell.is_empty() {
            eprintln!("  Only in cspell ({}):", only_cspell.len());
            for issue in &only_cspell {
                eprintln!(
                    "    {}:{}:{} '{}'",
                    relative_path, issue.line, issue.column, issue.word
                );
            }
        }
        if !only_matchum.is_empty() {
            eprintln!("  Only in matchum ({}):", only_matchum.len());
            for issue in &only_matchum {
                eprintln!(
                    "    {}:{}:{} '{}'",
                    relative_path, issue.line, issue.column, issue.word
                );
            }
        }
        eprintln!(
            "  Common issues: {}, cspell-only: {}, matchum-only: {}",
            cspell_issues.intersection(&matchum_issues).count(),
            only_cspell.len(),
            only_matchum.len()
        );
    }

    assert_eq!(
        only_cspell.len(),
        0,
        "cspell found issues that matchum missed in {}: {:?}",
        relative_path,
        only_cspell
    );
    assert_eq!(
        only_matchum.len(),
        0,
        "matchum found issues that cspell didn't in {}: {:?}",
        relative_path,
        only_matchum
    );
}

// ============================================================
// Individual test cases for each fixture file
// ============================================================

#[test]
#[ignore]
fn compat_samples_text() {
    compare_file("vendor/cspell/packages/cspell/samples/text.txt");
}

#[test]
#[ignore]
fn compat_samples_src_sample_c() {
    compare_file("vendor/cspell/packages/cspell/samples/src/sample.c");
}

#[test]
#[ignore]
fn compat_samples_src_sample_py() {
    compare_file("vendor/cspell/packages/cspell/samples/src/sample.py");
}

#[test]
#[ignore]
fn compat_reporter_issues_md() {
    compare_file("vendor/cspell/packages/cspell/fixtures/features/reporter/issues.md");
}

/// Run all sample files and report a summary of compatibility.
#[test]
#[ignore]
fn compat_summary() {
    let files = [
        "vendor/cspell/packages/cspell/samples/text.txt",
        "vendor/cspell/packages/cspell/samples/src/sample.c",
        "vendor/cspell/packages/cspell/samples/src/sample.py",
        "vendor/cspell/packages/cspell/fixtures/features/reporter/issues.md",
    ];

    let config = config_path();
    let mut total_common = 0;
    let mut total_cspell_only = 0;
    let mut total_matchum_only = 0;
    let mut files_tested = 0;

    for relative_path in &files {
        let file = project_root().join(relative_path);
        if !file.exists() {
            continue;
        }

        let cspell_out = match run_cspell(&file, &config) {
            Some(o) => o,
            None => continue,
        };
        let matchum_out = match run_matchum(&file, &config) {
            Some(o) => o,
            None => continue,
        };

        let cspell_issues = parse_cspell_output(&cspell_out);
        let matchum_issues = parse_matchum_output(&matchum_out);

        let common = cspell_issues.intersection(&matchum_issues).count();
        let c_only: Vec<_> = cspell_issues.difference(&matchum_issues).collect();
        let r_only: Vec<_> = matchum_issues.difference(&cspell_issues).collect();

        eprintln!(
            "{}: common={}, cspell-only={}, matchum-only={}",
            relative_path,
            common,
            c_only.len(),
            r_only.len()
        );

        if !c_only.is_empty() {
            for issue in &c_only {
                eprintln!(
                    "  [cspell-only] {}:{} '{}'",
                    issue.line, issue.column, issue.word
                );
            }
        }
        if !r_only.is_empty() {
            for issue in &r_only {
                eprintln!(
                    "  [matchum-only] {}:{} '{}'",
                    issue.line, issue.column, issue.word
                );
            }
        }

        total_common += common;
        total_cspell_only += c_only.len();
        total_matchum_only += r_only.len();
        files_tested += 1;
    }

    eprintln!("\n=== Summary ===");
    eprintln!("Files tested: {}", files_tested);
    eprintln!("Common issues: {}", total_common);
    eprintln!("cspell-only: {}", total_cspell_only);
    eprintln!("matchum-only: {}", total_matchum_only);

    let total = total_common + total_cspell_only + total_matchum_only;
    if total > 0 {
        let pct = (total_common as f64 / total as f64) * 100.0;
        eprintln!("Compatibility: {:.1}%", pct);
    }
}
