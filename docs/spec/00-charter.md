# Vibra charter

Status: normative target
Delivery status is tracked in the [release roadmap](../roadmap/README.md).
This specification defines the language that v0.0.2 and later releases
implement. The shipped v0.0.1 release implements an earlier surface; nothing
in these chapters is implemented unless a chapter's status line says so.

## Mission

Vibra is a general-purpose language for command-line programs, libraries, and
small services whose source is primarily written and changed by coding
agents. It is not a natural-language programming system. It is a small,
explicit language that gives an agent enough local, machine-readable
constraint to produce correct changes reliably.

The language is one product with five surfaces: syntax, static types, static
effects and authority, agent-facing tooling, and runtime behavior. All
affected surfaces MUST agree on each feature.

Claims about what agents find easy are dated hypotheses about particular
model populations. A design choice justified by such a claim records the
evidence it rests on and is reopened when the evidence changes.

## Requirement words

MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY state requirements on a
conforming toolchain and, where the subject is a program, on a conforming
program. MUST and MUST NOT are absolute. SHOULD and SHOULD NOT admit a
documented reason to differ. MAY marks latitude that no program can observe.

## Decision order

When two designs conflict, prefer the design that:

1. makes an invalid program unrepresentable or rejects it at compile time;
2. reports a failure as a value the program handles, rather than stopping the
   program;
3. gives an agent one canonical way to express an idea, built from shapes the
   language already has;
4. exposes type, effect, authority, and namespace boundaries explicitly;
5. permits precise context queries and transactional edits;
6. is deterministic across the interpreter and compiled code; and
7. keeps the implementation small enough to finish and verify.

The surface is kept small: close concepts are collapsed into one, a rule is
stated once and reused, and every necessary exception is written down where
the rule is.

## Usable toolchain target

A fresh installation of the complete toolchain can, using only the active
implementation:

- create a project and resolve pinned dependencies;
- format, check, test, run, and build it into a native executable;
- express nominal data, interfaces, generics, typed failure, exhaustive
  control flow, and statically checked contracts;
- perform console, filesystem, environment, clock, and random operations
  whose effects are checked against written ceilings and whose authority
  comes only from grants bound in the project target;
- query normalized syntax, identity, type, effect, context, and diagnostic
  metadata from a source position or entity;
- preview and atomically apply supported code changes; and
- produce the same observable result in the reference interpreter and in
  native code for every conforming program.

The release gate is evidence, not feature count. Every normative rule needs a
conformance case or an explicit review-only invariant.

## Commitments

- **Immutable and functional.** Bindings and values are immutable, functions
  are first-class values, and there is no assignment, loop, or user-visible
  mutation. Repetition is recursion or the standard `Iter` interface.
- **One surface.** Source files use `.vib`: keyword-led constructs with typed
  closers, no commas, no infix operators, parentheses only for application,
  braces for product shapes, brackets for collections. Compiler-owned
  persistent data uses `.vibon`, the data subset of the same syntax, which
  is never executed. Neither is inferred from contents.
- **Names.** Types are PascalCase and values camelCase, by convention that
  `check` reports and no rule depends on. Imports produce explicit names.
- **Nominal declarations.** Declared types, interfaces, and their explicit
  implementations are nominal. Anonymous tuples and records are structural
  and carry no members.
- **Complete public boundaries.** Public declarations carry complete types,
  effect ceilings, and contracts.
- **No implicit conversion.** Every widening relation is declared and applies
  only at a written type. Conversion is a call through `From` or `TryFrom`.
- **Errors as values.** `Option` and `Result` replace null and exceptions.
  Partial operations return them. No operation of the language or its
  standard library traps on user data: arithmetic saturates or returns a
  `Result`, lookups return an `Option`, and a contract either is proven at
  compile time or states the value produced when it fails. The traps that
  remain are toolchain defects; exhausting memory is a host event.
- **Contracts checked statically.** A `requires`, `ensures`, or type
  invariant without a stated consequence is an obligation the checker proves
  or rejects. It leaves no run-time check.
- **Effects and authority are separate and both explicit.** A procedure's
  written ceiling states which operations it may perform. Performing one
  also needs a grant: a sealed value, passed as an ordinary argument. Every
  grant descends from one of two roots: a binding the project target
  declares, received at entry, or the root grant the test runner issues for
  a test's isolated world. Derivation from a root is restricted to the
  narrowing operations of the effects chapter and the test-world operations
  of the programs chapter. No ambient authority exists.
- **No dependency cycles.** Imports, package dependencies, module-value
  initializers, startup grant bindings, and build inputs are acyclic.
  Recursion among the declarations of one module, in functions and in types,
  is not a dependency cycle.
- **Tail calls.** A call in tail position reuses the current activation.
  Recursion has no depth limit of its own.
- **Library-first core.** The compiler owns the representation of the
  scalar types and `List`, which the embedded `@std.builtin` module declares
  with `Intrinsic` bodies, and the callable and structural type forms. Every
  other type is standard-library Vibra, known to the compiler only by a
  closed language role. Toolchain-owned external declarations use only the
  closed `@compiler` and `@host` providers.
- **Two executions, one meaning.** The interpreter is permanent: it is the
  reference for semantics and the evaluator the compiler uses at compile
  time. The native compiler and runtime produce standalone executables. The
  WebAssembly backend of v0.0.1 is retired when native execution reaches
  parity; it is not extended to the features this specification adds.
- **Machine contracts.** Diagnostic, query, edit-plan, test, and command
  results have versioned schemas. The parser recovers; the formatter defines
  one canonical representation.
- **Determinism.** The runtime is deterministic for fixed source, inputs,
  and ordered host responses.

## Deliberate exclusions

The active specification excludes:

- compatibility with any earlier source, CLI, schema, package, or ABI;
- macros, reader extensions, compiler plugins, and runtime plugins;
- algebraic effect handlers, effect polymorphism, row variables, effect
  subtyping, permission prompts, and user-defined host providers;
- assignment and loop forms;
- untagged unions, anonymous variant types, ascription, variadic parameters,
  call-site type arguments, and tuple index projection, each recorded with
  the condition that reopens it;
- narrowing an interface value, run-time type tests,
  inheritance, and implicit conformance apart from `Any`;
- quantified contracts, user-written proofs, and an ownership or borrowing
  system;
- a language-level budget for time, memory, or host operations;
- a foreign function interface, dynamic loading, and a package registry;
- a dependency version solver;
- concurrency in the first release of this specification; it is added as a
  library with structured lifetimes, as the roadmap schedules;
- network and child-process host operations, until an integration gate adds
  each with its authority and outcome contract;
- arbitrary syntax-tree patch languages and unauthenticated remote writes;
- self-hosting, just-in-time compilation, optimization promises, and
  verified-compiler claims.

An excluded feature MUST receive a specification change before
implementation. The [scratchpad](../roadmap/scratchpad/README.md) holds
unscheduled proposals and is not normative.

## Compatibility policy

Before the first stable release (v1.0.0) the design may break deliberately.
Each break changes the specification, the conformance corpus, and the roadmap
together. There is no compatibility bridge: the toolchain MUST NOT detect or
accept a retired surface. Migration tools may exist as separate, one-shot
utilities.

From the stable release on, the source language, project-data format,
machine schemas, artifact format, and host interface are versioned
independently. A reader MUST reject a newer major version it cannot
interpret.
