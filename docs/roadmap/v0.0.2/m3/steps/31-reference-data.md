# Step 31: reference-data

Kind: infrastructure. The reference generator and the generated data
files: the grammar tables, the diagnostic registry, the command inventory,
and the published schemas, written from the implementation into
`docs/reference/data/` and checked against it by a test that fails on
drift.
Depends on: 01.
Crates: `vibra-conformance`, `docs/reference`.

## Prerequisites

- Read: [M3 README](../README.md) "Generated references";
  [05 Schemas, delivery and exclusions](../../../../spec/05-tooling.md#schemas-delivery-and-exclusions);
  [07 Required coverage](../../../../spec/07-diagnostics-and-conformance.md#required-coverage) 2;
  [questions](../spec-questions.md) Q38.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-syntax/src/source/keyword.rs:21` | `pub const CONSTRUCTS` | constructs and closers |
| `crates/vibra-syntax/src/source/keyword.rs:86` | `pub const ADMISSIBLE_SECTIONS` | sections per construct |
| `crates/vibra-syntax/src/source/keyword.rs:158` | `pub const TAG_SETS` | tag sets |
| `crates/vibra-syntax/src/source/keyword.rs:76` | `pub const TYPE_HEADS` | type heads |
| `crates/vibra-diagnostics/src/registry.rs:185` | `pub enum DiagnosticCode` | the registry |
| `crates/vibra-cli/src/inventory.rs:292` | `pub fn capabilities` | the command inventory |
| `crates/vibra-schema/src/lib.rs:64` | `pub const COMMAND_RESULT_SCHEMA` | the published schemas |
| `crates/vibra-conformance/src/bin/conformance.rs:1` | `//!` | the model of a binary of the crate |

Proposed: `src/bin/reference/main.rs`, `data.rs`; the test
`tests/it/reference.rs`.

## Files owned

- `crates/vibra-conformance/Cargo.toml`
- `crates/vibra-conformance/src/bin/reference/main.rs`
- `crates/vibra-conformance/src/bin/reference/data.rs`
- `crates/vibra-conformance/tests/it/reference.rs`
- `docs/reference/README.md`
- `docs/reference/data/`

## Ordered tasks

1. The binary `vibra-reference --out DIR`: writes `data/grammar.vibon`
   (reserved words, constructs with closers, admissible sections per
   construct in order, tag sets, type heads, expression words), as a
   canonical document laid out by the formatter's rules;
   `data/diagnostics.vibon` (every code with level, domain, summary, fix
   capability, in registry order); `data/commands.vibon` (every
   capability of `help`); `data/schemas/<id>.json` (each published schema
   as embedded). A dependency on `vibra-cli` and `vibra-schema` is added to
   the binary's crate; the architecture table already permits the arrows.
2. `docs/reference/README.md`: what each file is, that every file is
   generated, and the command that regenerates them.
3. The test `reference::committed_files_are_generated`: generates into
   memory and compares with every committed file; a difference names the
   file.

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `ATK-reference-drift-detected` | negative | one committed data file edited by hand | the test fails naming it | README: "generated from the implementation and checked against it" |
| grammar complete | positive | `data/grammar.vibon` | every construct of `CONSTRUCTS` and every section of `SECTIONS` appears once | 07 Required coverage 2 |
| registry complete | positive | `data/diagnostics.vibon` | one entry per code of the registry, in registry order, with the two `@safe` capabilities after step 28 | 07: "Every code is produced by a focused case and has exactly one entry" |
| commands complete | positive | `data/commands.vibon` | one entry per line of the command surface | 05 `CommandCapability`: "one per line of the command surface, in its order" |
| schemas complete | positive | `data/schemas/` | one file per `schemaIds` entry of every supported command | 05: "The toolchain MUST publish … JSON Schemas" |
| canonical layout | positive | each `.vibon` | decodes and re-serializes to the same bytes | 04 Decoded values and canonical bytes |

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

The card and the examples (32).

## Completion evidence

- [ ] `docs/reference/data/` holds the four kinds of file and nothing
      written by hand.
- [ ] The drift test was seen to fail on a hand edit.
- [ ] The validation commands exit 0.
