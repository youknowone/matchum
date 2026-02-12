# ruspell

High-performance Rust spell checker, cspell-compatible.

## Architecture

Workspace with 4 crates:

- `crates/ruspell-core/` — Word splitting (`splitter.rs`), validation pipeline (`validator.rs`)
- `crates/ruspell-dict/` — Dictionary loading (trie_v3, txt) and lookup (`hashdict.rs`)
- `crates/ruspell-config/` — cspell.json parsing, config resolution, inline directives, npm dictionary fetching
- `crates/ruspell-cli/` — CLI entry point. `check` command drives the main pipeline

Integration tests live in `tests/` at workspace root (10 test files, ~460 tests total).

## Build / Test / Run

```
cargo build --release -p ruspell-cli
cargo test --workspace
target/release/ruspell check --config .cspell.json <path>
```

## Key Dependencies

- `hashbrown` + `foldhash` — Fast hash maps/sets for dictionary storage and lookup
- `compact_str` — Inline string storage (avoids heap allocation for words <= 24 bytes)
- `rayon` — Parallel file processing in `check` command
- `aho-corasick` — Multi-pattern prefilter for skip patterns (URLs, emails, hex, etc.)
- `regex` — Word boundary detection, text splitting
- `memmap2` — Memory-mapped file reading
- `globset` — Glob pattern matching for overrides and file filtering

## Code Style

- Error types defined with `thiserror`
- `.unwrap()` only in tests
- Parallel processing with `rayon`

## Performance Architecture

### Word Validation Cache

`Validator` has an optional `WordCache` (`Arc<RwLock<HashMap<CompactString, bool>>>`) shared
across files with the same dictionary set. Cache is keyed per `language_id` (derived from file
extension) to ensure correctness when `languageSettings` activates different dictionaries per
file type.

Cache is bypassed when inline directives override dictionaries, compound words, or case sensitivity.

### Skip Pattern Prefilter

Aho-Corasick automaton scans text for literal anchors (e.g., `://`, `@`, `0x`) before running
expensive regex patterns. Only triggered regexes are evaluated.

### Hot Path

`Validator::validate_text` → `extract_words_into` (regex word splitting) →
`split_camel_case_into` → `is_word_valid` (cache check → dictionary lookup chain →
compound decomposition → case fallbacks).

## cspell Compatibility

- Same config format (cspell.json / cspell.jsonc)
- Same dictionary format (txt, trie v3)
- Same inline directives (`cspell:ignore`, `cspell:words`, `cspell:disable`, etc.)
- Same word splitting behavior
- 77/77 issue parity on RustPython `crates/` benchmark
