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

## Risks — investigated, and the stated one was wrong

See `risk-findings/248-scope-teardown.md` and `risk-findings/248-typed-path.md`.

**The scope-teardown hazard does not exist**, but only because the thing it
would break is not connected. `src/async_runtime.rs` has a correct, tested
teardown implementation whose `open_scope`/`close_scope` have **zero call sites
outside that file and its own tests**; there is no `scope` surface form at all.
`return` inside `(task ...)` is already a compile error (`E-TASK-002`,
`src/lower.rs:2895`, `:5844`), and `spawn`'s value is an expression, so `return`
cannot appear there. `try` can be built on the existing `ExecFlow::Return` /
`Instruction::Return` mechanism with no concurrency work. **Do not write a test
asserting `try` runs scope teardown — there is nothing to run.**

**The enclosing-return-type blocker was also wrong.** It is available on both
paths (`src/lower.rs:63`, `:2865`, `:2880`; `src/typed_body.rs:591`).

**A real pre-existing bug surfaced instead, and this issue should fix it.**
`body_semantics::validate_task_handles` (`src/body_semantics.rs:23-83`) has no
`Return` arm — `Return` falls into the catch-all at `:73`, so `return` is not
modelled as a scope exit and the live-handle set is only checked at end-of-body
(`:78`). An early return past a live spawn handle compiles and runs clean;
verified empirically. No test covers it. `try` multiplies the exposure, so fix
it here: it is small and squarely in the blast radius.

**One leak this issue should name but not own.** Host handles are closed only
by explicit `fd_close` (`src/execute.rs:278`, `:2389`); no scope, function, or
block boundary closes anything. Early return already leaks them today. `try`
amplifies a pre-existing bug rather than introducing one — say so in the PR, so
it does not read as a regression. Handle lifecycle is #255's subject.

**Decide first: does `try` lower to `Statement::Return` or a new statement
kind?** If `Return`, the exhaustive matches at `src/lower.rs:2895` and
`src/body_semantics.rs:23` do most of the safety work for free. If a new kind,
both must be extended or the task boundary silently opens. Also unsettled and
worth pinning before implementation: `do` value-position semantics.

**Path:** legacy, with partial rework accepted. Surface work (AST, parser,
printer, macro, `surface_adapter`) is mandatory and fully reusable; only the
desugaring is legacy-specific.

## Definition of done

`try` ships with diagnostics, the formatter is idempotent,
`fs-roundtrip.vib` demonstrates a measured depth reduction, scope teardown is
tested, and both suites pass.
