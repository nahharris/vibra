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

Tests written **in Vibra itself**, under `tests/*.vib`, exercising language
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
- Changing the **language surface or `stdlib/`**: add/adjust a `tests/*.vib`
  case and re-run `vibra test`. New stdlib modules should get a matching
  `stdlib-<module>.vib` test file.
- New `core` `.vib` tests must pass under a bare `vibra test`. Tests that
  touch real, non-hermetic host state (network, processes, real environment
  variables) belong in their own file under a non-`core` profile.

## Documentation and schemas

- Keep `README.md` accurate for user-facing commands, test syntax, and
  interface behavior when those interfaces change.
- Read [`docs/index.md`](docs/index.md) before changing language behavior or
  adding documentation. It defines the documentation source-of-truth map and
  the folders for references, decisions, status reports, and history.
- Update the canonical document in the same change as the language, runtime,
  standard-library, CLI, schema, or workflow change. Do not leave competing
  root-level drafts or status notes that look normative.
- Put accepted contracts in `docs/decisions/`, stable operational guides in
  `docs/reference/`, dated snapshots in `docs/status/`, and superseded
  material in `docs/archive/`. Add every new document to `docs/index.md`.
- Store implementation plans in `docs/plans/`; plans are working records, not
  language contracts.
- The repository-owned agent skills are under `skills/`. The `.agents/`
  directory is local-only and fully gitignored; do not copy third-party or
  machine-local skills into the repository.
- For changes to machine-readable interfaces, update the relevant JSON Schema
  under `schemas/`: source shape, project manifest, diagnostics, structural
  code, or editor query response. Add newly introduced stable diagnostic codes
  to `schemas/linter-codes.json`.
- Treat schema IDs and documented response shapes as tooling contracts. Check
  the README schema guide whenever adding, removing, or repurposing a schema.

## Conventions

- Symbols and test names are kebab-case (non-kebab symbols emit lint warnings).
- Keep `tests/*.vib` files flat in `tests/` so `../stdlib/<name>.vib` imports
  resolve consistently.
- Run `vibra fmt` and `vibra lint` on `.vib` changes where applicable.
