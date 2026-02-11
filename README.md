# ruspell

Performance-first Rust spell checker.

## Vision

`ruspell` has two explicit operating modes:

1. Native mode: `ruspell ...`
- Optimized for speed, low overhead, and Rust-native ergonomics.
- Not required to mirror cspell CLI surface 1:1.

2. Compatibility mode: `ruspell cspell ...`
- Dedicated cspell drop-in replacement layer.
- This is where cspell argument and behavior parity belongs.

## Why split modes?

Trying to preserve full cspell compatibility directly in native commands adds parser complexity,
runtime branching, and maintenance burden that can hurt performance goals.

By isolating compatibility in `ruspell cspell`, we keep:
- native path fast and clean
- compatibility path predictable for existing cspell users

## Planned Work

1. Add `ruspell cspell` command namespace
- separate command parser/dispatch for compatibility

2. Move cspell-style flags to compatibility namespace
- keep native command options minimal and performance-oriented

3. Build compatibility translation layer
- map cspell options to internal request model

4. Achieve drop-in parity incrementally
- prioritize `lint`, `check`, `trace`
- then `suggestions`, `dictionaries`, and config-related workflows

5. Validation and release gating
- parity tests against cspell fixtures
- binary comparison tests in CI for compatibility namespace
- performance benchmarks for native namespace

## Current Policy

- Default `ruspell` CLI: performance first.
- cspell parity: `ruspell cspell` only.
