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

fn workspace_root() -> PathBuf {
    project_root()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn matchum_binary() -> PathBuf {
    let root = workspace_root();
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
    let tmpdir =
        std::env::temp_dir().join(format!("matchum_init_exists_test_{}", std::process::id()));
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
    let tmpdir =
        std::env::temp_dir().join(format!("matchum_dict_list_test_{}", std::process::id()));
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

// ── tests: review command ─────────────────────────────────────────

/// Run matchum with piped stdin (for review command that reads from stdin).
fn run_matchum_with_stdin(args: &[&str], stdin_data: &[u8]) -> (String, String, i32) {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(matchum_binary())
        .args(args)
        .current_dir(project_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn matchum");

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(stdin_data);
    }

    let output = child
        .wait_with_output()
        .expect("failed to wait for matchum");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);
    (stdout, stderr, code)
}

/// Run matchum in a specific directory with piped stdin.
fn run_matchum_in_with_stdin(
    dir: &std::path::Path,
    args: &[&str],
    stdin_data: &[u8],
) -> (String, String, i32) {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(matchum_binary())
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn matchum");

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(stdin_data);
    }

    let output = child
        .wait_with_output()
        .expect("failed to wait for matchum");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);
    (stdout, stderr, code)
}

#[test]
fn cli_review_shows_relative_paths() {
    // When running `matchum review` with a fixture path, the output should
    // contain relative paths, not absolute paths.
    let (stdout, _stderr, _code) = run_matchum_with_stdin(
        &[
            "review",
            "--config",
            "tests/fixtures/cspell.json",
            "tests/fixtures/sample.txt",
        ],
        b"q\n",
    );

    // Output should contain the relative path
    assert!(
        stdout.contains("tests/fixtures/sample.txt"),
        "output should contain relative path: {stdout}"
    );
    // Output should NOT contain an absolute path
    let root = project_root().to_string_lossy().to_string();
    assert!(
        !stdout.contains(&root),
        "output should not contain absolute root path: {stdout}"
    );
}

#[test]
fn cli_review_absolute_input_shows_relative_paths() {
    // When the input path is absolute (as cargo-matchum does), the output
    // should still show relative paths.
    let root = project_root();
    let abs_fixture = root.join("tests/fixtures/sample.txt");

    let (stdout, _stderr, _code) = run_matchum_with_stdin(
        &[
            "review",
            "--config",
            "tests/fixtures/cspell.json",
            &abs_fixture.to_string_lossy(),
        ],
        b"q\n",
    );

    // Even with absolute input, the display should be relative
    // (at minimum, it should not show the full absolute path)
    let root_str = root.to_string_lossy().to_string();
    assert!(
        !stdout.contains(&root_str),
        "absolute input should not produce absolute path in output: {stdout}"
    );
}

#[test]
fn cli_review_dir_input_shows_relative_paths() {
    // When running review on a directory (which is what cargo-matchum does),
    // paths should be relative to that directory.
    let root = project_root();
    let fixtures_dir = root.join("tests/fixtures");

    let (stdout, _stderr, _code) = run_matchum_in_with_stdin(
        &root,
        &[
            "review",
            "--config",
            "tests/fixtures/cspell.json",
            &fixtures_dir.to_string_lossy(),
        ],
        b"q\n",
    );

    // Should show paths relative to the fixtures dir or the project root
    let fixtures_str = fixtures_dir.to_string_lossy().to_string();
    assert!(
        !stdout.contains(&fixtures_str),
        "dir input should not produce absolute path in output: {stdout}"
    );
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

// ── tests: add command (actual functionality) ─────────────────────

#[test]
fn cli_add_words_to_project() {
    let tmpdir = std::env::temp_dir().join(format!("matchum_add_test_{}", std::process::id()));
    std::fs::create_dir_all(&tmpdir).unwrap();

    // Create matchum.toml
    std::fs::write(tmpdir.join("matchum.toml"), "language = \"en\"\n").unwrap();

    // Add words
    let (_stdout, stderr, code) = run_matchum_in(&tmpdir, &["add", "myword", "anotherword"]);
    assert_eq!(code, 0, "add should succeed");
    assert!(
        stderr.contains("Added 2 word"),
        "stderr should confirm: {stderr}"
    );

    // Verify the words file was created
    let words_path = tmpdir.join(".matchum/words.txt");
    assert!(words_path.exists(), ".matchum/words.txt should exist");
    let content = std::fs::read_to_string(&words_path).unwrap();
    assert!(content.contains("myword"));
    assert!(content.contains("anotherword"));

    // Add duplicate — should not re-add
    let (_stdout, stderr2, code2) = run_matchum_in(&tmpdir, &["add", "myword"]);
    assert_eq!(code2, 0);
    assert!(
        stderr2.contains("already present"),
        "duplicate should report already present: {stderr2}"
    );

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmpdir);
}

#[test]
fn cli_add_forbidden_words() {
    let tmpdir =
        std::env::temp_dir().join(format!("matchum_add_forbid_test_{}", std::process::id()));
    std::fs::create_dir_all(&tmpdir).unwrap();

    std::fs::write(tmpdir.join("matchum.toml"), "language = \"en\"\n").unwrap();

    let (_stdout, stderr, code) = run_matchum_in(&tmpdir, &["add", "--forbidden", "badword"]);
    assert_eq!(code, 0, "add --forbidden should succeed");
    assert!(
        stderr.contains("Added 1 word"),
        "stderr should confirm addition: {stderr}"
    );

    let flagged_path = tmpdir.join(".matchum/flagged.txt");
    assert!(flagged_path.exists(), "flagged.txt should exist");
    let content = std::fs::read_to_string(&flagged_path).unwrap();
    assert!(content.contains("badword"));

    let _ = std::fs::remove_dir_all(&tmpdir);
}

#[test]
fn cli_add_no_config_fails() {
    let tmpdir =
        std::env::temp_dir().join(format!("matchum_add_noconfig_test_{}", std::process::id()));
    std::fs::create_dir_all(&tmpdir).unwrap();

    let (_stdout, stderr, code) = run_matchum_in(&tmpdir, &["add", "someword"]);
    assert_eq!(code, 2, "add without config should fail");
    assert!(
        stderr.contains("matchum.toml") || stderr.contains("matchum init"),
        "should mention matchum.toml: {stderr}"
    );

    let _ = std::fs::remove_dir_all(&tmpdir);
}

// ── tests: cspell subcommand (actual functionality) ───────────────

#[test]
fn cli_cspell_lint_with_issues() {
    let (stdout, _stderr, code) = run_matchum(&[
        "cspell",
        "lint",
        "--no-default-configuration",
        "--config",
        "tests/fixtures/cspell.json",
        "tests/fixtures/sample.txt",
    ]);

    // cspell lint should find issues and exit 1
    assert_eq!(code, 1, "cspell lint should exit 1 with issues");
    assert!(!stdout.is_empty(), "stdout should contain issues");
}

#[test]
fn cli_cspell_lint_no_issues() {
    let (_stdout, _stderr, code) = run_matchum(&[
        "cspell",
        "lint",
        "--no-default-configuration",
        "--config",
        "tests/fixtures/cspell.json",
        "tests/fixtures/clean_sample.txt",
    ]);

    assert_eq!(code, 0, "cspell lint should exit 0 with no issues");
}

#[test]
fn cli_cspell_lint_words_only() {
    let (stdout, _stderr, code) = run_matchum(&[
        "cspell",
        "lint",
        "--no-default-configuration",
        "--config",
        "tests/fixtures/cspell.json",
        "--words-only",
        "tests/fixtures/sample.txt",
    ]);

    assert_eq!(code, 1);
    // words-only mode should output just words, not full diagnostic lines
    for line in stdout.lines() {
        if !line.is_empty() {
            assert!(
                !line.contains(':'),
                "words-only should not contain path separators: {line}"
            );
        }
    }
}

#[test]
fn cli_cspell_lint_unique() {
    let (stdout, _stderr, _code) = run_matchum(&[
        "cspell",
        "lint",
        "--no-default-configuration",
        "--config",
        "tests/fixtures/cspell.json",
        "--unique",
        "--words-only",
        "tests/fixtures/sample.txt",
    ]);

    // Unique should not have duplicate words
    let words: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    let unique_words: std::collections::HashSet<&str> = words.iter().copied().collect();
    assert_eq!(
        words.len(),
        unique_words.len(),
        "unique mode should not produce duplicates"
    );
}

#[test]
fn cli_cspell_check_file() {
    let (stdout, _stderr, code) = run_matchum(&[
        "cspell",
        "check",
        "--no-default-configuration",
        "--config",
        "tests/fixtures/cspell.json",
        "tests/fixtures/sample.txt",
    ]);

    // cspell check shows the full file content with highlights
    assert_eq!(code, 1, "cspell check should exit 1 with issues");
    assert!(!stdout.is_empty(), "stdout should have output");
}

#[test]
fn cli_cspell_trace_word() {
    let (stdout, _stderr, code) = run_matchum(&[
        "cspell",
        "trace",
        "--no-default-configuration",
        "--config",
        "tests/fixtures/cspell-trace.json",
        "customword",
    ]);

    assert_eq!(code, 0);
    // Should indicate the word was found
    assert!(
        stdout.contains("Found") || stdout.contains("found") || stdout.contains("customword"),
        "trace should mention the word: {stdout}"
    );
}
