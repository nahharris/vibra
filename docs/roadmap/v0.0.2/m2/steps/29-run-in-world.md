# Step 29: run-in-world

Kind: vertical. Slice 5 completed in the engine: one function admits a
target, starts it, runs its entry, maps the ending to an outcome, and
releases; the conformance operation `workspaceRun` drives it under a fake
world.
Depends on: 20, 21, 26, 28.
Crates: `vibra-workspace`, `vibra-conformance`.

## Prerequisites

- Read: [runtime chapter](../../../../spec/06-runtime.md#entry-outcome)
  "Entry outcome" whole, "Traps", "Host world" (Recipes under a world);
  [tooling
  chapter](../../../../spec/05-tooling.md#versioned-envelope-and-results)
  the result table and "Exit code of a program"; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md#persistent-observations)
  "Persistent observations", the profile and operation tables;
  [architecture](../architecture.md) "The workspace and the tools";
  [questions](../spec-questions.md) Q17, Q21.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-workspace/src/semantic.rs:69` | `pub fn check_snapshot` | the check a run starts with |
| `crates/vibra-ir/src/program.rs:91` | `pub fn for_entry` | the reachable program of the entry |
| `crates/vibra-conformance/src/profile.rs:170` | `WorkspaceRun` | the operation, declared and unhandled |
| `crates/vibra-conformance/src/standard.rs:192` | `pub fn standard_dispatcher` | registers the handler |
| `crates/vibra-conformance/src/manifest.rs:170` | `pub struct CaseExpectations` | gains the run observation key |
| `crates/vibra-conformance/src/workspace_test.rs:28` | `pub struct WorkspaceTestHandler` | the model for the handler |

From step 20: `run_entry`. From step 26: `binding::plan_of`. From step
28: `startup::{admit, start, release}`.

Proposed: `vibra-workspace::run::{run_target, Launch, RunOutcome}` as the
architecture gives them; `vibra-conformance::workspace_run`.

## Files owned

- `crates/vibra-workspace/src/run.rs`
- `crates/vibra-workspace/tests/it/run.rs`
- `crates/vibra-conformance/src/workspace_run.rs`
- `crates/vibra-conformance/src/standard.rs`
- `crates/vibra-conformance/src/manifest.rs`
- `crates/vibra-conformance/src/lib.rs`
- `crates/vibra-conformance/tests/it/manifest.rs`
- `conformance/rules/m2-29.tsv`

## Ordered tasks

1. `run_target`: check the snapshot (static diagnostics end the run with
   `@command.diagnostics` and startup `notStarted`); select the target
   (the only binary target when none is named; an unknown or library
   target is `@command.invalidInput`); take `plan_of`; build the driver
   for the `Launch`; `admit` (no driver: `@command.unavailable` with
   `@tool.unavailable`); `start`; build `EntryArgument`s from the bound
   leaves; `run_entry`; map; release; return. Invariant: one exit path,
   which releases.
2. The mapping:

   | Ending | `result` | Record |
   | --- | --- | --- |
   | `ExitCode` zero | `@command.ok` | `applicationCode: 0`, `programResult` its canonical text |
   | `ExitCode` non-zero | `@command.programFailed` | `applicationCode` the code, no trap |
   | refused required leaf, failed acquisition | `@command.operationalFailure` | the startup diagnostic; `programResult` absent |
   | a defect trap | `@command.trap` | the trap record with its origin ordinal |
   | memory exhaustion | `@command.operationalFailure` | `@runtime.memoryExhausted`; limits reached `memory` |

3. Under `Launch::World` the program's streams are the world's; `stdout`
   and `stderr` of the outcome are what the entry wrote. The trace has
   `target` set and `revision` the workspace revision.
4. `workspace_run`: inputs a tree, an optional `hostWorld`, an optional
   target; it calls `run_target` with `Launch::World` and compares the
   observation of Q17 field by field. The manifest key is added under
   the name the chapter gives it; until then the handler is exercised by
   host tests only and no case uses it.
5. The handler constructs no `Launch::System`.

## Test matrix

Host tests of `vibra-workspace` (`tests/it/run.rs`), each a tree written
by the test and a world. Each row is also the content of the
`workspaceRun` case named by its id, added to the corpus when Q17 is
answered. `H` is the hello program: an entry taking `out: StdoutText`,
writing `hello\n`, returning `ExitCode(0u8)`, with the recipe `@out` from
`@host.io.stdout`.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-RUNTIME-run-hello` | positive | `H`, the default world | `@command.ok`; code 0; stdout `hello\n`; the trace is the chapter's example with this tree's identities | 06 Audit trace, the example; Entry outcome: "the entry returns `ExitCode` zero" |
| `V2-RUNTIME-run-nonzero` | positive | an entry returning `ExitCode(3u8)` | `@command.programFailed`; `applicationCode: 3`; no trap | 06: "A non-zero code maps to `@command.programFailed`, exit 6, with the actual code in `applicationCode` and no fabricated trap" |
| `V2-RUNTIME-run-code-255` | boundary | `ExitCode(255u8)` | `applicationCode: 255` | 05: "integer 0–255" |
| `V2-RUNTIME-run-static-error` | negative | `H` with a type error | `@command.diagnostics`; startup `notStarted`; an empty trace | 05: "static diagnostics prevent admission" |
| `V2-RUNTIME-run-required-refused` | negative | `M` of step 26 with world `args: ["/missing"]` | `@command.operationalFailure`; `@grant.requiredUnavailable` at the `@notes` recipe; `programResult` absent; the `@args` events are kept | 05: "After a startup failure `programResult` is `null`"; "A failed command keeps the startup events that completed" |
| `V2-RUNTIME-run-optional-absent` | positive | an entry with `metrics: Option(DirWriteText)` from a `@variablePath` whose variable is absent; it returns `ExitCode(0u8)` when it matches `Option.none()` | `@command.ok`; `@metrics` `refused` | 03: "An optional grant refused in the way its provider specifies becomes `Option.none()`" |
| `V2-RUNTIME-run-trap` | negative | an entry that divides by zero after one write | `@command.trap`; the trap record's origin ordinal; one `@host` event; bindings `released` | 06: "a defect trap" |
| `V2-RUNTIME-run-exhausted` | negative | an entry that recurses without end, under a small budget | `@command.operationalFailure`; `@runtime.memoryExhausted`; `applicationCode` absent | 06: "Memory exhaustion, startup failure, and tool failure keep their own categories" |
| `ATK-release-on-trap` | negative | the trap row, with a wrapper driver counting releases | every acquired leaf released once, in reverse | 06: "On every ending of the process" |
| `ATK-release-on-exhaustion` | negative | the exhaustion row, the same wrapper | the same | the same |
| `V2-RUNTIME-run-reads-note` | positive | `M` under the world of step 16 with `args: ["/notes"]`; the entry reads `a.txt` and writes it | stdout `hello`; invocations: 1 the startup read, 2 the entry, 3 and 4 the body's awaits | 06 Invocations |
| `V2-RUNTIME-run-denied-by-recipe-policy` | positive | the same entry reading `raw.bin` under the `.txt` policy, returning `ExitCode(4u8)` on `denied` | `applicationCode: 4`; no event for the read | 07: "A grant that denies a request at run time returns the operation's typed error" |
| `V2-RUNTIME-run-recipe-limit` | positive | `@out` with `limits: {outputBytes: 3u64}`; the entry writes `hello`, returns 5 on `limitExceeded` | `applicationCode: 5`; stdout empty | 03 Limits |
| `V2-GRANT-env-name-denied` | positive | an entry with `vars: EnvReadVar` from `@host.env.vars` with `names: ["HOME"]`; it reads `PATH` and returns 6 on `denied` | `applicationCode: 6`; no event for the read (Q12) | 06 Environment: "Neither grant exposes anything outside its reviewed scope" |
| `V2-RUNTIME-run-stdin` | positive | world `stdin: "x\ny"`; an entry echoing stdin | stdout `x\ny` | 06: "The world's `stdin` feeds an entry run under it" |
| `V2-RUNTIME-run-fresh-world` | positive | one world, the target run twice; the entry writes a file and reports whether it existed | both runs report absent | 06: "Every executor and every test gets a fresh copy of the world" |
| `V2-RUNTIME-run-entry-requires-else` | positive | an entry with `requires false else ExitCode(9u8)` | `applicationCode: 9`; the body's write is absent | 06: "behave as on any `proc`" |
| `V2-RUNTIME-run-target-selection` | negative | two binary targets, none named | `@command.invalidInput` | 05: "invalid command, selector, or typed input" |
| handler: no system launch | negative, architecture test | `crates/vibra-conformance/src` searched for `Launch::System` | none | 06: "The shipped launcher has no user-selectable world" |
| handler: observation compared | positive, host test | the hello run against a hand-written observation with one changed byte of stdout | a mismatch naming `stdout` | 07 Persistent observations |

## Diagnostics and schema changes

Codes in outcomes: `@grant.requiredUnavailable`,
`@grant.acquisitionFailed`, `@runtime.memoryExhausted`,
`@tool.unavailable`. Results: `@command.ok`, `@command.diagnostics`,
`@command.programFailed`, `@command.operationalFailure`, `@command.trap`,
`@command.unavailable`, `@command.invalidInput`. The run observation
document is Q17's and is not added to any schema until it is named.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-workspace -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0. The corpus count does not change in this step while Q17
is open; the report lists the `V2-` ids that wait for it.

## Stays unavailable

`vibra run` (32). `Launch::System` exists as a type and has no driver
until step 30.

## Excluded

The command and its payload (32); real drivers.

## Completion evidence

- [ ] Each row is a test, named in the report.
- [ ] `run_target` has one `return`; the report quotes it.
- [ ] If Q17 was answered before this step ran, every `V2-` row is a
      corpus case and `conformance/rules/m2-29.tsv` lists it `active`;
      otherwise the file holds the header only and the report says so.
- [ ] The validation commands exit 0.
