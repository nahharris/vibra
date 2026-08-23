# 1. Charter

Status: draft

## What Vibra is

Vibra is a statically typed, effect-tracked language with an S-expression
surface that compiles to WebAssembly, designed on the assumption that **most of
its source code is written, reviewed and rewritten by language models**.

Human readability matters, but it is not the primary constraint. The language
should make the correct continuation obvious to an author with limited context,
and should make ambiguous or unsafe constructions hard to express at all.

## What v1 is for

**v1 target: command-line tools and scripts.**

The day v1 ships, an agent should be able to write, check, test and run a real
CLI program in Vibra without leaving the language: read arguments and
environment, read and write files, run subprocesses, transform text, handle
failure with typed results, and exit with a status code. It should be able to
depend on a second Vibra package by path.

That is the smallest target that exercises every layer — reader, types,
interfaces, effects, backend, runtime, stdlib, test runner — end to end. Any
feature that a competent CLI program does not need is out of v1 by default and
belongs to a numbered wave in [Deferred and rejected](10-deferred.md).

Concretely, these three programs are the v1 acceptance bar. They are written in
Vibra, they ship in the repository, and they are covered by tests:

1. A file-processing filter: read a path from `argv`, stream it, transform each
   line, write to stdout, report typed errors on stderr, exit non-zero on
   failure.
2. A small structured-data tool that builds and matches over user-defined
   algebraic types and generic containers.
3. A tool that shells out to a subprocess, inspects the exit status, and
   composes the result with its own errors.

When those three are idiomatic, short, and pass both test suites, v1 is done.

## Why this reboot happened

The previous cycle discovered the right feature list — syntax, type system,
effect system, agent tooling, runtime — by building it. That produced five
subsystems at five different levels of maturity, a typed frontend bridged to a
legacy lowering path that was never retired, accepted contracts describing
syntax the parser rejected, and a security story that was documented but not
enforced.

The lesson is not that any of those features were wrong. It is that **a
language is a specification, and the specification has to exist first.** This
directory is that specification. The implementation is scheduled against it in
[`../ROADMAP.md`](../ROADMAP.md), and the previous implementation is preserved
on the `v0-archive` branch to be mined, not resumed.

## Design rules

These are ordered. When two rules conflict, the earlier one wins.

### 1. Optimize for a model with limited context

Every design question is settled by asking which option an author can complete
correctly from what is visible on screen. Prefer explicit structure over
cleverness, and local information over global convention.

### 2. Canonical form, because it makes the compiler a decoder

If two spellings mean the same thing, one is canonical and the other is
rejected. This is Vibra's most distinctive commitment, and it is important to
state the *correct* reason for it.

The weak reason — "every extra choice is another chance to hallucinate" — does
not survive measurement. In studied LLM-generated code, syntax accounts for a
small minority of compilation failures and type errors for the large majority;
grammar-level constraint alone has a low ceiling on error reduction.

The strong reason is this: **canonical syntax and mandatory signatures are what
make type-directed constrained decoding tractable and normalized retrieval
possible.** A prefix automaton carrying a typing context can only terminate its
search for well-typed continuations if annotations are explicit and each
construct has one spelling. Style-normalized retrieval only works if the
formatter is idempotent and applied symmetrically to corpus and query.

So the compiler has three products, not one:

| Product | Question it answers | v1 |
| --- | --- | --- |
| **Checker** | Is this program valid? | Yes |
| **Index** | What is the normalized structure of this corpus? | Deferred, wave 2 |
| **Decoder** | Which continuations here are well-typed? | Deferred, wave 6 |

v1 ships only the checker, but every type-system decision in this spec is made
with the other two in mind. The standing constraint on language evolution is:
**prefer type-system features whose decoding automaton stays small.**

### 3. The safe path is the default path

Correctness must not depend on the author remembering a rule. Typed `result`
and `option` instead of exceptions; exhaustive matching; a diagnostic when a
failure value is dropped; no shadowing; no silent numeric wraparound.

### 4. Intent is written down, not inferred

If something matters for typechecking, dispatch, or the host boundary, it is
written in the program. Explicit parameter and return types. Explicit
interface conformance. Explicit effect declarations at boundaries. Inference is
allowed only where it narrows a local expression and hides no cross-module
decision.

### 5. The toolchain is part of the language

The formatter, the diagnostics, the schemas and the test runner are not
accessories. A diagnostic without a stable machine-readable code is an
incomplete feature. Format is check-only by default; mutation requires
`--write`.

## What v1 promises

- Every accepted program is statically typed, with no dynamic escape hatch.
- Every function's effect ceiling is known statically, and is either declared
  at a boundary or inferred inside a module.
- Pattern matching is exhaustive.
- Integer arithmetic never silently wraps.
- The formatter is idempotent, and its output is the canonical form.
- Compilation is deterministic: identical inputs produce byte-identical
  artifacts.
- The guest-to-host boundary carries scalars only. Dynamic values cross as
  opaque indices resolved on the owning side, never as guest pointers.

## What v1 explicitly does not promise

Read this section before building anything on top of Vibra v1.

- **v1 is not a sandbox.** Effects are checked statically and then erased.
  `vibra run` grants the program the full authority of the process that
  launched it. A declared effect ceiling is a compile-time fact about the
  source; nothing re-checks it at run time.

  This is a deliberate scoping decision, and it is the one that most deserves
  scrutiny: a language that describes authority it does not enforce is
  describing decoration. Wave 1 of the post-v1 plan exists specifically to
  close it, and no v1 document may describe Vibra as capability-safe,
  sandboxed, or safe to run on untrusted source. **Do not execute untrusted
  Vibra source under v1.**

- **No resource bounds.** No fuel, no memory ceiling, no deadline. A v1 program
  can loop forever and allocate until the host refuses.

- **No claim that compilation prevents attacks.** Where the implementation
  makes a preservation claim, it claims only that observable behavior covered
  by its own tests is preserved. Semantic preservation and attack prevention
  are separate statements, and v1 makes only the first.

- **No stability guarantee across v1 pre-releases.** Until milestone M0 closes,
  the spec may change without migration.

## Non-goals, permanently

These are not scheduled for any wave. They are excluded on principle, and
proposing one requires arguing against a design rule above.

- A second surface syntax, or a compatibility mode for one.
- Exceptions, unwinding, or non-local control flow beyond returning from the
  immediately enclosing function.
- Implicit conversions, including numeric widening, at any position.
- Ambient global state, or any value reachable without naming it.
- Reader macros, infix operators, sigils beyond `@`, or a general-purpose Lisp
  reader.
- Verified compilation. Measured cost for a layout-changing verified pass runs
  to tens of lines of proof per line of implementation; the cheap design
  constraints that keep the option open are adopted instead.
