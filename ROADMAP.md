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
3. **Every gate is executable.** A milestone closes when a command exits zero,
   never when it looks done.
4. **Diagnostics ship with features.** A feature whose rejections have no
   stable code is not finished, and its milestone does not close.
5. **Both suites, every time.** `cargo test` and `vibra test` must pass before
   any commit that closes a milestone.

## Salvage from `v0-archive`

Worth reading before rewriting the equivalent. None of it is copied wholesale;
all of it saves a design cycle.

| Take | Where | Why |
| --- | --- | --- |
| The reader and lossless CST | `src/syntax/` | The token and trivia model held up across a whole language migration. |
| Formatter idempotence tests | `src/syntax/printer.rs`, `tests/` | The invariant is subtle and the test harness for it is proven. |
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

**Gate:** a spec-lint tool reports zero unreferenced diagnostic codes, zero
untagged code blocks, and zero broken cross-references.

### M1 — Reader and formatter

Lexer, CST with full trivia, spans, parser to an untyped form tree.

- `vibra fmt` and `vibra fmt --write`, implementing the canonical form rules in
  [Lexical structure](spec/02-lexical.md).
- `E-LEX-*` and `E-SYN-*` complete.

**Gate:** `fmt --write` is byte-idempotent over the conformance corpus;
comments survive a parse-print round trip verbatim; every `E-LEX` and `E-SYN`
block is rejected with exactly its code.

### M2 — Modules, names, scopes

Module loading, `project.vib`, import resolution, visibility, the resolution
order, and shadowing rejection.

- `E-IMPORT-*`, `E-PROJ-*`, `E-NAME-*`, `E-SCOPE-001`, `E-VIS-*`.
- `--format json` diagnostics against `diagnostic.schema.json`.

**Gate:** import cycles, escaping paths and shadowing are all rejected with the
specified code and primary span; the JSON envelope validates against its schema.

### M3 — Types

The core checker: primitives, records, tuples, enums, newtypes, generics with
bounds, monomorphization, pattern matching with exhaustiveness, and `deftype`.

- `E-TYPE-*`, `E-CALL-*`, `E-CAST-*`, `E-MATCH-001`, `E-MUT-001`, `E-FLOW-*`.
- No dynamic escape: the checker has no path that assigns an unknown type.

**Gate:** exhaustiveness names a concrete uncovered value for nested enums,
tuples and arrays; generic instantiation is deterministic; no `unwrap`-shaped
hole exists in the checker for an unresolved type.

### M4 — Interfaces

`defint`, `implements:`, required and provided members, superinterfaces,
dispatch, and the `(self self)` rule.

- `E-INT-*`.

**Gate:** conformance failures list every missing member; a provided method
cannot be overridden; dispatch through an interface-typed parameter resolves at
run time and through a monomorphized generic resolves statically.

### M5 — Effects

`deffect`, boundary declarations, the inference fixpoint, root subsumption,
interface ceilings, and `vibra effects`.

- `E-EFFECT-*`, `W-EFFECT-001`.
- `effects.schema.json`.

**Gate:** inference converges through a mutually recursive private call chain;
over-declaration warns and under-declaration errors with the uncovered
operations named; the report's inferred rows come from the same fixpoint the
checker uses, proven by a test that would fail if a second implementation
existed.

### M6 — Backend and runtime

Lowering to WebAssembly, the `vibra_v1` host ABI, the value arena, and
`vibra run` / `vibra build`.

- `E-ABI-*`; arithmetic traps; `E-OP-002` for literal operands.
- Scope-tied arena reclamation.
- Deterministic artifacts.

**Gate:** a loop allocating host-arena values in a scope runs to completion with
bounded host memory — the previous implementation's arena grew without bound and
this is the test that must fail before it passes; two builds of the same source
are byte-identical; a cross-boundary signature carrying a reference is rejected;
the pass-ordering invariant is asserted by a test.

### M7 — Kernel standard library

The modules in [Standard library](spec/07-stdlib.md), in two stages.

1. **Pure:** `core`, `text`, `bytes`, `array`, `map`, `math`, `convert`,
   `path`, `error`, and the core interfaces implemented for primitives.
2. **Effectful:** `stream`, `io`, `fs`, `env`, `process`, `sys`, `time`,
   `random`.

**Gate:** every module has a `tests/stdlib-<module>.vib`; a bare `vibra test` is
hermetic — no network, no subprocesses, no real environment reads, no writes
outside the runner's temporary directory; every error type implements `error`
and every cross-module conversion is explicit.

### M8 — Test runner and lint

`vibra test` with profiles, tags and filters; `vibra lint` with `--fix`;
`test-report.schema.json`.

- `W-RESULT-001`, `W-BIND-001`, `W-MATCH-001`, `W-NAME-001`, `W-IMPORT-001`.

**Gate:** `--deny-warnings` fails a run with any warning; every mechanical fix
produces output that is already canonical, so `lint --fix` followed by
`fmt --check` passes without a rewrite.

### M9 — v1

- The three acceptance programs from the Charter compile, run, and are covered
  by tests.
- `README.md` documents only commands that exist.
- The spec-conformance suite passes in CI on every supported platform.
- A tagged release, and a written statement of what v1 does not promise, taken
  verbatim from the Charter.

**Gate:** a reader who has only `spec/` and the toolchain can write and run a
CLI program without reading the compiler source.

## After v1

The waves in [Deferred and rejected](spec/10-deferred.md), in order:

| Wave | Content |
| --- | --- |
| 1 | Enforcement: capability grants, execution limits, arena bounds |
| 2 | Agent surface: `vibra index`, LSP, MCP, structural rewrites |
| 3 | Handle lifecycle: in-danger propagation |
| 4 | Packages and distribution: libraries, version solving, native and OCI targets |
| 5 | Concurrency and network |
| 6 | Macros and the type-constrained decoding service |

Wave 1 goes first because v1 ships a language that describes authority it does
not check. Wave 2 is the pillar v1 most visibly lacks, and it needs a type
system that has stopped moving — which is exactly what M0 through M9 produce.

## Progress

Milestones are tracked as GitHub issues, one per milestone, each listing its
gate as a checklist. A milestone issue closes only when its gate command exits
zero on `main`.

| Milestone | Status |
| --- | --- |
| M0 Freeze the spec | In progress |
| M1 Reader and formatter | Not started |
| M2 Modules, names, scopes | Not started |
| M3 Types | Not started |
| M4 Interfaces | Not started |
| M5 Effects | Not started |
| M6 Backend and runtime | Not started |
| M7 Kernel standard library | Not started |
| M8 Test runner and lint | Not started |
| M9 v1 | Not started |
