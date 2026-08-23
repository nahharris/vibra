# 10. Deferred and rejected

Status: draft

Everything Vibra might reasonably have, that v1 does not have, is listed here
with the wave that owns it or the reason it will never be built. Nothing is
allowed to be merely absent.

Waves are ordered but not dated. A wave starts when the previous one closes and
v1 has not regressed.

## Wave 1 — Enforcement

**Effects stop being erased.**

This wave exists because v1 ships a language that describes authority it does
not check, and that is the single largest gap between what Vibra claims and
what it does. It goes first for that reason.

| Item | Shape |
| --- | --- |
| Capability grants | `(authority (grant fs.read "/safe") ...)` in `project.vib`. A declared effect ceiling lowers to a grant requirement. Omitting authority is fail-closed. |
| Operation-time checking | Every concrete host operation re-checks its canonical `<root>.<operation>` grant and resource prefix at the boundary. Scope entry is a fast path; the operation check is the guarantee. |
| Monotone attenuation | A child scope may hold a subset of its parent's grants and never a superset. |
| Execution limits | `(limits fuel: ... memory: ...)` in `project.vib`. Fuel charged per call and loop iteration; memory tracked as logical high-water over arena values. Children narrow, never amplify. |
| Arena reclamation | Scope-tied region reclamation, so the memory ceiling is enforceable rather than nominal. |

Design constraints carried in from the start: budget enforcement is checked at
coarse boundaries, because the dominant cost of sandboxing is
boundary-crossing frequency and data volume, not the enforcement primitive.

Closing this wave is what lets Vibra describe itself as capability-safe. Until
then no document may.

## Wave 2 — Agent surface

**The compiler's second product: the index.**

| Item | Shape |
| --- | --- |
| `vibra index` | One deterministic record per function: `fmt`-normalized source, signature, effect row, outgoing call edges, error types, module path. JSON, schema'd, sorted keys. |
| LSP server | Diagnostics, hover, go-to-definition, and a context query reporting which names, types and labels are valid at a position. |
| MCP server | The same queries plus project management and structured code manipulation, for agent hosts. |
| `vibra expand`, `vibra rewrite rename` | Structural code manipulation over the typed tree. |

Two constraints on the index, both counter-intuitive and both worth writing
down before anyone builds it:

- **Evaluate first-stage retrieval by rejection precision, not top-k ranking.**
  A first stage that eliminates candidates reliably is worth more than one that
  ranks well, provided an exact verifier sits behind it. Vibra has that
  verifier — the type checker.
- **Normalization must be symmetric.** Apply `vibra fmt` to the query and the
  corpus, and do not strip comments or identifiers. One-sided normalization does
  not help.
- **Bake in no model choice.** The artifact is a format, not a retriever.

## Wave 3 — Handle lifecycle

Use-after-close and double-close become compile-time errors instead of
deterministic runtime failures.

The intended mechanism is **in-danger propagation**: statically taint everything
that shares structure with a destroyed value. This gives borrow-checker-like
safety without linearity and without new type theory, which matters because
handles are copyable in v1 and linearity would break that model.

Start with a spike establishing whether the sharing analysis is tractable over
Vibra's value semantics — the analysis, not the typing rules, is the expensive
part. Vibra's position is favorable (value semantics, unforgeable nominal
endpoints, less aliasing than the ML-family setting the technique comes from),
but that is a hypothesis to test before committing.

Path-dependent typestate with revocable capabilities is the better long-term
design and is **not** scheduled: it needs type theory Vibra does not have, the
source work has no benchmarks and no mutable-field support, and adopting it
wholesale is a research project rather than a feature.

## Wave 4 — Packages and distribution

| Item | Shape |
| --- | --- |
| `kind: @lib` targets | Libraries as first-class build outputs. |
| Git and registry dependencies | `git:`, `rev:` on `(dependency ...)`. |
| Version solving and lockfile | `vibra.lock.json` — canonical JSON, sorted keys, trailing newline. Generated metadata is JSON, not Vibra source. |
| `.vapp` artifacts | Reproducible packaged applications. |
| `vibra build --native` | AOT-compiled single executable with an embedded host shim. Must keep enforcing wave 1's grants by default; opting out has to be explicit. |
| `vibra build --oci` | The same module wrapped as an OCI artifact. Packaging, not a different compilation. |
| Foreign WebAssembly linking | Dependency-provided `.wasm`. This is a weaker, pointer-carrying boundary and stays explicitly outside the `vibra_v1` isolation claim. |
| Compile-time embedding | `(embed "path" format: @json)` and the decoders behind it. |

## Wave 5 — Concurrency and network

| Item | Shape |
| --- | --- |
| Structured concurrency | `task`, `spawn`, `join`, scopes with deadlines and cancellation. Handles are affine across a spawn boundary. |
| `net` module | `net.connect`, `net.bind`, TCP and UDP endpoints implementing the `stream` interfaces. |
| Deterministic testing | Seeded clock, seeded randomness, deterministic scheduling for reproducible concurrent tests. |

Concurrency is deferred past packages deliberately: it interacts with wave 1's
scopes and wave 3's handle lifetimes, and doing it before either means doing it
twice.

## Wave 6 — Macros and the decoding service

| Item | Shape |
| --- | --- |
| Hygienic macros | Typed, syntax-category-tagged expansion with compiler-resolved binder identities, so expansion-introduced names do not trip `E-SCOPE-001`. |
| Type-constrained decoding service | Given a source prefix and a cursor, which continuations are well-typed? A prefix automaton whose states carry a typing context, driving type-inhabitation search over a type graph. |

The decoding service is the most valuable and most novel item on the whole
plan, and it is scheduled last on purpose. It is a service *about* a type
system; building it against one that is still moving is exactly what produced
the last cycle's permanent bridge layers.

It also runs in the other direction: **prefer type-system features whose
decoding automaton stays small** is a standing constraint on every earlier wave,
so that when this is built the language is already shaped for it.

A first step is a design document, not an implementation: which subset of the
type system the automaton covers, what it falls back to, how the type graph is
built and cached, and a measurement plan with a stated baseline using a
published error taxonomy so the numbers are comparable.

## Research track — not scheduled

Open questions that need a design before they need a wave.

- **Algebraic effects with handlers and resumptions.** v1's effects are
  nominal, static, and not handled. Handlers raise real questions about
  laundering host authority and about duplicating or discarding resumptions,
  and none of them should be answered under schedule pressure.
- **Effect polymorphism.** A generic combinator currently declares a fixed
  ceiling, forcing every caller to the maximum. Effect sets as type-level
  values would fix it. This is the most likely first item to graduate from this
  track, because the pain is real and measurable with `vibra effects`.
- **Extension implementations.** Implementing an interface for a type you do
  not own, without reintroducing an orphan rule or losing structural coherence.
- **Contracts and refinement types.** No evidence yet that they help
  model-authored code; revisit if evidence appears.

## Rejected, with reasons

These are not deferred. Building one requires arguing against a design rule in
the [Charter](01-charter.md).

| Proposal | Why not |
| --- | --- |
| A sigil or PascalCase marker on type and effect names | Type and value positions are already structurally disjoint, so the marker adds no information the reader lacks. It costs a second casing rule and a whole-corpus break. |
| Type aliases | Two names for one type is the ambiguity canonical form exists to remove. `newtype` covers the legitimate uses with an explicit boundary. |
| `?`-suffix propagation, or any operator spelling | The reader has no operator characters, and adding one for a single form is a special case that every future form would want too. `(try ...)` is a form like every other form. |
| Match guards | They make exhaustiveness undecidable in general and enlarge the decoding automaton. Restructure into nested matches or an explicit `if`. |
| Implicit numeric widening | The most common source of silent behavior change in systems languages, and it hides a decision an author should state. |
| A second surface syntax, or a YAML compatibility mode | The last cycle spent a full migration removing one. Migration tools are better than permanent alternate syntaxes. |
| Verified compilation | Measured at tens of lines of proof per line of implementation for a layout-changing pass, and comparable projects took team-years to compile tiny programs. The cheap design constraints that keep the option open — scalars-only ABI, hardening last — are adopted instead. |
| Claiming the compiler prevents attacks | Semantic preservation and attack prevention are separate claims. Vibra makes the first, about behavior its own tests cover, and never the second. |
