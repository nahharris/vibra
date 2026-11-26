# Step 36: release-program

Kind: vertical. The release program of v0.0.2, checked in: a command-line
program that reads text files under a directory chosen at run time, writes
a log under another, and optionally reports a metric, with tests that take
every grant from `TestWorld`, and the four showings of the release gate.
Depends on: 35.
Crates: `examples`, `vibra-cli`.

## Prerequisites

- Read: [release README](../../README.md#release-gate) "Release gate",
  items 1 and 2; this milestone's [README](../README.md) "Exit evidence";
  [effects chapter](../../../../spec/03-effects.md#capability-values)
  "Capability values"; [programs
  chapter](../../../../spec/04-programs-and-packages.md#targets)
  "Targets"; [questions](../spec-questions.md) Q17, Q22.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-cli/src/lib.rs:84` | `fn execute` | the command the showings run |

Example projects: `examples/hello/`, and the pure modules of the gate
in `examples/gate-pure/`, which the exit step of M1 adds.

Proposed: `examples/gate/` (`project.vibon`, `src/gate/`, `tests/`);
`crates/vibra-cli/tests/it/gate.rs`.

## Files owned

- `examples/gate/`
- `crates/vibra-cli/tests/it/gate.rs`
- `crates/vibra-cli/tests/it/main.rs`
- `docs/roadmap/v0.0.2/m2/evidence/release-program.md`

## Ordered tasks

1. The entry:

   ```vibra
   proc main
     takes {
       args: EnvReadArgs
       vars: EnvReadVar
       notes: {list: DirList reader: DirReadText}
       logs: DirWriteText
       metrics: Option(DirWriteText)
       out: StdoutText
     }
     returns code: ExitCode
     performs {EnvReadArgs DirList DirReadText DirWriteText StdoutText}
   ```

   `@notes.list` and `@notes.reader` come from an `@argumentPath` of index
   0 with a `.txt` read rule and a listing rule; `@logs` from a
   `@fixedPath` with create and replace; `@metrics` from a
   `@variablePath` of `GATE_METRICS` through `@vars`; `@out` from
   `@host.io.stdout`. `effects` lists exactly the roots performed by the
   entry and the startup reads. Invariant: the program has no other
   authority, and its project file states all of it.
2. The body lists the notes, counts lines with the pure modules of
   `examples/gate-pure/` (a path dependency), and writes the log through
   a callback of type `Proc({Str} Result(Void FsError) {DirWriteText})`
   that closes over `logs` only.
3. Tests under `examples/gate/tests/`: every grant from `TestWorld`;
   they cover the summary, the log's text, the absent metric, a denied
   request, and an exhausted limit.
4. `tests/it/gate.rs` runs the four showings through the built binary,
   and the three compile-time rejections through `check` on variants of
   the project written by the test.
5. `evidence/release-program.md`: per showing, the command, the exit
   status, and the envelope.

## Test matrix

Tests of `vibra-cli` running the built binary on `examples/gate/` and on
variants the test writes; the directory rows are Windows only.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| check, format, test | positive | `vibra check`, `vibra fmt --check`, `vibra test` in `examples/gate/` | exit 0 each; every test `@test.passed` | release gate 1: "Its tests obtain every grant from `TestWorld`" |
| tests take only the world | positive | every `test` of `examples/gate/tests/` | its `takes` is `{world: TestWorld}` or absent | the same |
| run | positive | a directory with `a.txt` (2 lines) and `b.txt` (3 lines); `vibra run -- <dir>` | exit 0; stdout the two counts; a log file under the log directory | release gate 2: "Run it with `vibra run` under the target's grant bindings" |
| showing: the callback cannot read | negative | a variant whose log callback awaits `DirReadText.read` | `vibra check` exit 1; `@effect.outsideCeiling` in the callback | release gate 2: "a callback limited to the log grant cannot reach the reader" |
| showing: callback row | negative | a variant passing a callback of row `{DirWriteText DirReadText}` | `@type.argumentMismatch` | exit evidence: "a callback whose row exceeds its parameter's row" |
| `V2-GRANT-release-optional-absent` | positive | `GATE_METRICS` not set | exit 0; `@metrics` `refused`; no metric file; stdout unchanged (Q22) | release gate 2: "an optional grant whose leaf is refused arrives as `Option.none()`" |
| showing: optional present | positive | `GATE_METRICS` names a directory | exit 0; a metric file there | 03: "An optional grant is an `Option`" |
| showing: required fails startup | negative | `vibra run -- C:\no\such\dir` | exit 3; `@grant.requiredUnavailable` at the `@notes.list` recipe; stdout empty; no log file | release gate 2: "a required grant that cannot be acquired fails startup before the entry runs" |
| showing: typed denial | positive | the notes directory also holds `c.md`; the program tries each listed name | exit 0; `c.md` is reported as denied in the log; no read event for it | release gate 2: "a request outside a grant's policy returns a typed denial" |
| showing: exhausted shared limit | positive | a variant with `limits: {invocations: 2u64}` on `@notes.reader` and three notes | the third read is `limitExceeded`, reported in the log; exit 0 | exit evidence: "an exhausted shared limit" |
| rejection: operation outside a ceiling | negative | a variant whose helper `proc` omits `DirList` from `performs` | `@effect.outsideCeiling` | exit evidence |
| rejection: entry row outside the admission ceiling | negative | a variant whose `effects` omits `StdoutText` | `@effect.outsideCeiling` at the `effects` field | exit evidence |
| rejection: missing binding | negative | a variant without the `@logs` recipe | `@grant.invalidBinding` | exit evidence |
| rejection: cyclic binding | negative | a variant where `@logs` and `@metrics` are each `after` the other | `@grant.bindingCycle` | exit evidence |
| the same program under a world | positive | `workspaceRun` of `examples/gate/` with a world holding the same files | the same stdout and codes as the real run (Q17) | 06 Recipes under a world |
| no grant from data | negative | a variant whose test decodes a record shaped like a grant and passes it | `@type.argumentMismatch` | exit evidence: "No grant is obtainable from data" |
| no grant from a row | negative | a variant test with `performs {DirReadText}` and no world, naming a reader | `@name.unknownSymbol` | exit evidence: "from a row alone" |

## Diagnostics and schema changes

None new. Codes shown: `@effect.outsideCeiling`, `@type.argumentMismatch`,
`@grant.invalidBinding`, `@grant.bindingCycle`,
`@grant.requiredUnavailable`, `@name.unknownSymbol`.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-cli -p vibra-workspace -p vibra-conformance --all-features -- --nocapture
cargo run --locked --offline -p vibra-cli --bin vibra -- --workspace examples/gate check
cargo run --locked --offline -p vibra-cli --bin vibra -- --workspace examples/gate test
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
```

Read: all exit 0. On Linux and macOS the run rows print `NOT RUN: no
directory driver` and the check and test rows still pass.

## Stays unavailable

Items 3 and 4 of the release gate: the edit workflow (M3) and the native
fixture (M4).

## Excluded

A network metric: no network operation exists; the metric is a file under
an optional directory grant.

## Completion evidence

- [ ] `evidence/release-program.md` holds the command, exit status, and
      envelope of each showing.
- [ ] `examples/gate/project.vibon` is quoted whole in the evidence.
- [ ] Every construction the checker rejected while the program was
      written is listed with the repair used.
- [ ] The validation commands exit 0.
