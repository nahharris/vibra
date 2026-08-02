---
title: Plan — Result propagation form
category: plans
status: proposed
updated: 2026-08-02
issue: 248
---

# Plan: result propagation (#248)

**Wave 1, third of three.** Land after #247: propagation is the ergonomic
escape hatch that makes the unhandled-result diagnostic tolerable rather than
punitive. Shipping the diagnostic without the escape hatch would make every
fallible call a four-line `match` *and* mandatory.

## Why

Nesting depth is a first-order design concern for an LLM-authored language, and
error handling is where Vibra's depth comes from. LLMON found flattened
representations outperform nested trees at the model interface. Anka lost 10
points to Python on nested-conditional logic — added structure hurts
specifically where control flow is deep. `examples/fs-roundtrip.vib` nests
three matches for a write-then-read.

The stdlib combinators do not solve this: `result.and` and friends are
call-nested, which reads worse than the `match` they replace.

## Design decisions

**Spelling: `(try expr)`.** The reader contract has no operator characters and
explicitly rejects sigils and reader macros; a `?` suffix would require a
lexer change that contradicts the grammar's stated shape. `try` is a head
symbol like every other form. It is one token, unambiguous, and searchable.

**Exact error-type match, plus an explicit conversion call.** This is the
consequential decision. `into`-style automatic conversion is more ergonomic and
is what Rust does, but `philosophy.md`'s explicit-intent rule says an intent
that matters for typechecking should be written, not inferred — and a silent
error-type conversion at a propagation site is exactly a cross-module type
decision being hidden. Require the types to match; make the author write the
conversion.

The falsifiable cost: if this produces a conversion call at the majority of
`try` sites in the migrated corpus, the rule is wrong and should be revisited
before the form is widely adopted. Measure this during corpus migration and
report the number in the PR — do not defend the decision if the data
contradicts it.

**Return type constraint.** `(try e)` is legal only where the enclosing
function returns `(result t err)` and `e` has type `(result u err)` for the
same `err`. Anywhere else is an error naming both types.

**`option` gets `try` too**, with the same rule against a function returning
`(option t)`. Excluding it would create exactly the kind of "two similar things
that behave differently" asymmetry the one-canonical-form rule exists to
prevent.

**Interaction with #247.** `(try e)` counts as handling `e`. This must be wired
deliberately, not left to fall out.

## Phases

1. **Grammar and reader.** Add `try` to the expression grammar in
   `decisions/s-expression-language.md` and `src/syntax/parser.rs`.
2. **Typing and lowering** on the typed path (same constraint as #247 — this
   needs the enclosing function's return type).
3. **Formatter.** `src/syntax/printer.rs`; confirm `vibra fmt --write` stays
   idempotent across the corpus.
4. **Diagnostics** for: used outside a `result`/`option`-returning function;
   error-type mismatch; applied to a non-`result` value. Register in
   `schemas/linter-codes.json`.
5. **Corpus migration and measurement.** Rewrite `examples/fs-roundtrip.vib`;
   report before/after line count and nesting depth, plus the conversion-call
   frequency that tests the exact-match decision.

## Testing

Rust tests: success path; error path returns early; nested `try` in one
expression; `try` inside loop and match bodies (early return must respect
enclosing scopes and structured-concurrency cleanup); type mismatch; use in a
`void`-returning function; `option` variant. One `tests/*.vib` case covering
success and propagation.

## Risks

**Early return interacting with structured concurrency is the real hazard.**
`try` inside a `task` or an open scope must run the same cleanup path as an
explicit `return`. If it bypasses scope teardown, this form introduces resource
leaks into the language's most safety-critical machinery. Test it explicitly
against `src/async_runtime.rs` scope semantics; treat a missing test here as a
blocking review comment.

## Definition of done

`try` ships with diagnostics, the formatter is idempotent,
`fs-roundtrip.vib` demonstrates a measured depth reduction, scope teardown is
tested, and both suites pass.
