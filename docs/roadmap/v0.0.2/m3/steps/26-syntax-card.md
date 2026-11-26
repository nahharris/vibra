# Step 26: syntax-card

Kind: vertical. The syntax card rendered from the reference data and a
template, and the checked examples, each run through the engine for its
registered verdict, the three shapes the pilot's models got wrong among the
negative ones.
Depends on: 25.
Crates: `vibra-conformance`, `docs/reference`.

## Prerequisites

- Read: [M2 pilot registration](../../m2/pilot/registration.md) "Card,
  `endlet` arm" (the card this one replaces, hand-written there);
  [M2 pilot observation](../../m2/pilot/observation.md) "Exploratory";
  [01 Design of the surface](../../../../spec/01-source-language.md#design-of-the-surface);
  [questions](../spec-questions.md) Q38.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-types/src/check/mod.rs:355` | `pub fn check_source` | checking a one-module example |
| `crates/vibra-workspace/src/semantic.rs:80` | `pub fn check_snapshot` | checking a project example |
| `crates/vibra-conformance/src/bin/conformance.rs:1` | `//!` | the model of a binary |

From 25: the generator and its data. Proposed: `src/bin/reference/card.rs`,
`examples.rs`; `docs/reference/syntax-card.md`, `docs/reference/examples/`.

## Files owned

- `crates/vibra-conformance/src/bin/reference/main.rs`
- `crates/vibra-conformance/src/bin/reference/card.rs`
- `crates/vibra-conformance/src/bin/reference/examples.rs`
- `crates/vibra-conformance/src/bin/reference/card-template.md`
- `crates/vibra-conformance/tests/it/reference_card.rs`
- `docs/reference/syntax-card.md`
- `docs/reference/examples/`

## Ordered tasks

1. The template: the sections of the M2 card (shape, values and types,
   declarations, statements, patterns, return and try, comparisons,
   guards, procedures, plans, grants, an example), with every list of
   reserved words, constructs and closers, sections, tag values, type
   heads, and prelude names rendered from `data/grammar.vibon` and the
   library manifest, never typed by hand. Invariant: a token the card
   names exists in the reader's tables.
2. `examples/`: each accepted example of the card as a `.vib` file or a
   project tree, and the negative examples, each with its registered
   verdict and codes in `examples/examples.tsv` (`path`, `verdict`,
   `codes`). The negative examples include the three shapes of the pilot:
   a record bound with a tuple pattern (`@type.mismatch` at the pattern),
   a `RelativePath` used without its import (`@name.unknownSymbol`), and a
   module beginning `module @pilot.gate` (`@syntax.invalidForm` at the
   token).
3. The test `reference_card::examples_give_registered_verdicts` runs each
   example through `check_source` or `check_snapshot` and compares verdict
   and codes; `reference_card::card_is_generated` compares the committed
   card with a regeneration.

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `ATK-reference-example-verdicts` | negative | one example's registered verdict changed in the table | the test fails naming it | README: "checked examples … checked against it" |
| card lists from data | positive | the card's reserved-word list | equals `data/grammar.vibon`'s | Q38 |
| tuple pattern on a record | negative | `let {first second} be pair endlet` where `pair` is a record | `@type.mismatch` at the pattern; the card's pattern section shows the record form `{first: f second: s}` | 07 Binding diagnostics; pilot observation |
| missing import | negative | `RelativePath([])` without `use @std.fs.RelativePath enduse` | `@name.unknownSymbol`; the card's declarations section shows the import | 04 Modules and imports |
| invented first line | negative | a module beginning `module @pilot.gate` | `@syntax.invalidForm` at `module`; the card's shape section says a file declares no module | 01: "There is no module declaration" |
| every accepted example checks | positive | each accepted example | `accepted` | 07: "Every accepted source example in this specification is a corpus case, or is marked schematic" |
| card drift | negative | the committed card edited by hand | `card_is_generated` fails | Q38 |

## Diagnostics and schema changes

None.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-reference -- --out docs/reference
git status --short docs/reference
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0; `git status` prints nothing after the regeneration.

## Stays unavailable

Unchanged.

## Excluded

A card per arm (the pilot is over); prose about semantics beyond the
template.

## Completion evidence

- [ ] `docs/reference/examples/examples.tsv` lists every example with its
      verdict; the three pilot shapes are among the negatives.
- [ ] The two tests were seen to fail on a hand edit.
- [ ] The validation commands exit 0.
