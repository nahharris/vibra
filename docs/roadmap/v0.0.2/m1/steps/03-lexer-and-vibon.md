# Step 03: lexer-and-vibon

Kind: vertical. The lexical structure of the source chapter and the complete
VIBON data grammar, with the source-preserving data formatter, the
`@tree.v2` observation, the `reader` handler, and the first cases. VIBON is
the smallest document grammar that exercises every literal inside a
complete, formattable tree.

## Prerequisites

- Step 02 landed.
- Read: [source chapter](../../../../spec/01-source-language.md) "Design of the
  surface", "Lexical structure" (all of it, the EBNF included), and
  "Canonical format"; [programs
  chapter](../../../../spec/04-programs-and-packages.md) "VIBON data documents"
  with "Schemas" (steps 1, 2, and 6 of the validation order) and "Decoded
  values and canonical bytes"; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md) "Syntax and data
  diagnostics", "Recovery", the `reader` row of "Case operations and
  inputs", and `@tree.v2` under "Persistent observations".

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-syntax-v1/src/reader.rs:98` (`TokenKind`), `:248` (`lex`), `:439` (`SyntaxKind`, `CstNode`) | the design of a lossless token stream and tree; the S-expression lexer itself is not reused |
| `crates/vibra-syntax-v1/src/literal.rs` | literal classification, escapes, suffix handling: reuse the decoding algorithms where the rules are unchanged (escapes, suffix list) |
| `crates/vibra-syntax-v1/src/data.rs:17` (`DataNode`), `:180` (`decode_data_root`), `:506` (`canonical_data`) | the generic data value shape and the iterative drop; the grammar changes |
| `crates/vibra-fmt-v1/src/lib.rs:216` (`format_document`) | the entry shape |
| `crates/vibra-conformance-v1/src/reader.rs:19` (`ReaderV1Handler`) | the handler shape: parse by mode, format, reparse, compare |

## Files

Created:

| Path | Contents |
| --- | --- |
| `crates/vibra-syntax/Cargo.toml` | package `vibra-syntax`; dependency `vibra-diagnostics` |
| `crates/vibra-syntax/src/lib.rs` | module list and re-exports |
| `crates/vibra-syntax/src/lexer.rs` | bytes to tokens |
| `crates/vibra-syntax/src/literal.rs` | literal validation and decoding |
| `crates/vibra-syntax/src/cst.rs` | `CstNode`, `SyntaxKind`, bracket matching and recovery |
| `crates/vibra-syntax/src/data.rs` | the data grammar, `DataNode`, `DataValue`, decoded-value comparison |
| `crates/vibra-syntax/src/document.rs` | `DocumentMode`, `Document`, `parse_data`, `parse_source` |
| `crates/vibra-syntax/src/tree.rs` | the `@tree.v2` document of a `Document` |
| `crates/vibra-syntax/tests/it/main.rs`, `lexer.rs`, `data.rs`, `tree.rs` | host tests |
| `crates/vibra-fmt/Cargo.toml` | package `vibra-fmt`; dependencies `vibra-diagnostics`, `vibra-syntax` |
| `crates/vibra-fmt/src/lib.rs` | `format_document` |
| `crates/vibra-fmt/src/layout.rs` | line breaking, indentation, comments |
| `crates/vibra-fmt/src/data.rs` | the source-preserving VIBON formatter |
| `crates/vibra-fmt/tests/it/main.rs`, `data.rs` | host tests |
| `crates/vibra-conformance/src/reader.rs` | `ReaderV2Handler` |
| `conformance/cases/SRC-READER/<id>/` | one directory per case of the matrix: `case.toml`, the input, and the `formatted` and `tree` snapshots the row names |

Edited: the root `Cargo.toml` and `Cargo.lock` (two packages);
`crates/vibra-conformance/Cargo.toml` (dependencies `vibra-syntax`,
`vibra-fmt`), `src/lib.rs`, and `src/standard.rs` (the handler is registered for
the operation `reader`);
`crates/vibra-conformance/tests/it/architecture_boundary.rs` (rows
`vibra-syntax: [vibra-diagnostics]`,
`vibra-fmt: [vibra-diagnostics, vibra-syntax]`, and the two new entries of the
`vibra-conformance` row); `conformance/rules.tsv` (one `active` row per case
identity of the matrix, anchor `01-source-language.md#lexical-structure` or
`04-programs-and-packages.md#vibon-data-documents`); `.github/workflows/ci.yml`
(a second runner line after the v1 line of the conformance job: `cargo run
--locked --offline -p vibra-conformance --bin vibra-conformance -- --root
conformance/cases`).

## Ordered tasks

1. `lexer`: bytes to tokens. A byte-order mark, an invalid UTF-8 sequence,
   and a NUL are `@syntax.invalidEncoding` over the offending bytes. A
   document that is not valid UTF-8 has no tree. Token classes:
   whitespace, comment, identifier, reserved word (the 47 of the chapter,
   `void` among them), atom, discard, integer, float, string, doc string,
   character, and the punctuation `(`, `)`, `{`, `}`, `[`, `]`, `:`, `.`.
   Invariant: the concatenation of token texts is the input.
2. Literal rules: numeric suffixes, exponent, `-` directly before a digit;
   `@syntax.invalidNumericLiteral` over the token for an unknown suffix,
   an integer suffix on a float body, or trailing token text;
   `@syntax.invalidName` over the token for an identifier with `_`, `-`,
   `?`, `!`, or a non-ASCII letter; string escapes with `\u{…}` of one to
   six hexadecimal digits naming a scalar (`@syntax.invalidStringLiteral`
   over the token for a bad escape, a surrogate, or a value above
   U+10FFFF); a string that reaches the end of the file is
   `@syntax.unmatchedDelimiter` over the token and nothing else; the
   character literal rules (`@syntax.invalidCharacterLiteral` over the
   token). Two adjacent tokens of the classes identifier, reserved word,
   number, atom, string, character, and doc string with nothing between
   them are `@syntax.missingSeparator` at a zero-width span where the
   second token starts; both tokens are kept; a bracket or `:` between
   them never triggers it. Invariant: a malformed literal and an
   out-of-range literal are different errors; range is not checked here.
3. `cst`: the lossless tree for the bracket forms `{…}`, `[…]`, `(…)`,
   with the recovery rule of the diagnostics chapter. A closer that does
   not match the innermost open construct is resolved by kind: when a
   construct of its kind is open, every construct inside it closes with a
   `@missing` leaf and the closer closes that construct, with one
   `@syntax.unmatchedDelimiter` at the closer related to the innermost
   opener; when none is open, the closer is kept as its `@punctuation`
   leaf inside an `@error` node and the same code is reported at it. An
   opener still open at the end of the file gets a `@missing` leaf and
   `@syntax.unmatchedDelimiter` at the opener. Invariant: every byte is
   in exactly one leaf; a recovery step consumes input or returns.
4. `data`: validate the data grammar over the tree. Productions `tuple`,
   `record`, `field`, `list`, `dict`, `entry` are interior nodes;
   `string`, `character`, `integer`, `float`, `atom`, the `void` word, and
   the `true` and `false` identifiers are leaves. A document is exactly
   one value: a second value is `@syntax.invalidForm` over it. Any
   identifier other than `true` and `false` outside a field name, a doc
   string, and an application are `@syntax.invalidForm` at the token; an
   application is reported once, at its first token. A repeated record
   field is `@data.duplicateField` at the later field name. A repeated
   dict key is `@data.duplicateKey` at the later key: without a schema
   the keys are compared as decoded values with the written suffix as
   part of a number's value, so `"a"` and `"\u{61}"` are one key and `1`
   and `1u8` are two. Invariant: a `DataNode` keeps its span; decoding
   and drop are iterative.
5. `document`: `DocumentMode::from_path` by exact extension; a document
   presented to the wrong loader is `@data.invalidExtension` before its
   contents are read. `parse_data` is complete. `parse_source` lexes,
   builds the bracket tree, and reports one `@tool.unavailable` over the
   whole document, because the source grammar arrives in step 04.
   Invariant: the source mode accepts nothing in this step.
6. `tree`: the `@tree.v2` document of a `Document`: root kind `@data` or
   `@source`; interior nodes `{kind children}`, leaves `{kind text}`; node
   kinds are the production names in camelCase (`@tuple`, `@record`,
   `@field`, `@list`, `@dict`, `@entry`); the choice production `data` is
   never a node; leaf kinds `@identifier`, `@reservedWord`, `@integer`,
   `@float`, `@atom`, `@string`, `@docString`, `@character`,
   `@punctuation`, `@whitespace`, `@comment`, `@missing`; recovery is the
   `@error` node and the `@missing` leaf. Invariant: the leaf texts
   concatenate to the input.
7. `vibra-fmt::data`: the source-preserving formatter. It keeps comments,
   written spellings, and the written order of fields, elements, and
   entries, and applies the layout rules of "Canonical format": UTF-8, LF,
   one trailing newline, no trailing whitespace, two-space indentation, 88
   columns, one space between tokens, none inside brackets or before `:`,
   one after `:`, a form on one line when it fits and holds no comment,
   otherwise one entry per line with the closer on its own line; a
   comment on a construct's line stays at the end of that line and any
   other comment stays on its own line before the form that follows it. A
   document with any recovery is returned byte for byte. Invariant:
   formatting is idempotent and the reparse of the output has the same
   decoded value. It never produces canonical bytes.
8. `ReaderV2Handler`: parse the declared input by its extension, collect
   the diagnostics, format the document and check idempotence through the
   same loader, and produce the `formatted` and `tree` observations. A
   snapshot is compared as a decoded VIBON value, never as text, so the
   layout of a snapshot file is free. Register the handler, add the CI
   line.
9. Author the cases of the matrix and their `rules.tsv` rows.

## Test matrix

Case identities are under `V2-SRC-READER`. Inputs are complete documents,
named `input.vibon` unless stated, with no trailing newline unless shown.
Spans are byte offsets `start..end`.

| Case id | Kind | Input | Expected |
| --- | --- | --- | --- |
| `V2-SRC-READER-vibon-literals` | positive | `{text: "a\n" ch: 'x' n: -7i64 u: 1u8 f: 2.5f64 e: 1e3 b: true a: @std.io v: void}` | accepted; formatted equals the input plus one LF |
| `V2-SRC-READER-vibon-nested` | positive, formatter | `[{1i32 "a"}  {} [] [:] ["k": {x: 1u8}]]` (two spaces after the first tuple) | accepted; formatted `[{1i32 "a"} {} [] [:] ["k": {x: 1u8}]]` and LF; idempotent |
| `V2-SRC-READER-vibon-comment` | positive, formatter | `// top`, LF, `{a: 1u8 // one`, LF, `b: 2u8}` | accepted; formatted is five lines: `// top`, `{`, `  a: 1u8 // one`, `  b: 2u8`, `}` |
| `V2-SRC-READER-vibon-wraps` | formatter | a record of four string fields whose one-line form is 95 columns | one field per line, two-space indentation, closer on its own line |
| `V2-SRC-READER-vibon-order-kept` | formatter | `["b": 1 "\u{61}": 2.0]` | accepted; formatted equals the input plus one LF: no reordering, no respelling, no suffix added |
| `V2-SRC-READER-tree-list` | positive | `[1u8]` | accepted; the `tree` snapshot is the chapter's first `@tree.v2` example |
| `V2-SRC-READER-tree-recovered` | recovery | `[1u8}` | `@syntax.unmatchedDelimiter` at `4..5` and at `0..1`; the `tree` snapshot is the chapter's second example; formatter returns the bytes unchanged |
| `V2-SRC-READER-encoding-bom` | negative | the bytes `EF BB BF 7B 7D` | `@syntax.invalidEncoding` at `0..3` |
| `V2-SRC-READER-encoding-nul` | negative | the bytes `7B 00 7D` | `@syntax.invalidEncoding` at `1..2` |
| `V2-SRC-READER-encoding-utf8` | negative | the bytes `7B FF 7D` | `@syntax.invalidEncoding` at `1..2`; no tree |
| `V2-SRC-READER-numeric-suffix` | negative | `{a: 1i128 b: 2.5i32 c: 3u8x}` | `@syntax.invalidNumericLiteral` at `4..9`, `13..19`, `23..27` |
| `V2-SRC-READER-string-escape` | negative | `{a: "\q" b: "\u{d800}" c: "\u{110000}"}` | `@syntax.invalidStringLiteral` at `4..8`, `12..22`, `26..38` |
| `V2-SRC-READER-string-unterminated` | recovery | `"open` | one `@syntax.unmatchedDelimiter` at `0..5` and nothing else; formatter returns the bytes unchanged |
| `V2-SRC-READER-character` | negative | `{a: '' b: 'ab' c: '\q'}` | `@syntax.invalidCharacterLiteral` at `4..6`, `10..14`, `18..22` |
| `V2-SRC-READER-name-invalid` | negative | `{my_field: 1u8}` | `@syntax.invalidName` at `1..9` |
| `V2-SRC-READER-separator` | negative | `{a: 1u8"x"}` | `@syntax.missingSeparator` at `7..7` |
| `V2-SRC-READER-separator-bracket` | positive | `{{1u8}{2u8}}` | accepted: a bracket separates |
| `V2-SRC-READER-closer-crossed` | recovery | `{a: [1u8 2u8}` | one `@syntax.unmatchedDelimiter` at `12..13`, related `4..5`; the list closes with a `@missing` leaf; bytes unchanged by the formatter |
| `V2-SRC-READER-closer-missing` | recovery | `{a: 1u8` | `@syntax.unmatchedDelimiter` at `0..1` |
| `V2-SRC-READER-vibon-two-values` | negative | `{} {}` | `@syntax.invalidForm` at `3..5` |
| `V2-SRC-READER-vibon-identifier` | negative | `{a: foo}` | `@syntax.invalidForm` at `4..7` |
| `V2-SRC-READER-vibon-application` | negative | `{a: Option.some(value: 1i32)}` | one `@syntax.invalidForm` at `4..10` |
| `V2-SRC-READER-vibon-doc-string` | negative | `{a: """x"""}` | `@syntax.invalidForm` at `4..11` |
| `V2-SRC-READER-data-duplicate-field` | negative | `{a: 1u8 a: 2u8}` | `@data.duplicateField` at `8..9` |
| `V2-SRC-READER-data-duplicate-key` | negative | `["a": 1u8 "\u{61}": 2u8]` | `@data.duplicateKey` at `10..18` |
| `V2-SRC-READER-data-number-keys` | positive | `[1: "a" 1u8: "b"]` | accepted: without a schema the suffix is part of the key |
| `V2-SRC-READER-data-extension` | negative | `input.vib` declared as the data input | `@data.invalidExtension`; no contents read |

Host test, not a case: `parse_source` of
`fn f returns r: I32 do return 1i32 endfn` gives one `@tool.unavailable` at
`0..40` and no other diagnostic. Step 04 deletes this test.

## Diagnostics and schema changes

Codes emitted: `@syntax.invalidEncoding`, `@syntax.invalidName`,
`@syntax.invalidNumericLiteral`, `@syntax.invalidStringLiteral`,
`@syntax.invalidCharacterLiteral`, `@syntax.unmatchedDelimiter`,
`@syntax.missingSeparator`, `@syntax.invalidForm`, `@data.duplicateField`,
`@data.duplicateKey`, `@data.invalidExtension`, `@tool.unavailable`. All
registered. Schema: `@tree.v2`, written by `vibra-syntax::tree`; no JSON
schema.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-syntax -p vibra-fmt -p vibra-conformance
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: the runner prints one `PASS` line per case above and
`readerV2 conformance: 27 passed, 0 failed, 0 unknown, 0 unavailable`. The
host tests include a property test that lexing then concatenating token
texts is the identity over random bytes that are valid UTF-8, and that
`format(parse(format(parse(x))))` equals `format(parse(x))` over the corpus
inputs.

## Excluded

The source grammar (step 04 onward); typed VIBON schemas and canonical
bytes (step 09); structural queries (M3).

## Completion evidence

- [ ] Every case id above exists under `conformance/cases/SRC-READER/` and
      passes.
- [ ] `conformance/rules.tsv` has one `active` row per case id above.
- [ ] `.github/workflows/ci.yml` runs the new runner against
      `conformance/cases` beside the v1 line.
- [ ] A host test proves every byte of a random input is in exactly one
      leaf.
- [ ] The two `@tree.v2` examples of the diagnostics chapter are host tests
      of `vibra-syntax::tree` as well as cases.
- [ ] The validation commands exit 0.
