# Step 25: cli-fmt-check

Kind: vertical. `vibra fmt PATH [--write]` and `vibra check [TARGET]` on
the workspace engine, with the `format` conformance operation. This is the
first step where a user reads, checks, and formats the new language through
the command line.

## Prerequisites

- Steps 08, 15, and 24 landed. Later checker steps (16 to 22) widen what
  `check` accepts without changing this command.
- Read: [tooling chapter](../../../../spec/05-tooling.md) "One workspace
  engine", "Command surface" (the `TARGET` and `PATH` operands; the rule that
  `check` without a target covers every local target and the `@tests`
  unit), "Formatting", "Versioned
  envelope and results" (statuses), "Command payloads" (`fmt`, `check`,
  `ObligationRecord`), "Snapshot, claim and witness identity"
  (`workspaceRevision`); [programs
  chapter](../../../../spec/04-programs-and-packages.md) "VIBON data documents"
  and "Decoded values and canonical bytes" (the source-preserving
  formatter);
  [diagnostics chapter](../../../../spec/07-diagnostics-and-conformance.md)
  "Case operations and inputs" (`format`), "Recovery".

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-fmt/src/lib.rs` (steps 03 to 07) | `format_source`, `format_data` |
| `crates/vibra-workspace/src/{snapshot,source_graph,semantic}.rs` (steps 09, 10, 15) | the snapshot, the graph, the checked workspace |
| `crates/vibra-workspace-v1/src/format_plan.rs` | the captured-module rule: check the candidate text in place of the captured bytes |
| `crates/vibra-cli-v1/tests/it/{check_and_run,filesystem}.rs` | the host-test shapes for confined paths and check outcomes |
| `crates/vibra-cli/src/inventory.rs` (step 24) | two rows turn `supported: true` |

Proposed modules: `vibra-workspace::format_plan`; `vibra-cli::{fmt,check}`;
`vibra-conformance::format` (the handler); the `fmt` and `check` payloads in
`schemas/v2/command-result.json`.

## Ordered tasks

1. Path admission: `PATH` is workspace-relative, written with `/`, confined
   beneath the workspace root after resolving links; an escape is
   `@module.pathEscape` with result `@command.diagnostics`; an unreadable
   file is `@module.ioError`; an extension other than `.vib` or `.vibon` is
   `@command.invalidInput`.
2. `fmt` on a `.vib` file that is a captured module: read, format, check
   the candidate text in place of the captured bytes; the payload is
   `path`, `changed`, `written`, `text`. Preview by default; `--write`
   replaces the file atomically only when no error diagnostic is present.
   With diagnostics, `written` is `false` and `text` is the planned text.
   Invariant: when the reader recovered from a syntax error anywhere in
   the file, the text is the file's bytes unchanged; no identifier is
   renamed and no operand, labelled argument, field initializer, or dict
   pair is reordered.
3. `fmt` on a `.vib` file outside every unit and on a `.vibon` file: by
   syntax alone. For VIBON it is the source-preserving formatter of step
   03: it keeps comments, spellings, and the written order of entries,
   and changes only layout. For source, top-level forms keep their
   written order.
4. `check [TARGET]`: discover the project and build the snapshot. With no
   target, check every local target and the `@tests` unit; with a target,
   that target only. Payload `accepted` and `obligations`. Status `passed`
   when accepted; `refuted` when any obligation is refuted; `unknown`
   when an obligation is not established or a limit was reached;
   `unsupported` when a valid construct outside the profile was met
   (`@tool.unavailable` at the form) or a target has a kind the profile
   does not support (`@tool.unavailable` at its target record; the other
   targets are still checked). Result `@command.diagnostics`, exit 1,
   when any error diagnostic is present. A `TARGET` that names no local
   target is `@command.invalidInput`.
5. `workspaceRevision`: the digest of the snapshot, `null` only when
   discovery failed (`@project.notFound`).
6. `limits`: the published profile name with `reached` and `complete`.
7. Conformance: the `format` operation runs a confined tree through the
   same `format_plan` and compares `changed` and the text; the
   `workspaceCheck` handler of step 15 is unchanged.
8. Inventory: `fmt` and `check` become `supported: true` with profiles
   `readerV2` and `staticV2`.

## Test matrix

| Case or test | Kind | Input | Expected |
| --- | --- | --- | --- |
| `V2-TOOL-format-module` | formatter, format | `src/app/main.vib`: `fn  id takes {x:I32} returns r:I32 do return x endfn` | `changed: true`; text `fn id takes {x: I32} returns r: I32 do return x endfn\n`: one space between tokens, one after `:`, one line because it fits |
| `V2-TOOL-format-canonical` | formatter, format | the canonical text of the case above | `changed: false`; the same bytes |
| `V2-TOOL-format-recovered` | recovery, format | a module with `fn f takes {x: I32 returns r: I32 do return x endfn` (unclosed brace) | `@syntax.unmatchedDelimiter`; `text` is the file's bytes unchanged; `changed: false`, `written: false` |
| `V2-TOOL-format-semantic-error` | negative, format | a well-formed module with `return "a"` in a `fn` returning `I32` | `@type.mismatch`; `text` is the formatted text; `written: false` under `--write` |
| `V2-TOOL-format-vibon` | formatter, format | `project.vibon` with irregular spacing, a comment, entries out of canonical order, and the integer `0x10` | regular spacing; the comment, the written entry order, and the spelling `0x10` are kept |
| `V2-TOOL-format-top-order` | formatter, format | a module whose `fn` precedes its `type`, each irregularly spaced | both laid out; their written order is kept |
| `V2-TOOL-format-outside-unit` | boundary, format | `notes/scratch.vib` outside every target root, containing an unknown name | formatted by syntax alone; no `@name.unknownSymbol` |
| `fmt_preview_leaves_file` | positive, host | `vibra fmt src/app/main.vib` | exit 0; file bytes unchanged; `written: false` |
| `fmt_write` | positive, host | `vibra fmt src/app/main.vib --write` | file replaced; `written: true`, `text: null`; a second run reports `changed: false` |
| `fmt_extension` | negative, host | `vibra fmt README.md` | exit 2 |
| `fmt_escape` | negative, host | `vibra fmt ../other/a.vib` | exit 1; `@module.pathEscape` |
| `fmt_missing_file` | negative, host | `vibra fmt src/app/none.vib` | exit 1; `@module.ioError` |
| `check_passed` | positive, host | a library project with one valid module | exit 0; `status: "passed"`, `accepted: true`, `obligations: []` |
| `check_diagnostics` | negative, host | the same with `let x: I32 be "a" endlet` | exit 1; `@command.diagnostics`; `@type.mismatch` with a `SpanDocument` |
| `check_unknown_target` | negative, host | `vibra check @nope` | exit 2 |
| `check_target_is_path` | negative, host | `vibra check src/app` | exit 2: a target is an atom, never a path |
| `check_no_project` | negative, host | an empty directory | exit 1; `@project.notFound`; `workspaceRevision: null` |
| `check_unsupported` | boundary, host | a module with a `proc` declaration | exit 4; `status: "unsupported"`; `@tool.unavailable` over the declaration |
| `check_bin_target_unsupported` | boundary, host | a project with a valid `@lib` target and a `@bin` target | exit 4; `status: "unsupported"`; one `@tool.unavailable` at the `@bin` target's record in `project.vibon`; no diagnostic from the library |
| `check_covers_tests` | negative, host | a valid library and `tests/a.vib` with `let x: I32 be "a" endlet` in a test; `vibra check`, then `vibra check` with the library target named | the first run exits 1 with `@type.mismatch` in `tests/a.vib`; the second exits 0 |
| `check_revision_stable` | boundary, host | two runs on an unchanged tree, then one after an edit | the first two revisions are equal, the third differs |
| `check_json_validates` | positive, host | every host test above in JSON format | each envelope validates against the schema |

After steps 21 and 22 land, their steps add `check_refuted` and
`check_unknown` here (statuses `refuted` and `unknown`); the mapping is
implemented and unit-tested in this step with constructed obligation
records.

## Diagnostics and schema changes

Codes: `@module.pathEscape`, `@module.ioError`, `@project.notFound`,
`@syntax.unmatchedDelimiter`, `@type.mismatch`, `@tool.unavailable`.
Schema: `fmt` and `check` payloads and `ObligationRecord` added to
`urn:vibra:schema:v2:command-result`.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-cli -p vibra-schema -p vibra-workspace -p vibra-conformance
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo run --locked --offline -p vibra-cli --bin vibra -- --format json help
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: the conformance run lists the `V2-TOOL-format-*` cases as passed under
`toolingV2`; `help` reports three supported commands.

## Excluded

`lint` and `edit imports` (M3); `--write` plans with revision
checks (M3); `test` (26); formatting a directory (not in the surface).

## Completion evidence

- [ ] `conformance/rules.tsv` has one `active` row per case id above.
- [ ] A host test formats every `.vib` file under `stdlib/` and
      `conformance/cases` inputs marked canonical, and reports `changed: false`.
- [ ] The status mapping test covers `passed`, `refuted`, `unknown`,
      `unsupported`.
- [ ] Validation commands exit 0.
