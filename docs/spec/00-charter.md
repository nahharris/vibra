# Vibra v1 charter

Status: normative target
Implementation status: not started
Version: 1.0 design line

## Mission

Vibra is a general-purpose language for small services, command-line programs,
local automation, and libraries whose source is primarily written and changed
by coding agents. It is not a natural-language programming system. It is a
small, explicit programming language that gives an agent enough local,
machine-readable constraint to produce correct changes reliably.

The language is one product with five surfaces: syntax, static types, static
effects plus runtime authority, agent-facing tooling, and runtime behavior. A
feature is not part of v1 unless all affected surfaces agree.

## Decision order

When two designs conflict, prefer the design that:

1. makes invalid programs unrepresentable or locally diagnosable;
2. gives an agent one canonical way to express an idea;
3. exposes type, effect, namespace, and authority boundaries explicitly;
4. permits precise context queries and transactional edits;
5. is deterministic across checker, interpreter, and compiled runtime; and
6. keeps the v1 implementation small enough to finish and verify.

Canonical syntax is a means, not the thesis. Most generation failures are
semantic. Vibra therefore spends complexity on the type/effect checker and its
query service before adding surface convenience.

## Definition of a usable v1

V1 is usable when a fresh installation can, without the pre-v1 tree:

- create a project and resolve pinned dependencies;
- format, check, test, run, and build it;
- express nominal data, interfaces, generics, typed failure, and exhaustive
  control flow;
- perform approved console, filesystem, environment, clock, and random
  operations through checked effects and fail-closed authority;
- query expected types/effects and symbol context at a source position;
- preview and atomically apply supported semantic code changes through the CLI
  or MCP; and
- produce the same observable result in the reference interpreter and the
  WebAssembly backend for every conforming program.

The release gate is evidence, not feature count. Every normative rule needs a
conformance case or an explicit review-only invariant.

## V1 commitments

- Source files use `.vib` and UTF-8 S-expressions.
- Names use kebab-case and imports produce explicit aliases.
- Public boundaries carry complete types and effect ceilings.
- Types, interfaces, implementations, and effects are nominal.
- Typed `option` and `result` replace null and exceptions.
- Effects describe possible operations; runtime grants decide permitted
  authority. Neither substitutes for the other.
- Host resources are scoped and cannot escape their owning lexical resource
  scope.
- Project, diagnostic, query, edit-plan, test, and build outputs have versioned
  JSON schemas.
- The parser is recovery-oriented, while the formatter defines one canonical
  representation.
- The runtime is deterministic for fixed source, inputs, grants, budgets, and
  host responses.

## Deliberate v1 exclusions

The following are not partially implemented in v1:

- pre-v1 source, CLI, schema, package, or ABI compatibility;
- macros, reader extensions, compiler plugins, and runtime plugins;
- algebraic effect handlers, effect polymorphism, and user-defined runtime
  authority providers;
- async functions, tasks, channels, threads, and shared mutable state;
- raw WebAssembly FFI, native FFI, dynamic loading, and a package registry;
- a SemVer dependency solver; dependencies are local or exact-revision Git;
- arbitrary unions, inheritance, implicit interface conformance, dependent or
  refinement types, and a general ownership/borrowing system;
- network and child-process host APIs;
- arbitrary syntax-tree patch languages or unauthenticated remote writes;
- self-hosting, JIT compilation, optimization promises, or verified-compiler
  claims.

An excluded feature requires a post-v1 specification. It MUST NOT appear as an
undocumented experimental branch in the active v1 implementation.

## Compatibility policy

Before the first 1.0 release, the v1 design may break deliberately, but every
break changes the specification, conformance corpus, and roadmap together.
There is no compatibility bridge from pre-v1. Migration tools may exist as
separate, one-shot utilities; the compiler MUST NOT auto-detect or accept a
retired surface.

After 1.0, the source language, project format, machine schemas, artifact
format, and host ABI are independently versioned. A reader MUST reject a newer
major version it cannot interpret rather than guessing.
