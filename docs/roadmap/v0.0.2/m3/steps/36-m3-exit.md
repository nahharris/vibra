# Step 36: m3-exit

Kind: evidence. The exit record: every exit clause with its evidence, the
coverage audit over the owner table, the identity and evidence argument,
and required coverage 14, the schema validation of every ending of every
command, the counts of both suites, and what stays unavailable.
Depends on: 35.
Crates: `vibra-conformance`, `vibra-cli`, `conformance/cases`.

## Prerequisites

- Read: [M3 README](../README.md) exit evidence; [M2 exit](../../m2/exit.md)
  as the model; [07 Required coverage](../../../../spec/07-diagnostics-and-conformance.md#required-coverage) 14;
  [architecture](../architecture.md) section 2; [questions](../spec-questions.md)
  Q29.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-conformance/tests/it/m2_coverage.rs:1` | `//!` | the model of the audit |
| `crates/vibra-cli/tests/it/support.rs:41` | `pub(crate) fn envelope` | the harness that validates every envelope |
| `crates/vibra-schema/src/envelope.rs:67` | `pub const ALL: [Self; 8]` | every result the ending test must reach |

Proposed: `tests/it/m3_coverage.rs`; `crates/vibra-cli/tests/it/endings.rs`;
`docs/roadmap/v0.0.2/m3/exit.md`.

## Files owned

- `crates/vibra-conformance/tests/it/m3_coverage.rs`
- `crates/vibra-cli/tests/it/endings.rs`
- `docs/roadmap/v0.0.2/m3/exit.md`
- `docs/roadmap/v0.0.2/m3/README.md`
- `docs/roadmap/v0.0.2/README.md`
- `conformance/rules/m3-36.tsv`
- the case directories the audit requires

## Ordered tasks

1. `m3_coverage.rs`: reads the owner table rows whose step names M3, the
   identity and evidence argument's identities, and required coverage 14,
   and reports every gap at once: a case identity the corpus lacks, an
   `ATK-` test the workspace lacks, a code the table gives to M3 that no
   case expects, an invariant without a passing case, an item of coverage
   14 without a case or test, a case that expects `@tool.unavailable` for
   an M3 form.
2. `endings.rs`: drives every supported command to every result of the
   result table it can reach (`@command.ok`, `diagnostics`, `testFailed`,
   `invalidInput`, `operationalFailure`, `unavailable`, `trap`,
   `programFailed`), the denied, stale, malformed, and exhausted paths
   included, and validates every envelope; a result a command cannot reach
   is listed with the sentence that makes it so.
3. Close each gap with a case written by hand from the chapters, in
   `conformance/rules/m3-36.tsv`.
4. `exit.md`: the clause table, both suites' commands and counts, the
   identity and evidence rows with their results, coverage 14 item by
   item, what stays unavailable (`project *`, `build`, `mcp`, `--write`
   off Windows, the slow checks, no time limit, the forms the questions
   left open), `NOT RUN` rows, the questions' state. The README's status
   and the release roadmap's M3 row are updated on the integration branch.

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `m3_owner_forms_have_cases` | positive | the owner table | every M3 row's identities resolve, with an accepted and, where a diagnostic is named, a rejected case | execution model, "Definition of done" |
| `m3_owner_gap_detected` | negative | the table with one invented identity | exactly that identity reported | the same |
| `m3_diagnostics_have_cases` | positive | the codes the table gives to M3 | each expected by a case, or host-only with its sentence | 07 Required coverage 2 |
| `m3_evidence_rows_have_cases` | positive | rows I1 to I25 | each `V2-` identity an active case, each `ATK-` a host test | architecture, section 2 |
| `m3_coverage_14` | positive | the items of required coverage 14 | each has a case or host test | 07 Required coverage 14 |
| `m3_no_unavailable_m3_form` | positive | every case expecting `@tool.unavailable` | none is an M3 form | 07: "Unavailable is not invalid" |
| `ATK-schema-every-ending` | positive | every command through every reachable result | every envelope validates; the unreachable pairs are listed with their sentence | 05: "A producer MUST validate every document it emits"; release gate: "including denied, stale, malformed, and exhausted paths" |
| both suites | positive | the corpus runner and the test command of this step | 0 failed, 0 unknown, 0 unavailable required | execution model, "Milestone completion" |

## Diagnostics and schema changes

None.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-syntax -p vibra-fmt -p vibra-resolve -p vibra-types -p vibra-fs-scope -p vibra-host -p vibra-workspace -p vibra-schema -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo run --locked --offline -p vibra-conformance --bin vibra-reference -- --out docs/reference
pwsh -NoProfile -File conformance/tools/revisions.ps1 -Root conformance/cases -Check
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0; the counts are pasted into `exit.md`.

## Stays unavailable

Listed in `exit.md`.

## Excluded

Merging to `main`, which is Hannah's.

## Completion evidence

- [ ] `exit.md` has every clause met with its evidence, or the milestone
      is not exited and the record says which clause.
- [ ] The six audit tests and `ATK-schema-every-ending` pass.
- [ ] `conformance/rules/m3-36.tsv` has one `active` row per case the
      audit required.
- [ ] The validation commands exit 0.
