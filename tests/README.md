# Vibra language test suite

The `.vib` files in this directory exercise the language and standard library:

```sh
cargo run -- test
```

Rust integration tests remain separate and run through `cargo test`.

## Grouped tests

The runner discovers every `test.case` inside a top-level `test.scenario`. Each
case executes independently; scenarios only provide grouping. Case metadata is
not inherited.

```vibra
(import test "../stdlib/src/test.vib")

(test.scenario "arithmetic"
  (test.case "adds integers"
    (test.assert-eq-int (add 2 3) 5)
    tags: (language arithmetic)))
```

`profile:` defaults to `core`. Scenario names must be unique in a module, and
case names must be unique within a scenario. Filtering and child identities use
`path::scenario::case`.

## Expected errors

Use `expect-error:` for diagnostic regression cases:

```vibra
(test.scenario "casts"
  (test.case "rejects invalid casts"
    expect-error: (compile E-CAST-001 "no valid cast path")))
```

Valid phases are `load`, `compile`, and `runtime`. Load and compile expectations
require a stable diagnostic code. Runtime expectations require a message
substring.

## Metadata

Metadata belongs to each case:

```vibra
(test.scenario "fixtures"
  (test.case "is reproducible"
    (exercise-random-and-clock)
    profile: system
    tags: (clock random)
    timeout-ms: 30000
    random-seed: 42
    clock: (fixed 1000 0)
    workspace: temp))
```

`workspace: temp` creates an isolated temporary working directory when the run
uses `--allow-test-workspace`. Otherwise the case is skipped.

## Conventions

- Keep test files directly under `tests/` so `../stdlib/src/<module>.vib`
  imports resolve consistently.
- Use kebab-case symbols, scenario names, case names, profiles, and tags.
- `lang-*.vib` covers language behavior; `stdlib-*.vib` covers standard
  library modules.
- Bare `vibra test` selects the `core` profile. Put non-hermetic host tests in
  explicit profiles such as `env`, `net`, `process`, `random`, or `system`.
- A case whose point is to *emit* a compiler warning belongs in the
  `diagnostics` profile, not `core`. Such a case can never pass
  `--deny-warnings`, and leaving it in `core` makes that flag unusable as a
  whole-repo gate. `vibra test --deny-warnings` must stay green.

## Assertions

The test module provides typed equality helpers:

```vibra
(test.assert-eq-bool actual expected)
(test.assert-eq-int actual expected)
(test.assert-eq-float actual expected)
(test.assert-eq-str actual expected)
```

For other values, use `match` with a wildcard fallback:

```vibra
(match value
  7 (test.assert true)
  _ (test.fail "expected 7"))
```

## Useful flags

```sh
vibra test --filter lang-match
vibra test --profile fs --profile env
vibra test --tag language --tag fast
vibra test --deny-skips
vibra test --deny-warnings
vibra test --allow-test-workspace
vibra test --jobs 4
vibra test --fail-fast
vibra test --timeout-ms 30000
vibra test --format json --report-file report.json
```

Profiles are repeatable with OR semantics; tags are repeatable with AND
semantics. `--deny-warnings` turns compiler warnings from a child into a failed
case while retaining them in the JSON report.

Any selected case can run in benchmark mode:

```sh
vibra test --filter parse-table --benchmark \
  --benchmark-warmup 2 --benchmark-iterations 20 --format json
```
