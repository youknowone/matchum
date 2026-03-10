//! Benchmark: run matchum and cspell on all multi-repo test repositories
//! and compare wall-clock performance.
//!
//! Usage:
//!   cargo test --release --test bench_repos -- --ignored --nocapture
//!
//! Prerequisites:
//!   - `cargo build --release -p matchum-cli` (matchum binary)
//!   - `npx cspell --version` (cspell available via npx)
//!   - Repos cloned via: `cargo test --test multi_repo_compat generate_all_snapshots -- --ignored`

mod multi_repo;

use multi_repo::harness::{ensure_cloned, RepoSpec};
use multi_repo::repos::REPOS;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

/// Number of runs per tool per repo for averaging.
const RUNS: usize = 3;

struct BenchResult {
    name: &'static str,
    matchum: Duration,
    cspell: Duration,
    matchum_issues: usize,
    cspell_issues: usize,
}

fn matchum_binary() -> std::path::PathBuf {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    ["target/release/matchum", "target/debug/matchum"]
        .iter()
        .map(|p| root.join(p))
        .find(|p| p.exists())
        .expect("matchum binary not found. Build with: cargo build --release -p matchum")
}

fn build_matchum_cmd(binary: &Path, repo_dir: &Path, spec: &RepoSpec) -> Command {
    let mut cmd = Command::new(binary);
    // Use `cspell lint` subcommand for cspell-compatible behavior
    cmd.args(["cspell", "lint", "--no-progress", "--no-summary"]);

    if let Some(config) = spec.cspell_config {
        let config_path = repo_dir.join(config);
        if config_path.exists() {
            cmd.arg("--config").arg(&config_path);
        }
    }

    for arg in spec.args {
        cmd.arg(arg);
    }

    // Pass the same glob patterns as cspell
    for path in spec.check_paths {
        cmd.arg(path);
    }

    cmd.current_dir(repo_dir);
    cmd
}

fn build_cspell_cmd(repo_dir: &Path, spec: &RepoSpec) -> Command {
    let mut cmd = Command::new("npx");
    cmd.args(["cspell", "lint", "--no-progress", "--no-summary", "--relative"]);

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
    cmd
}

fn time_matchum(repo_dir: &Path, spec: &RepoSpec) -> Duration {
    let binary = matchum_binary();
    let mut best = Duration::from_secs(u64::MAX);

    for _ in 0..RUNS {
        let mut cmd = build_matchum_cmd(&binary, repo_dir, spec);
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());

        let start = Instant::now();
        let _ = cmd.status().expect("failed to run matchum");
        let elapsed = start.elapsed();

        if elapsed < best {
            best = elapsed;
        }
    }
    best
}

fn time_cspell(repo_dir: &Path, spec: &RepoSpec) -> Duration {
    let mut best = Duration::from_secs(u64::MAX);

    for _ in 0..RUNS {
        let mut cmd = build_cspell_cmd(repo_dir, spec);
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());

        let start = Instant::now();
        let _ = cmd.status().expect("failed to run npx cspell");
        let elapsed = start.elapsed();

        if elapsed < best {
            best = elapsed;
        }
    }
    best
}

/// Count issues found by each tool (single run) to verify they actually work.
fn count_issues(repo_dir: &Path, spec: &RepoSpec) -> (usize, usize) {
    let binary = matchum_binary();

    let mut mcmd = build_matchum_cmd(&binary, repo_dir, spec);
    mcmd.stderr(std::process::Stdio::null());
    let mout = mcmd.output().expect("failed to run matchum");
    let matchum_count = String::from_utf8_lossy(&mout.stdout)
        .lines()
        .filter(|l| l.contains(" - Unknown word (") || l.contains(" - Forbidden word ("))
        .count();

    let mut ccmd = build_cspell_cmd(repo_dir, spec);
    ccmd.stderr(std::process::Stdio::null());
    let cout = ccmd.output().expect("failed to run cspell");
    let cspell_count = String::from_utf8_lossy(&cout.stdout)
        .lines()
        .filter(|l| l.contains(" - Unknown word (") || l.contains(" - Forbidden word ("))
        .count();

    (matchum_count, cspell_count)
}

fn format_duration(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", d.as_secs_f64())
    }
}

#[test]
#[ignore]
fn bench_all_repos() {
    eprintln!("\n=== matchum vs cspell benchmark ({RUNS} runs, best-of) ===\n");

    // Ensure repos are cloned first
    let repo_dirs: Vec<_> = REPOS
        .iter()
        .map(|spec| (spec, ensure_cloned(spec)))
        .collect();

    let mut results: Vec<BenchResult> = Vec::new();

    for (spec, repo_dir) in &repo_dirs {
        eprint!("  {:<35}", spec.name);

        // First: verify both tools actually find issues
        let (m_issues, c_issues) = count_issues(repo_dir, spec);
        eprint!("issues(m/c): {:>6}/{:<6}", m_issues, c_issues);

        let matchum_time = time_matchum(repo_dir, spec);
        eprint!("matchum: {:>8}", format_duration(matchum_time));

        let cspell_time = time_cspell(repo_dir, spec);
        let speedup = cspell_time.as_secs_f64() / matchum_time.as_secs_f64();
        eprintln!(
            "  cspell: {:>8}  speedup: {:.1}x",
            format_duration(cspell_time),
            speedup,
        );

        results.push(BenchResult {
            name: spec.name,
            matchum: matchum_time,
            cspell: cspell_time,
            matchum_issues: m_issues,
            cspell_issues: c_issues,
        });
    }

    // Summary table
    let total_matchum: Duration = results.iter().map(|r| r.matchum).sum();
    let total_cspell: Duration = results.iter().map(|r| r.cspell).sum();
    let total_speedup = total_cspell.as_secs_f64() / total_matchum.as_secs_f64();

    eprintln!("\n{}", "=".repeat(100));
    eprintln!(
        "  {:<35} {:>10} {:>10} {:>10} {:>8} {:>8}",
        "REPO", "MATCHUM", "CSPELL", "SPEEDUP", "M_ISSUE", "C_ISSUE"
    );
    eprintln!("{}", "-".repeat(100));

    for r in &results {
        let speedup = r.cspell.as_secs_f64() / r.matchum.as_secs_f64();
        eprintln!(
            "  {:<35} {:>10} {:>10} {:>9.1}x {:>8} {:>8}",
            r.name,
            format_duration(r.matchum),
            format_duration(r.cspell),
            speedup,
            r.matchum_issues,
            r.cspell_issues,
        );
    }

    let total_m_issues: usize = results.iter().map(|r| r.matchum_issues).sum();
    let total_c_issues: usize = results.iter().map(|r| r.cspell_issues).sum();

    eprintln!("{}", "-".repeat(100));
    eprintln!(
        "  {:<35} {:>10} {:>10} {:>9.1}x {:>8} {:>8}",
        "TOTAL",
        format_duration(total_matchum),
        format_duration(total_cspell),
        total_speedup,
        total_m_issues,
        total_c_issues,
    );
    eprintln!("{}", "=".repeat(100));

    // Also find fastest/slowest speedups
    if let Some(best) = results
        .iter()
        .max_by(|a, b| {
            let sa = a.cspell.as_secs_f64() / a.matchum.as_secs_f64();
            let sb = b.cspell.as_secs_f64() / b.matchum.as_secs_f64();
            sa.partial_cmp(&sb).unwrap()
        })
    {
        let s = best.cspell.as_secs_f64() / best.matchum.as_secs_f64();
        eprintln!("  Best speedup:  {} ({:.1}x)", best.name, s);
    }
    if let Some(worst) = results
        .iter()
        .min_by(|a, b| {
            let sa = a.cspell.as_secs_f64() / a.matchum.as_secs_f64();
            let sb = b.cspell.as_secs_f64() / b.matchum.as_secs_f64();
            sa.partial_cmp(&sb).unwrap()
        })
    {
        let s = worst.cspell.as_secs_f64() / worst.matchum.as_secs_f64();
        eprintln!("  Worst speedup: {} ({:.1}x)", worst.name, s);
    }

    eprintln!();
}

/// Bench a single repo by name. Usage:
///   cargo test --release --test bench_repos bench_single -- --ignored --nocapture REPO_NAME
#[test]
#[ignore]
fn bench_single() {
    let target = std::env::var("BENCH_REPO").unwrap_or_default();
    if target.is_empty() {
        eprintln!("Set BENCH_REPO=<name> to bench a single repo. Example:");
        eprintln!("  BENCH_REPO=rustpython cargo test --release --test bench_repos bench_single -- --ignored --nocapture");
        return;
    }

    let spec = REPOS
        .iter()
        .find(|s| s.name == target)
        .unwrap_or_else(|| panic!("Unknown repo: {target}"));

    let repo_dir = ensure_cloned(spec);

    eprintln!("\n=== Benchmarking: {} ({RUNS} runs, best-of) ===\n", spec.name);

    let (m_issues, c_issues) = count_issues(&repo_dir, spec);
    eprintln!("  matchum issues: {}", m_issues);
    eprintln!("  cspell issues:  {}", c_issues);

    let matchum_time = time_matchum(&repo_dir, spec);
    let cspell_time = time_cspell(&repo_dir, spec);
    let speedup = cspell_time.as_secs_f64() / matchum_time.as_secs_f64();

    eprintln!("  matchum: {}", format_duration(matchum_time));
    eprintln!("  cspell:  {}", format_duration(cspell_time));
    eprintln!("  speedup: {:.1}x", speedup);
    eprintln!();
}
