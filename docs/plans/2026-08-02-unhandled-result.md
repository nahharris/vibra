---
title: Plan — Diagnose unhandled result and option
category: plans
status: proposed
updated: 2026-08-02
issue: 247
---

# Plan: diagnose unhandled `result`/`option` (#247)

**Wave 1, second of three.** Depends on nothing in #246 technically, but
shares the corpus migration.

## Why

`examples/fs-roundtrip.vib` discards a `(result void stream.error)` from
`(stream.write.string out "from vibra fs")` and binds an error it never reads.
Neither is diagnosed. For a language whose thesis is that the safe path is the
default path, silently dropping a typed failure is the wrong default, and it is
precisely the mistake a model makes while focused on the happy path.

## The sequencing constraint this plan exposes

Unlike #246, this check **needs type information**, which puts it against the
dual-path problem: `src/lower.rs` (9.1k lines, legacy, proven) currently
receives S-expression input through the temporary `src/surface_adapter.rs`
bridge, while `src/typed_body.rs` (5.4k lines) is the intended future path with
acknowledged lower coverage.

**Decision: implement against the typed path, and gate this issue on the typed
path being able to answer "what is the type of this expression in statement
position" for the corpus.** Implementing in `lower.rs` means writing a new
check into a module explicitly slated for deletion, and then writing it a
second time. If the typed path cannot yet answer the question, that is the real
blocker and should be surfaced as such rather than worked around — a temporary
implementation in `lower.rs` would be the third pillar the adapter amendment
explicitly warns against.

This constraint applies to #248 and #249 as well, and is the strongest argument
for prioritizing adapter retirement.

## Design decisions

**Discard is binding to `_`, not a new form.** The reader already has `_` as
the sole wildcard. `(let _ expr)` reads as deliberate discard, costs no
grammar, and keeps the one-canonical-form rule. A dedicated `ignore` form would
be a second spelling for an existing idea.

**Scope: `result` and `option` only, for now.** A general must-use marker in
the type system is more principled and is the right long-term answer, but it
needs a type-level attribute that does not exist, and no paper in the review
evaluates it. Special-casing the two stdlib types buys the measured benefit
now. Record the generalization as a follow-up rather than pretending the
special case is the end state.

**Two distinct diagnostics, not one.** Unhandled value in statement position
and bound-but-never-read have different fixes (handle it vs. remove the
binding) and must not share a code.

**Final-expression position is exempt.** In a `do`, the last expression is the
block's value and is returned, not dropped.

## Phases

1. **Confirm typed-path readiness** for statement-position type queries. If
   absent, stop and re-scope; do not implement in `lower.rs`.
2. **Unhandled-value check** over typed statement positions, exempting final
   expressions, `return` operands, and `let`/`let-as` right-hand sides.
3. **Unused-binding check** for `(bind name)` and `let` binders never read.
4. **Diagnostics** registered in `schemas/linter-codes.json`, each with a
   suggested fix span.
5. **Corpus migration**, batched with #246. `examples/fs-roundtrip.vib` must
   stop dropping its write result — it is the motivating example and should
   read as exemplary afterward.

## Testing

Rust tests: unhandled in statement position; handled by `match`, by binding, by
`return`; explicit `_` discard accepted; final-expression exemption; unused
`(bind ...)`; a binding read only in one `match` arm (accepted). One
`tests/*.vib` case per diagnostic.

## Risks

The unused-binding check has a false-positive mode that will annoy: a binding
read only on some paths. The test above pins the intended behavior — read on
*any* path counts as read. Getting this wrong produces exactly the kind of
noisy diagnostic that trains authors to ignore diagnostics.

## Definition of done

Both diagnostics ship with fixes, the corpus is clean, `fs-roundtrip.vib`
handles its errors, and both suites pass.
