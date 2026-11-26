# Step 02: canonical-text

Kind: vertical. The canonical source text of one node: the formatter's
tokens with comments dropped and the one-space layout of canonical bytes.
It is the text every claim and witness hashes, and the two 05 vectors are
its first tests.
Depends on: 01.
Crates: `vibra-fmt`.

## Prerequisites

- Read: [05 Snapshot, claim and witness identity](../../../../spec/05-tooling.md#snapshot-claim-and-witness-identity)
  (the vectors); [04 Decoded values and canonical bytes](../../../../spec/04-programs-and-packages.md#decoded-values-and-canonical-bytes)
  ("Canonical serialization"); [01 Canonical format](../../../../spec/01-source-language.md#canonical-format);
  [questions](../spec-questions.md) Q2.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-fmt/src/lib.rs:42` | `pub fn format_document` | the formatter whose token stream the text is taken from |
| `crates/vibra-syntax/src/cst.rs:370` | `pub fn leaves` | the leaves of a node in order |
| `crates/vibra-syntax/src/cst.rs:364` | `pub fn text` | a leaf's text |
| `crates/vibra-syntax/src/source/section.rs:46` | `pub struct Construct` | the node of one declaration |

Proposed: `vibra_fmt::fragment::canonical_text(document, node) ->
Option<String>`, and `canonical_declaration_text(document, construct)`.

## Files owned

- `crates/vibra-fmt/src/fragment.rs`
- `crates/vibra-fmt/tests/it/fragment.rs`

## Ordered tasks

1. `canonical_text`: format the document, locate the node's image in the
   formatted tree (the formatter keeps every token, so the node's token
   sequence is found by position in the leaf order), drop comment and
   whitespace leaves, and join the token texts: one space between adjacent
   tokens, none after `(`, `[`, `{`, none before `)`, `]`, `}`, none
   before `:`, one after it. A document the formatter returns unchanged
   because of recovery gives `None`. Invariant: the result has no newline
   and no comment, and the tokens are the formatter's.
2. Doc strings are tokens and stay; their text is the token as written.
3. A string or character token keeps the spelling the formatter gives it
   (the formatter keeps spellings), so two spellings of one scalar are two
   texts. Record this in the report as the consequence of Q2.
4. Iterative over the tree; no recursion proportional to source nesting
   beyond the formatter's own.

## Test matrix

Host tests in `vibra-fmt`.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| the witness vector's body | positive | `proc main\n  returns code: ExitCode\n  do\n    return ExitCode(0u8)\nendproc` | `proc main returns code: ExitCode do return ExitCode(0u8) endproc` | 05: the `@witness.v2` vector |
| a comment is dropped | positive | the same with `// note` after `do` | the same text | Q2 |
| layout is dropped | positive | the same on one line, and with three blank lines | the same text | Q2 |
| a doc string stays | positive | `fn f doc "d" returns n: I32 do return 1i32 endfn` | `fn f doc "d" returns n: I32 do return 1i32 endfn` | Q2 |
| brackets and colons | positive | `{a: I32 b: Str}` written `{ a : I32 b : Str }` | `{a: I32 b: Str}` | 04 Canonical serialization: "exactly one space between two adjacent elements … one after each `:`" |
| nested application | positive | `f ( g ( x ) y )` | `f(g(x) y)` | the same |
| an expression node | positive | the predicate of a `requires` | its text alone | 05: "each text the canonical source of the expression" |
| a recovered document | boundary | a declaration with a missing closer | `None` | 05 Formatting: "Source the parser recovered from ambiguously is kept byte for byte" |
| deep nesting | boundary | 1,024 nested lists on a 256 KiB thread | a text, no overflow | architecture, risk 6 |

## Diagnostics and schema changes

None.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-fmt -p vibra-workspace -p vibra-conformance --all-features
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0.

## Stays unavailable

Unchanged: nothing observable uses the text until step 03.

## Excluded

Hashing (03, 04); the formatter's own rules.

## Completion evidence

- [ ] Each of the nine rows is a test, named in the report.
- [ ] The report quotes the spacing rule as implemented and states that
      the tokens are taken from the formatter's output, not re-lexed.
- [ ] The validation commands exit 0.
