---
title: Vibra Current-State Assessment
category: research
status: working
updated: 2026-08-02
summary: >-
  Factual baseline of Vibra as implemented, scored against the Principles
  techniques, with the concrete friction observed in real corpus code.
---

# Current-state assessment

This is the baseline for the ground-up design review. It records what Vibra
*is* on 2026-08-02, not what the docs aspire to. Where prose and code disagree,
code wins and the disagreement is recorded as drift.

Companion documents: [`notes/`](notes/) (paper distillations) and the synthesis
in [`01-design-directions.md`](01-design-directions.md).

## What Vibra is today

| Dimension | Current state |
| --- | --- |
| Surface syntax | Small, regular S-expression reader. No reader macros, no infix, no sigils. Labels (`name:`) carry optional configuration; positional operands carry required input. |
| Type system | Nominal, static, monomorphic-at-use generics with `where:` bounds; ADTs (`enum`, `record`, `tuple`, `union`), `newtype`, interfaces + `impl`, `option`/`result` in stdlib. |
| Effects | Nominal roots declared with `deffect`. Every function declares its complete effect *ceiling* by hand. Static, erased at runtime, additive union across calls. No handlers, no rows, no inference. |
| Capabilities | **None at runtime.** Removed in #213. Every host operation is unconditionally available to any program. Host-backed endpoints are unforgeable nominal newtypes, which is a *typing* guarantee, not an authority guarantee. |
| Memory | Value semantics with `mut`/`ref`; host handles are copyable. No ownership, no affinity, no borrow checking. Affine handles are explicitly deferred. |
| Backend | Compiles to WebAssembly; closed versioned `vibra_v1` host ABI; `intrinsic` is the only primitive host boundary; raw `wasm` reserved for dependency-provided modules. |
| Tooling | `fmt` (idempotent, single canonical form), `lint` (stable codes), `check`, `test` (in-language runner), `effects` report, LSP, MCP, `expand`, `rewrite rename`. JSON is the machine format. |
| Compiler | ~45 Rust source files. A typed frontend (`ast` → `typed_*`) exists alongside a legacy `Value`-based lowering path, bridged one-directionally by `surface_adapter.rs` (temporary, documented). |

## Scored against the Principles techniques

The Principles page lists ten techniques. Vibra's coverage is uneven, and the
gaps are not evenly important.

| Technique | Vibra today | Verdict |
| --- | --- | --- |
| Static type systems | Nominal, ADTs, interfaces, generics, exhaustive `match` | **Strong.** The core is sound and matches the principle. |
| Memory safety | Wasm linear memory + value semantics; no ownership/affinity; handles copyable | **Adequate but shallow.** Safety is inherited from Wasm and from not exposing pointers, not earned by the type system. Handle lifecycle (use-after-close, double-close) is unenforced. |
| Effect systems | `deffect` roots, hand-declared ceilings, static + erased | **Present but ergonomically expensive.** See friction below. |
| Capability security | Removed | **Absent.** This is the single largest divergence from the stated principles. |
| Immutability / encapsulation | Value semantics default, explicit `mut`/`ref`, `private` visibility | **Good.** |
| Sandboxing | None in-language; explicitly delegated to embedders | **Absent.** Philosophy doc concedes: embedders "must not run untrusted Vibra source without their own sandboxing." |
| Contracts / refinement types | None | **Absent.** No `where` predicates, no non-empty types, no pre/postconditions. |
| Safe defaults / restricted syntax | One canonical spelling per idea; `fmt` enforces it; `result`/`option` instead of exceptions | **Strong.** This is Vibra's best-executed principle. |
| Runtime checks and monitoring | Checked arithmetic; typed `result` flows; expansion limits | **Partial.** No resource bounds, no fuel, no deadlines, no observable execution trace. |
| Verified compilation | None | **Absent.** No formal statement of what compilation preserves. |

The pattern: Vibra is strong on *syntactic determinism* and *type structure*,
and weak on *authority*, *isolation*, and *what the compiler promises to
preserve*. The Principles page's own summary line — make invalid programs
unrepresentable, and make dangerous authority explicit — is currently
half-implemented. The first clause is well served. The second is not: authority
is documented, not enforced.

## Friction observed in real corpus code

Read [`examples/fs-roundtrip.vib`](../../examples/fs-roundtrip.vib) as the
representative case. A file write-then-read is ~30 lines with three nesting
levels and a seven-element effect annotation.

1. **Effect ceilings are hand-maintained and grow with the call graph.**
   `main` must declare `(fs.read fs.write io.stdout io.stderr stream.read
   stream.write stream.manage)`. Nothing about that list is derivable from
   local context — it is the transitive union of everything `main` reaches. An
   LLM writing this must either know the whole stdlib's effect table or
   iterate against the compiler. This directly contradicts the philosophy's
   own "explicit imports and names that make local context sufficient."

2. **Qualified names stutter.** `result.result.ok` is module `result`, type
   `result`, tag `ok`. `stream.write.string` is module, root, operation. The
   grammar is regular, but the token cost and the near-miss error surface
   (`result.ok` vs `result.result.ok`) are both real.

3. **`match` on `result` is the dominant control-flow shape and it is
   verbose.** Every fallible call becomes a four-line match. The example nests
   them, and in the inner arm the error binding is bound and then discarded.
   There is no `?`-style propagation operator, no `try` form, no
   `and-then`-in-syntax. The stdlib has combinators (`result.and`,
   `unwrap-or`), but they are call-nested, which reads worse than the match.

4. **Errors can be silently dropped and nothing complains.** In the example,
   `(stream.write.string out "from vibra fs")` returns a
   `(result void stream.error)` in statement position and the value is
   discarded. `(bind write-err)` binds an error that is never used. Neither is
   an error or a warning. For an LLM-authored language this is the wrong
   default: unhandled `result` in statement position should be a diagnostic.

5. **Deep nesting is a token and attention tax.** S-expressions make structure
   unambiguous, which is the point, but the `(do ...)`-wrapping of every body
   plus match arms plus let-chains produces indentation depth that correlates
   with the model losing track of which arm it is in.

6. **Documentation drift.** The accepted grammar contract specifies `fn`,
   `(def name Type expr)` for constants, and `(case pattern body)` inside
   `match`. The corpus uses `defn`, `def` with a type constructor, and bare
   pattern/body pairs in `match`. `philosophy.md` still recommends "regular
   YAML shapes" and "reserved `$` keys" months after the S-expression cutover,
   and `s-expression-language.md` still carries `policy.narrow` and capability
   grammar behind a note saying they were decommissioned. For a language whose
   thesis is that tooling and docs are part of the safety model, stale
   normative prose is a first-order bug, not a chore.

## Two findings the documentation does not record

**Capability machinery still exists in the runtime.**
[`src/async_runtime.rs`](../../src/async_runtime.rs) carries a
`CapabilityGrant` set per concurrency scope, and `open_scope_with_limits`
rejects a child scope requesting a grant its parent does not hold — a
monotone-narrowing authority lattice, already implemented and already tested.
What #213 removed was the *source-language* authority type, not the runtime
substrate. Reintroducing capability security is therefore substantially
cheaper than "capabilities were removed" suggests: the enforcement point
exists and needs a surface syntax and a propagation rule, not a new runtime.

**There is no execution bound and no reclamation.** Scopes support deadlines,
but there is no CPU fuel, no memory ceiling, and no whole-program budget. The
backend's value arena (`src/wasm_backend.rs`) appends and never frees, so a
long-running program's host-side value space grows monotonically. Any claim of
"bounded execution" in the Principles pipeline is currently unbacked.

## Open tensions to resolve in this review

These are the questions the paper synthesis should answer, not assumptions to
carry forward.

- **T1 — Authority.** Effects are static and erased. The Principles page wants
  capabilities enforced. Are these the same mechanism at different times
  (static effect = compile-time proof, capability = runtime token), and should
  Vibra have both, or should one subsume the other?
- **T2 — Effect burden.** Hand-declared transitive ceilings do not scale. Is
  the fix inference-with-boundary-declaration, effect polymorphism, or coarser
  granularity?
- **T3 — Handle lifecycle.** Handles are copyable and closeable. Use-after-close
  is currently a runtime error at best. Typestate or affine ownership would
  make it a type error — at what ergonomic cost for an LLM author?
- **T4 — Verbosity vs. constraint.** Does the LLM-generation literature
  actually support "more explicit is better," or is there a measured point
  where added annotation burden costs more than it buys?
- **T5 — What does compilation preserve?** Vibra makes no formal claim. Which
  claims are worth stating, and which require research-scale proof effort a
  small team cannot fund?
- **T6 — Retrieval.** The Principles page calls for retrieval over normalized
  structure (types, effects, API calls, control-flow summaries), not raw text.
  Vibra emits none of these as a retrieval artifact today.
- **T7 — Bounded execution.** No fuel, deadlines, memory caps, or determinism
  guarantee for a program run. The Principles pipeline diagram ends at a
  "sandboxed deterministic runtime" that does not exist.
