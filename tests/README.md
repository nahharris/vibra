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
sequence. Use `core` for the capability-free suite.

```yaml
test:
  $import: ../stdlib/src/test.vibra

truth:
  $test: core
  do:
    - $test.assert: true
```

When a test needs authority, declare a sibling `policy:` containing a `$policy`
type. Narrow `$args.policy` to the required `$capability.<domain>` type and pass
that value to the privileged call. Profiles select tests only; they never
confer authority.

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
- **Capability-free by default.** Every test here passes under a bare `vibra test` with no
  permission flags. Tests use only pure/stdout operations (`io`, `code`) and never
  require `--allow-read`/`--allow-write`/etc. Add capability-gated tests in their own
  file and document the flags they need.
- **Self-contained.** Each file declares the helper functions, enums, and newtypes it
  needs alongside its `$test` declarations (the runner shares module-level definitions
  with the tests in that file).

## Capability-profile contracts

The bare command selects the `core` profile only, so it never grants host
permissions and remains the required default suite:

```sh
vibra test
```

Non-core profiles select contract tests; a profile does not grant anything by
itself. Run each capability contract with the permission it declares:

```sh
vibra test --profile env --filter get-reads-an-explicitly-granted-variable --allow-env PATH
vibra test --profile net --filter connect-reports-the-current-unsupported-runtime --allow-net 127.0.0.1:9
vibra test --profile process --filter run-reports-the-current-unsupported-runtime --allow-run echo
vibra test --profile random --filter bytes-uses-the-granted-random-source --allow-random
vibra test --profile system --filter now-unix-millis-uses-the-granted-clock --allow-clock
vibra test --profile system --filter info-uses-the-granted-system-capability --allow-sys-info
vibra test --profile system --filter privileged-stdlib-operations-return-their-declared-shapes --allow-clock --allow-random --allow-sys-info
```

Keep these invocations narrow. Selecting several profiles (or all `system`
tests) requires the union of their explicit policies; the runner never infers or
widens permissions from profile names.

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
| `stdlib-code.vibra` | typed structural paths, node lookup, forms, and lossless replacement |
| `stdlib-io.vibra` | stdout/stderr writes and the returned `result` |

## Useful flags

```sh
vibra test --filter lang-match          # substring filter on path::name
vibra test --profile fs --profile env   # profiles are repeatable (OR); bare test selects core
vibra test --tag language --tag fast    # tags are repeatable (AND)
vibra test --deny-skips                 # fail if a selected test is skipped
vibra test --deny-warnings              # fail tests that produce compiler warnings
vibra test --allow-test-workspace read-write # enable an isolated temp cwd and fs policy
vibra test --jobs 4                      # parallel workers
vibra test --fail-fast                   # stop after first failure
vibra test --timeout-ms 30000            # per-test timeout
vibra test --format yaml --report-file report.yaml
```

`workspace: temp` is test metadata, not a profile capability. It creates an
empty temporary working directory for that one child test only when explicitly
enabled with `--allow-test-workspace read`, `write`, or `read-write`. Without
the flag, the selected test is reported as skipped (and `--deny-skips` turns
that into a failing command). The test runner clears ordinary host filesystem
policy for workspace tests, then approves only the selected access mode on that
temporary directory.

`--deny-warnings` turns a child test's compiler warnings into a failure. The
warnings remain available in the YAML report for diagnosis.
