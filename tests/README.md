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
**passes** when its process exits cleanly and **fails** when any statement errors —
most commonly via `$test.assert: false` or `$test.fail: "..."`.

```yaml
test:
  $import: ../stdlib/test.vibra

truth:
  $test:
    do:
      - $test.assert: true
```

## Conventions

- **Flat layout.** Files live directly in `tests/` so the relative import
  `../stdlib/<name>.vibra` resolves the same way in every file. (Imports resolve
  relative to the importing file's directory; a nested file would need `../../stdlib`.)
- **Naming.** `lang-*.vibra` cover core language features; `stdlib-*.vibra` cover the
  standard library modules. Test (and symbol) names are kebab-case to avoid lint
  warnings.
- **Grant-free by default.** Every test here passes under a bare `vibra test` with no
  permission flags. Tests use only pure/stdout operations (`io`, `code`) and never
  require `--allow-read`/`--allow-write`/etc. Add capability-gated tests in their own
  file and document the flags they need.
- **Self-contained.** Each file declares the helper functions, enums, and newtypes it
  needs alongside its `$test` declarations (the runner shares module-level definitions
  with the tests in that file).

## Asserting equality

The runtime has no comparison operators, so equality is asserted with a `$match`
literal arm plus a catch-all that fails:

```yaml
- $match: $value
  when:
    - pattern: 7
      do:
        - $test.assert: true
    - pattern: {$wildcard: null}
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
| `lang-control-flow.vibra` | `$if` / `$while` |
| `lang-functions.vibra` | zero/single/multi-arg user functions, nested calls |
| `lang-generics.vibra` | generic functions with `=where` and explicit type args |
| `lang-match.vibra` | literal / wildcard / bind patterns, nested matches |
| `lang-enums.vibra` | enum constructors, void variants, payload matching |
| `lang-newtype.vibra` | `$newtype` + `$cast` and newtype pattern matching |
| `stdlib-result.vibra` | `result` ok/err construction and matching |
| `stdlib-option.vibra` | `option` union coercion of value/absence |
| `stdlib-code.vibra` | pure `code` parse / get / set document operations |
| `stdlib-io.vibra` | stdout/stderr writes and the returned `result` |

## Useful flags

```sh
vibra test --filter lang-match          # substring filter on path::name
vibra test --jobs 4                      # parallel workers
vibra test --fail-fast                   # stop after first failure
vibra test --timeout-ms 30000            # per-test timeout
vibra test --report yaml --report-file report.yaml
```
