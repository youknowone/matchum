//! Test harness for multi-repository binary comparison.
//!
//! Clones external repositories at pinned commits, runs both matchum and cspell,
//! and compares results against stored snapshots.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

/// Specification for an external repository to test against.
#[derive(Debug, Clone)]
pub struct RepoSpec {
    pub name: &'static str,
    pub url: &'static str,
    pub commit: &'static str,
    pub check_paths: &'static [&'static str],
    pub cspell_config: Option<&'static str>,
    /// Extra CLI arguments passed to both cspell and matchum (e.g. --locale, --exclude).
    pub args: &'static [&'static str],
}

/// A single spelling issue detected by either tool.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Issue {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub word: String,
}

/// Returns the cache directory for cloned repos: `~/.matchum_cache/repos/`.
fn cache_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME environment variable not set");
    PathBuf::from(home).join(".matchum_cache").join("repos")
}

/// Returns the project root (workspace root).
fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Clone the repository at the pinned commit (shallow), or skip if already present
/// with the correct commit checked out.
pub fn ensure_cloned(spec: &RepoSpec) -> PathBuf {
    let repo_dir = cache_dir().join(spec.name);

    // Check if already cloned at the correct commit
    if repo_dir.join(".git").exists() {
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo_dir)
            .output();

        if let Ok(output) = output {
            let current_commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if current_commit == spec.commit {
                eprintln!("[harness] {} already at {}", spec.name, spec.commit);
                return repo_dir;
            }
            eprintln!(
                "[harness] {} at {} but want {}, re-cloning",
                spec.name, current_commit, spec.commit
            );
            fs::remove_dir_all(&repo_dir).expect("failed to remove stale repo dir");
        }
    }

    eprintln!("[harness] cloning {} at {}", spec.name, spec.commit);
    fs::create_dir_all(&repo_dir).expect("failed to create repo cache dir");

    // git init
    let status = Command::new("git")
        .args(["init"])
        .current_dir(&repo_dir)
        .status()
        .expect("failed to run git init");
    assert!(status.success(), "git init failed for {}", spec.name);

    // git remote add origin <url>
    let status = Command::new("git")
        .args(["remote", "add", "origin", spec.url])
        .current_dir(&repo_dir)
        .status()
        .expect("failed to run git remote add");
    assert!(status.success(), "git remote add failed for {}", spec.name);

    // git fetch origin <commit> --depth=1
    let status = Command::new("git")
        .args(["fetch", "origin", spec.commit, "--depth=1"])
        .current_dir(&repo_dir)
        .status()
        .expect("failed to run git fetch");
    assert!(
        status.success(),
        "git fetch failed for {} (commit: {})",
        spec.name,
        spec.commit
    );

    // git checkout FETCH_HEAD
    let status = Command::new("git")
        .args(["checkout", "FETCH_HEAD"])
        .current_dir(&repo_dir)
        .status()
        .expect("failed to run git checkout");
    assert!(
        status.success(),
        "git checkout FETCH_HEAD failed for {}",
        spec.name
    );

    repo_dir
}

/// Run matchum on the repo and collect issues.
///
/// Looks for the matchum binary in `target/debug/` or `target/release/`.
/// Output format: `path:line:col - Unknown word: 'word'`
pub fn run_matchum(repo_dir: &Path, spec: &RepoSpec) -> BTreeSet<Issue> {
    let root = project_root();
    let binary = ["target/debug/matchum", "target/release/matchum"]
        .iter()
        .map(|p| root.join(p))
        .find(|p| p.exists())
        .expect("matchum binary not found. Build with: cargo build -p matchum");

    let mut cmd = Command::new(&binary);
    cmd.arg("check");

    // Use the repo's cspell config if available, otherwise use a minimal default
    if let Some(config) = spec.cspell_config {
        let config_path = repo_dir.join(config);
        if config_path.exists() {
            cmd.args(["--config"]).arg(&config_path);
        }
    }

    for arg in spec.args {
        cmd.arg(arg);
    }

    for path in spec.check_paths {
        cmd.arg(repo_dir.join(path));
    }

    cmd.current_dir(repo_dir);

    let output = cmd.output().expect("failed to run matchum");

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_matchum_output(&stdout, repo_dir)
}

/// Run cspell on the repo and collect issues.
///
/// Output format: `path:line:col - Unknown word (word)`
pub fn run_cspell(repo_dir: &Path, spec: &RepoSpec) -> BTreeSet<Issue> {
    let mut cmd = Command::new("npx");
    cmd.args([
        "cspell",
        "lint",
        "--no-progress",
        "--no-summary",
        "--relative",
    ]);

    if let Some(config) = spec.cspell_config {
        let config_path = repo_dir.join(config);
        if config_path.exists() {
            cmd.arg("--config").arg(&config_path);
        }
    }

    for arg in spec.args {
        cmd.arg(arg);
    }

    for path in spec.check_paths {
        cmd.arg(path);
    }

    cmd.current_dir(repo_dir);

    let output = cmd.output().expect("failed to run npx cspell");

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_cspell_output(&stdout, repo_dir)
}

/// Parse matchum output lines.
///
/// Format: `/absolute/path/to/file.rs:10:5 - Unknown word (foobar)`
///         `/absolute/path/to/file.rs:10:5 - Forbidden word (foobar)`
fn parse_matchum_output(output: &str, repo_dir: &Path) -> BTreeSet<Issue> {
    let mut issues = BTreeSet::new();
    let repo_prefix = format!("{}/", repo_dir.display());

    for line in output.lines() {
        let marker = if line.contains(" - Unknown word (") {
            " - Unknown word ("
        } else if line.contains(" - Forbidden word (") {
            " - Forbidden word ("
        } else {
            continue;
        };

        if let Some((loc, rest)) = line.split_once(marker) {
            let word = match rest.find(')') {
                Some(end) => &rest[..end],
                None => continue,
            };

            let parts: Vec<&str> = loc.rsplitn(3, ':').collect();
            if parts.len() < 3 {
                continue;
            }

            let (col, ln, file_path) = match (parts[0].parse::<usize>(), parts[1].parse::<usize>())
            {
                (Ok(c), Ok(l)) => (c, l, parts[2]),
                _ => continue,
            };

            // Make path relative to repo root
            let relative = file_path
                .strip_prefix(&repo_prefix)
                .unwrap_or(file_path)
                .to_string();

            issues.insert(Issue {
                file: relative,
                line: ln,
                column: col,
                word: word.to_string(),
            });
        }
    }
    issues
}

/// Parse cspell output lines.
///
/// Format: `relative/path/file.rs:10:5 - Unknown word (foobar)`
///         `relative/path/file.rs:10:5 - Unknown word (foobar) fix: (suggestion)`
fn parse_cspell_output(output: &str, _repo_dir: &Path) -> BTreeSet<Issue> {
    let mut issues = BTreeSet::new();

    for line in output.lines() {
        let marker = if line.contains(" - Unknown word (") {
            " - Unknown word ("
        } else if line.contains(" - Forbidden word (") {
            " - Forbidden word ("
        } else {
            continue;
        };

        if let Some((loc, rest)) = line.split_once(marker) {
            let word = match rest.find(')') {
                Some(end) => &rest[..end],
                None => continue,
            };

            let parts: Vec<&str> = loc.rsplitn(3, ':').collect();
            if parts.len() < 3 {
                continue;
            }

            let (col, ln, file_path) = match (parts[0].parse::<usize>(), parts[1].parse::<usize>())
            {
                (Ok(c), Ok(l)) => (c, l, parts[2]),
                _ => continue,
            };

            issues.insert(Issue {
                file: file_path.to_string(),
                line: ln,
                column: col,
                word: word.to_string(),
            });
        }
    }
    issues
}

/// Returns the path to the snapshot file for a repo.
fn snapshot_path(spec: &RepoSpec) -> PathBuf {
    project_root()
        .join("tests/multi_repo/snapshots")
        .join(format!("{}.json", spec.name))
}

/// Load a previously saved cspell snapshot.
pub fn load_snapshot(spec: &RepoSpec) -> Option<BTreeSet<Issue>> {
    let path = snapshot_path(spec);
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read snapshot {}: {e}", path.display()));
    let issues: BTreeSet<Issue> = serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("failed to parse snapshot {}: {e}", path.display()));
    Some(issues)
}

/// Save a cspell snapshot to disk.
pub fn save_snapshot(spec: &RepoSpec, issues: &BTreeSet<Issue>) {
    let path = snapshot_path(spec);
    fs::create_dir_all(path.parent().unwrap()).expect("failed to create snapshots dir");
    let json = serde_json::to_string_pretty(issues).expect("failed to serialize snapshot");
    fs::write(&path, json)
        .unwrap_or_else(|e| panic!("failed to write snapshot {}: {e}", path.display()));
    eprintln!(
        "[harness] saved snapshot for {} ({} issues) -> {}",
        spec.name,
        issues.len(),
        path.display()
    );
}

/// Generate a cspell snapshot for a repo. Requires `npx cspell` to be available.
pub fn generate_snapshot(spec: &RepoSpec) {
    let repo_dir = ensure_cloned(spec);
    let cspell_issues = run_cspell(&repo_dir, spec);
    save_snapshot(spec, &cspell_issues);
}

/// Main comparison function: run matchum and compare against cspell snapshot.
///
/// 1. Clone repo at pinned commit
/// 2. Run matchum
/// 3. Load cspell snapshot (or fail with instructions to generate)
/// 4. Compare and report differences
pub fn compare_repo(spec: &RepoSpec) {
    eprintln!("\n========================================");
    eprintln!("  Testing: {} ({})", spec.name, spec.url);
    eprintln!("  Commit:  {}", spec.commit);
    eprintln!("========================================\n");

    let repo_dir = ensure_cloned(spec);
    let matchum_issues = run_matchum(&repo_dir, spec);

    let cspell_issues = match load_snapshot(spec) {
        Some(issues) => issues,
        None => {
            panic!(
                "No cspell snapshot found for '{}'. \
                 Generate it first with:\n  \
                 cargo test --test multi_repo_compat generate_all_snapshots -- --ignored\n  \
                 or:\n  \
                 cargo test --test multi_repo_compat generate_snapshot_{} -- --ignored",
                spec.name, spec.name
            );
        }
    };

    // Compute differences
    let only_cspell: Vec<_> = cspell_issues.difference(&matchum_issues).collect();
    let only_matchum: Vec<_> = matchum_issues.difference(&cspell_issues).collect();
    let common_count = cspell_issues.intersection(&matchum_issues).count();

    eprintln!("  matchum issues:  {}", matchum_issues.len());
    eprintln!("  cspell issues:   {}", cspell_issues.len());
    eprintln!("  common:          {}", common_count);
    eprintln!("  cspell-only:     {}", only_cspell.len());
    eprintln!("  matchum-only:    {}", only_matchum.len());

    let total = common_count + only_cspell.len() + only_matchum.len();
    if total > 0 {
        let pct = (common_count as f64 / total as f64) * 100.0;
        eprintln!("  compatibility:   {:.1}%", pct);
    }

    if !only_cspell.is_empty() {
        eprintln!("\n  --- Issues found only by cspell (matchum missed) ---");
        for (i, issue) in only_cspell.iter().enumerate() {
            if i >= 50 {
                eprintln!("    ... and {} more", only_cspell.len() - 50);
                break;
            }
            eprintln!(
                "    {}:{}:{} '{}'",
                issue.file, issue.line, issue.column, issue.word
            );
        }
    }

    if !only_matchum.is_empty() {
        eprintln!("\n  --- Issues found only by matchum (false positives?) ---");
        for (i, issue) in only_matchum.iter().enumerate() {
            if i >= 50 {
                eprintln!("    ... and {} more", only_matchum.len() - 50);
                break;
            }
            eprintln!(
                "    {}:{}:{} '{}'",
                issue.file, issue.line, issue.column, issue.word
            );
        }
    }

    assert_eq!(
        only_cspell.len(),
        0,
        "{}: cspell found {} issues that matchum missed",
        spec.name,
        only_cspell.len()
    );
    assert_eq!(
        only_matchum.len(),
        0,
        "{}: matchum found {} extra issues not in cspell",
        spec.name,
        only_matchum.len()
    );
}
