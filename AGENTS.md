# AGENTS.md

## Product Direction

`matchum` is a performance-first native spell checker.

- Default command surface (`matchum ...`) is optimized for speed and Rust-native UX.
- Full cspell CLI argument compatibility must be isolated under:
  - `matchum cspell ...`
- `matchum cspell` is the drop-in replacement layer for cspell workflows.

## CLI Strategy

1. Native mode (fast path)
- Keep native subcommands focused, minimal, and performance-oriented.
- Do not force cspell argument/behavior parity into native commands.

2. Compatibility mode (`matchum cspell`)
- Implement cspell-compatible command tree and flags here.
- Prioritize behavior parity over internal purity in this namespace.
- Compatibility shims and translation logic should stay in this layer.

## Engineering Constraints

- Avoid regressions to native mode startup and check performance.
- Compatibility code must not leak complexity into native fast path.
- Add targeted tests for both paths:
  - native behavior tests
  - compatibility parity tests (`matchum cspell ...` vs `cspell ...`)

## Execution Plan (High Level)

1. Introduce `cspell` subcommand namespace
- Add `matchum cspell` with dedicated parser and dispatch.
- Keep existing native commands unchanged.

2. Move compatibility flags into `matchum cspell`
- Rehome cspell-style arguments from native commands into this namespace.
- Leave native commands with performance-first option set.

3. Implement compatibility translator
- Translate cspell options to internal engine request structs.
- Keep translation logic separate from core validator/dictionary engine.

4. Reach drop-in parity iteratively
- Start with `lint/check/trace`, then `suggestions/dictionaries/init/link` as needed.
- Continuously validate against cspell fixtures and binary compat tests.

5. Stabilize and document
- Maintain a parity matrix in README.
- Document known differences and migration guidance.
