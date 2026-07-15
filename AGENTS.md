# AGENTS.md

Guidance for AI agents and contributors working in this repository.

Vibra is a YAML-surface, statically typed language that compiles to WebAssembly.
The compiler/runtime is written in Rust (`src/`); the language and standard
library live in `stdlib/`, `examples/`, and `schemas/`.

## Build

```sh
cargo build
```

## Test suites — run BOTH before committing or opening a PR

This project has **two** distinct test suites. A change is not "done" until both
pass. Always run them and confirm the output before claiming success.

### 1. Rust suite (`cargo test`)

Unit and integration tests for compiler, lowering, runtime, runner, and CLI
mechanics. Coverage lives in focused Rust integration targets under `tests/`.

```sh
cargo test
```

### 2. Vibra-language suite (`vibra test`)

Tests written **in Vibra itself**, under `tests/*.vibra`, exercising language
features and the standard library through the built-in runner. See
[`tests/README.md`](tests/README.md) for conventions (flat layout, grant-free
tests, typed assertions, profiles, and capability-gated commands).

```sh
cargo run -- test            # from the repo root
# or, after `cargo install --path .`:
vibra test
```

Useful flags: `--filter <name>`, `--profile <name>`, `--tag <tag>`,
`--jobs <n>`, `--fail-fast`, and `--report yaml --report-file report.yaml`.
Profiles select tests; they never grant capabilities.

## When you add or change behavior

- Changing the **compiler/runtime** (`src/`): add/adjust a focused Rust test
  under `tests/` and re-run `cargo test`.
- Changing the **language surface or `stdlib/`**: add/adjust a `tests/*.vibra`
  case and re-run `vibra test`. New stdlib modules should get a matching
  `stdlib-<module>.vibra` test file.
- New `.vibra` tests must pass under a bare `vibra test` (no `--allow-*` flags).
  Capability-gated tests belong in their own file with the required flags
  documented.

## Conventions

- Symbols and test names are kebab-case (non-kebab symbols emit lint warnings).
- Keep `tests/*.vibra` files flat in `tests/` so `../stdlib/<name>.vibra` imports
  resolve consistently.
- Run `vibra fmt` and `vibra lint` on `.vibra` changes where applicable.
