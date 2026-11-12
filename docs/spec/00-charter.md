# Vibra charter

Status: normative target
Delivery status is tracked in the [release roadmap](../roadmap/README.md).
The active specification includes contracts for work scheduled after the
shipped release.

## Mission

Vibra is a general-purpose language for small services, command-line programs,
local automation, and libraries whose source is primarily written and changed
by coding agents. It is not a natural-language programming system. It is a
small, explicit programming language that gives an agent enough local,
machine-readable constraint to produce correct changes reliably.

The language is one product with five surfaces: syntax, static types, static
effects, agent-facing tooling, and runtime behavior. All affected surfaces
must agree on each feature.

## Decision order

When two designs conflict, prefer the design that:

1. makes invalid programs unrepresentable or locally diagnosable;
2. gives an agent one canonical way to express an idea;
3. exposes type, effect, and namespace boundaries explicitly;
4. permits precise context queries and transactional edits;
5. is deterministic across checker, interpreter, and compiled runtime; and
6. keeps the implementation small enough to finish and verify.

Canonical syntax is a means, not the thesis. Most generation failures are
semantic. Vibra therefore spends complexity on the type/effect checker and its
query service before adding surface convenience.

## Usable toolchain target

A fresh installation of the complete toolchain can do the following using
only the active implementation.

- create a project and resolve pinned dependencies;
- format, check, test, run, and build it;
- express nominal data, interfaces, generics, typed failure, and exhaustive
  control flow;
- perform console, filesystem, environment, clock, and random operations whose
  nominal effects are checked against the selected project target;
- query normalized syntax, identity, type, effect, context, and diagnostic
  metadata from a source position or entity;
- preview and atomically apply supported semantic code changes through the CLI
  or MCP; and
- produce the same observable result in the reference interpreter and the
  WebAssembly backend for every conforming program.

The release gate is evidence, not feature count. Every normative rule needs a
conformance case or an explicit review-only invariant.

## Commitments

- Vibra is expression-oriented and functional: bindings and values are
  immutable, functions are first-class values, and there is no assignment or
  user-visible mutation.
- Pure collection transforms use the standard `iter` interface. Effectful walks
  are recursive functions over `iter.next` with an explicit written effect
  ceiling. There is no separate loop or foreach form.
- Every call in tail position MUST reuse the current activation and so MUST NOT
  consume additional language-level stack, whatever its callee is; the
  interpreter and WebAssembly backend MUST implement this obligation.
- Non-tail recursion has no depth limit of its own: neither backend consumes
  host stack per activation, so depth is bounded only by the memory of the
  instance, and exhausting that memory is a host event rather than a program
  result.
- Source files use `.vib`; compiler-owned persistent data uses `.vibon`. Both
  are UTF-8 S-expression document grammars over one lexical reader and are
  never inferred from contents.
- Names use kebab-case and imports produce explicit aliases.
- Public boundaries carry complete types and effect ceilings.
- Declared types, interfaces, their explicit implementations, and effects are
  nominal: every `deftype` introduces a new identity. Anonymous `tuple`,
  `record`, `enum`, and `union` type expressions are structural and carry no
  methods or implementations.
- Every widening relation is declared and every narrowing is written. A value
  of a member type widens into a union that lists it, `match` narrows that
  union back to one member, `as` ascribes a type at any expression, and the
  `from` and `try-from` interfaces convert. No conversion is implicit.
- Typed `option` and `result` replace null and exceptions.
- Bindings are `let` and `let-else` elements of a body sequence, and they last
  to its end. Early exit is `try`, which propagates a failure, or `return`,
  which leaves the innermost function with a value; `let-else` states a
  refutable binding whose mismatch must leave. The predeclared uninhabited type
  `never` types an expression that does not complete, and it is admitted at
  any expected type without becoming a subtype or a computed join.
- Effects describe possible operations statically. A binary target's declared
  effect roots are its complete execution consent. Runtime grants are excluded.
- Project and compiler-owned persistent data use canonical, non-executable
  `.vibon` literal data. JSON is reserved for CLI and MCP interoperability.
- The language core is minimal. The compiler owns the scalar types, `(array t)`,
  `fn` types, and the structural type constructors; every other type, including
  `bool`, `str`, `dict`, `option`, and `result`, and every operation over one, is
  standard-library Vibra. The compiler knows such a type only by the closed
  language role it plays, and a toolchain accelerates library code only through
  native implementations whose meaning stays their Vibra body.
- Toolchain-owned external declarations use only the closed `@compiler` and
  `@host` providers; ordinary packages cannot add providers or registry symbols.
- Diagnostic, query, edit-plan, test, and command results have versioned JSON
  schemas.
- The parser is recovery-oriented, while the formatter defines one canonical
  representation.
- The runtime is deterministic for fixed source, inputs, and ordered host
  responses.

## Deliberate exclusions

The active specification excludes the following features.

- pre-v1 source, CLI, schema, package, or ABI compatibility;
- macros, reader extensions, compiler plugins, and runtime plugins;
- algebraic effect handlers, effect polymorphism, sealed effect roots, declared
  effect dominance or sub-effect relations, runtime grants, permission prompts,
  path-scoped capabilities, and user-defined host providers;
- assignment, `while`, `for`, `break`, and `continue` forms;
- async functions, tasks, channels, threads, and shared mutable state;
- raw WebAssembly FFI, native FFI, dynamic loading, and a package registry;
- a SemVer dependency solver; dependencies are local or exact-revision Git;
- union subtyping and computed least upper bounds; a union is only ever the one
  an author wrote over a closed, written member set, and it is never inferred;
  `never` is no exception: it is admitted at an expected type and skipped when
  branches are compared, it is never inferred for a generic parameter, and it
  satisfies the bound `any` and no other interface;
- narrowing an interface value to a concrete type, runtime type tests, and
  interface-typed `match` arms;
- associated types on an interface contract, and therefore per-implementation
  error types on `try-from`;
- inheritance, implicit interface conformance apart from the predeclared empty
  interface `any`, dependent or refinement types, and a general
  ownership/borrowing system;
- language-defined fuel, memory, or host-operation budgets and scoped host
  resource or handle lifetimes;
- network and child-process host APIs;
- arbitrary syntax-tree patch languages or unauthenticated remote writes;
- self-hosting, JIT compilation, optimization promises, or verified-compiler
  claims.

An excluded feature MUST receive a specification change before implementation.
The [scratchpad](../roadmap/scratchpad/README.md) holds unscheduled proposals.

## Compatibility policy

Before the first stable release (v1.0.0), the design may break deliberately.
Each break changes the specification, conformance corpus, and roadmap together.
There is no compatibility bridge from pre-v1. Migration tools may exist as
separate, one-shot utilities; the compiler MUST NOT auto-detect or accept a
retired surface.

Starting with the stable release, the source language, project-data format, machine schemas, artifact
format, and host ABI are independently versioned. A reader MUST reject a newer
major version it cannot interpret rather than guessing.

## M1 codebase container

For M1, a local SQLite file MAY serve as a private physical container for compiler-owned persistent data. The database file is not a VIBON document. Every authoritative record stored in it MUST be canonical non-executable VIBON bytes under a closed versioned schema; its table layout, derived indexes, page bytes, and journal format are private implementation details. The container does not weaken the commitment that compiler-owned persistent records use VIBON, and it does not create a portable database or arbitrary database-query surface. The canonical VIBON records, not the physical database bytes, define portable record meaning and content addresses.
