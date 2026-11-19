# Step 08: reader-exit

Kind: evidence. The reader and formatter are complete after step 07; this
step produces slice 1's exit evidence: golden trees for every production,
the idempotence corpus, the test that reads every fenced block of the
specification, and the fuzz campaign.

## Prerequisites

- Step 07 landed.
- Read: [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md) "Required
  coverage" item 1, "Required implementation suites" (fuzzing), "Rule identities
  and example coverage"; the whole source chapter once more, production by
  production; `@tree.v2` under "Persistent observations".

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-conformance-v1/tests/it/evidence.rs:360` (the ignored inventory test), `:527` (fence scanner) | the fence scanner is copied; the ignored test, its helpers, and its inventory file `docs/roadmap/v0.0.1/m1/syntax-examples.tsv` are deleted |
| `crates/vibra-conformance-v1/src/bin/m1-fuzz.rs:183` | the fuzz driver shape: targets, iterations, seed, deep limit, corpus root |
| `fuzz-v1/m1.toml`, `fuzz-v1/README.md` | the profile file and its documentation |

Proposed: `crates/vibra-conformance/src/bin/fuzz.rs` (targets `lexer`,
`source`, `data`, `format`), `fuzz/corpus/*.vib|*.vibon`, `fuzz/README.md`,
and the test `crates/vibra-conformance/tests/it/spec_examples.rs`.

## Ordered tasks

1. Golden trees: one `reader` case per production of the EBNF that has
   none yet, each with a `@tree.v2` snapshot. A choice production has no
   node, so its case shows the alternative it chose. Invariant: the owner
   table's production column names every case.
2. Idempotence corpus: every `.vib` and `.vibon` input of the corpus,
   formatted twice; a host test in `vibra-fmt` walks `conformance/cases`
   and asserts `format(parse(format(x))) == format(parse(x))` and that the
   token sequence without trivia is unchanged. Invariant: meaning is
   preserved by construction (same tokens, same order).
3. Specification examples: the test
   `every_fenced_block_of_the_specification_is_read` scans
   `docs/spec/*.md`, runs every `vibra` fence through `parse_source` and
   every `vibon` fence through `parse_data`, and asserts that the leaf
   texts concatenate to the fence and that the fence reads with no
   diagnostic. A fence the chapter presents as rejected or as incomplete
   grammar is named in one table inside the test, by chapter file and
   fence ordinal, with the exact codes it must produce; the test fails on
   a table row that matches no fence and on a row whose fence now reads
   clean. There is no inventory file. Delete the ignored v1 test
   `evidence::spec_examples_are_inventoried_and_losslessly_exercised`
   with the helpers only it uses, and delete
   `docs/roadmap/v0.0.1/m1/syntax-examples.tsv`. Invariant: a fence is
   never silently skipped.
4. Fuzz: the driver with targets for the lexer (random bytes), the source
   reader (keyword soup from the reserved-word table plus corpus mutations),
   the data reader, and the formatter (idempotence under mutation); the
   `ci-smoke` profile runs in CI; a finding is closed by a reduced
   `V2-SRC-READER-*` regression case. Invariant: no panic, no byte lost, no
   non-idempotent format.
5. Deep nesting: a host test parses, formats, and drops a 100,000-deep
   `[[[…]]]` and `do do … enddo enddo` without consuming the host stack.
6. Follow-ups: clear every entry of
   [reader-follow-ups.md](../reader-follow-ups.md), each with the code
   change and the cases it names, `@project.duplicateName` among them, then
   delete the file and this task's link. Invariant: the specification and
   the code agree on every entry before the file is deleted.
7. Record the counts in this step's PR and in the README's status row.

## Test matrix

| Case id or test | Kind | Input | Expected |
| --- | --- | --- | --- |
| `V2-SRC-READER-golden-<production>` for each production still without a case | positive | the smallest complete document using the production | accepted; tree snapshot equals the committed golden tree |
| `V2-SRC-FMT-idempotence-<n>` | formatter | the ten longest corpus inputs | formatted twice equals formatted once |
| `every_fenced_block_of_the_specification_is_read` | host | every `vibra` and `vibon` fence of `docs/spec` | lossless; no diagnostic outside the test's table of rejected fences; the table has no stale row |
| `fuzz --target lexer --iterations 100000` | boundary | random bytes | no panic; concatenated token texts equal the input |
| `fuzz --target source --iterations 100000` | recovery | keyword soup | no panic; every byte in one leaf; every diagnostic has a span inside the input |
| `fuzz --target format --iterations 20000` | formatter | mutated corpus inputs | idempotent; token order unchanged |
| `deeply_nested_source_and_data_survive` | boundary | depth 100,000 | parses, formats, drops; no stack overflow |

## Diagnostics and schema changes

None new in the registry. `@project.duplicateName` is first produced by
the follow-ups of task 6. The golden trees are `@tree.v2` documents.

## Ignored tests

`evidence::spec_examples_are_inventoried_and_losslessly_exercised` is
deleted by this step from `vibra-conformance-v1` and replaced by
`spec_examples::every_fenced_block_of_the_specification_is_read` in the
new `vibra-conformance`, which is never ignored. After this step one
ignored test remains,
`native_parity_inventory::a_pending_cell_names_a_package_of_the_plan`,
which [step 28](28-cutover.md) deletes.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-syntax -p vibra-fmt -p vibra-conformance
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo run --locked --offline -p vibra-conformance --bin fuzz -- --profile ci-smoke
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: `every_fenced_block_of_the_specification_is_read` is `ok`; the
workspace run lists no ignored test named `spec_examples_…`; the fuzz
smoke prints the
iteration counts per target and `0 findings`; the runner's `readerV2`
line shows the full count of reader cases with `0 failed, 0 unavailable`.

## Excluded

Structural queries (M3); any semantic rule.

## Completion evidence

- [ ] The fenced-block test passes and its table of rejected fences is
      reviewed against the chapters, row by row, in the PR.
- [ ] `docs/roadmap/v0.0.1/m1/syntax-examples.tsv` and the ignored v1 test
      no longer exist.
- [ ] Every production of the source chapter's EBNF has a `reader` case (the
      owner-table script's empty diff, re-run and pasted).
- [ ] The CI conformance job runs the new fuzz smoke beside the v1 one.
- [ ] `docs/roadmap/v0.0.2/m1/reader-follow-ups.md` no longer exists, and
      every case it named passes.
- [ ] The validation commands exit 0; counts pasted in the PR.
