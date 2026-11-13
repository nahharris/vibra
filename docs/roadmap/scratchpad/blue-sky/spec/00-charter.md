# 00 Charter

Status: draft specification; not normative

## Mission

Vibra is a general-purpose language whose programs are written and changed
by AI agents and whose intent is reviewed by people. It exists to make a
wrong program hard to write, hard to admit, hard to ship, and impossible to
hide.

It serves libraries, command-line programs, services, and long-running
workflows. It is not a natural-language programming system and not a proof
assistant. It is a programming language in which every definition carries
a checked statement of what it is for.

## The author

The design is fitted to an author with this profile.

| The author is | So the language |
| --- | --- |
| Cheap at volume, repetition, and retries | Asks for redundancy: types on every binding, examples, invariants |
| Good at formal detail when a checker returns counterexamples | Checks statically and reports concrete inputs |
| Unreliable over long contexts | Makes every check local to a cell and the claims it uses |
| Tempted to pass a check by weakening it | Separates who writes a claim from who satisfies it |
| Weak at stating intent | Measures claims before admitting them |
| Dependent on familiar notation | Uses familiar words and shapes wherever behavior is familiar |

## Requirement words

MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY have their usual meanings. A
MUST is a condition of conformance. A SHOULD may be departed from only for
a stated reason. Sections titled "Open" and text marked as a sketch are
not requirements.

## Decision order

When two designs conflict, prefer the one that:

1. makes the wrong program impossible to write or to admit;
2. makes each check local, with a bounded context;
3. keeps the trusted base small, whatever the untrusted machinery costs;
4. looks familiar where it behaves familiarly, and unlike anything where
   it does not;
5. gives one canonical way to say a thing;
6. is deterministic across every evaluator and backend.

Implementation size is not on this list. Machinery outside the trusted
base is allowed to be large.

## Commitments

**Programs**

- A program is a set of cells in a content-addressed store. Text files are
  a projection of the store.
- A cell has a claim, a witness, and evidence. A dependent of a cell sees
  its claim and never its witness.
- The store admits a cell only with evidence for each of its obligations
  or a recorded debt in place of it.

**Pure code**

- Functions are total. Recursion and `while` loops carry a decreasing
  measure.
- Pure code cannot fail at run time. Division, indexing, narrowing, and
  fixed-width arithmetic either carry a proof obligation or return an
  `option`.
- Values are immutable. A body may use local variables and loops. Nothing
  outside the body can observe them.
- There are no exceptions, no null, no implicit conversions, no
  overloading, no inheritance, and no reflection.

**Effects**

- Only procedures and machines have effects, and only by returning
  intents addressed to ports named in their `uses` clause.
- The runtime owns the event loop, time, randomness, and the performing of
  intents. Everything that enters or leaves a program is journaled.
- The state of a running program is a function of its journal.

**Checking**

- Every guarantee is reported with a grade. A grade is never rounded up.
- Every escape is a debt that is named, recorded, and inherited by
  dependents.
- A claim is checked for strength before it is admitted.

**Notation**

- The store holds one canonical tree per cell. Each text surface
  round-trips to it.
- Names are kebab-case. Namespaces are explicit. Nothing is imported
  implicitly.
- Source text uses `.vib`. Stored trees and all tool data use `.vibon`.

## The trusted base

A conforming toolchain MUST identify its trusted base, and it MUST consist
of no more than:

1. the reader for the canonical tree;
2. the kernel: the checker for the core language, the generator of
   obligations, and the checker for proof certificates;
3. the reference evaluator;
4. the journal and the scheduler of the reference runtime;
5. for grade `solved` only, the solver named in the evidence;
6. drivers, which are outside the language and are constrained by port
   models.

Everything else is untrusted: surface parsers and printers, elaboration,
solvers when certificates are checked, optimizers, backends, agents, and
the tools that propose code. An untrusted component is acceptable when a
trusted one checks its output.

## Deliberate exclusions

| Excluded | Reason |
| --- | --- |
| Macros and compile-time code execution | Notations are checked cells with a round-trip law instead |
| User-defined effect handlers | The runtime alone decides what an intent means |
| Ambient authority: globals, singletons, implicit context | Authority is a port named in `uses` |
| Exceptions and unwinding | Failure is a value or a ruled-out case |
| Shared mutable state | State belongs to one machine and changes one step at a time |
| Threads and locks | Concurrency is machines and composite intents |
| Operator precedence and infix operators | Prefix application has no table to recall |
| Anonymous functions | A function used as a value is declared and named |
| Tactic proof scripts | Lemmas are bodies checked by a solver |
| Direct foreign calls from pure code | Foreign code is a driver behind a port, or an `extern` witness with a debt |
| A compatibility bridge to earlier Vibra | The earlier corpus converts mechanically or is rewritten |

## Evidence over features

A layer is complete when its conformance suite passes on the reference
evaluator and on every backend claimed for it. A rule without a
conformance case or a stated review-only invariant is not part of a
release.
