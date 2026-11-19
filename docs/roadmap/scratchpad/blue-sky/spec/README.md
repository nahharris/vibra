# Vibra, rethought: draft specification

Status: draft specification; not normative

This directory turns the [blue-sky rethink](../README.md) into a complete
language design: what a program is, how it is written, what is checked, how
it runs, and what the toolchain does. It is written as a specification so
that a roadmap can be planned against it. It is not the active
specification. [`docs/spec/`](../../../../spec/00-charter.md) remains
authoritative until a change replaces it and the roadmap together.

The design assumes the current implementation can be discarded. It keeps
from today's Vibra only what it would choose again: nominal types, explicit
namespaces, kebab-case names, a content-addressed codebase, structured
diagnostics, `.vib` for source text, and `.vibon` for stored data.

## Chapters

| Chapter | Subject |
| --- | --- |
| [00 Charter](00-charter.md) | Mission, author model, decision order, commitments, exclusions |
| [01 Programs](01-programs.md) | The store, cells, identity, names, modules, roles, admission |
| [02 Syntax](02-syntax.md) | The canonical tree, the line surface, tables, notations |
| [03 Types](03-types.md) | Values, nominal types, refinements, generics, ownership, labels, totality |
| [04 Claims](04-claims.md) | Claims, obligations, evidence grades, strength checks, debts |
| [05 Execution](05-execution.md) | Ports, intents, procedures, machines, the journal, logical time |
| [06 Runtime](06-runtime.md) | Journal format, drivers, scheduling, memory, replay, simulation, targets |
| [07 Compiler](07-compiler.md) | Pipeline, kernel, core language, readings, backends, validation |
| [08 Tooling](08-tooling.md) | Commands, staging, holes, queries, evaluation, diagnostics, conformance |
| [09 Sample](09-sample.md) | One system written out in full, in every surface |

## The language in one page

- A program is a **store of cells**. A cell has a claim, a witness, and
  evidence. Dependents see only claims.
- Pure code is written as `fn`. It is total, has no effects, and cannot
  fail at run time: every index, conversion, and arithmetic operation
  carries a proof obligation or returns an `option`.
- Effectful code is written as `proc` or `machine`. It cannot touch the
  world. It returns **intents** addressed to declared **ports**, and the
  runtime performs them and records everything in a **journal**.
- Text is a projection. The store holds trees. The **line surface** is the
  projection agents write: one keyword per line, shallow prefix
  expressions, familiar words, declarations closed by name.
- Every guarantee has a **grade** (`proved`, `solved`, `bounded`,
  `tested`, `assumed`), and every exception is a recorded **debt**.
- The party that implements a cell cannot change the claim it is judged
  against.

A complete cell:

```text
fn fib-fast
  takes n: int
  returns int
  requires (int.ge n 0)
  means (fib n)
  example (fib-fast 10) = 55
  cost (bound.linear n)
body
  return (fib-from step: 0 previous: 0 current: 1 limit: n)
end fib-fast
```

Everything above `body` is the claim. Everything below is the witness. The
claim says the result equals the slow reference `fib` and costs linear
steps. The [sample](09-sample.md) has `fib`, the helper `fib-from`, and a
whole system around them.

## Paradigms and where they are specified

| Paradigm | Verdict in the [field notes](../paradigms.md) | Specified in | Layer |
| --- | --- | --- | --- |
| Content-addressed cells with opaque witnesses | Core | [01](01-programs.md) | 0 |
| One tree, several round-tripping surfaces | Core | [02](02-syntax.md) | 0 |
| Totality | Add | [03](03-types.md#totality) | 0 |
| Refinement types and obligations instead of run-time failure | Core | [03](03-types.md), [04](04-claims.md) | 1 |
| Reference meaning with verified realization | Core | [04](04-claims.md#meaning) | 1 |
| Measured claims: vacuity, determinacy, mutation | Core | [04](04-claims.md#strength-checks) | 1 |
| Graded evidence and debts | Core | [04](04-claims.md#grades) | 1 |
| Separation of powers | Core | [01](01-programs.md#roles) | 1 |
| Typed holes and refinement moves | Core | [08](08-tooling.md#holes-and-moves) | 1 |
| Effects as intents to ports | Core | [05](05-execution.md) | 2 |
| Journaled step machines, replay, durability | Core | [05](05-execution.md), [06](06-runtime.md) | 2 |
| Deterministic simulation with fault injection | Core | [06](06-runtime.md#simulation) | 2 |
| Logical time | Core | [05](05-execution.md#logical-time) | 2 |
| Model checking the program itself | Core | [06](06-runtime.md#exploration) | 3 |
| Static cost bounds | Add | [04](04-claims.md#cost) | 3 |
| Ownership and in-place reuse | Add | [03](03-types.md#ownership) | 3 |
| Information-flow labels | Add | [03](03-types.md#labels) | 3 |
| Checked notations and invertible definitions | Add | [02](02-syntax.md#notations) | 4 |
| Choreographies | Add | [05](05-execution.md#protocols) | 4 |
| Incremental, generative, and superposed readings | Explore | [07](07-compiler.md#readings) | 4 |
| Equality saturation for realization search | Explore | [07](07-compiler.md#realization) | 4 |

Rejected paradigms stay rejected: interaction nets as the runtime, tactic
proofs as the default surface, token-minimal syntax, natural language as
source, and user-defined effect handlers.

## Layers

The specification is one design, built in layers. Each layer is usable on
its own and is a precondition for the next. A roadmap can treat each layer
as a release line.

| Layer | Name | Contains | Usable result |
| --- | --- | --- | --- |
| 0 | Kernel | Tree, line surface, store, types, `fn`, totality, reference evaluator, `eval` | A pure total language with a content-addressed store and a REPL |
| 1 | Evidence | Claims, obligations, solver bridge, grades, debts, strength checks, roles, staging, holes | Verified pure libraries with measured claims |
| 2 | Execution | Ports, intents, `proc`, `machine`, journal, replay, simulation, drivers, one compiled backend | Durable services that replay and simulate |
| 3 | Resources | Cost bounds, ownership, labels, exploration, upgrade evidence, target policies | Performance, security, and state-space claims that are checked |
| 4 | Notation and scale | Tables, notations, layouts, protocols, distribution, further readings | Domain notations and multi-party systems |

Dependencies inside the layers:

- The reference evaluator precedes everything. It is the meaning of the
  language and the oracle for every backend.
- The store precedes claims. Evidence is attached to hashes.
- Obligations precede refinement types. A refinement is an obligation at
  each construction.
- Port models precede simulation. A port without a model cannot be
  simulated.
- Cost bounds precede step budgets in the scheduler.

## What changes from today's Vibra

| Today | In this design |
| --- | --- |
| S-expression source files | Stored trees with a line-oriented text projection |
| `defn` | `fn`: claim and witness with separate owners |
| Effect rows and ceilings | `uses` clauses naming ports; effects are intents |
| No mutation, no loops | Kept. Transforms and recursion, with tail calls |
| `lambda` | Named nested `fn` |
| Patterns with binders in `match` | `match` narrows a named value by case |
| `option` from indexing | `at` carries a proof obligation; `get` returns `option` |
| Tests | Examples in claims, plus graded evidence |
| Host calls | Drivers behind ports, each with a model |
| Interpreter and Wasm parity tests | Translation validation against the reference evaluator |
| Nexus checkpoints | Journal boundaries |
| Local actors | Machines with logical time |

One commitment of the current charter is changed. Run-time failure of
pure code is removed instead of typed, because an obligation is a stronger
guardrail than a handled error. Immutability and the absence of loops are
kept: see open question 7 for the evidence weighed.

## Open questions

These are undecided and marked where they arise.

1. **Local names.** The rethink proposed computed local names. This draft
   lets the author choose them and forbids shadowing.
2. **Liveness.** Machines state safety invariants. A bounded form of
   "eventually" is sketched, not specified.
3. **Floating point.** `f64` is specified as deterministic. Whether claims
   can reason about it beyond testing is open.
4. **Solver trust.** Grade `solved` trusts a solver's verdict. Whether
   proof certificates can be required everywhere is open.
5. **Distribution.** One journal per runtime instance is specified.
   Journals shared across nodes are not.
6. **Model training.** A surface per model family and a generated corpus
   are possible and unspecified.
7. **Loops and local variables.** This draft has neither, as today's
   Vibra has neither. An earlier revision added them. Nothing found
   shows their absence to be a mistake.
   - For loops: models pass fewer tasks in Haskell and OCaml than in Java
     (42% and 52% against 61% for one model) and write imperative
     patterns in functional languages 80% to 94% of the time
     ([FPEval](https://arxiv.org/html/2601.02060)). The comparison is
     confounded by how much of each language models have seen.
   - Against loops: a loop invariant that is not maintained is the most
     common verifier error in generated Dafny
     ([study](https://arxiv.org/html/2503.14183)), and models repaired a
     wrong invariant at most 16% of the time
     ([study](https://arxiv.org/html/2511.06552)). A `map` or a `fold`
     needs no invariant at its call site.
   - No study found compares loops with recursion while holding the
     language fixed.

   The same tasks written both ways would settle it. Until then the
   rule with fewer proof obligations and one way to iterate stands.
8. **Surface shapes.** Brackets, separators, and closers are listed with
   their alternatives in [02](02-syntax.md#shape-decisions) and are
   awaiting review.

## Reading order

Read the [charter](00-charter.md), then [01](01-programs.md) and
[02](02-syntax.md). After that the chapters are independent. The research
behind each decision is in the [rethink](../README.md), the
[field notes](../paradigms.md), the [syntax note](../syntax.md), and the
[tooling note](../tooling.md).
