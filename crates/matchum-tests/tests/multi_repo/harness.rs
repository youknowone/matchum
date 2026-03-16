//! Test harness for multi-repository binary comparison.
//!
//! Clones external repositories at pinned commits, runs both matchum and cspell,
//! and compares results against stored snapshots.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};

/// Specification for an external repository to test against.
#[derive(Debug, Clone)]
pub struct RepoSpec {
    pub name: &'static str,
    pub url: &'static str,
    pub commit: &'static str,
    pub check_paths: &'static [&'static str],
    /// Config file inside the cloned repo (resolved relative to repo dir).
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

#[derive(Debug, Clone, Deserialize)]
struct VendorConfig {
    repositories: Vec<VendorRepo>,
}

#[derive(Debug, Clone, Deserialize)]
struct VendorRepo {
    path: String,
    url: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(rename = "uniqueOnly")]
    unique_only: Option<bool>,
}

/// Returns the cache directory for cloned repos: `~/.matchum_cache/repos/`.
fn cache_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME environment variable not set");
    PathBuf::from(home).join(".matchum_cache").join("repos")
}

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Returns the workspace root (two levels up from this crate).
fn workspace_root() -> PathBuf {
    project_root()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn vendor_temp_dir() -> PathBuf {
    workspace_root().join("vendor/cspell/integration-tests/repositories/temp")
}

fn vendor_root() -> PathBuf {
    workspace_root().join("vendor/cspell/integration-tests")
}

fn vendor_snapshot_dir() -> PathBuf {
    vendor_root().join("snapshots")
}

fn normalize_git_url(url: &str) -> &str {
    url.strip_suffix(".git")
        .unwrap_or(url)
        .trim_end_matches('/')
}

fn vendor_repositories() -> &'static [VendorRepo] {
    static REPOS: OnceLock<Vec<VendorRepo>> = OnceLock::new();
    REPOS.get_or_init(|| {
        let path = vendor_root().join("config/config.json");
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read vendor config {}: {e}", path.display()));
        let config: VendorConfig = serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("failed to parse vendor config {}: {e}", path.display()));
        config.repositories
    })
}

fn vendor_repo(spec: &RepoSpec) -> Option<&'static VendorRepo> {
    let url = normalize_git_url(spec.url);
    vendor_repositories()
        .iter()
        .find(|repo| normalize_git_url(&repo.url) == url)
}

fn resolve_vendor_args(repo: &VendorRepo) -> Vec<String> {
    let replacements = [
        (
            "${repoConfig}",
            format!("../../../../config/repositories/{}", repo.path),
        ),
        ("${commonRoot}", "../../../../repositories".to_string()),
        ("${commonBase}", "../../../../repositories/temp".to_string()),
        (
            "${commonConfig}",
            "../../../../repositories/cspell.yaml".to_string(),
        ),
        (
            "${pathReporter}",
            "../../../../custom-reporter.js".to_string(),
        ),
        (
            "${pathReporterListAll}",
            "../../../../custom-reporter-list-all.js".to_string(),
        ),
        ("${repoPath}", repo.path.clone()),
    ];

    repo.args
        .iter()
        .map(|arg| {
            let mut resolved = arg.clone();
            for (needle, replacement) in &replacements {
                resolved = resolved.replace(needle, replacement);
            }
            assert!(
                !resolved.contains("${"),
                "unresolved vendor placeholder in args for {}: {}",
                repo.path,
                resolved
            );
            resolved
        })
        .collect()
}

fn vendor_snapshot_path(repo: &VendorRepo) -> PathBuf {
    vendor_snapshot_dir().join(&repo.path).join("snapshot.txt")
}

fn checkout_dir(spec: &RepoSpec) -> PathBuf {
    if let Some(repo) = vendor_repo(spec) {
        return vendor_temp_dir().join(&repo.path);
    }
    cache_dir().join(spec.name)
}

fn repo_checkout_lock(repo_dir: &Path) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<std::collections::HashMap<PathBuf, Arc<Mutex<()>>>>> =
        OnceLock::new();
    let locks = LOCKS.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let mut guard = locks.lock().expect("repo lock map poisoned");
    guard
        .entry(repo_dir.to_path_buf())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

/// Clone the repository at the pinned commit (shallow), or skip if already present
/// with the correct commit checked out.
pub fn ensure_cloned(spec: &RepoSpec) -> PathBuf {
    let repo_dir = checkout_dir(spec);
    let checkout_lock = repo_checkout_lock(&repo_dir);
    let _checkout_guard = checkout_lock.lock().expect("repo checkout lock poisoned");

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
            if fs::remove_dir_all(&repo_dir).is_err() {
                // fs::remove_dir_all can fail on macOS; fall back to rm -rf
                let status = Command::new("rm")
                    .args(["-rf"])
                    .arg(&repo_dir)
                    .status()
                    .expect("failed to run rm -rf");
                assert!(status.success(), "rm -rf failed for {}", repo_dir.display());
            }
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

/// Resolve the config file path for a repo spec.
/// Only used for non-vendor repos; vendor-backed repos resolve args from cspell's
/// integration-tests `config.json`.
fn resolve_config(repo_dir: &Path, spec: &RepoSpec) -> Option<PathBuf> {
    if let Some(config) = spec.cspell_config {
        let path = repo_dir.join(config);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Build the effective cspell-compatible CLI args for a repo spec.
/// Shared by compatibility and benchmark harnesses so they exercise the same setup.
pub fn build_repo_args(repo_dir: &Path, spec: &RepoSpec) -> (Vec<String>, bool) {
    if let Some(repo) = vendor_repo(spec) {
        return (resolve_vendor_args(repo), repo.unique_only.unwrap_or(true));
    }

    let mut args = Vec::new();
    if let Some(config_path) = resolve_config(repo_dir, spec) {
        args.push("--config".to_string());
        args.push(config_path.display().to_string());
    }
    args.extend(spec.args.iter().map(|arg| (*arg).to_string()));
    args.extend(spec.check_paths.iter().map(|path| (*path).to_string()));

    (args, true)
}

fn assert_lint_exit_status(
    tool: &str,
    output: &std::process::Output,
    repo_dir: &Path,
    spec: &RepoSpec,
) {
    if output.status.success() || output.status.code() == Some(1) {
        return;
    }

    panic!(
        "{tool} failed for {} in {} with status {:?}\nstdout:\n{}\nstderr:\n{}",
        spec.name,
        repo_dir.display(),
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// Run matchum on the repo and collect issues.
///
/// Uses the release binary directly for fast execution.
/// Build with: `cargo build --release -p matchum`
/// Output format: `path:line:col - Unknown word (word)`
pub fn run_matchum(repo_dir: &Path, spec: &RepoSpec) -> BTreeSet<Issue> {
    let root = workspace_root();
    let binary = root.join("target/release/matchum");
    assert!(
        binary.exists(),
        "matchum release binary not found. Build with: cargo build --release -p matchum"
    );

    let mut cmd = Command::new(&binary);
    let (repo_args, unique) = build_repo_args(repo_dir, spec);

    cmd.args([
        "cspell",
        "lint",
        "--no-progress",
        "--relative",
        "--show-context",
        "--gitignore",
        "--gitignore-root=.",
    ]);

    if unique {
        cmd.arg("--unique");
    }

    for arg in &repo_args {
        cmd.arg(arg);
    }

    cmd.current_dir(repo_dir);

    let output = cmd.output().expect("failed to run matchum");
    assert_lint_exit_status("matchum", &output, repo_dir, spec);

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_matchum_output(&stdout, repo_dir)
}

/// Run cspell on the repo and collect issues.
///
/// Output format: `path:line:col - Unknown word (word)`
pub fn run_cspell(repo_dir: &Path, spec: &RepoSpec) -> BTreeSet<Issue> {
    let (repo_args, unique) = build_repo_args(repo_dir, spec);
    let mut cmd = Command::new("npx");
    cmd.args([
        "cspell",
        "lint",
        "--no-progress",
        "--relative",
        "--show-context",
        "--gitignore",
        "--gitignore-root=.",
    ]);

    if unique {
        cmd.arg("--unique");
    }

    for arg in &repo_args {
        cmd.arg(arg);
    }

    cmd.current_dir(repo_dir);

    let output = cmd.output().expect("failed to run npx cspell");
    assert_lint_exit_status("cspell", &output, repo_dir, spec);

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

            let (col, ln, file_path) = match (
                parts[0].trim().parse::<usize>(),
                parts[1].trim().parse::<usize>(),
            ) {
                (Ok(c), Ok(l)) => (c, l, parts[2].trim()),
                _ => continue,
            };

            // Make path relative to repo root, strip ./ prefix
            let mut relative = file_path
                .strip_prefix(&repo_prefix)
                .unwrap_or(file_path)
                .to_string();
            if let Some(stripped) = relative.strip_prefix("./") {
                relative = stripped.to_string();
            }

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

            let (col, ln, file_path) = match (
                parts[0].trim().parse::<usize>(),
                parts[1].trim().parse::<usize>(),
            ) {
                (Ok(c), Ok(l)) => (c, l, parts[2].trim()),
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

fn load_vendor_snapshot(spec: &RepoSpec) -> Option<BTreeSet<Issue>> {
    let repo = vendor_repo(spec)?;
    let path = vendor_snapshot_path(repo);
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read vendor snapshot {}: {e}", path.display()));
    Some(parse_cspell_output(&content, Path::new(".")))
}

/// Load a previously saved cspell snapshot.
pub fn load_snapshot(spec: &RepoSpec) -> Option<BTreeSet<Issue>> {
    if let Some(issues) = load_vendor_snapshot(spec) {
        return Some(issues);
    }

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

    // Compare using (file, line, word) tuples to ignore column differences.
    // Column offsets can differ between tools due to encoding/splitting details
    // without affecting the actual correctness of the detection.
    let to_key = |i: &Issue| (i.file.clone(), i.line, i.word.clone());
    let cspell_keys: BTreeSet<_> = cspell_issues.iter().map(to_key).collect();
    let matchum_keys: BTreeSet<_> = matchum_issues.iter().map(to_key).collect();

    let only_cspell_keys: BTreeSet<_> = cspell_keys.difference(&matchum_keys).cloned().collect();
    let only_matchum_keys: BTreeSet<_> = matchum_keys.difference(&cspell_keys).cloned().collect();
    let common_count = cspell_keys.intersection(&matchum_keys).count();

    // Map keys back to issues for display
    let only_cspell: Vec<_> = cspell_issues
        .iter()
        .filter(|i| only_cspell_keys.contains(&to_key(i)))
        .collect();
    let only_matchum: Vec<_> = matchum_issues
        .iter()
        .filter(|i| only_matchum_keys.contains(&to_key(i)))
        .collect();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkout_dir_uses_vendor_temp_for_vendor_repos() {
        let spec = RepoSpec {
            name: "eslint",
            url: "https://github.com/eslint/eslint.git",
            commit: "deadbeef",
            check_paths: &["."],
            cspell_config: None,
            args: &[],
        };

        assert_eq!(
            checkout_dir(&spec),
            vendor_temp_dir().join("eslint").join("eslint")
        );
    }

    #[test]
    fn resolve_vendor_args_matches_cspell_placeholders() {
        let spec = RepoSpec {
            name: "slint",
            url: "https://github.com/slint-ui/slint.git",
            commit: "deadbeef",
            check_paths: &[],
            cspell_config: None,
            args: &[],
        };

        let repo = vendor_repo(&spec).expect("vendor repo");
        assert_eq!(
            resolve_vendor_args(repo),
            vec![
                "--config=../../../../repositories/config/slint/cspell.config.yaml",
                "--issues-summary-report",
                ".",
            ]
        );
    }

    #[test]
    fn load_snapshot_prefers_vendor_snapshot_txt() {
        let spec = RepoSpec {
            name: "eslint",
            url: "https://github.com/eslint/eslint.git",
            commit: "deadbeef",
            check_paths: &[],
            cspell_config: None,
            args: &[],
        };

        let issues = load_snapshot(&spec).expect("snapshot");
        assert!(issues.contains(&Issue {
            file: "docs/README.md".to_string(),
            line: 44,
            column: 51,
            word: "openjsf".to_string(),
        }));
    }
}
