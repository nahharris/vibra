---
title: Plan — Reject symbol shadowing
category: plans
status: proposed
updated: 2026-08-02
issue: 246
---

# Plan: reject symbol shadowing (#246)

**Wave 1, first of three diagnostics changes.** Land before #247 and #248;
all three touch the corpus and batching the migrations avoids three passes
over the same files.

## Why

Anka attributes 42% of its Python failure cases to variable shadowing, and
three independent papers converge on identifier-based reference to named
intermediates as the primitive worth having. Vibra already has `let`; what is
missing is the guarantee that a name means one thing.

This is the best evidence-to-cost ratio in the roadmap: it targets a measured
failure mode, needs no type information, and therefore sits entirely above the
`lower.rs` / typed-path split that complicates the next two plans.

## Design decisions

**Hard error, not a lint.** A lint is suppressible and advisory; the whole
value here is the invariant that a reader (human or model) can resolve a name
without scanning outward. A suppressible version delivers approximately none of
that.

**Scope of the rule.** Reject when a binder collides with any binding visible
in an enclosing lexical scope: `let`, `let-as`, parameters, `(bind name)` in
match arms, and `for` binders. Do *not* reject collisions with module-level or
imported names — that is a different rule with a different fix (qualify the
reference), and folding it in here would force churn on the stdlib for no
measured benefit.

**`_` stays exempt.** It is the wildcard, not a binding, and multiple `_` in
one scope must remain legal.

**Macro-expanded binders are exempt by construction, not by special case.**
The contract already states quoted binders use compiler symbol identities
rather than textual suffixes. The check must therefore run on resolved symbol
identity, not on source text — which is the correct implementation anyway, and
makes the exemption fall out rather than needing a carve-out. If it needs a
carve-out, the check is being done at the wrong layer.

## Phases

1. **Scope-resolution pass.** Implement over the typed surface AST
   (`src/ast/`), walking binding scopes and reporting collisions. This is a
   pure analysis over resolved identities with no dependence on `lower.rs`.
2. **Diagnostic.** New `E-SCOPE-*` code registered in
   `schemas/linter-codes.json`, with the primary span on the shadowing binder
   and a related span on the binding it shadows. The related span is the part
   that makes the fix obvious; do not ship without it.
3. **Corpus migration.** Rename in `stdlib/`, `examples/`, `tests/*.vib`.
   Expect few sites — the corpus is already largely shadow-free — but confirm
   rather than assume.
4. **Contract update.** Record the rule in
   `decisions/s-expression-language.md` under names and scoping.

## Testing

Rust tests for: nested `let`; parameter collision; `match` arm binding
shadowing an outer `let`; `for` binder; sibling scopes reusing a name (must be
**accepted** — this is the case a naive implementation breaks); repeated `_`;
and a macro expansion whose hygienic binder coincides textually with a
call-site name (must be accepted).

One `tests/*.vib` case asserting the diagnostic through the language runner.

## Risks

The sibling-scope case is the one to get right. Two `match` arms each binding
`value`, or two sequential blocks each with `let x`, are legal and idiomatic;
an implementation that tracks a flat set of seen names instead of a scope stack
will reject them and make the language worse. That test is the acceptance gate.

## Definition of done

Shadowing is rejected with a two-span diagnostic, sibling scopes still work,
the corpus is clean, and both suites pass.
