# Step 08: reader-exit

Kind: evidence. The reader and formatter are complete after step 07; this
step produces slice 1's exit evidence: golden trees for every production,
the idempotence corpus, the specification example inventory, and the fuzz
campaign.

## Prerequisites

- Step 07 landed.
- Read: [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md) "Required
  coverage" item 1, "Required implementation suites" (fuzzing), "Rule identities
  and example coverage"; the whole source chapter once more, production by
  production.
- Open questions: Q4 (tree snapshot format) must be answered before the
  golden trees are committed.

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-conformance-v1/tests/it/evidence.rs:236` (`parse_inventory`), `:360` (the ignored inventory test), `:527` (fence scanner) | the scanner and inventory shape are reused; the inventory moves to `conformance/spec-examples.tsv` |
| `crates/vibra-conformance-v1/src/bin/m1-fuzz.rs:183` | the fuzz driver shape: targets, iterations, seed, deep limit, corpus root |
| `fuzz-v1/m1.toml`, `fuzz-v1/README.md` | the profile file and its documentation |

Proposed: `crates/vibra-conformance/src/bin/fuzz.rs` (targets `lexer`,
`source`, `data`, `format`), `fuzz/corpus/*.vib|*.vibon`, `fuzz/README.md`,
`conformance/spec-examples.tsv`, tests `spec_examples.rs`.

## Ordered tasks

1. Golden trees: one `reader` case per production of the EBNF that has
   none yet, each with a `tree` snapshot in the Q4 format. Invariant: the
   owner table's production column names every case.
2. Idempotence corpus: every `.vib` and `.vibon` input of the corpus,
   formatted twice; a host test in `vibra-fmt` walks `conformance/cases`
   and asserts `format(parse(format(x))) == format(parse(x))` and that the
   token sequence without trivia is unchanged. Invariant: meaning is
   preserved by construction (same tokens, same order).
3. Specification inventory: scan `docs/spec/*.md` for `vibra` and `vibon`
   fences and inline backtick fragments; `conformance/spec-examples.tsv`
   lists each with `mode`, `classification` (`accepted`, `schematic`,
   `negative`), and the diagnostic a negative example expects; the test
   parses every fence losslessly, asserts accepted fences have no
   diagnostic, and fails on a stale or missing row. Invariant: a fence is
   never silently skipped.
4. Fuzz: the driver with targets for the lexer (random bytes), the source
   reader (keyword soup from the reserved-word table plus corpus mutations),
   the data reader, and the formatter (idempotence under mutation); the
   `ci-smoke` profile runs in CI; a finding is closed by a reduced
   `V2-SRC-READER-*` regression case. Invariant: no panic, no byte lost, no
   non-idempotent format.
5. Deep nesting: a host test parses, formats, and drops a 100,000-deep
   `[[[…]]]` and `do do … enddo enddo` without consuming the host stack.
6. Record the counts in this step's PR and in the README's status row.

## Test matrix

| Case id or test | Kind | Input | Expected |
| --- | --- | --- | --- |
| `V2-SRC-READER-golden-<production>` for each production still without a case | positive | the smallest complete document using the production | accepted; tree snapshot equals the committed golden tree |
| `V2-SRC-FMT-idempotence-<n>` | formatter | the ten longest corpus inputs | formatted twice equals formatted once |
| `spec_examples_are_inventoried_and_losslessly_exercised` | host | every fence of `docs/spec` | every `accepted` fence parses with no diagnostic; every row current |
| `fuzz --target lexer --iterations 100000` | boundary | random bytes | no panic; concatenated token texts equal the input |
| `fuzz --target source --iterations 100000` | recovery | keyword soup | no panic; every byte in one leaf; every diagnostic has a span inside the input |
| `fuzz --target format --iterations 20000` | formatter | mutated corpus inputs | idempotent; token order unchanged |
| `deeply_nested_source_and_data_survive` | boundary | depth 100,000 | parses, formats, drops; no stack overflow |

## Diagnostics and schema changes

None new. The tree snapshot format of Q4 is used by the golden trees.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-syntax -p vibra-fmt -p vibra-conformance
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo run --locked --offline -p vibra-conformance --bin fuzz -- --profile ci-smoke
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: the inventory test is `ok`, not `ignored`; the fuzz smoke prints the
iteration counts per target and `0 findings`; the runner's `readerV2`
line shows the full count of reader cases with `0 failed, 0 unavailable`.

## Excluded

Structural queries (M3); any semantic rule.

## Completion evidence

- [ ] `conformance/spec-examples.tsv` has one row per fence and the test passes
      un-ignored.
- [ ] Every production of the source chapter's EBNF has a `reader` case (the
      owner-table script's empty diff, re-run and pasted).
- [ ] The CI conformance job runs the new fuzz smoke beside the v1 one.
- [ ] The validation commands exit 0; counts pasted in the PR.
