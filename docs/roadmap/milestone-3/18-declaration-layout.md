# Step 18 — declaration layout in the canonical format

Prerequisite: Step 17 merged. A formatter step; it changes no language rule.

## Read before editing

- [Source language](../../spec/01-source-language.md): **Canonical format**.
- [Decision ledger](decision-ledger.md) row D25.1.

## Scope

Before this step a multiline declaration put every form on its own line, so a
method read `(defn`, then its name, its parameters, and its result on four
lines, and `visibility:` and `@public` on two more. An inline last form that
exactly filled its line pushed the closing delimiter onto a line of its own.
The canonical format now says (D25.1):

1. **Header on the opening line.** The declaration head and each following
   header form share the opening line for as long as each is inline, no
   comment separates it from the form before, and the line stays within 88
   columns. A header form past that point takes its own line.
2. **Attributes beside their values.** A labelled attribute shares one line
   with its value when both are inline, no comment separates them, and the
   pair leaves room for one closing delimiter.
3. **No orphaned delimiter after a list.** A last form that is a list is laid
   out multiline when, on one line, it would leave no room for the closing
   delimiters that follow it. A closing delimiter stands alone only after a
   comment or an atom that does not fit.
4. **One layout.** A declaration with comments and one without lay out the
   same way; a comment only forces the form after it onto a new line.

Formatting stays idempotent, and nothing but whitespace moves.

## Test matrix

- `V1-SRC-FMT-declaration-layout`: a generic type with two attributes and an
  attributed method; a function whose parameter list does not fit beside its
  name; a last form that would leave no room for two closing delimiters; and a
  commented interface.
- `V1-SRC-FMT-nested-indent-budget`: the innermost list stays inline only
  when its closing delimiters fit with it.
- Host tests `declaration_layout` and `comment_layout`: each rule, its
  boundary at 88 columns, and idempotence.
- Every formatter snapshot, the index `text` fields, and both demos are
  regenerated.

## Done

The demos read as they would be written by hand, and validation passes.
