# matchum

A next-generation spell checker, inspired by [cspell](https://cspell.org/).

## Overview

matchum is a high-performance spell checker written in Rust. It draws heavy inspiration from cspell's ecosystem — configuration format, dictionary format, inline directives, and word splitting rules — while leveraging Rust's performance characteristics to deliver significantly faster execution.

Currently, `matchum cspell` is the primary interface. It provides **100% compatibility with cspell**, serving as a drop-in replacement that reads the same config files, uses the same dictionaries, and produces equivalent results.

Beyond cspell compatibility, matchum will offer its own set of spell-checking features with a streamlined, performance-oriented interface. For now, consider `matchum cspell` to be the only production-ready feature.

## Installation

```
cargo install matchum
```

## Usage

### `matchum cspell` — cspell-compatible interface

Drop-in replacement for cspell. Supports the full cspell CLI surface:

```bash
# Lint files (strict mode)
matchum cspell lint .

# Check files
matchum cspell check src/

# Trace a word through dictionaries
matchum cspell trace "misspelled"

# Get spelling suggestions
matchum cspell suggestions "wrods"

# List available dictionaries
matchum cspell dictionaries

# Initialize config
matchum cspell init
```

All cspell flags are supported: `--config`, `--locale`, `--unique`, `--no-progress`, `--dot`, `--fail-fast`, etc.

### `matchum` — native interface

Preparing...

The native interface provides the same core functionality with a cleaner command set:

```bash
matchum check .

```

### `cargo matchum` — Rust crate spell checking

`cargo-matchum` runs matchum on valid Rust source files within a crate, integrating spell checking into the Cargo workflow:

```bash
cargo matchum
```

## Architecture

Workspace with 5 crates:

| Crate | Role |
|---|---|
| `matchum` | CLI entry point, `check`/`lint`/`trace`/`review`/`add` commands |
| `matchum-core` | Word splitting, validation pipeline |
| `matchum-dict` | Dictionary loading (trie v3, txt) and lookup |
| `matchum-config` | cspell.json parsing, config resolution, inline directives |
| `cargo-matchum` | Cargo subcommand for Rust projects |

## Performance

matchum uses several techniques to achieve high throughput:

- **mimalloc** — Global allocator (default), ~13% average improvement over system allocator
- **Parallel file processing** — `rayon` for file-level parallelism
- **Parallel directory walking** — `ignore::WalkBuilder::build_parallel()` with up to 12 threads
- **Clean-only caching** — Only cache files with zero issues
- **Memory-mapped I/O** — mmap for large file reading
- **O(1) context line lookup** — Pre-computed line offset arrays

## Build

```bash
cargo build --release -p matchum
cargo build --release -p cargo-matchum
cargo test --workspace
```
