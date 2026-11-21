# Step 37: m2-exit

Kind: evidence. The exit record of M2: an audit of the corpus against the
owner table and the authority argument, the exit clauses with their
evidence, and the list of what stays unavailable. It adds cases only where
the audit finds a gap.
Depends on: 36.
Crates: `vibra-conformance`, `conformance/cases`.

## Prerequisites

- Steps 01 to 36 are merged.
- Read: this milestone's [README](../README.md) "Exit evidence" and
  "Exit clauses"; [architecture](../architecture.md) section 2; [owner
  table](../../m0/owners.md); [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md#required-coverage)
  required coverage 10, 11, 12; [questions](../spec-questions.md).

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-conformance/src/rules.rs:253` | `pub fn parse` | the active rule identities |
| `crates/vibra-conformance/src/corpus.rs:168` | `pub fn discover` | the cases the audit reads |

Proposed: `crates/vibra-conformance/tests/it/m2_coverage.rs`;
`docs/roadmap/v0.0.2/m2/exit.md`.

## Files owned

- `crates/vibra-conformance/tests/it/m2_coverage.rs`
- `crates/vibra-conformance/tests/it/main.rs`
- `conformance/rules/m2-37.tsv`
- `docs/roadmap/v0.0.2/m2/exit.md`
- `docs/roadmap/v0.0.2/m2/README.md`
- `docs/roadmap/README.md`

## Ordered tasks

1. Owner audit: a host test reads the owner table and the corpus and
   fails when a case id named for an M2 form does not exist, when an M2
   form has no positive case or no negative case where it has a
   diagnostic, and when a diagnostic code owned by M2 appears in no
   case's expectation. Invariant: a gap is closed by a case written from
   the specification, never by editing the table to match.
2. Authority audit: for every row of the authority argument, the test
   checks that each `V2-` id is an active case and each `ATK-` id is the
   name of a host test; `exit.md` lists them with their last result.
3. Both suites pass; the per-profile counts go into `exit.md`.
4. Exit clauses: `exit.md` gives, for each clause of the README, the case
   ids or evidence files, and for each `NOT RUN` row the platform and
   reason.
5. Open questions: each question of the list is marked answered, with the
   ruling and the rows that changed, or open, with the rows that wait.
6. The tree's contradictions listed in the questions are checked again;
   those that remain are reported to the specification's owner.
7. Status: the README says **Exited** with a link to `exit.md`; every
   step row says `landed`; the roadmap index line is current.

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `m2_owner_forms_have_cases` | positive, host | the owner table and the corpus | every M2 form has its cases | 07 Rule identities and example coverage |
| `m2_owner_gap_detected` | negative, host | the audit function given a table with one invented case id | a failure naming that id | the same |
| `m2_diagnostics_have_cases` | positive, host | the codes the owner table gives to M2 | each appears in an expectation | 07 Required coverage 2: "Every code is produced by a focused case" |
| `m2_authority_rows_have_cases` | positive, host | the authority table | each id resolves | architecture, section 2 |
| `m2_coverage_10` | positive, host | required coverage 10, item by item | a case id per item | 07 Required coverage 10 |
| `m2_coverage_11` | positive, host | required coverage 11, without the reserved HTTP items | a case or host-test id per item | 07 Required coverage 11 |
| `m2_coverage_12` | positive, host | required coverage 12 | a case id per item | 07 Required coverage 12 |
| `m2_no_unavailable_m2_form` | negative, host | every `@tool.unavailable` case of the corpus | none is an M2 form | this milestone's README, "Scope" |
| the corpus on three platforms | positive | CI | green | architecture, section 3 |

## Diagnostics and schema changes

None.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-ir -p vibra-interp -p vibra-types -p vibra-fs-scope -p vibra-host -p vibra-workspace -p vibra-schema -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0; no failed, unknown, or unavailable required case under
`staticV2` or `interpreterV2` for an M2 form. The test command names the
nine crates M2 touched, one build at a time, and is the widest run of the
milestone.

## Stays unavailable

`eval`, `query`, `lint`, `build`, the project commands, `@git`
dependencies and the lock, native execution; each is listed in `exit.md`
with its owner.

## Excluded

The pilot (38 to 41); the `fullV2` claim.

## Completion evidence

- [ ] `exit.md` has the exit-clause table, the authority table with
      results, the coverage maps, the question list, and the counts.
- [ ] Every `NOT RUN` of the milestone is listed once, with its reason.
- [ ] The README status and every step row are current.
- [ ] The validation commands exit 0.
