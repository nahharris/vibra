# Step 03: lexer-and-vibon

Kind: vertical. The lexical structure of the source chapter and the complete
VIBON data grammar, with the canonical data format, the `reader` handler,
and the first cases. VIBON is the smallest document grammar that exercises
every literal inside a complete, formattable tree.

## Prerequisites

- Step 02 landed.
- Read: [source chapter](../../../../spec/01-source-language.md) "Design of the
  surface", "Lexical structure" (all of it, the EBNF included); [programs
  chapter](../../../../spec/04-programs-and-packages.md) "VIBON data documents";
  [diagnostics chapter](../../../../spec/07-diagnostics-and-conformance.md)
  "Recovery" and the `reader` row of "Case operations and inputs".
- Open questions: Q2 (`atom-part` with `-` in the EBNF), Q4 (tree snapshot
  format), Q5 (what `fmt` does to a bare `.vibon`), Q6 (VIBON terminals
  that are identifiers in source), Q7 (`@syntax.missingSeparator` trigger).
  Q2, Q6, and Q7 must be answered before this step starts; Q4 and Q5 have
  the provisional answers the architecture states.

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-syntax-v1/src/reader.rs:98` (`TokenKind`), `:248` (`lex`), `:439` (`SyntaxKind`, `CstNode`) | the design of a lossless token stream and tree; the S-expression lexer itself is not reused |
| `crates/vibra-syntax-v1/src/literal.rs` | literal classification, escapes, suffix handling: reuse the decoding algorithms where the rules are unchanged (escapes, suffix list) |
| `crates/vibra-syntax-v1/src/data.rs:17` (`DataNode`), `:180` (`decode_data_root`), `:506` (`canonical_data`) | the generic data value shape and the iterative drop; the grammar changes |
| `crates/vibra-fmt-v1/src/lib.rs:216` (`format_document`) | the entry shape |
| `crates/vibra-conformance-v1/src/reader.rs:19` (`ReaderV1Handler`) | the handler shape: parse by mode, format, reparse, compare |

Proposed modules: `vibra-syntax::{lexer, cst, data, document}`,
`vibra-fmt::{layout, data}`, `vibra-conformance::reader`
(`ReaderV2Handler`).

## Ordered tasks

1. `lexer`: bytes to tokens. Reject BOM, invalid UTF-8, NUL
   (`@syntax.invalidEncoding`, span of the offending bytes; one diagnostic
   and the rest of the file is still tokenized as bytes). Token kinds:
   whitespace, line comment, identifier, reserved word, atom, discard,
   integer, float, string, doc string, character, `(`, `)`, `{`, `}`, `[`,
   `]`, `:`, error. Invariant: the concatenation of token texts is the
   input.
2. Literal rules: numeric suffixes, exponent, `-` followed by a digit;
   `@syntax.invalidNumericLiteral` over the token for an unknown suffix, an
   integer suffix on a float body, or trailing text; `@syntax.invalidName`
   for an identifier with `_`, `-`, `?`, `!`, or a non-ASCII letter; string
   escapes with `\u{…}` of one to six digits naming a scalar
   (`@syntax.invalidStringLiteral` over the token for a bad escape, a
   surrogate, or above U+10FFFF); an unterminated string is
   `@syntax.unmatchedDelimiter` only, keeps its bytes, and ends at EOF;
   character literal rules (`@syntax.invalidCharacterLiteral`); adjacent
   tokens without whitespace per Q7 (`@syntax.missingSeparator`).
   Invariant: a malformed literal and an out-of-range literal are
   different errors; range is not checked here.
3. `cst`: the generic lossless tree for bracket forms: `{…}`, `[…]`, `(…)`
   with `@syntax.unmatchedDelimiter` for an opener without closer or a
   closer that does not match, recovery by the rule Q8 fixes (see the
   architecture's provisional rule: a mismatched closer closes up to the
   construct it names when that one is open, else it is skipped as an
   `Error` leaf). Invariant: every byte is in exactly one leaf; a recovery
   step consumes input or returns.
4. `data`: validate the VIBON grammar over the tree: `tuple`, `record`,
   `list`, `dict`, `[:]`, literals, `true`/`false`/`void`, atoms; a
   document is exactly one value (`@syntax.invalidForm` over the extra or
   missing value); a record's fields are unique (`@data.duplicateField` at
   the later label); a later duplicate dict key is `@data.duplicateKey` at
   the later key; any identifier other than `true`/`false`, any application,
   and a doc string are `@syntax.invalidForm` over the smallest enclosing
   construct. Invariant: a `DataNode` keeps its span and bytes; decoding is
   iterative.
5. `document`: `DocumentMode::from_path` by exact extension; a wrong
   extension is `@data.invalidExtension` before reading. `parse_data` and
   a `parse_source` that, for now, lexes and builds the bracket tree and
   reports every top-level token as `@tool.unavailable` over the token
   (the source grammar arrives in step 04). Invariant: the source mode
   never accepts anything in this step.
6. `vibra-fmt::data`: canonical VIBON whitespace: two-space indentation,
   one entry per line when a form does not fit 88 columns, one space
   between tokens, none inside brackets, LF, one trailing newline, comments
   kept; no reordering (Q5 provisional). A recovered document is returned
   byte for byte. Invariant: formatting is idempotent and the reparse of
   the output has the same `DataValue`.
7. `ReaderV2Handler`: parse the declared inputs by mode, collect
   diagnostics with source ids, format each document and check idempotence
   through the same loader, write the `formatted` and `tree` observations.
   Register it for `readerV2`. Add the new runner line to CI.
8. Author the cases of the matrix.

## Test matrix

Case ids are under `V2-SRC-READER`. Inputs are complete documents.

| Case id | Kind | Input (`.vibon` unless stated) | Expected |
| --- | --- | --- | --- |
| `V2-SRC-READER-vibon-literals` | positive | `{text: "a\n" ch: 'x' n: -7i64 u: 1u8 f: 2.5f64 e: 1e3 b: true a: @std.io v: void}` | accepted; tree with nine fields; formatted equals input plus newline |
| `V2-SRC-READER-vibon-nested` | positive, formatter | `[{1i32 "a"} {} [] [:] ["k": {x: 1u8}]]` | accepted; formatted on one line; idempotent |
| `V2-SRC-READER-vibon-comment` | positive, formatter | `// top\n{a: 1u8 // one\nb: 2u8}` | accepted; comments kept on their lines |
| `V2-SRC-READER-vibon-wraps` | formatter | a record whose one-line form is 95 columns | one field per line, two-space indent, closer on its own line |
| `V2-SRC-READER-encoding-bom` | negative | the bytes `EF BB BF 7B 7D` | `@syntax.invalidEncoding` at `0..3`; the `{}` still parses |
| `V2-SRC-READER-encoding-nul` | negative | `{a: "x\u0000"}` with a raw NUL byte | `@syntax.invalidEncoding` at the NUL |
| `V2-SRC-READER-numeric-suffix` | negative | `{a: 1i128 b: 2.5i32 c: 3u8x}` | three `@syntax.invalidNumericLiteral`, each over its whole token |
| `V2-SRC-READER-string-escape` | negative | `{a: "\q" b: "\u{d800}" c: "\u{110000}"}` | three `@syntax.invalidStringLiteral`, each over the token |
| `V2-SRC-READER-string-unterminated` | recovery | `{a: "open\n}` | one `@syntax.unmatchedDelimiter` over `"open\n}` ; nothing else; formatter returns bytes unchanged |
| `V2-SRC-READER-character` | negative | `{a: '' b: 'ab' c: '\q'}` | three `@syntax.invalidCharacterLiteral` |
| `V2-SRC-READER-name-invalid` | negative | `{my_field: 1u8}` | `@syntax.invalidName` over `my_field` |
| `V2-SRC-READER-separator` | negative | `{a: 1u8"x"}` | `@syntax.missingSeparator` at the boundary (per Q7) |
| `V2-SRC-READER-closer-crossed` | recovery | `{a: [1u8 2u8}` | `@syntax.unmatchedDelimiter` at `}`; the list is recovered; bytes unchanged by the formatter |
| `V2-SRC-READER-closer-missing` | recovery | `{a: 1u8` | `@syntax.unmatchedDelimiter` at EOF (zero-width) |
| `V2-SRC-READER-vibon-two-values` | negative | `{} {}` | `@syntax.invalidForm` over the second `{}` |
| `V2-SRC-READER-vibon-identifier` | negative | `{a: foo}` | `@syntax.invalidForm` over `foo` |
| `V2-SRC-READER-vibon-application` | negative | `{a: f(1u8)}` | `@syntax.invalidForm` over `f(1u8)` |
| `V2-SRC-READER-vibon-doc-string` | negative | `{a: """x"""}` | `@syntax.invalidForm` over the doc string |
| `V2-SRC-READER-data-duplicate-field` | negative | `{a: 1u8 a: 2u8}` | `@data.duplicateField` at the second `a` |
| `V2-SRC-READER-data-duplicate-key` | negative | `["a": 1u8 "a": 2u8]` | `@data.duplicateKey` at the second `"a"` |
| `V2-SRC-READER-data-extension` | negative | input path `tree.vib` declared as data | `@data.invalidExtension`, no contents read |
| `V2-SRC-READER-source-unavailable` | boundary | `fn f returns r: I32 do return 1i32 endfn` (`.vib`) | `@tool.unavailable` over the whole form; removed in step 04 |

## Diagnostics and schema changes

Codes emitted: `@syntax.invalidEncoding`, `@syntax.invalidName`,
`@syntax.invalidNumericLiteral`, `@syntax.invalidStringLiteral`,
`@syntax.invalidCharacterLiteral`, `@syntax.unmatchedDelimiter`,
`@syntax.missingSeparator`, `@syntax.invalidForm`, `@data.duplicateField`,
`@data.duplicateKey`, `@data.invalidExtension`, `@tool.unavailable`. All
registered. Schema: the provisional tree snapshot (Q4); no JSON schema.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-syntax -p vibra-fmt -p vibra-conformance
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: the runner prints one `PASS` per case above and
`readerV2 conformance: 22 passed, 0 failed, 0 unavailable`. The host tests
include a property test that lexing then concatenating token texts is the
identity over random bytes, and that `format(parse(format(parse(x))))`
equals `format(parse(x))` over the corpus inputs.

## Excluded

The source grammar (step 04 onward); typed VIBON schemas (step 09);
structural queries (M3); reordering of record fields or dict entries (Q5).

## Completion evidence

- [ ] Every case id above exists under `conformance/cases/SRC-READER/` and
      passes.
- [ ] `conformance/rules.tsv` has `V2-SRC-READER` active.
- [ ] `.github/workflows/ci.yml` runs the new runner against `conformance/cases`
      beside the v1 line.
- [ ] A host test proves every byte of a random input is in exactly one leaf.
- [ ] The validation commands exit 0.
