# Verification

Status: design direction; not normative
Lines: V0 in [1.x](README.md#1x--foundations), V1 in
[Vibra 2](README.md#vibra-2--concurrent-services), V2 in
[Vibra 3](README.md#vibra-3--distributed-and-proven), V3 on the
[horizon](README.md#horizon)

## Why Vibra should verify programs

Agents produce plausible code quickly. What they lack is a cheap, local signal
that the code means what was asked. V1 supplies that signal for shape (types)
and reach (effects). Verification supplies it for meaning. A contract states
what a function promises. A checker either proves the promise or returns a
concrete counterexample the agent can act on.

Vibra's v1 design already removes most of what makes verification expensive in
mainstream languages:

| V1 property | Consequence for verification |
| --- | --- |
| Immutable values, no references or aliasing | No heap reasoning, frame conditions, or separation logic for Vibra values |
| No assignment or loops, mandatory tail calls | Every loop is a recursive function with explicit parameters, so no loop invariants are needed beyond function contracts |
| Checked arithmetic returning `result` | Overflow is explicit in types, so obligations can use mathematical integers with range facts |
| Static dispatch and nominal interfaces | Every call target is known, or bounded by a contract member |
| Static effect rows | Pure code is identifiable, so specifications can only call pure code |
| Deterministic semantics shared by two backends | One semantics to model, validated by differential testing |

## Design choice: auto-active verification first

Two broad styles exist:

- **Interactive proof over dependent types**, as in Lean, Coq/Rocq, Agda, and
  Kind. Propositions are types and proofs are programs or tactic scripts. It
  is very expressive, but specifications live in a richer type theory than the
  host language, and the checker cannot say *why* an unproven goal is false.
- **Auto-active verification**, as in Dafny, Verus, F\*, SPARK, and Liquid
  Haskell. Contracts are written in the host language. An SMT solver
  discharges verification conditions automatically, the author adds hints
  (lemmas, assertions, measures) only where it gets stuck, and a failure
  comes with a counterexample model.

Vibra adopts **auto-active verification as the primary model** and keeps a
path to interactive proof for the hard residue:

1. The charter excludes dependent and refinement types from v1. Contracts in
   ordinary Vibra expressions add meaning without changing the type system's
   decidable core.
2. Counterexamples are the agent-native failure. A structured, concrete input
   that breaks `ensures:` is a diagnostic an agent can fix. An open tactic goal
   is not.
3. Specifications are ordinary pure Vibra, so the same query, rename, format,
   and edit-plan machinery covers them.
4. Recent model progress on Lean proofs is real. Stage V3 therefore keeps a
   bridge to Lean's kernel for obligations the solver cannot close, instead of
   betting the whole design on either style.

A note on Bend: Bend (on the HVM2 runtime) is a massively parallel functional
language. It is not primarily a proof system. Its relevance to Vibra is the
parallelism track, discussed in
[Concurrency and distribution](concurrency.md#data-parallelism). The same group's proof-oriented
language is Kind, which is in the interactive, dependently typed family above.

## Stages

Each stage is usable on its own and is a strict extension of the previous
one. Source written for an earlier stage keeps its meaning.

### V0 — executable contracts and properties

Line: 1.x. No solver.

**Contracts** are attributes on `defn`, nested methods, and interface contract
members. They are pure boolean expressions over the parameters. `ensures:`
takes one flat binder/expression pair that names the result:

```vibra
(defn clamp (value i64 low i64 high i64) i64
  requires: (integer.le low high)
  ensures: (out (bool.and (integer.le low out) (integer.le out high)))
  visibility: @public
  (if (integer.lt value low) low (if (integer.lt high value) high value)))
```

- Contract expressions MUST be pure (empty performed row). An effectful
  contract is a static error.
- During `vibra test`, every call inside a test checks `requires:` on entry and
  `ensures:` on exit. A violation stops that test with a contract outcome that
  names the violated clause, the call site, and the argument values as VIBON
  data. It is distinct from an assertion failure and from a trap.
- In `run` and `build`, V0 contracts are erased. They never change production
  semantics, so adding a contract never changes a program's behavior.
- An interface contract member's clauses apply to every implementation.

**Property tests** extend `test` with a labelled `for-all:` binder list:

```vibra
(test "reverse is an involution"
  for-all: (xs (array i32))
  (assert.equal (array.reverse (array.reverse xs)) xs))
```

- Inputs come from the standard `arbitrary` interface. The toolchain provides
  closed conformance for primitives and builtin collections, and nominal types
  implement it explicitly. There is no derive mechanism, because a derive
  would be a macro.
- Generation is driven by an injected seed, so a run is deterministic. A
  failure reports the seed, the original input, and a shrunk minimal input as
  VIBON values. `vibra test --replay <seed>` reproduces it.
- A callee's `requires:` filters generated inputs, and its `ensures:` is checked
  on every call. Contracts and properties therefore strengthen each other
  before any solver exists.

V0 is valuable on its own. It is also the adoption path: an agent that writes
contracts for tests has already written V1's specifications.

### V1 — static contracts and termination

Line: Vibra 2. Adds a pinned SMT solver to the toolchain.

**Termination.** A verified function MUST terminate, because a non-terminating
function can "prove" anything. Termination is checked per recursive group:

- structural recursion on enum payloads and on strictly shorter arrays,
  strings, and bytes is recognized automatically; and
- otherwise the function writes a `decreases:` measure, a pure expression of a
  well-founded type (unsigned integers, or tuples of them ordered
  lexicographically), which must decrease on every recursive call in the
  group.

Termination is a property recorded on the function and exposed to queries. It
is required only where verification needs it: contract expressions, lemma
bodies, and functions whose contracts are claimed as verified. Ordinary
programs may still write non-terminating service loops. A process's receive
loop is not expected to terminate.

**Static discharge.** For each function with contracts, the checker generates
verification conditions from typed IR and asks the solver to prove them:

- callers must establish the callee's `requires:`, and may assume its
  `ensures:` without reading its body;
- bodies are translated using exact integers with range facts, sequence
  theory for arrays, strings, and bytes, and a presence-aware array theory for
  maps. Floats start uninterpreted, apart from IEEE facts the stdlib states
  explicitly;
- a `match` over an enum or union yields one case split per arm, and
  exhaustiveness is already known;
- an implementation of an interface member must satisfy the member's
  contract, with a weaker or equal `requires:` and a stronger or equal
  `ensures:`, which gives behavioral subtyping for interface values; and
- effectful functions may carry contracts over their parameters and results.
  Host responses are unconstrained inputs unless the registry entry states a
  postcondition.

**Invariant newtypes** turn v1 newtypes into checked refinements without
refinement types:

```vibra
(deftype port (newtype u16)
  invariant: (value (u16.gt value 0u16))
  visibility: @public)
```

The constructor gains the invariant as its `requires:`, and every unwrap may
assume it. Where verification is off, the only way to construct the newtype
from untrusted data is its `try-from` conversion, which checks the invariant at
run time. Either way, no `port` value ever violates its invariant.

**Specification-only functions.** Quantifiers are unbounded and cannot run.
They are standard-library functions (`spec.all`, `spec.exists`) that accept a
predicate lambda and are marked `mode: @spec`. A `@spec` function may be called
only from contracts, measures, invariants, lemmas, and other `@spec` functions.
Executable code that calls one is a static error, and both backends erase it.

**Policy and status.** Every contract has a queryable status: `@checked`
(tested only), `@verified`, `@failed` (with a counterexample), or `@assumed`.
`@assumed` is the explicit escape hatch: an attribute `assume: "reason"` on the
function, reported by queries and build metadata like an audited `unsafe`. A
target or library may set `verification: @required` in `project.vibon`, which
turns every unverified contract reachable from it into an error.

**Determinism.** Solver outcomes must not depend on the machine:

- the solver is part of the toolchain and pinned by its version;
- limits are deterministic resource counts, never wall-clock timeouts;
- each obligation is small and per function, and function bodies are opaque to
  other functions unless explicitly revealed, which limits proof brittleness;
  and
- results are cached by the hash of the obligation and the solver version. The
  cache is a speed optimization, and a clean check reproduces every result.

**Agent-facing surface.**

- `vibra query --include obligations` lists each obligation at a position, its
  status, and the facts in scope.
- A failed obligation reports its counterexample as typed VIBON values, mapped
  back to source names, with the failing path through `if` and `match` arms.
- Safe fixes offer edit plans that strengthen a `requires:` or add a
  `decreases:` candidate. These are suggestions and never auto-apply.

### V2 — lemmas and interface laws

Line: Vibra 3.

**Lemmas** are pure, erased, terminating functions whose result is a fact:

```vibra
(deflemma reverse-involutive (xs (array t))
  where: (t any)
  ensures: (array.equal (array.reverse (array.reverse xs)) xs)
  decreases: (array.length xs)
  ...)
```

The body is an ordinary pure expression (case splits, recursive lemma calls
for induction). A call to a lemma makes its `ensures:` available to the solver
at that point. A lemma adds no runtime code and no effect edge. `deflemma` is a
new top-level form. The top-level form set is closed, so adding one is
additive.

**Interface laws** make the algebraic laws that M3 documents into obligations:

```vibra
(defint ordered
  visibility: @public
  (defn compare (left self right self) ordering)
  (deflaw transitive (a self b self c self)
    ...))
```

A law is a contract member with one identity per interface, addressable as
`ordered.transitive`. Every `impl` must prove each law, or mark it `@assumed`.
Laws let `map` keys, `par.reduce` (which requires associativity), and sorted
collections rely on their interfaces soundly.

**Proof status travels with packages.** The lock and `@build` metadata record,
per public declaration, which contracts are verified or assumed, and under
which toolchain and solver version. `vibra query` exposes a dependency's
claims, so an agent can see that a library's `parse` is verified and its
`format` only tested.

### V3 — horizon

- **Lean bridge.** Export a stuck obligation, with a Lean model of the Vibra
  definitions it depends on, as a Lean theorem. Accept back a proof checked by
  Lean's kernel as a certificate stored beside the source. The model's fidelity
  to Vibra semantics becomes part of the trusted base.
- **Protocol verification for processes.** Typed mailboxes give message types,
  and simulation gives executions. Session types or TLA+-style model checking
  of process protocols are the next step once distributed systems ship.
- **Verified compilation.** Translation validation between typed IR and
  emitted Wasm, building on the differential harness.

## Trusted computing base

A verification claim is only as strong as what it trusts. Each stage's
specification MUST list:

- the checker's VC generator and the typed-IR semantics it models;
- the pinned solver, or the Lean kernel for V3 certificates;
- every `@assumed` contract and law in the dependency closure;
- the host registry's stated postconditions; and
- the compiler and runtime, which remain unverified before V3.

The query service reports this list for any declaration, so "verified" never
appears without its assumptions.

## Diagnostics sketch

Final names are chosen by the owning specification change. These names only
show that every outcome has a registry entry:

`@contract.effectful-clause`, `@contract.violated` (test outcome),
`@verify.precondition-unproven`, `@verify.postcondition-unproven`,
`@verify.termination-unproven`, `@verify.invariant-unproven`,
`@verify.law-unproven`, `@verify.spec-call-in-executable-code`,
`@verify.assumption` (warning).
