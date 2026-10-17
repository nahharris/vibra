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
