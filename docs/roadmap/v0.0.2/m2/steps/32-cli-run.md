# Step 32: cli-run

Kind: vertical. Slice 5 at the command line: `vibra run [TARGET] [--
ARGS…]` with the system launch, its payload, its exit status, and the
human and JSON forms.
Depends on: 29, 30, 31.
Crates: `vibra-cli`, `vibra-schema`.

## Prerequisites

- Read: [tooling chapter](../../../../spec/05-tooling.md#command-surface)
  "Command surface" (the `run` line), "Program arguments, execution and
  evaluation", "Versioned envelope and results", "Command payloads" (the
  `run` row, `StartupRecord`, `TrapRecord`); [runtime
  chapter](../../../../spec/06-runtime.md#entry-outcome) the outcome
  table; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md#conformance-profiles)
  "Conformance profiles"; [questions](../spec-questions.md) Q18, Q21, Q35.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-cli/src/lib.rs:82` | `fn execute` | dispatches `run` |
| `crates/vibra-cli/src/lib.rs:47` | `pub const fn exit_code` | maps the result to 0 to 6 |
| `crates/vibra-cli/src/args.rs:130` | `pub fn parse` | accepts `--` after `run` |
| `crates/vibra-cli/src/args.rs:46` | `program_arguments` | the tokens after `--` |
| `crates/vibra-cli/src/inventory.rs:158` | `run` | the command line marked supported |
| `crates/vibra-cli/src/test.rs:54` | `pub fn run` | the model for a command module |
| `crates/vibra-schema/src/test_payload.rs:129` | `pub struct TestItem` | the model for a payload type |

From step 29: `run_target`, `Launch::System`, `RunOutcome`.

Proposed: `vibra-cli::run`; `vibra-schema::run_payload::{RunPayload,
StartupRecordDocument, BindingRecordDocument, TrapRecordDocument}`.

## Files owned

- `crates/vibra-cli/src/run.rs`
- `crates/vibra-cli/src/lib.rs`
- `crates/vibra-cli/src/args.rs`
- `crates/vibra-cli/src/inventory.rs`
- `crates/vibra-cli/src/render.rs`
- `crates/vibra-cli/tests/it/run_command.rs`
- `crates/vibra-cli/tests/it/main.rs`
- `crates/vibra-cli/tests/it/unavailable.rs`
- `crates/vibra-cli/tests/it/inventory.rs`
- `crates/vibra-schema/src/run_payload.rs`
- `crates/vibra-schema/src/test_payload.rs`
- `crates/vibra-schema/src/lib.rs`
- `crates/vibra-schema/schemas/v2/command-result.json`
- `crates/vibra-schema/tests/it/payloads.rs`

## Ordered tasks

1. Parsing: `run`, an optional target, then `--` and the program
   arguments, kept as operating-system strings. `--` on another command
   stays invalid input.
2. `run::run`: take the snapshot; call `run_target` with
   `Launch::System { arguments }`. This is the only place in the
   workspace that constructs `Launch::System`.
3. The payload: `target`, `programResult`, `applicationCode`, `stdout`,
   `stderr`, `auditTrace` (the `@host` events, Q18), `startup` (status,
   bindings, the `@startup` events and startup-phase reads, failure),
   `trap`. Unavailable facts are `null`. Bytes that are not UTF-8 show
   as U+FFFD in `stdout` and `stderr`.
4. JSON format: one envelope on standard output; the program's output is
   captured into the payload. Human format: the program's bytes go
   straight through and diagnostics go to standard error.
5. Exit status from the result: 0, 1, 2, 3, 4, 5, 6 as the table gives.
6. The inventory marks `run` supported with its schema identifiers and
   the operation `workspaceRun`. Every command's `profiles` are derived
   from its `operations` through the profile table of the diagnostics
   chapter (C13): `fmt` lists `toolingV2`, `test` and `run` list
   `interpreterV2`, `help` lists `toolingV2`. Invariant: the inventory
   holds one table from operation to profile, and no command names a
   profile by hand.
7. The envelope validates against the published schema before it is
   written.
8. `ObservationError.code` is spelled `"@tool.unsupportedValue"` in
   JSON, in `test_payload.rs` and in the `const` of the published schema,
   as every code is (Q35). Invariant: no code in a JSON document loses its
   `@`.

## Test matrix

Tests of `vibra-cli` that run the built binary in a temporary workspace.
Trees are those of step 29. The directory rows run on Windows only and
print `NOT RUN` elsewhere.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| hello, human | positive | `vibra run` in `H` | exit 0; standard output is exactly `hello\n` | 05 Streams: "In human format the program's bytes go straight through" |
| hello, JSON | positive | `vibra run --format json` | exit 0; one envelope; `result: "@command.ok"`, `status: "passed"`, `payload.stdout: "hello\n"`, `applicationCode: 0`, one `@host` event, `startup.status: "acquired"`, the binding `released` | 05 Command payloads, the `run` row |
| envelope validates | positive | every JSON row of this table | valid against `urn:vibra:schema:v2:command-result` | 05: "A producer MUST validate every document it emits" |
| non-zero code | positive | an entry returning `ExitCode(3u8)` | exit 6; `@command.programFailed`; `status: "unknown"`; `applicationCode: 3`; `trap: null` | 05: "maps any non-zero code to `@command.programFailed`, exit 6" |
| static error | negative | a type error | exit 1; `@command.diagnostics`; `startup.status: "notStarted"`; `programResult: null` | 05, the result table |
| required refused | negative | `M` run as `vibra run -- C:\missing` | exit 3; `@command.operationalFailure`; `failure.code: "@grant.requiredUnavailable"`; `programResult: null` | 05: "tool, host, or startup failure or exhaustion" |
| trap | negative | the dividing entry | exit 5; `@command.trap`; `trap.trapCode` set | 05, the result table |
| exhaustion | negative | the recursing entry | exit 3; `limits.reached: ["memory"]`; `complete: false` | 05: "the memory limit is `\"memory\"`" |
| arguments after `--` | positive | an entry echoing its arguments; `vibra run -- a -- -b` | stdout lists `a`, `--`, `-b` | 05: "Every token after it is a program argument, in order" |
| `--` with nothing | positive | `vibra run --` | the argument list is empty | 05: "`--` with nothing after it supplies no arguments" |
| target then `--` | positive | two binary targets; `vibra run tool -- x` | runs `tool` with `["x"]` | 05: "after its optional target" |
| `--` on another command | negative | `vibra check -- x` | exit 2; `@command.invalidInput` | 05: "Only `run` accepts `--`" |
| unknown target | negative | `vibra run nope` | exit 2; `@command.invalidInput` | 05, the result table |
| stdin passes through | positive | `echo x \| vibra run` on the entry of `V2-RUNTIME-run-stdin` | stdout `x` and the line ending the shell wrote (Q10) | 06 Standard input |
| invalid UTF-8 output, JSON | boundary | an entry writing the byte 255 | `payload.stdout` holds U+FFFD; the trace's `args` hold the exact byte | 05: "bytes that are not UTF-8 show as U+FFFD there, and the audit trace keeps the exact bytes" |
| directory target, Windows | positive | `M` with a real directory holding `a.txt`; `vibra run -- <dir>` | exit 0; stdout the file's text; `drivers` lists `@host.fs.directory` | 05 Run: "and then invokes the entry" |
| directory target, other platforms | boundary | the same on Linux and macOS | exit 4; `@command.unavailable`; `@tool.unavailable`; nothing acquired | 04: "rejected at admission, as `@tool.unavailable`" |
| no permission flag | negative | `vibra run --allow-all` | exit 2 | 05: "A tool permission flag never approves a program's resource policy" |
| no world switch | negative | `vibra run --world w.vibon` | exit 2 | 06: "The shipped launcher has no user-selectable world or replay switch" |
| inventory | positive | `vibra help --format json` | the `run` capability has `supported: true`, `operations: ["workspaceRun"]`, and `profiles: ["interpreterV2"]` | 05 `CommandCapability`: "the profiles of those operations in the" |
| inventory derived from the profiles | positive | `vibra help --format json` | `fmt` has `profiles: ["toolingV2"]`; `test` has `["interpreterV2"]`; `check` has `["staticV2"]`; `help` has `["toolingV2"]` (C13) | 07 Conformance profiles: "a command's `profiles` are the profiles of the operations it offers" |
| observation error code in JSON | positive | `vibra test --format json` on the project of `V2-PROJECT-test-unobservable` | `failure.observationError.code` is `"@tool.unsupportedValue"`; the envelope validates (Q35) | 05 Versioned envelope and results: "is spelled in JSON as its full atom text, `@` included" |
| only caller of the system launch | negative, architecture test | the workspace searched for `Launch::System` | constructed in `crates/vibra-cli/src/run.rs`, and in the test files `crates/vibra-workspace/tests/it/dual_run.rs` and `crates/vibra-workspace/tests/it/attacks_run.rs`, which the test allows by name whether or not they exist | architecture, row A27 |

## Diagnostics and schema changes

Results: every `@command.*` of the table. Schema:
`urn:vibra:schema:v2:command-result` gains the `run` payload and its
records, and `ObservationError.code` becomes `"@tool.unsupportedValue"`;
`payload.auditTrace` and `startup.auditTrace` as Q18 rules.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-schema -p vibra-cli -p vibra-workspace -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0.

## Stays unavailable

`eval`, `build`, `query`, `lint`, and the project commands, as the
inventory lists them.

## Excluded

A world or replay switch; approval of a target's authority.

## Completion evidence

- [ ] Each of the 23 rows is a test, named in the report.
- [ ] The report pastes the JSON envelope of the hello run.
- [ ] `vibra help` lists `run` as supported; the profiles of `fmt` and
      `test` changed as the derivation gives, and nothing else changed.
- [ ] The validation commands exit 0.
