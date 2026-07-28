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
[`tests/README.md`](tests/README.md) for conventions (flat layout, typed
assertions, and profiles).

```sh
cargo run -- test            # from the repo root
# or, after `cargo install --path .`:
vibra test
```

Useful flags: `--filter <name>`, `--profile <name>`, `--tag <tag>`,
`--jobs <n>`, `--fail-fast`, `--deny-skips`, `--deny-warnings`, and
`--report yaml --report-file report.yaml`. Profiles and tags only select
tests. Tests that require `workspace: temp` also need
`--allow-test-workspace`.

## When you add or change behavior

- Changing the **compiler/runtime** (`src/`): add/adjust a focused Rust test
  under `tests/` and re-run `cargo test`.
- Changing the **language surface or `stdlib/`**: add/adjust a `tests/*.vibra`
  case and re-run `vibra test`. New stdlib modules should get a matching
  `stdlib-<module>.vibra` test file.
- New `core` `.vibra` tests must pass under a bare `vibra test`. Tests that
  touch real, non-hermetic host state (network, processes, real environment
  variables) belong in their own file under a non-`core` profile.

## Documentation and schemas

- Keep `README.md` accurate for user-facing commands, test syntax, and
  interface behavior when those interfaces change.
- For changes to machine-readable interfaces, update the relevant JSON Schema
  under `schemas/`: source shape, project manifest, diagnostics, structural
  code, or editor query response. Add newly introduced stable diagnostic codes
  to `schemas/linter-codes.json`.
- Treat schema IDs and documented response shapes as tooling contracts. Check
  the README schema guide whenever adding, removing, or repurposing a schema.

## Conventions

- Symbols and test names are kebab-case (non-kebab symbols emit lint warnings).
- Keep `tests/*.vibra` files flat in `tests/` so `../stdlib/<name>.vibra` imports
  resolve consistently.
- Run `vibra fmt` and `vibra lint` on `.vibra` changes where applicable.
