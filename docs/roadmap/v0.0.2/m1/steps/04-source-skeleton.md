# Step 04: source-skeleton

Kind: vertical. The source document grammar's skeleton: the module, the
seven top-level forms recognized by keyword and typed closer, the generic
section structure, `use … enduse` in full, the recovery rules, and the
canonical layout of top-level forms. Bodies of the other six forms are
reported unavailable until steps 05 to 07.

## Prerequisites

- Step 03 landed.
- Read: [source chapter](../../../../spec/01-source-language.md) "Grammar"
  (the `module`, `top`, `use` productions and every note after the EBNF),
  "Declarations" (the first paragraphs and `use`), "Canonical format";
  [programs chapter](../../../../spec/04-programs-and-packages.md) "Modules
  and imports" for what `use` means (resolution is step 10);
  [diagnostics chapter](../../../../spec/07-diagnostics-and-conformance.md)
  "Recovery".
- Open questions: Q3 (`@syntax.retiredForm` trigger), Q8 (closer recovery
  rule). Both must be answered before this step starts.

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-syntax/src/cst.rs` (step 03) | the bracket tree is extended with keyword-led constructs |
| `crates/vibra-syntax-v1/src/ast.rs:1666` (`decode_source_root`) | the idea of typed views over a lossless tree; nothing of its S-expression logic |
| `crates/vibra-fmt/src/layout.rs` (step 03) | line breaking and indentation |

Proposed: `vibra-syntax::source::{module, declaration, section}` with
`Module`, `Declaration` (seven variants), `Section` (keyword, operand
tokens), `Use { locator: Atom, alias: Option<Ident> }`;
`vibra-fmt::source` for top-level layout.

## Ordered tasks

1. Reserved words: the lexer classifies the 47 reserved words; a reserved
   word can never be an identifier. Invariant: the list is one table shared
   by lexer, formatter, and tests.
2. Construct tree: every opener keyword (`type`, `interface`, `impl`, `fn`,
   `proc`, `let`, `test`, `use`, `if`, `match`, `do`) opens a construct
   closed by its typed closer; `then`, `else`, `case` are separators inside
   their constructs; section keywords (`takes`, `labelled`, `returns`,
   `given`, `where`, `requires`, `ensures`, `performs`, `role`,
   `visibility`, `representation`, `external`, `symbol`, `native`, `doc`,
   `of`, `be`, `as`) start sections whose operands run to the next section
   keyword, closer, or nested opener at the same depth. A closer that does
   not match the innermost open construct is `@syntax.unmatchedDelimiter`
   and recovers by the Q8 rule; an opener without its closer is
   `@syntax.unmatchedDelimiter` at EOF. Invariant: sections and nested
   constructs are generic nodes; no form-specific knowledge yet.
3. Module and top: `module = { top }, EOF`; a token at top level that is
   not one of the seven openers is `@syntax.invalidForm` over the smallest
   enclosing construct (one diagnostic per run of such tokens; recovery
   skips to the next top-level opener). `(` at top level and the other
   triggers Q3 fixes are `@syntax.retiredForm`. Invariant: every top-level
   diagnostic has a non-empty span except the EOF closer.
4. `use`: `use atom [as identifier] enduse`; a missing atom, a non-atom
   locator, a missing `as` name, or extra tokens are `@syntax.invalidForm`
   over the `use`. The locator atom's parts are kept with exact case.
   Invariant: the `Use` view exposes locator parts and the alias.
5. Section rules at the generic level: a repeated section keyword in one
   construct is `@syntax.duplicateSection` at the repeat; `do` must come
   last and the name and type parameters first (a section before the name
   is `@syntax.invalidForm`). The admissibility of a section for a
   declaration kind is `@syntax.invalidSection` and is checked when the
   kind's typed view exists (05 to 07); until then a `type`, `interface`,
   `fn`, `proc`, `let`, or `test` top is `@tool.unavailable` over the
   whole construct, with its closers matched.
6. Formatter: one blank line between top-level forms; `use` forms first in
   written order, each on one line; comments preserved per the chapter;
   sections of a multi-line declaration in canonical order on their own
   lines (the order list is one table); `do` last; closer at the opener's
   indentation; a recovered document returned unchanged. Invariant: the
   formatter never changes token order inside a section's operand.
7. Cases.

## Test matrix

| Case id | Kind | Input (`.vib`) | Expected |
| --- | --- | --- | --- |
| `V2-SRC-DECL-use-plain` | positive, formatter | `use @std.io enduse` | accepted; tree `Module[Use(locator=@std.io)]`; formatted equals input |
| `V2-SRC-DECL-use-alias` | positive | `use @billing.cents as money enduse` | accepted; alias `money` |
| `V2-SRC-DECL-use-first` | formatter | `type A of U8 endtype\n\nuse @std.io enduse` | formatted has the `use` first, one blank line, then the type (whose body is unavailable in this step; the case is re-recorded in 05 as accepted) |
| `V2-SRC-DECL-use-missing-locator` | negative | `use enduse` | `@syntax.invalidForm` over `use enduse` |
| `V2-SRC-DECL-use-string-locator` | negative | `use "std.io" enduse` | `@syntax.invalidForm` over the form |
| `V2-SRC-DECL-use-alias-missing` | negative | `use @std.io as enduse` | `@syntax.invalidForm` |
| `V2-SRC-READER-top-literal` | negative | `1i32` | `@syntax.invalidForm` over `1i32` |
| `V2-SRC-READER-top-retired` | negative | `(defn main () void void)` | `@syntax.retiredForm` over the parenthesized form (per Q3); nothing else |
| `V2-SRC-READER-closer-wrong` | recovery | `use @std.io endfn` | `@syntax.unmatchedDelimiter` at `endfn`; the `use` is recovered; a following `use @std.fs enduse` on the next line is accepted exactly |
| `V2-SRC-READER-closer-eof` | recovery | `fn f` | `@syntax.unmatchedDelimiter` zero-width at EOF, plus `@tool.unavailable` over `fn f` |
| `V2-SRC-READER-duplicate-section` | negative | `fn f returns r: I32 returns s: I32 endfn` | `@syntax.duplicateSection` at the second `returns`; `@tool.unavailable` over the form until step 07 |
| `V2-SRC-READER-recovery-then-valid` | recovery | `use @a enduse\n???\nuse @b enduse` | `@syntax.invalidForm` over `???`; both imports exact; formatter returns bytes unchanged |
| `V2-SRC-READER-comments-kept` | formatter | `// head\nuse @std.io enduse // trail\n\n\n// own line\nuse @std.fs enduse` | formatted keeps `// head`, the trailing comment on its line, one blank line, the own-line comment before the second import |
| `V2-SRC-READER-crlf` | formatter | the plain case with CRLF endings | formatted has LF only and one trailing newline |

## Diagnostics and schema changes

Codes: `@syntax.invalidForm`, `@syntax.retiredForm`,
`@syntax.unmatchedDelimiter`, `@syntax.duplicateSection`,
`@tool.unavailable`. `@syntax.invalidSection` is first emitted in 05. No
schema change.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-syntax -p vibra-fmt
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: every `V2-SRC-DECL-use-*` and the new `V2-SRC-READER-*` cases print
`PASS`; the step 03 case `V2-SRC-READER-source-unavailable` is updated to
expect `@tool.unavailable` over the `fn` form (its closers now match) and
still passes.

## Excluded

The bodies and typed views of `type`, `interface`, `fn`, `proc`, `let`,
`test` (05 to 07); resolution of locators (10).

## Completion evidence

- [ ] The reserved-word table has 47 entries and a host test checks each is
      rejected as an identifier.
- [ ] A fuzz-style host test feeds random keyword sequences and asserts no panic
      and every byte in one leaf.
- [ ] All cases above pass; `conformance/rules.tsv` has `V2-SRC-DECL` active.
- [ ] The validation commands exit 0.
