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
effects, agent-facing tooling, and runtime behavior. A feature is not part of
v1 unless all affected surfaces agree.

## Decision order

When two designs conflict, prefer the design that:

1. makes invalid programs unrepresentable or locally diagnosable;
2. gives an agent one canonical way to express an idea;
3. exposes type, effect, and namespace boundaries explicitly;
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

## V1 commitments

- Vibra is expression-oriented and functional: bindings and values are
  immutable, functions are first-class values, and there is no assignment or
  user-visible mutation.
- Pure collection transforms use the standard `iter` interface. Effectful walks
  are recursive functions over `iter.next` with an explicit written effect
  ceiling. There is no separate loop or foreach form.
- Tail-position recursive calls MUST NOT consume additional language-level
  stack; the interpreter and WebAssembly backend MUST implement this
  obligation.
- Source files use `.vib`; compiler-owned persistent data uses `.vibon`. Both
  are UTF-8 S-expression document grammars over one lexical reader and are
  never inferred from contents.
- Names use kebab-case and imports produce explicit aliases.
- Public boundaries carry complete types and effect ceilings.
- Types, interfaces, their explicit implementations, and effects are nominal.
- Typed `option` and `result` replace null and exceptions.
- Effects describe possible operations statically. A binary target's declared
  effect roots are its complete execution consent; v1 has no runtime grants.
- Project and compiler-owned persistent data use canonical, non-executable
  `.vibon` literal data. JSON is reserved for CLI and MCP interoperability.
- Toolchain-owned external declarations use only the closed `@compiler` and
  `@host` providers; ordinary packages cannot add providers or registry symbols.
- Diagnostic, query, edit-plan, test, and command results have versioned JSON
  schemas.
- The parser is recovery-oriented, while the formatter defines one canonical
  representation.
- The runtime is deterministic for fixed source, inputs, and ordered host
  responses.

## Deliberate v1 exclusions

The following are not partially implemented in v1:

- pre-v1 source, CLI, schema, package, or ABI compatibility;
- macros, reader extensions, compiler plugins, and runtime plugins;
- algebraic effect handlers, effect polymorphism, sealed effect roots, declared
  effect dominance or sub-effect relations, runtime grants, permission prompts,
  path-scoped capabilities, and user-defined host providers;
- assignment, `while`, `for`, `break`, `continue`, and `return` forms;
- async functions, tasks, channels, threads, and shared mutable state;
- raw WebAssembly FFI, native FFI, dynamic loading, and a package registry;
- a SemVer dependency solver; dependencies are local or exact-revision Git;
- arbitrary unions, inheritance, implicit interface conformance apart from the
  predeclared empty interface `any`, dependent or refinement types, and a
  general ownership/borrowing system;
- language-defined fuel, memory, or host-operation budgets and scoped host
  resource or handle lifetimes;
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

After 1.0, the source language, project-data format, machine schemas, artifact
format, and host ABI are independently versioned. A reader MUST reject a newer
major version it cannot interpret rather than guessing.
