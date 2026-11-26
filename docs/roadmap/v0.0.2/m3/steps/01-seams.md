# Step 01: seams

Kind: infrastructure. Every file two M3 steps would otherwise share gains
its seam here, before any feature lands: module files, dispatch arms that
answer unavailable, payload variants and schema files, hooks that record
nothing, case operations and manifest keys, the output bound. No observable
behaviour changes except the `help` inventory's `operations` of `eval` and
`edit`, which the new case operations fix.
Depends on: —.
Crates: `vibra-workspace`, `vibra-types`, `vibra-syntax`, `vibra-fmt`,
`vibra-cli`, `vibra-schema`, `vibra-conformance`, `conformance/rules`.

## Prerequisites

- Read: [architecture](../architecture.md) "The seams of step 01" and
  "Boundary types"; [05 Command surface](../../../../spec/05-tooling.md#command-surface);
  [05 Versioned envelope and results](../../../../spec/05-tooling.md#versioned-envelope-and-results);
  [07 Case operations and inputs](../../../../spec/07-diagnostics-and-conformance.md#case-operations-and-inputs);
  [questions](../spec-questions.md) Q14, Q28, Q30, Q31.
- The M2 seams: [m2 step 01](../../m2/steps/01-seams.md) and
  [m2 step 04](../../m2/steps/04-checker-effects-seam.md), whose shape this
  step repeats.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-cli/src/lib.rs:130` | `fn execute` | gains one arm per new command |
| `crates/vibra-cli/src/inventory.rs:68` | `const UNSUPPORTED: Line` | `supported` becomes a read of the command module's constant |
| `crates/vibra-cli/src/inventory.rs:254` | `pub static OPERATION_PROFILES` | gains `eval` and `edit` |
| `crates/vibra-cli/src/args.rs:130` | `pub fn parse` | the operands and options of the new commands |
| `crates/vibra-cli/src/render.rs:268` | `pub fn emit` | the output bound |
| `crates/vibra-schema/src/envelope.rs:199` | `pub enum Payload` | one variant per new payload |
| `crates/vibra-schema/src/validate.rs:260` | `pub fn validate_command_result` | loads the payload schemas by `$ref` |
| `crates/vibra-conformance/src/profile.rs:148` | `pub enum ConformanceOperation` | `Eval`, `Edit` |
| `crates/vibra-conformance/src/standard.rs:192` | `pub fn standard_dispatcher` | registers the three stub handlers |
| `crates/vibra-conformance/src/manifest.rs:113` | `pub struct CaseInputs` | `subject`, `include`, `expand`, `arguments`, `edit` keys |
| `crates/vibra-conformance/src/manifest.rs:171` | `pub struct CaseExpectations` | `query`, `index`, `eval`, `editPlan` |
| `crates/vibra-workspace/src/snapshot.rs:167` | `pub fn capture` | calls `approval::capture` |
| `crates/vibra-workspace/src/semantic.rs:80` | `pub fn check_snapshot` | calls `approval::validate` |
| `crates/vibra-types/src/check/expr.rs:527` | `pub(crate) struct BodyChecker` | one call into `facts` where an expression's type is final |
| `crates/vibra-types/src/check/mod.rs:66` | `pub struct CheckResult` | `facts()` returns the empty table |
| `crates/vibra-conformance/tests/it/architecture_boundary.rs:23` | `const ARCHITECTURE` | the new arrow `vibra-workspace` → `vibra-fs-scope` |

Proposed: the modules listed under "Files owned", each empty or returning
its "not yet" value, with a doc comment naming the step that fills it.

## Files owned

- `crates/vibra-workspace/Cargo.toml`
- `crates/vibra-workspace/src/lib.rs`
- `crates/vibra-workspace/src/snapshot.rs`
- `crates/vibra-workspace/src/semantic.rs`
- `crates/vibra-workspace/src/identity/mod.rs`
- `crates/vibra-workspace/src/identity/claim.rs`
- `crates/vibra-workspace/src/identity/witness.rs`
- `crates/vibra-workspace/src/approval.rs`
- `crates/vibra-workspace/src/present.rs`
- `crates/vibra-workspace/src/eval.rs`
- `crates/vibra-workspace/src/query/mod.rs`
- `crates/vibra-workspace/src/query/subject.rs`
- `crates/vibra-workspace/src/query/structural.rs`
- `crates/vibra-workspace/src/query/semantic.rs`
- `crates/vibra-workspace/src/query/effects.rs`
- `crates/vibra-workspace/src/query/expansions.rs`
- `crates/vibra-workspace/src/query/index.rs`
- `crates/vibra-workspace/src/edit/mod.rs`
- `crates/vibra-workspace/src/edit/plan.rs`
- `crates/vibra-workspace/src/edit/replace.rs`
- `crates/vibra-workspace/src/edit/apply.rs`
- `crates/vibra-workspace/src/edit/journal.rs`
- `crates/vibra-workspace/src/edit/rename.rs`
- `crates/vibra-workspace/src/edit/imports.rs`
- `crates/vibra-workspace/src/edit/fix.rs`
- `crates/vibra-workspace/src/edit/lint.rs`
- `crates/vibra-workspace/tests/it/main.rs`
- `crates/vibra-types/src/check/facts.rs`
- `crates/vibra-types/src/check/mod.rs`
- `crates/vibra-types/src/check/expr.rs`
- `crates/vibra-types/src/check/pattern.rs`
- `crates/vibra-types/src/check/declare.rs`
- `crates/vibra-types/src/check/tests.rs`
- `crates/vibra-syntax/src/lib.rs`
- `crates/vibra-syntax/src/position.rs`
- `crates/vibra-syntax/tests/it/main.rs`
- `crates/vibra-fmt/src/lib.rs`
- `crates/vibra-fmt/src/fragment.rs`
- `crates/vibra-fmt/tests/it/main.rs`
- `crates/vibra-cli/src/lib.rs`
- `crates/vibra-cli/src/args.rs`
- `crates/vibra-cli/src/inventory.rs`
- `crates/vibra-cli/src/render.rs`
- `crates/vibra-cli/src/query.rs`
- `crates/vibra-cli/src/eval.rs`
- `crates/vibra-cli/src/edit.rs`
- `crates/vibra-cli/src/approve.rs`
- `crates/vibra-cli/src/lint.rs`
- `crates/vibra-cli/tests/it/main.rs`
- `crates/vibra-cli/tests/it/output_bound.rs`
- `crates/vibra-cli/tests/it/inventory.rs`
- `crates/vibra-cli/tests/it/unavailable.rs`
- `crates/vibra-schema/src/lib.rs`
- `crates/vibra-schema/src/envelope.rs`
- `crates/vibra-schema/src/validate.rs`
- `crates/vibra-schema/src/query_payload.rs`
- `crates/vibra-schema/src/eval_payload.rs`
- `crates/vibra-schema/src/edit_plan.rs`
- `crates/vibra-schema/src/approve_payload.rs`
- `crates/vibra-schema/src/lint_payload.rs`
- `crates/vibra-schema/src/index.rs`
- `crates/vibra-schema/schemas/v2/command-result.json`
- `crates/vibra-schema/schemas/v2/payloads/`
- `crates/vibra-schema/tests/it/main.rs`
- `crates/vibra-schema/tests/it/command_result.rs`
- `crates/vibra-conformance/src/lib.rs`
- `crates/vibra-conformance/src/profile.rs`
- `crates/vibra-conformance/src/standard.rs`
- `crates/vibra-conformance/src/manifest.rs`
- `crates/vibra-conformance/src/query.rs`
- `crates/vibra-conformance/src/eval.rs`
- `crates/vibra-conformance/src/edit.rs`
- `crates/vibra-conformance/tests/it/main.rs`
- `crates/vibra-conformance/tests/it/manifest.rs`
- `crates/vibra-conformance/tests/it/architecture_boundary.rs`
- `conformance/rules/m3-01.tsv`

## Ordered tasks

1. `vibra-workspace`: add the dependency `vibra-fs-scope`, the `mod` lines
   and the `pub use` of the boundary types' names as empty modules, and in
   `snapshot.rs` the call `approval::capture(root)` whose result (no
   record) joins the revision records, and in `semantic.rs` the call
   `approval::validate(..)` whose result (no diagnostic) joins the check.
   Invariant: `cargo test -p vibra-workspace` passes unchanged; the
   revision of `examples/hello` is unchanged.
2. `vibra-types`: `check/facts.rs` with `Facts`, `NodeFact`, and the three
   recording functions as no-ops; one call from `expr.rs` where an
   expression's type becomes final, one from `pattern.rs` where a pattern
   is typed, one from `declare.rs` where a header is declared;
   `CheckResult::facts()`. Invariant: no checker test changes; the shared
   files make no fact decision of their own.
3. `vibra-syntax::position` and `vibra-fmt::fragment`: empty modules with
   their `mod` lines. `edit::apply` holds the two stubs `apply` and
   `publish_document`, each returning `Applied::Unavailable`, so that
   steps 13 and 14 never share a file.
4. `vibra-cli`: the command modules, each `pub const SUPPORTED: bool =
   false` and `pub fn run(&Invocation) -> Response` answering
   `@command.unavailable`; the `execute` arms; `inventory::Line.supported`
   read from the modules; `OPERATION_PROFILES` with `("eval",
   "interpreterV2")` and `("edit", "toolingV2")`; `args.rs` parses the
   operands of `query`, `eval`, `approve`, `lint`, `edit replace`, `edit
   rename`, `edit imports`, `edit fix`, and the option `--expect-revision
   DIGEST` on every mutating command, into `Invocation`, while a
   well-formed line of an unavailable command stays unavailable whatever
   its operands hold. Invariant: `help` lists the same support as before,
   and `edit` and `eval` list their operations.
5. `render.rs`: `OUTPUT_LIMIT` (16 MiB), the limit name `outputBytes`, and
   an assertion in `emit` that the written envelope is within the bound;
   a document over it is `@command.trap` with the empty payload.
6. `vibra-schema`: the `Payload` variants `Query`, `Eval`, `Edit`,
   `Approve`, `Lint`, each a `Box` of an empty struct in its module; the
   command-result schema split so each payload is one file under
   `schemas/v2/payloads/`, referenced by `$ref`, and the existing five
   payload tests pass byte for byte on their documents.
7. `vibra-conformance`: `ConformanceOperation::{Eval, Edit}` with their
   profiles and `as_str`; `CaseInputs` gains `subject`, `include`,
   `expand`, `arguments`, `edit` (a record of `kind`, `subject`,
   `newName`, `path`, `data`); `CaseExpectations` gains `query`, `index`,
   `eval`, `editPlan`; the `query`, `eval`, and `edit` handlers registered
   and answering unavailable; the architecture table's new arrow.
8. `conformance/rules/m3-01.tsv` with the header line only.

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| help unchanged | positive | `vibra --format json help` | the same `supported` flags as before this step; `eval` lists `["eval"]` and `interpreterV2`, `edit fix` and the other edits list `["edit"]` and `toolingV2` | 05 Command payloads: `CommandCapability` |
| unavailable operands not validated | positive | `vibra --format json query nonsense --include bogus` | `@command.unavailable`, exit 4, no diagnostic | 05 Command surface: "A well-formed line of such a command is unavailable whatever its operands hold" |
| expect-revision parsed | positive | `vibra --format json edit imports src/app/main.vib --expect-revision sha256:00` | `@command.unavailable` (the command is not yet supported); the option is accepted by the grammar | Q14 |
| expect-revision repeated | negative | the same with the option twice | `@command.invalidInput`, exit 2 | 05 Command surface: "a repeated single option … are invalid input" |
| `ATK-output-bound-asserted` | negative | a test hook makes `check` render a payload of 17 MiB | `@command.trap` with the empty payload; the envelope written is within the bound and validates | 05: "It never emits truncated JSON" |
| payload schemas by reference | positive | every envelope of the existing CLI tests | validates against the split schema exactly as before | 05 Versioned envelope and results |
| manifest keys | positive | a `case.toml` with `operation = "query"`, `subject`, `include`, `expand`, `expect.query` | decodes; `operation = "eval"` with `arguments` decodes under `interpreterV2`; `operation = "edit"` with `edit = {kind = "replace", subject = "...", data = "new.vibon"}` decodes under `toolingV2` | 07 Case operations and inputs; Q30, Q31 |
| manifest: wrong profile | negative | `operation = "eval"` with `profile = "toolingV2"` | rejected | 07 Conformance profiles: "a case manifest that names any other is malformed" |
| handlers unavailable | positive | a `query` case run through the runner | reported unavailable, not passed | 07: "A missing conformance handler is an unavailable observation, not a passing one" |
| facts hook is silent | positive | `cargo test -p vibra-types` | passes unchanged; `CheckResult::facts()` of any check is empty | architecture, "The seams of step 01" |
| approval hooks are silent | positive | `cargo test -p vibra-workspace -p vibra-cli` | passes unchanged; the `examples/hello` revision pin is unchanged | the same |
| architecture arrow | positive | `architecture_boundary::workspace_matches_the_declared_architecture` | passes with `vibra-fs-scope` under `vibra-workspace`; removing the table row fails it | roadmap, "Architecture boundary" |

## Diagnostics and schema changes

Codes: none new. Schema: the command-result schema is restructured into one
file per payload under `schemas/v2/payloads/`; no document changes meaning.
The five new payload schemas are empty objects until their steps.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-syntax -p vibra-fmt -p vibra-types -p vibra-workspace -p vibra-schema -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
pwsh -NoProfile -File conformance/tools/revisions.ps1 -Root conformance/cases -Check
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0; the corpus counts are those of the M2 exit plus fix E's
cases, unchanged by this step.

## Stays unavailable

Every M3 command: `query`, `eval`, `approve`, `lint`, `edit fix`, `edit
rename`, `edit imports`, `edit replace`. The `query`, `eval`, and `edit`
case operations.

## Excluded

Every behaviour; this step makes room.

## Completion evidence

- [ ] `git diff --stat` names every file of "Files owned" and no other.
- [ ] `git grep -n "SUPPORTED" crates/vibra-cli/src` shows one constant per
      command module and one read in `inventory.rs`.
- [ ] `git grep -n "facts::" crates/vibra-types/src/check` shows exactly
      three call sites, in `expr.rs`, `pattern.rs`, and `declare.rs`.
- [ ] The validation commands exit 0, and the corpus counts are unchanged.
