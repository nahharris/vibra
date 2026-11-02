# Step 18 — environment, clocks, random bytes, and program arguments

Prerequisite: Step 17 merged. Stage 4B behavior step: `env.read`, `time.now`, and
`random.generate` in both backends with the deterministic world fields behind them,
and the CLI grammar `vibra run TARGET [-- ARG...]` that gives `env.read.args` a
value (gap G24, ledger D22.1).

## Read before editing

- [Runtime](../../spec/06-runtime.md): **The host registry** (the `env`, `time`,
  and `random` rows), **The host world** (environment, arguments, clock, SplitMix64).
- [Tooling](../../spec/05-tooling.md): **M2 command contract** (the closed grammar
  and **Program arguments**).
- [Effects](../../spec/03-effects.md): the inventory.
- [Ledger](decision-ledger.md) rows D17, D20.2, D20.4, D22.1.

## Scope

1. `@std.env`, `@std.time`, `@std.random`, their error enums, roots, and the five
   operations `env.read-var`, `env.read-args`, `time.now-wall`,
   `time.now-monotonic`, `random.generate-bytes`.
2. The system provider's environment, arguments, wall and monotonic clocks, and
   operating-system random bytes, with `not-present`, `not-unicode`, and
   `unavailable` mapped and never trapping.
3. The world's `env`, `args`, `clock`, and `random` behavior, completing the
   `@host-world.v1` fields, with pinned SplitMix64 vectors.
4. `vibra run TARGET [-- ARG...]`: the grammar, the validation of `--` (only `run`,
   only after `TARGET`, once; before `TARGET` and on other commands is
   `@command.invalid-input`), and passing the arguments to the system provider.
   `test` takes none.
5. The help text and the tooling chapter's closed grammar are the same text.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-cli/src/lib.rs` (`Invocation`, `Action`, the argument parser, `USAGE`) | The grammar change and the `usage` payload |
| `crates/vibra-workspace/src/semantic.rs` (`run_target_with_budget`) | Carries the provider into the run |
| `crates/vibra-host` | The system provider's arguments; the world's clock and generator |
| `crates/vibra-schema/tests/command_result_contract.rs` | The `help` payload is unchanged in shape |

## Ordered tasks

1. The three modules, symbols, digests, and the manifest `host` list.
2. The operations once, then both backends' trait implementations (no new builder
   kinds are needed: `str`, `array` of `str`, `bytes`, `enum`).
3. The world and system provider behavior, with vectors for the generator and the
   clock (the first monotonic read returns `monotonic`, then adds `step`).
4. The CLI parser change and its process tests: `run app -- a b`, `run app --`,
   `run app -- --flag`, `run -- app` (invalid), `test -- x` (invalid), `fmt --`
   (invalid), and the help text.
5. A demo-shaped corpus case whose world supplies `args` and a file.

Invariants preserved: the arguments are verbatim and ordered; a non-Unicode argument
is `not-unicode` at the operation, not a command error; `test` is deterministic
whatever the invoking environment is; the monotonic clock never decreases within an
instance.

## Test matrix

- Positive: a present and an absent variable; arguments in order; both clocks in
  the world (the stepped sequence) and in the system provider (nondecreasing);
  the generator vectors for several `count` values and across calls; a real random
  call returns exactly `count` bytes.
- Negative: a variable name that is empty or holds `=` or NUL; a non-Unicode
  variable and argument (system provider test on Unix); a monotonic read past the
  `u64` range in a world; a malformed command line for each rule above.
- Recovery: after a command-line error nothing runs and the exit code is 2.
- Boundary: no arguments, ten thousand arguments, an argument of 1 MiB, `count` 0
  and `count` 1 MiB, `step` 0.
- Formatter: none.

## Diagnostic and schema changes

None to the registry. The CLI grammar and its `help` text change; the command-result
schema is unchanged.

## Validation

The [pre-merge list](validation.md#before-merging-each-step) and `cargo test
--locked --offline -p vibra-cli -p vibra-host -p vibra-wasm-run -p
vibra-conformance`.

## Excluded

Environment writes, sleep and timers, an `MCP` `--allow-run` argument channel,
options on `test`.

## Completion evidence

The five operations match in both backends; the demo gate's runtime path works
through `vibra run TARGET -- ARG...` in a process test.

## As built

Landed on its branch, conditional on its pull request merging; the base is the head
of Step 17 (PR #377, not yet merged).

- **The modules.** `std/env.vib`, `std/time.vib`, and `std/random.vib` declare the
  three error enums, the roots `read`, `now`, and `generate`, and the five
  `visibility: @public` operations. The manifest lists the modules and the five
  symbols and stays `0.3.0`. The checker, the operations (`vibra-host`), both
  providers, and both backends needed no change: the registry rows, the world's
  clock and SplitMix64 generator, and the system provider's environment, clocks, and
  randomness were built in Step 16a.
- **The CLI.** `parse_run_command` splits `run`'s tokens at the first literal `--`.
  What precedes it must be exactly one `TARGET`; everything after it, including
  another `--`, an option-like token, and the empty string, is one program argument,
  verbatim and in order, handed to `SystemProvider::with_arguments`. `--` before
  `TARGET` leaves no target and is invalid input, and `--` on `test`, `check`, `fmt`,
  `help`, and `project` was already invalid input because those commands take no
  such token. The `USAGE` text and the tooling chapter's grammar are the same text
  (`run TARGET [-- ARG...]`); no schema changed.

### Rulings

None. "Everything after the first `--` is an argument" is the tooling chapter's
existing sentence, and the second `--` of `run app -- a -- b` is therefore an
argument.

### Cases

Added, all `matched` in both backends: `V1-RUNTIME-env-read-var`,
`V1-RUNTIME-env-read-args-from-the-world`, `V1-RUNTIME-env-read-args-default-empty`,
`V1-RUNTIME-time-wall-from-the-world`, `V1-RUNTIME-time-monotonic-steps`,
`V1-RUNTIME-time-monotonic-unavailable-at-the-top`, `V1-RUNTIME-random-seeded-bytes`
(seed 1234567, bytes written from the SplitMix64 output),
`V1-RUNTIME-random-state-continues` (seed 0, three bytes then nine),
`V1-RUNTIME-env-error-propagates-with-try`, `V1-RUNTIME-env-demo-args-name-a-file`,
and `V1-RUNTIME-world-sets-env-args-clock-random` (`workspace-test`, three tests, the
third seeing the clock at its start again). Negatives:
`V1-EFFECT-host-target-array-omits-env-read`, `-time-now`, and `-random-generate`.
No case was re-expected, deleted, or downgraded. Parity: 11 rows added, 162 of 162
`matched`.

### Evidence

- `process_m4_arguments` (8 portable tests and 1 Unix-only): `run app -- a b c`,
  `run app --`, `run app -- --flag -- ...`, the JSON trace, the invalid forms
  (`run -- app`, `-- run app`, `test -- x`, `test --`, `check`, `fmt`, `help`, and
  `project init` with `--`), nothing runs after a command-line error, the help text,
  ten thousand arguments and a 1 MiB argument (through the library entry point,
  because a Windows command line cannot hold them), and a non-Unicode argument as
  `not-unicode` at the operation.
- `env_clock_random_m4_step18` (6 host tests, 1 Unix-only): the system environment,
  verbatim large arguments, the world's wall and stepped monotonic clock with
  `step: 0`, a read past the `u64` range, the system clocks, and `count` 0 and 1 MiB.
