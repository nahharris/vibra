# Vibra implementation roadmap

This is the plan for building Vibra v1 against [`spec/`](spec/README.md). It is
a schedule, not a contract: milestones may be reordered or resized without
amending the specification. The specification may not be changed to match what
the implementation happens to do.

The pre-reboot implementation is on the `v0-archive` branch. It is a source of
salvage and of lessons, not a starting point.

## Ground rules

1. **Spec first.** No milestone starts before the spec sections it implements
   are frozen. A behavior that is not specified is not built.
2. **No bridge layers.** The previous cycle's most expensive artifact was a
   temporary adapter between a typed frontend and a legacy lowering path, which
   became permanent because it worked. If a milestone needs a shim to hit its
   gate, the gate is wrong or the milestone is too big. Split it.

   The reference interpreter is not a bridge. It is a permanent product,
   specified as the reference semantics, and it consumes the same typed IR as
   the WebAssembly backend forever. A bridge is a second path that exists to
   be deleted; the interpreter is a first path that exists to be compared
   against.
3. **Every milestone ends with something that runs.** Vertical slices, not
   layers. Parser-only, checker-only, or docs-only versions of a promised
   feature do not close a milestone. From M3 onward, every milestone's gate
   includes a program a user could actually write, run, and test.
4. **Every gate is executable.** A milestone closes when a command exits zero,
   never when it looks done.
5. **Diagnostics ship with features.** A feature whose rejections have no
   stable code is not finished, and its milestone does not close.
6. **Both suites, every time.** `cargo test` and `vibra test` must pass before
   any commit that closes a milestone.
7. **When implementation exposes a spec gap, amend the spec before writing
   more code.** The amendment process is in [`spec/README.md`](spec/README.md);
   the roadmap never overrides it.

## Architecture boundary

The implementation is a new Rust workspace with one dependency direction:

```text
syntax (lossless CST) -> resolved AST -> typed IR -+-> reference interpreter
                                                   |        |
                                                   |        +-> host registry
                                                   +-> Wasm backend

workspace snapshot service -> syntax / resolution / types / effects
CLI and MCP -> workspace snapshot service and execution entrypoints
```

There is exactly one lossless syntax tree, one resolved typed IR, one
diagnostic model, one effect call graph, and one workspace snapshot service.
The interpreter and the Wasm backend consume the same typed IR; the effect
report and the effect checker share one fixpoint; the CLI and the MCP server
are adapters over one engine, not alternate implementations.

Crate boundaries may evolve. Dependency arrows may not point from language
semantics into the CLI, the MCP server, or a backend. A milestone gate that
would be easier to hit by violating an arrow is the "no bridge layers" rule
being tested.

## Salvage from `v0-archive`

Worth reading before rewriting the equivalent. None of it is copied wholesale;
all of it saves a design cycle.

| Take | Where | Why |
| --- | --- | --- |
| The reader and lossless CST | `src/syntax/` | The token and trivia model held up across a whole language migration. |
| Formatter idempotence tests | `src/syntax/printer.rs`, `tests/` | The invariant is subtle and the test harness for it is proven. |
| The interpreter/Wasm differential pattern | `src/execute.rs`, `src/wasm_backend.rs` tests | v0 already proved parity testing between an interpreter and the backend catches lowering bugs nothing else catches. The pattern graduates from a test trick to the architecture. |
| The `vibra_v1` registry shape | `src/host_abi.rs`, `src/intrinsics.rs` | Already audited as scalars-only; the audit conclusion transfers. |
| Effect fixpoint over the call graph | `src/effect_semantics.rs` | The inference algorithm is sound; only its declaration policy changes. |
| The in-language test runner | `src/test_runner.rs`, `tests/README.md` | Profiles, tags and hermeticity conventions are worth keeping as designed. |
| Scope machinery with monotone narrowing | `src/async_runtime.rs` | Wave 1 needs exactly this, already implemented and tested. |

| Do not take | Why |
| --- | --- |
| `src/surface_adapter.rs` and the `Value`-based lowering path | The bridge this roadmap exists to avoid. |
| The macro expander | Macros are wave 6; carrying the machinery forward would constrain the type system for a feature nobody is using yet. |
| Accepted decision documents | Superseded by `spec/`. They are history, and history that looks normative is what caused the reboot. |

## Milestones

### M0 — Freeze the spec

Close the open questions in `spec/`, then stop moving it.

- Every section's status line reads `frozen`.
- Every grammar production has at least one conformance block.
- Every diagnostic code in the registry has a `vibra-bad` block that triggers
  it, or is removed from the registry.
- The three v1 acceptance programs from the [Charter](spec/01-charter.md) are
  written out as Vibra source and reviewed by hand. They do not compile yet;
  they are the target the spec is being checked against.

**Open amendments.** These came out of the parallel `codex/v1-spec-reboot`
review and must each be resolved — adopted or rejected, in the spec, with
conformance blocks updated — before any section freezes:

1. **Flat forms.** Flatten function parameters, interface member lists, map
   literals, and `match` arms, removing one nesting level from each. The
   proposal exists in the codex branch's source-language chapter; adopting it
   touches [Lexical](spec/02-lexical.md), [Types](spec/04-types.md), and
   [Expressions](spec/05-expressions.md).
2. **Entry admission.** A binary target's declared effect roots in
   `project.vib` are its complete execution consent, checked statically at the
   entry point. This strengthens the [Effects](spec/06-effects.md) boundary
   story without adding any runtime machinery, and it gives wave 1 a surface
   to attach grants to.
3. **Project-owned data is Vibra data.** Every file the toolchain owns —
   `project.vib` today, lockfiles and build plans in wave 4 — is canonical
   `.vib` record data. JSON is reserved for CLI/MCP interchange. Wave 4's
   lockfile entry currently says JSON and must follow whichever way this
   decision goes.
4. **`process` in or out of v1.** The [Charter](spec/01-charter.md) acceptance
   programs and [Standard library](spec/07-stdlib.md) include subprocesses;
   the codex review excluded them from v1. Keeping them holds the CLI thesis;
   dropping them shrinks M5. This one is genuinely open and needs an owner
   decision.
5. **Stdlib delivery.** In-repo and versioned with the compiler, or an
   ordinary pinned dependency the resolver treats like any other. The second
   is cleaner and exercises M6's machinery on day one; the first is less
   moving parts before the resolver exists.

**Gate:** a spec-lint tool reports zero unreferenced diagnostic codes, zero
untagged code blocks, zero broken cross-references, and zero occurrences of
the amendment markers above in an unresolved state.

### M1 — Reader and formatter

Lexer, CST with full trivia, spans, parser to an untyped form tree, error
recovery good enough to diagnose several problems per file.

- `vibra fmt` and `vibra fmt --write`, implementing the canonical form rules in
  [Lexical structure](spec/02-lexical.md).
- `E-LEX-*` and `E-SYN-*` complete.

**Gate:** `fmt --write` is byte-idempotent over the conformance corpus;
comments survive a parse-print round trip verbatim; every `E-LEX` and `E-SYN`
block is rejected with exactly its code; a configured fuzz campaign finds no
panic and no accepted input on which `fmt` is not idempotent.

### M2 — Modules, names, scopes

Module loading, `project.vib` decoding, import resolution, visibility, the
resolution order, and shadowing rejection.

- `E-IMPORT-*`, `E-PROJ-*`, `E-NAME-*`, `E-SCOPE-001`, `E-VIS-*`.
- `--format json` diagnostics against `diagnostic.schema.json`.

**Gate:** import cycles, escaping paths and shadowing are all rejected with the
specified code and primary span; the JSON envelope validates against its schema.

### M3 — Executable pure core

The walking skeleton: the smallest typed subset that runs, end to end, and
never a larger one. Primitives, `def`, functions, immutable bindings, calls,
`do`, `if`, loops, returns, checked arithmetic. The typed IR, the reference
interpreter, and interpreter-backed `vibra run` and `vibra test` for pure
programs, with the `core` slice of the stdlib and typed assertions.

- `E-TYPE-*` and `E-CALL-*` for the subset; `E-FLOW-*`; `E-OP-002`.
- Forms specified for later milestones are rejected with an explicit
  availability diagnostic, never misparsed as something else.

**Demo:** from a clean checkout, initialize a pure multi-module project, run
it, and run its tests — no network, no host operations, no compiler-private
escape hatch.

**Gate:** the interpreter conformance profile passes for the subset; pure
execution emits no host event; no semantics crate depends on a CLI or MCP
crate; the subset checker has no path that assigns an unknown type.

### M4 — Complete nominal static core

Everything pure that remains: records, tuples, enums, newtypes, generics with
bounds and monomorphization, `defint`, `implements:`, dispatch, exhaustive
`match`, `option`, `result`, `try`, and the unhandled-failure diagnostics. The
rest of the pure stdlib: `text`, `bytes`, `array`, `map`, `math`, `convert`,
`path`, `error`, and the core interfaces implemented for primitives.

- `E-TYPE-*`, `E-CALL-*`, `E-CAST-*`, `E-MATCH-001`, `E-MUT-001`, `E-INT-*`,
  `W-RESULT-001`, `W-BIND-001`.

**Demo:** implement and test a generic collection-processing library with a
nominal error type and an interface implementation — acceptance program 2
from the Charter, running under the interpreter.

**Gate:** exhaustiveness names a concrete uncovered value for nested enums,
tuples and arrays; generic instantiation is deterministic; conformance
failures list every missing member; a provided method cannot be overridden;
interpreter behavior is independent of map iteration order; silently dropped
fallible values produce their diagnostic.

### M5 — Effects and host operations

`deffect`, boundary declarations, the inference fixpoint, root subsumption,
interface ceilings, target entry admission, and `vibra effects`. The closed
`vibra_v1` host registry and the effectful stdlib: `stream`, `io`, `fs`,
`env`, `sys`, `time`, `random`, and `process` if M0's amendment kept it.
Deterministic injected test providers, so the `@core` test profile stays
hermetic while effectful code is still testable.

- `E-EFFECT-*`, `W-EFFECT-001`, `E-ABI-*`.
- `effects.schema.json`.

**Demo:** acceptance program 1 — the file-processing filter — runs under the
interpreter. The same source is rejected, with the uncovered roots named, when
its boundary declaration or its target's effect roots omit an operation it
performs.

**Gate:** inference converges through a mutually recursive private call chain;
over-declaration warns and under-declaration errors; the report's inferred
rows come from the same fixpoint the checker uses, proven by a test that would
fail if a second implementation existed; every host operation has exactly one
registry entry with an owner effect and a typed scalar signature; no
compiler-generated host read exists outside the registry.

### M6 — Projects and path dependencies

The Charter promises a program can depend on a second Vibra package by path;
this is where it happens. Multi-package projects, dependency-scoped names and
visibility across package boundaries, workspace hashing and invalidation, and
deterministic resolution. No registry, no version solving, no lockfile — those
are wave 4; a path dependency is resolved from what is on disk, every time,
reproducibly.

**Demo:** an application package depending on a library package by path
builds, runs, and tests from a clean checkout with the network disabled.

**Gate:** escaping-path and cycle attacks between packages have negative
tests; a dependency's private symbols are invisible with the specified
diagnostic; two checks of an unchanged workspace do identical work, proven by
the snapshot hash.

### M7 — Agent surface

The pillar the previous cycle proved valuable and the Opus draft deferred:
v1 ships with the machine interfaces an agent needs, built as adapters over
the same engine the CLI uses.

- `--format json` complete across every command, each format schema'd.
- `vibra query context`: which names, types, labels, and effect roots are
  valid at a position, over one workspace snapshot.
- The MCP server: check, fmt, lint, test, run, effects, and query, mirroring
  CLI results exactly; read-only tools cannot mutate or execute.
- `vibra lint` with `--fix`; the full test runner surface (profiles, tags,
  filters, `--deny-warnings`, `test-report.schema.json`).
- `W-MATCH-001`, `W-NAME-001`, `W-IMPORT-001`.

The retrieval index, the LSP server, and structural rewrites stay in wave 2:
they describe a type system and are built once it has stopped moving for a
release, not while it moves. The MCP surface here is the engine's existing
answers exposed to agents, which is why it is safe to ship now.

**Demo:** through MCP alone — no terminal scraping — inspect a project,
query the expected type and valid continuations at a position, run the tests,
and read the structured report.

**Gate:** tooling conformance passes against CLI and MCP renderings of the
same workspace and asserts they are byte-identical; a read-only MCP session
cannot mutate the workspace or execute code; every mechanical `lint --fix`
output is already canonical, so `fmt --check` passes without a rewrite;
`--deny-warnings` fails a run with any warning.

### M8 — WebAssembly backend

Lowering from the same typed IR the interpreter executes, the `vibra_v1` host
ABI, the value arena with scope-tied reclamation, `vibra build`, and
deterministic artifacts.

- `E-ABI-*` complete; arithmetic traps at the specified sites.

**Demo:** every conformance program and all three acceptance programs produce
identical observable output under the interpreter and the Wasm backend, and
`vibra run` executes the Wasm path by default.

**Gate:** the differential harness runs interpreter and backend over the whole
conformance corpus and fails on any divergence in result, diagnostic, trap, or
exit status; a loop allocating host-arena values in a scope runs to completion
with bounded host memory — the v0 arena grew without bound, and this test must
fail before it passes; two builds of the same source are byte-identical; a
cross-boundary signature carrying a reference is rejected.

### M9 — v1

- The three acceptance programs from the Charter compile, run identically on
  both execution paths, and are covered by tests.
- `README.md` documents only commands that exist.
- The spec-conformance suite passes in CI on every supported platform.
- A tagged release, and a written statement of what v1 does not promise, taken
  verbatim from the Charter.

**Gate:** a reader who has only `spec/` and the toolchain can write and run a
CLI program without reading the compiler source — and an agent who has only
the MCP server can do the same without reading human terminal output.

## After v1

The waves in [Deferred and rejected](spec/10-deferred.md), in order:

| Wave | Content |
| --- | --- |
| 1 | Enforcement: capability grants, execution limits, arena bounds |
| 2 | Deep agent surface: `vibra index`, LSP, structural rewrites, retrieval |
| 3 | Handle lifecycle: in-danger propagation |
| 4 | Packages and distribution: libraries, version solving, native and OCI targets |
| 5 | Concurrency and network |
| 6 | Macros and the type-constrained decoding service |

Wave 1 goes first because v1 ships a language that describes authority it does
not check — M5's entry admission gives it the exact surface to attach grants
to. Wave 2 completes the agent surface that M7 begins: the index, the LSP
server, and typed rewrites need a type system that has stopped moving, which
is exactly what M0 through M9 produce.

## Progress

Milestones are tracked as GitHub issues, one per milestone, each listing its
gate as a checklist. A milestone issue closes only when its gate command exits
zero on `main`.

| Milestone | Status |
| --- | --- |
| M0 Freeze the spec | In progress |
| M1 Reader and formatter | Not started |
| M2 Modules, names, scopes | Not started |
| M3 Executable pure core | Not started |
| M4 Complete nominal static core | Not started |
| M5 Effects and host operations | Not started |
| M6 Projects and path dependencies | Not started |
| M7 Agent surface | Not started |
| M8 WebAssembly backend | Not started |
| M9 v1 | Not started |
