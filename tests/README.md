# Vibra test suite (written in Vibra)

This directory holds the language/stdlib test suite **written in Vibra itself** and
executed by the built-in runner:

```sh
cargo run -- test          # or, after `cargo install --path .`:
vibra test
```

`tests/integration.rs` is the separate Rust integration harness driven by
`cargo test`; the `.vibra` files here are driven by `vibra test`.

## How discovery works

`vibra test` walks `tests/` (recursively), loads every `.vibra` file, and runs each
top-level `$test` declaration as an isolated test case in a child process. A test
**passes** when its body succeeds and **fails** when any statement errors — most
commonly via `$test.assert: false` or `$test.fail: "..."`. The worker reports its
load, compile, or runtime outcome to the parent, so expected failures are matched
reliably rather than inferred from an exit code.

`$test` is a non-empty kebab-case profile scalar and its body is the sibling `do`
sequence. Use `core` for the default suite.

```yaml
test:
  $import: ../stdlib/src/test.vibra

truth:
  $test: core
  do:
    - $test.assert: true
```

Every host operation (filesystem, network, process, clock, random, environment,
stdin) is unconditionally available — there is no runtime authorization model.
Profiles select which tests run; they carry no other meaning.

## Expected errors

Use the sibling `expect-error` mapping for diagnostic regression tests. Load and
compile expectations require a stable error code; runtime expectations require a
message substring.

```yaml
invalid-cast:
  $test: core
  expect-error:
    phase: compile
    code: E-CAST-001
    message-contains: no valid cast path
  do: []
```

Valid phases are `load`, `compile`, and `runtime`. A test fails if it succeeds,
fails in another phase, or its code/message does not match.

## Conventions

- **Flat layout.** Files live directly in `tests/` so the relative import
  `../stdlib/<name>.vibra` resolves the same way in every file. (Imports resolve
  relative to the importing file's directory; a nested file would need `../../stdlib`.)
- **Naming.** `lang-*.vibra` cover core language features; `stdlib-*.vibra` cover the
  standard library modules. Test (and symbol) names are kebab-case to avoid lint
  warnings.
- **`core` runs by default.** A bare `vibra test` selects only the `core`
  profile. Tests that touch real, non-hermetic state (network, processes, real
  environment variables) belong in their own file under a non-`core` profile,
  selected explicitly with `--profile`.
- **Self-contained.** Each file declares the helper functions, enums, and newtypes it
  needs alongside its `$test` declarations (the runner shares module-level definitions
  with the tests in that file).

## Non-core profiles

The bare command selects the `core` profile only:

```sh
vibra test
```

Non-`core` profiles (`env`, `net`, `process`, `random`, `system`) group tests
that touch real, non-hermetic host state. Select them explicitly:

```sh
vibra test --profile env --filter get-reads-an-explicitly-granted-variable
vibra test --profile process --filter run-rejects-stream-stdio-without-spawning
vibra test --profile random --filter bytes-uses-the-random-source
vibra test --profile system --filter now-unix-millis-reads-the-clock
vibra test --profile system --filter info-returns-system-information
vibra test --profile system --filter privileged-stdlib-operations-return-their-declared-shapes
```

## Asserting equality

Use the typed helpers from `stdlib/src/test.vibra` for primitive equality. They show
both values when an assertion fails:

```yaml
- $test.assert-eq-int:
    actual: $value
    expected: 7
```

Available helpers are `assert-eq-bool`, `assert-eq-int` (`$int64`),
`assert-eq-float` (`$float64`), and `assert-eq-str`. For values without a typed
helper, use a `$match` literal arm plus a catch-all that fails:

```yaml
- $match: $value
  when:
    - case: 7
      do:
        - $test.assert: true
    - case: {$wildcard: null}
      do:
        - $test.fail: "expected 7"
```

`$match` over open-ended types (`$int*`, `$float*`, `$str`, `$bool`) **requires a
`$wildcard` arm**; a lone `$bind` arm does not satisfy exhaustiveness. Matches over
enums are exhaustive when every tag is covered.

## What's covered

| File | Area |
| --- | --- |
| `lang-values.vibra` | literals, `$let` bindings, match-arm scope isolation |
| `stdlib-test.vibra` | typed primitive equality assertions |
| `lang-control-flow.vibra` | `$if` / `$while` |
| `lang-iteration.vibra` | ranges, array/map/string traversal, nesting, `$break` / `$continue` |
| `lang-functions.vibra` | zero/single/multi-arg user functions, nested calls |
| `lang-generics.vibra` | generic functions with `=where` and explicit type args |
| `lang-match.vibra` | literal / wildcard / bind patterns, nested matches |
| `lang-enums.vibra` | enum constructors, void variants, payload matching |
| `lang-newtype.vibra` | `$newtype` + `$cast` and newtype pattern matching |
| `stdlib-result.vibra` | `result` ok/err construction and matching |
| `stdlib-option.vibra` | `option` union coercion of value/absence |
| `stdlib-io.vibra` | stdout/stderr writes and the returned `result` |

## Useful flags

```sh
vibra test --filter lang-match          # substring filter on path::name
vibra test --profile fs --profile env   # profiles are repeatable (OR); bare test selects core
vibra test --tag language --tag fast    # tags are repeatable (AND)
vibra test --deny-skips                 # fail if a selected test is skipped
vibra test --deny-warnings              # fail tests that produce compiler warnings
vibra test --allow-test-workspace       # enable an isolated temp cwd for `workspace: temp` tests
vibra test --jobs 4                      # parallel workers
vibra test --fail-fast                   # stop after first failure
vibra test --timeout-ms 30000            # per-test timeout
vibra test --format json --report-file report.json
```

`workspace: temp` is test metadata, not a profile. It creates an empty
temporary working directory and runs that one child test with it as the
current directory, only when explicitly enabled with `--allow-test-workspace`.
Without the flag, the selected test is reported as skipped (and
`--deny-skips` turns that into a failing command).

`--deny-warnings` turns a child test's compiler warnings into a failure. The
warnings remain available in the JSON report for diagnosis.

## Deterministic clock and random fixtures

A test may declare `random-seed` and/or `clock` sibling metadata. The runner
creates a fresh source for that child and never reads the OS random source or
wall clock. The same seed always produces the same byte stream. Fake-clock
sleep advances both values instead of blocking:

```yaml
reproducible:
  $test: core
  random-seed: 42
  clock:
    unix-millis: 1000
    monotonic-millis: 0
  do: [...]
```

Without the metadata, clock/random operations use their production sources.
The seeded generator is for tests, not cryptography.

## Table/property helpers and benchmarks

Embedders can use `vibra::test_support::check_table` for named table cases and
`check_int_property` for deterministic generated integer cases. Property
generation always requires an explicit seed and reports the seed, case index,
original value, and a counterexample deterministically shrunk toward zero.
This makes failures replayable without random or clock authority.

Any selected `$test` declaration can also be measured as a benchmark:

```sh
vibra test --filter parse-table --benchmark \
  --benchmark-warmup 2 --benchmark-iterations 20 --format json
```

Benchmark mode uses the same discovery, profiles, tags, fixtures, timeout, and
isolated worker as normal tests. Warm-ups are unmeasured;
the stable report contract records sorted nanosecond samples plus minimum,
median, maximum, and integer mean. `--benchmark-iterations 0` fails with
`E-BENCH-001`. Without `--benchmark`, each test still runs exactly once and
normal pass/fail semantics are unchanged.
