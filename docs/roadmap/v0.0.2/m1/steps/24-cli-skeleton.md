# Step 24: cli-skeleton

Kind: vertical. The new `vibra` binary and the major-2 machine envelope:
argument parsing for the whole command surface, `help`, the support
inventory, invalid input, and the unavailable outcome for every command no
step has landed yet. It serves the tool surface of every slice and depends
only on the diagnostics crate, so it runs beside the reader from the start.

## Prerequisites

- Step 02 landed.
- Read: [tooling chapter](../../../../spec/05-tooling.md) "Command surface" (the
  grammar, the operand table, the six rules under it), "Versioned envelope and
  results" (the envelope, the result table, "Streams", "Invalid input"),
  "Command payloads" (`help`, `CommandCapability`), "Schemas, delivery and
  exclusions"; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md) "Diagnostic
  documents and spans", "Conformance profiles".
- Open questions: none blocks this step.

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-cli-v1/src/lib.rs:36` (`OutputFormat`), `:45` (`CommandResult`), `:235` (`run`) | the shape of the pure `run(args, cwd, stdout, stderr)` function and the result-to-exit mapping; the command set is replaced |
| `crates/vibra-cli-v1/src/main.rs` | seven lines; the same thin `main` |
| `crates/vibra-schema-v1/src/lib.rs:59` (`COMMAND_RESULT_SCHEMA`) and `schemas/v1/command-result.json` | how a schema is embedded and validated by its producer |
| `crates/vibra-schema-v1/tests/it/command_result.rs` | the producer-validates test pattern |
| `crates/vibra-cli-v1/tests/it/support.rs` | the in-process harness for host tests |

Proposed crates and modules:

- `crates/vibra-schema` (new): `schemas/v2/command-result.json`,
  `schemas/v2/diagnostic.json`; `src/envelope.rs` (`Envelope`, `Status`,
  `CommandOutcome`, `Limits`), `src/diagnostic.rs` (`DiagnosticDocument`,
  `SpanDocument` rendering from `vibra-diagnostics`), `src/capability.rs`
  (`CommandCapability`), `src/validate.rs`. Depends on `vibra-diagnostics`
  only.
- `crates/vibra-cli` (new, binary `vibra`): `src/args.rs` (the grammar),
  `src/inventory.rs` (one row per command), `src/help.rs`, `src/render.rs`
  (human and JSON), `src/lib.rs` (`run`), `src/main.rs`.

## Ordered tasks

1. Schemas: `urn:vibra:schema:v2:command-result` as a closed object with
   `schemaVersion` 2, `toolVersion`, `command`, `workspaceRevision`
   (nullable), `status` (`passed`, `refuted`, `unknown`, `unsupported`),
   `result` (the eight `@command.*` atoms), `diagnostics`, `limits`
   (`profile`, `reached`, `complete`), `payload`. The payload is a `oneOf`
   keyed by `command`; this step publishes the `help` payload and the empty
   payload. `urn:vibra:schema:v2:diagnostic` per the diagnostics chapter.
   Invariant: every emitted document is validated against its schema before
   it is written; a validation failure is `@command.trap`, exit 5.
2. Argument grammar: `GLOBAL` options `--format human|json` and
   `--workspace PATH`, each at most once and before the command; the 18
   command lines of the surface, with their operands and options. An
   unknown command or option, a repeated single option, and an extra
   positional are `@command.invalidInput`, exit 2, with no diagnostics, the
   recognized command name or an empty one, and an empty payload. No alias
   is accepted (`init`, `show`, `find`).
3. Inventory: one `CommandCapability` per command line with `command`,
   `operations`, `profiles`, `supported`, `schemaIds`. In this step only
   `help` has `supported: true`. Invariant: the inventory is the single
   table the parser, `help`, and the unavailable outcome read; a host test
   fails when a parsed command has no inventory row.
4. `help`, `--help`, `-h` in the command position: equivalent, no
   arguments; payload `usage` (the hierarchy, with unavailable commands
   marked) and `capabilities`; `workspaceRevision` is `null`; status
   `passed`, result `@command.ok`.
5. Every other well-formed command: status `unsupported`, result
   `@command.unavailable`, exit 4, one `@tool.unavailable` diagnostic with
   no span naming the command, `workspaceRevision` `null`, the empty
   payload.
6. Streams: in JSON format exactly one envelope on standard output and
   nothing else; logs on standard error. In human format the usage text or
   one line per diagnostic.
7. Result-to-exit mapping for all eight results in one function with a
   host test over the table.

## Test matrix

The cases are host tests in `crates/vibra-cli/tests/it/`; the conformance
corpus has no CLI operation in M1.

| Test | Kind | Input | Expected |
| --- | --- | --- | --- |
| `help_json` | positive | `vibra --format json help` | exit 0; one envelope; `command: "help"`, `status: "passed"`, `result: "@command.ok"`, `workspaceRevision: null`; 18 capabilities; validates against the schema |
| `help_spellings` | positive | `vibra help`, `vibra --help`, `vibra -h` | byte-identical output |
| `help_takes_no_arguments` | negative | `vibra help fmt` | exit 2, `@command.invalidInput`, `command: "help"`, empty payload |
| `help_marks_unavailable` | boundary | `vibra help` (human) | every command but `help` carries the unavailable mark; none is shown as working |
| `unknown_command` | negative | `vibra frobnicate` | exit 2; `command: ""`; no diagnostics |
| `alias_rejected` | negative | `vibra init`, `vibra show`, `vibra find` | exit 2 each |
| `unknown_option` | negative | `vibra fmt a.vib --wirte` | exit 2; `command: "fmt"` |
| `repeated_global` | negative | `vibra --format json --format human help` | exit 2 |
| `global_after_command` | negative | `vibra help --format json` | exit 2 |
| `extra_positional` | negative | `vibra check @app @lib` | exit 2 |
| `unavailable_command` | boundary | `vibra --format json check` | exit 4; `status: "unsupported"`, `result: "@command.unavailable"`, one `@tool.unavailable` diagnostic |
| `unavailable_each` | boundary | one well-formed line per command of the inventory with `supported: false` | exit 4 for each; the test is generated from the inventory |
| `json_single_document` | boundary | any command in JSON format | standard output parses as exactly one JSON value followed by one newline |
| `exit_table` | positive | the eight results | exits 0, 1, 1, 2, 3, 4, 5, 6 |
| `recovery_after_bad_global` | recovery | `vibra --format xml help` | exit 2, human format, because the format was not recognized |

Formatter: not applicable; this step formats no source.

## Diagnostics and schema changes

Codes: `@tool.unavailable`. Schemas published:
`urn:vibra:schema:v2:command-result`, `urn:vibra:schema:v2:diagnostic`.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-schema -p vibra-cli
cargo run --locked --offline -p vibra-cli --bin vibra -- --format json help
cargo test --locked --offline --workspace --all-targets --all-features
```

`vibra-schema` and `vibra-cli` are proposed packages this step creates.
Read: the `help` run prints one JSON object whose `capabilities` has 18
entries, one with `supported: true`; the workspace test run is green with
the `-v1` binary `vibra-v1` untouched.

## Excluded

Every command body (25, 26, M2, M3); `query`, `edit`, `mcp` (M3); `run`,
`eval`, `project` commands (M2); `build` (not in this release); workspace
discovery (25).

## Completion evidence

- [ ] `cargo metadata` lists `vibra-schema` and `vibra-cli`; the binary is named
      `vibra`.
- [ ] The inventory has one row per line of the command surface, and a host test
      compares the two.
- [ ] Every emitted envelope in the host tests validates against
      `schemas/v2/command-result.json`.
- [ ] The architecture test of step 02 has rows for both new crates.
- [ ] Validation commands exit 0.
