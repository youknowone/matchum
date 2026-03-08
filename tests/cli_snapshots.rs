//! CLI output snapshot tests using `insta`.
//!
//! These tests invoke the `matchum` binary via `std::process::Command` and
//! snapshot stdout / stderr / exit-code so that any unintended change to the
//! user-facing output is caught automatically.

use std::path::PathBuf;
use std::process::Command;

// ── helpers ────────────────────────────────────────────────────────

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn matchum_binary() -> PathBuf {
    let root = project_root();
    ["target/debug/matchum", "target/release/matchum"]
        .iter()
        .map(|p| root.join(p))
        .find(|p| p.exists())
        .expect("matchum binary not found — run `cargo build -p matchum` first")
}

/// Run the matchum CLI and return (stdout, stderr, exit_code).
fn run_matchum(args: &[&str]) -> (String, String, i32) {
    let output = Command::new(matchum_binary())
        .args(args)
        .current_dir(project_root())
        .output()
        .expect("failed to execute matchum binary");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);
    (stdout, stderr, code)
}

/// Run the matchum CLI in a specific directory.
fn run_matchum_in(dir: &std::path::Path, args: &[&str]) -> (String, String, i32) {
    let output = Command::new(matchum_binary())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to execute matchum binary");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);
    (stdout, stderr, code)
}

/// Replace the project root path with `[ROOT]` so that snapshots are stable
/// across machines and worktree locations.
fn normalize(text: &str) -> String {
    let root_str = project_root().to_string_lossy().to_string();
    text.replace(&root_str, "[ROOT]")
}

/// Strip transient stderr lines (dictionary fetch progress, cspell migration
/// hints) that depend on network state or cache, keeping only deterministic
/// output such as the summary line.
fn normalize_stderr(text: &str) -> String {
    let root_str = project_root().to_string_lossy().to_string();
    text.replace(&root_str, "[ROOT]")
        .lines()
        .filter(|line| {
            !line.starts_with("Fetching ")
                && !line.starts_with("Installed ")
                && !line.starts_with("hint: using cspell config")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strip the version number from output so snapshots survive version bumps.
fn strip_version(text: &str) -> String {
    // Replace "matchum X.Y.Z" with "matchum [VERSION]"
    let re = regex::Regex::new(r"matchum \d+\.\d+\.\d+").unwrap();
    re.replace_all(text, "matchum [VERSION]").to_string()
}

// ── tests: text format ─────────────────────────────────────────────

#[test]
fn cli_check_text_with_issues() {
    let (stdout, stderr, code) = run_matchum(&[
        "check",
        "--config",
        "tests/fixtures/cspell.json",
        "tests/fixtures/sample.txt",
    ]);

    assert_eq!(code, 0);
    insta::assert_snapshot!("text_stdout", normalize(&stdout));
    insta::assert_snapshot!("text_stderr", normalize_stderr(&stderr));
}

#[test]
fn cli_check_text_no_issues() {
    let (stdout, stderr, code) = run_matchum(&[
        "check",
        "--config",
        "tests/fixtures/cspell.json",
        "tests/fixtures/clean_sample.txt",
    ]);

    assert_eq!(code, 0);
    // stdout should be empty when there are no issues
    assert!(stdout.is_empty(), "expected empty stdout, got: {stdout:?}");
    insta::assert_snapshot!("no_issues_stderr", normalize_stderr(&stderr));
}

// ── tests: json format ─────────────────────────────────────────────

#[test]
fn cli_check_json_with_issues() {
    let (stdout, _stderr, code) = run_matchum(&[
        "check",
        "--config",
        "tests/fixtures/cspell.json",
        "--format",
        "json",
        "tests/fixtures/sample.txt",
    ]);

    assert_eq!(code, 0);
    // Normalize the path inside the JSON before snapshotting
    insta::assert_snapshot!("json_stdout", normalize(&stdout));
}

// ── tests: github format ───────────────────────────────────────────

#[test]
fn cli_check_github_format() {
    let (stdout, _stderr, code) = run_matchum(&[
        "check",
        "--config",
        "tests/fixtures/cspell.json",
        "--format",
        "github",
        "tests/fixtures/sample.txt",
    ]);

    assert_eq!(code, 0);
    insta::assert_snapshot!("github_stdout", normalize(&stdout));
}

// ── tests: words-only format ───────────────────────────────────────

#[test]
fn cli_check_words_only_format() {
    let (stdout, _stderr, code) = run_matchum(&[
        "check",
        "--config",
        "tests/fixtures/cspell.json",
        "--format",
        "words-only",
        "tests/fixtures/sample.txt",
    ]);

    assert_eq!(code, 0);
    insta::assert_snapshot!("words_only_stdout", normalize(&stdout));
}

// ── tests: error cases ─────────────────────────────────────────────

#[test]
fn cli_check_nonexistent_file() {
    let (stdout, stderr, code) = run_matchum(&[
        "check",
        "--config",
        "tests/fixtures/cspell.json",
        "tests/fixtures/this_file_does_not_exist.txt",
    ]);

    // No files found is not an error exit
    insta::assert_snapshot!("nonexistent_file_stdout", normalize(&stdout));
    insta::assert_snapshot!("nonexistent_file_stderr", normalize_stderr(&stderr));
    assert_eq!(code, 0);
}

#[test]
fn cli_check_nonexistent_config() {
    let (_stdout, stderr, code) = run_matchum(&[
        "check",
        "--config",
        "tests/fixtures/no_such_config.json",
        "tests/fixtures/sample.txt",
    ]);

    assert_eq!(code, 2, "non-existent config should exit with code 2");
    insta::assert_snapshot!("nonexistent_config_stderr", normalize_stderr(&stderr));
}

// ── tests: strict mode ─────────────────────────────────────────────

#[test]
fn cli_check_strict_with_issues() {
    let (_stdout, _stderr, code) = run_matchum(&[
        "check",
        "--config",
        "tests/fixtures/cspell.json",
        "--strict",
        "tests/fixtures/sample.txt",
    ]);

    assert_eq!(code, 1, "--strict should exit 1 when issues are found");
}

#[test]
fn cli_check_strict_no_issues() {
    let (_stdout, _stderr, code) = run_matchum(&[
        "check",
        "--config",
        "tests/fixtures/cspell.json",
        "--strict",
        "tests/fixtures/clean_sample.txt",
    ]);

    assert_eq!(code, 0, "--strict should exit 0 when no issues found");
}

// ── tests: suggestions flag ────────────────────────────────────────

#[test]
fn cli_check_text_with_suggestions() {
    let (stdout, _stderr, code) = run_matchum(&[
        "check",
        "--config",
        "tests/fixtures/cspell.json",
        "--suggestions",
        "tests/fixtures/sample.txt",
    ]);

    assert_eq!(code, 0);
    insta::assert_snapshot!("suggestions_stdout", normalize(&stdout));
}

// ── tests: trace command ──────────────────────────────────────────

#[test]
fn cli_trace_valid_word() {
    let (stdout, _stderr, code) = run_matchum(&[
        "trace",
        "--config",
        "tests/fixtures/cspell-trace.json",
        "customword",
    ]);

    assert_eq!(code, 0);
    insta::assert_snapshot!("trace_valid_word_stdout", stdout);
}

#[test]
fn cli_trace_unknown_word() {
    let (stdout, _stderr, code) = run_matchum(&[
        "trace",
        "--config",
        "tests/fixtures/cspell-trace.json",
        "xyznotaword",
    ]);

    assert_eq!(code, 0);
    insta::assert_snapshot!("trace_unknown_word_stdout", stdout);
}

#[test]
fn cli_trace_with_config() {
    let (stdout, _stderr, code) = run_matchum(&[
        "trace",
        "--config",
        "tests/fixtures/cspell-trace.json",
        "ignoredword",
    ]);

    assert_eq!(code, 0);
    insta::assert_snapshot!("trace_ignored_word_stdout", stdout);
}

// ── tests: init command ───────────────────────────────────────────

#[test]
fn cli_init_creates_config() {
    let tmpdir = std::env::temp_dir().join(format!("matchum_init_test_{}", std::process::id()));
    std::fs::create_dir_all(&tmpdir).unwrap();

    let (stdout, stderr, code) = run_matchum_in(&tmpdir, &["init"]);

    assert_eq!(code, 0, "init should exit 0");
    let config_path = tmpdir.join("matchum.toml");
    assert!(config_path.exists(), "matchum.toml should be created");

    let content = std::fs::read_to_string(&config_path).unwrap();
    insta::assert_snapshot!("init_config_content", content);
    insta::assert_snapshot!("init_stdout", stdout);
    insta::assert_snapshot!("init_stderr", stderr);

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmpdir);
}

#[test]
fn cli_init_already_exists() {
    let tmpdir = std::env::temp_dir().join(format!(
        "matchum_init_exists_test_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&tmpdir).unwrap();
    std::fs::write(tmpdir.join("matchum.toml"), "# existing").unwrap();

    let (_stdout, stderr, code) = run_matchum_in(&tmpdir, &["init"]);

    assert_eq!(code, 2, "init should fail when matchum.toml already exists");
    insta::assert_snapshot!("init_already_exists_stderr", stderr);

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmpdir);
}

// ── tests: lint command ───────────────────────────────────────────

#[test]
fn cli_lint_with_issues() {
    let (stdout, stderr, code) = run_matchum(&[
        "lint",
        "--config",
        "tests/fixtures/cspell.json",
        "tests/fixtures/sample.txt",
    ]);

    assert_eq!(code, 1, "lint should exit 1 when issues are found");
    insta::assert_snapshot!("lint_with_issues_stdout", normalize(&stdout));
    insta::assert_snapshot!("lint_with_issues_stderr", normalize_stderr(&stderr));
}

#[test]
fn cli_lint_no_issues() {
    let (_stdout, _stderr, code) = run_matchum(&[
        "lint",
        "--config",
        "tests/fixtures/cspell.json",
        "tests/fixtures/clean_sample.txt",
    ]);

    assert_eq!(code, 0, "lint should exit 0 when no issues found");
}

// ── tests: dict command ───────────────────────────────────────────

#[test]
fn cli_dict_list_empty() {
    let tmpdir = std::env::temp_dir().join(format!(
        "matchum_dict_list_test_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&tmpdir).unwrap();

    let (stdout, _stderr, code) = run_matchum_in(&tmpdir, &["dict", "list"]);

    assert_eq!(code, 0);
    insta::assert_snapshot!("dict_list_empty_stdout", stdout);

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmpdir);
}

// ── tests: help and version ───────────────────────────────────────

#[test]
fn cli_help_output() {
    let (stdout, _stderr, code) = run_matchum(&["--help"]);

    assert_eq!(code, 0);
    insta::assert_snapshot!("help_stdout", stdout);
}

#[test]
fn cli_version_output() {
    let (stdout, _stderr, code) = run_matchum(&["--version"]);

    assert_eq!(code, 0);
    insta::assert_snapshot!("version_stdout", strip_version(&stdout));
}

#[test]
fn cli_check_help() {
    let (stdout, _stderr, code) = run_matchum(&["check", "--help"]);

    assert_eq!(code, 0);
    insta::assert_snapshot!("check_help_stdout", stdout);
}

#[test]
fn cli_trace_help() {
    let (stdout, _stderr, code) = run_matchum(&["trace", "--help"]);

    assert_eq!(code, 0);
    insta::assert_snapshot!("trace_help_stdout", stdout);
}

#[test]
fn cli_lint_help() {
    let (stdout, _stderr, code) = run_matchum(&["lint", "--help"]);

    assert_eq!(code, 0);
    insta::assert_snapshot!("lint_help_stdout", stdout);
}

#[test]
fn cli_init_help() {
    let (stdout, _stderr, code) = run_matchum(&["init", "--help"]);

    assert_eq!(code, 0);
    insta::assert_snapshot!("init_help_stdout", stdout);
}

#[test]
fn cli_add_help() {
    let (stdout, _stderr, code) = run_matchum(&["add", "--help"]);

    assert_eq!(code, 0);
    insta::assert_snapshot!("add_help_stdout", stdout);
}

#[test]
fn cli_dict_help() {
    let (stdout, _stderr, code) = run_matchum(&["dict", "--help"]);

    assert_eq!(code, 0);
    insta::assert_snapshot!("dict_help_stdout", stdout);
}

#[test]
fn cli_cspell_help() {
    let (stdout, _stderr, code) = run_matchum(&["cspell", "--help"]);

    assert_eq!(code, 0);
    insta::assert_snapshot!("cspell_help_stdout", stdout);
}

#[test]
fn cli_review_help() {
    let (stdout, _stderr, code) = run_matchum(&["review", "--help"]);

    assert_eq!(code, 0);
    insta::assert_snapshot!("review_help_stdout", stdout);
}

// ── tests: error cases (additional) ───────────────────────────────

#[test]
fn cli_add_no_words() {
    let (_stdout, stderr, code) = run_matchum(&["add"]);

    assert_eq!(code, 2, "add with no words should exit 2");
    insta::assert_snapshot!("add_no_words_stderr", stderr);
}

#[test]
fn cli_trace_nonexistent_config() {
    let (_stdout, stderr, code) = run_matchum(&[
        "trace",
        "--config",
        "tests/fixtures/no_such_config.json",
        "hello",
    ]);

    assert_eq!(code, 2, "trace with non-existent config should exit 2");
    insta::assert_snapshot!("trace_nonexistent_config_stderr", normalize(&stderr));
}

#[test]
fn cli_no_subcommand() {
    let (_stdout, stderr, code) = run_matchum(&[]);

    assert_ne!(code, 0, "no subcommand should fail");
    insta::assert_snapshot!("no_subcommand_stderr", stderr);
}
