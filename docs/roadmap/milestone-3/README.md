# Milestone 3 step plan

Status: Steps 1–2 landed
Milestone: [Milestone 3 — complete nominal static core](../v1.md#milestone-3--complete-nominal-static-core)
Execution model: [execution.md](../execution.md)
Integration branch: `m3`

M3 turns the M2 pure core into a language for reusable, strongly typed pure
libraries: nominal data, collections, patterns, typed failure, unions,
generics, interfaces, conversion, and iteration. The roadmap delivers it in two
ordered stages on `m3`. This planning change is the branch bootstrap, not an
implemented step.

## Start here

`m3` was created from `origin/main` at
`f25bc43e3af142b9a0391fb2748baae7747d2c7d`. That commit contains the M2
integration merge ([PR #295](https://github.com/nahharris/vibra/pull/295),
`a8d53e4`) and the M3–M7 roadmap refinement
([PR #298](https://github.com/nahharris/vibra/pull/298), `e245736`). M2's exit
evidence is in [its Step 14 report](../milestone-2/14-exit-evidence.md).

Baseline at `f25bc43`: the independent corpus reports 73 reader, 104 static,
24 interpreter, and 4 tooling cases — 205 passed, 0 failed, 0 unavailable.

1. Read `AGENTS.md`, [the charter](../../spec/00-charter.md), the M3 section of
   [`v1.md`](../v1.md), this plan, and the chosen step's guide. Read the exact
   specification sections the guide names before editing code.
2. Fetch `origin/m3` and branch from its current head. Do not start from
   `main`, `m2`, or a previous step's unmerged branch.
3. Implement the earliest unfinished step whose predecessors have merged.
   Every step of Stage 3A lands before the first step of Stage 3B.
4. Reuse the [M2 validation commands](../milestone-2/validation.md#before-merging-each-step)
   against `origin/m3`, and the [M2 handoff template](../milestone-2/validation.md#step-handoff),
   until Step 1 lands [M3 validation](validation.md).
5. The completing PR sets its row to `landed`, conditional on the merge. Record
   the PR and verified merge commit.

Keep one standing draft PR from `m3` to `main`. It stays draft until Step 16
evidences every exit-gate clause.

## Fixed implementation decisions

- **Extend the M2 crates; add no parallel pipeline.** Nominal, generic, and
  interface semantics extend `vibra-resolve`, `vibra-types`, `vibra-ir`, and
  `vibra-interp`. A new crate is added only when a real slice needs a new
  dependency node, and the architecture boundary test changes in the same PR.
- **One unifier.** A single bound-agnostic unification routine in `vibra-types`
  decides generic inference, union-member overlap, implementation-target
  overlap, conversion-source overlap, and destination dispatch. No rule gets a
  private variant.
- **One exhaustiveness engine.** `match` exhaustiveness, unreachable arms, and
  binding irrefutability for `let`, parameters, and lambdas share one engine.
- **Deterministic collections.** Interpreter maps are ordered by the canonical
  key order, never by host hash iteration, so the exit clause on map/hash
  independence holds by construction and is also tested.
- **Standard library through the trust input.** The standard library still
  arrives embedded in the toolchain until M5; Step 1 retires the M2 signature
  and replaces the M2 artifact (ledger D5.1). No ambient prelude, no unsigned
  fallback, and no compiler-private escape hatch in the demo.
- **Availability shrinks monotonically.** Each step moves forms from
  `@tool.unavailable` to supported with positive and negative cases. It never
  reclassifies a valid form as malformed. A form still unavailable at Step 16
  is either reassigned to a named later milestone or the gate fails.

These are implementation constraints, not language rules. Observable decisions
are closed in the owning specification by Step 1 (Stage 3A) or Step 10
(Stage 3B) before dependent code.

## Contract gaps found while planning

The following were found while reviewing the specification against the M3
deliverables. None may be settled in implementation or tests. Step 1 closes
the Stage 3A items; Step 10 closes the Stage 3B items.

| ID | Gap | Why it blocks | Owner |
| --- | --- | --- | --- |
| G1 | The roadmap places all `where:` generics in Stage 3B, but `option` and `result` are generic `deftype`s (`where: (t any)`), every collection lookup returns `(option t)`, and the collection library is generic. | Stage 3A cannot deliver lookups, `try`, or the collection library without parametric generics. Recommendation: move `any`-bounded parametric generics (`where: (t any)`, applied types, inference, and `types:`) into Stage 3A, and keep interface-bounded generics in 3B. That changes `v1.md`'s stage lists and must be reviewed in Step 1. | Step 1 — closed by ledger D1.1 |
| G2 | Map keys must implement `hashable`, `equatable`, and `ordered`, but those interfaces are never declared, and no rule gives primitive types their conformance. The canonical map key order (runtime, `iter`) depends on `ordered`. | Maps are Stage 3A; interfaces are 3B. Recommendation: declare the three contracts and a closed builtin-conformance registry for primitive keys (like the `iter` registry) in Step 1, and admit user-type keys only with Stage 3B `impl` blocks. | Steps 1, 10 |
| G3 | The "core/text/bytes/collection/option/result" library and the checked integer operations have no closed symbol list, signatures, or error types. The source chapter's example returns `(result i32 overflow)`, but no `overflow` type exists. | Equality, comparison, and arithmetic exist only as library functions, so even the Stage 3A demo cannot compare characters without them. | Step 1 |
| G4 | Extending the library changes `stdlib/m2/bootstrap.vibon`, whose digest, signature, and version `vibra-stdlib@0.1.0` are fixed contract values. Only the public key is in the repository; no signing procedure is documented. | Every library step needs a re-signed artifact. Step 1 decides the artifact path and version for M3, the review rule for re-signing, and who holds the private key. | Step 1 — closed by ledger D5.1: signature retired, artifact replaced |
| G5 | The diagnostic registry has no code for a non-exhaustive `match`, an unreachable arm, an unhandled fallible value, a `try` container/error mismatch, general ambiguous inference, infinite-size types, missing/duplicate constructor fields, odd `map.of` arity, an empty collection with no expected type, an invalid map key, or extra/duplicate `impl` members. | The exit gate requires stable atom diagnostics for several of these. | Steps 1, 10 |
| G6 | Canonical value observations (typed/execution snapshots, assertion `expected`/`actual`) are defined only for primitives; the assertion registry is monomorphic. | Every Stage 3A execution case observes records, enums, and collections. | Step 1 |
| G7 | The `iter` default-member table is malformed (`(value self f (fn (item) item) (iter item))` merges parameters and result), and `map` cannot change the element type. | The roadmap requires this review before Stage 3B implementation. | Step 10 |
| G8 | Editorial defects in `02-type-system.md`: two joined lines in **Model**, and the **Generics** example bound `storable`, which is not declared anywhere. | The chapter is the exit gate's coverage reference. | Step 1 |
| G9 | The resolved symbol/reference/index record schema, including the type-keyed `impl` block and member spelling, is not defined. The schema should let an external retrieval consumer read, per declaration, its canonical identity and module, signature, effect row, error types, outgoing application edges, and formatter-normalized source, with byte-identical output for an identical snapshot. The toolchain emits records only; embedding and ranking stay outside it. | Needed before Step 15 can emit it. | Step 10 |
| G10 | A generic function over `(map k v)` needs `k` to conform to `equatable`, `ordered`, and `hashable`, but v1 allows one bound per generic parameter, and with deterministic ordered maps `hashable` may be unnecessary. | Stage 3B map operations over a generic key cannot be declared. Candidate resolutions: require only `ordered` of map keys, or declare one standard key interface. | Step 10 |
| G11 | `@std.builtin` declares static methods of builtin types (`i32`, `array`, `map`, `tuple`), but no source form can attach a member to a builtin type: every builtin name is a reserved `deftype` spelling. | Step 4 cannot write `array.of` or the array operations in Vibra source, and Step 8 cannot declare the numeric methods. Needs a trusted-module-only declaration form, or a rule that the toolchain declares these members as compiler data. | Step 1 — closed by ledger D12.5: `(deftype i32 (intrinsic-type @i32) …)` |
| G12 | M2 call-flow analysis requires every reachable indirect call target to be statically bounded. A function stored in a record field, and later in an array, map, or option, has unbounded targets once projected, so calling it makes the whole module `@tool.unavailable`. | The `iter` adapter types (Step 14) store and call `(fn (item) item)`, and ordinary higher-order data needs it earlier. The sound fix treats an unknown target as every escaping function plus every closure. | Step 4 — closed: an unknown call target stands for every escaping function and closure |
| G13 | The type chapter lets a `deftype` body be any type expression, but only the structural forms, `newtype`, and `intrinsic-type` have a constructor. `(deftype x i32)` or `(deftype x (option i32))` would be an unconstructable type, and giving it a constructor would duplicate `newtype`. | Step 2 reports such a body `@tool.unavailable`. Recommendation: restrict `deftype-body` to the structural forms, `newtype`, and `intrinsic-type`, and reject any other body as malformed. | Closed by ledger D13.1: `newtype` removed; any non-structural body is a wrapper type |
| G14 | The spec gives `types:` a length rule but no diagnostic for an applied type expression whose argument count differs from its head's parameter list, such as `(box i32 str)` for a one-parameter `box` or a bare generic `box`. | Step 3 must report it with some code. | Step 3 — closed by ledger D14.1: `@type.type-argument-mismatch` |
| G15 | The source grammar and reader admitted no `where:` on `lambda`, although a lambda is the one anonymous function form and the Step 3 scope names it. | A local generic helper could not be written. | Step 3 — closed by ledger D15.1: `lambda` takes `where:`; a `let`-bound generic lambda stays generic |
| G16 | The type chapter gives `array.of` and `map.of` a Vibra body over their variadic tail, but the registry lists only the array operations, and a builtin member reached without an import needs one declaration shape both check paths can bind without checking a standard-library body. | Step 4 must bind every `@std.builtin` member the same way. | Step 4 — closed by ledger D16.1: `array.of` and `map.of` are registry operations |
| G17 | The type model makes `bool`, `str`, `bytes`, `map`, `option`, and `result` compiler-owned, although the language can express every one of them over a smaller core. Each compiler-owned type is behavior that lives outside the standard library and outside Vibra. | Library steps 7, 8, and 11 would harden that split. | Step 4a — closed by ledger D17.1: the core is the scalars, `(array t)`, `fn`, and the structural constructors |
| G18 | The compiler recognizes `option` and `result` by a hardcoded canonical identity, and syntax such as `if`, string literals, and `try` needs types it cannot define itself. | Moving a type into the library must not require compiler knowledge of its definition. | Step 4a — closed by ledger D17.2: a closed table of language roles claimed with `role:` |
| G19 | Every library operation is either Vibra (slow in the interpreter) or a bodiless `@compiler` intrinsic (compiler-owned behavior). | Library-first needs speed without moving meaning into the compiler, and without breaking interpreter/Wasm parity. | Step 4a — closed by ledger D17.3: `native:` implementations that keep their Vibra body |
| G20 | An import binds only a module, so a type named like its module is written `option.option`, and the only alternative anyone reaches for is a prelude. | Explicit imports read badly for single-type modules. | Step 4a — closed by ledger D17.4: single-declaration imports |
| G21 | An entry may return `(result void e)`, and returning `err` "produces a structured nonzero program result", but the command result set has no atom for a program that returned an error, so `vibra run` reports `@command.ok` with the `err` value as its `programResult`. | Scripts cannot tell a failed program from a successful one by exit status. | Open — needs a tooling-chapter decision (a result atom and exit code); Step 7 checks and runs such entries |

## Steps

Stage 3A — nominal data and typed failure. Stage demo: a pure module parses a
small line-oriented format into a nominal record/enum model and reports
failures through a nominal error union with `try`, with no interfaces.

| Step | One-PR slice | Requires | Status | PR / merge evidence |
| --- | --- | --- | --- | --- |
| 1 | [Freeze Stage 3A contracts](01-contracts.md) — specification/infrastructure prerequisite | M2 on `main`; this bootstrap | landed | PR #307, merge `23d5067` |
| 2 | [Structural and nominal data: reader and formatter support for anonymous `tuple`/`record`/`enum`/`union` types, `tupleof`/`recordof`/`enumof`, and `intrinsic-type`; `deftype` record/enum/wrapper bodies, type-name resolution, flat member namespace, finite-size check, anonymous-body rejection, constructors, record projection, nested non-interface methods](02-nominal.md) | 1 | landed | PR #308, merge `bd15e36` |
| 3 | [Parametric generics (scope per G1): `where:` with `any`, applied types, invariant inference, complete `types:` lists including inherited names, reserved `types` label, the shared unifier](03-generics.md) | 2 | landed | PR #310, merge `cbafc12` |
| 4 | [Collections: anonymous and declared tuples with `tupleof`, `array`/`map` types with `array.of`/`map.of`, tuple projection, bounds/presence lookups returning `option`, variadic array/map declarations and operands, admissible map keys (per G2), canonical map order](04-collections.md) | 3 | landed | PR #311, merge `15f394d` |
| 4a | [Library architecture contracts: the library-first core, language roles, native implementations, representation latitude, single-declaration imports](04a-library-architecture.md) — specification prerequisite | 4 | landed | PR #312, merge `6d90cde` |
| 4b | [Library-first core mechanics: single-declaration imports, `role:` binding in place of hardcoded identities, the `native:` mechanism with the manifest `native` list and a body/native differential harness, and the primitive/native registry split](04b-library-core.md) | 4a | landed | PR #313, merge `32649ca` |
| 5 | [Patterns and `match`: literal, constructor, tuple, record, and array patterns; destructuring `let`, parameters, and lambdas; the shared exhaustiveness/irrefutability engine; unreachable arms](05-patterns.md) | 4b | landed | PR #314, merge `f696089` |
| 6 | [Unions, widening, and `as`: union `deftype`s, member overlap and concreteness, union and atom-singleton widening at written expected types, `as` ascription, `as` narrowing patterns](06-unions.md) | 5 | landed | PR #315, merge `c653f39` |
| 7 | [Typed failure: `result` as a standard-library `deftype` claiming `@result`, `try` propagation, unhandled-fallible-value checks, discard intent](07-failure.md) | 6 | landed | PR #316, merge `6f8bfe3` |
| 8 | [Registry: every Stage 3A `@compiler` row — the numeric static methods in `@std.builtin`, and `@std.char`, `@std.text`, and `@std.bytes` — with interpreter semantics and boundary tests](08-library.md) | 7 | landed | PR #317, merge `b0c3092` |
| 8b | [Reviewed Vibra composites (boolean connectives, `char` comparison and classes, text search/split/trim) and the generic `assert.equal` (per G6)](08-library.md) | 8 | landed | PR #318, merge `5407632` |
| 8c | [`bool`, `str`, `bytes`, `ordering`, and the error enums as standard-library `deftype`s under their roles (per D17.1); the text and bytes rows become native implementations (per G3/G4, D17.3); array folds as Vibra-bodied `@std.builtin` members](08-library.md) | 8b | landed | PR #319, merge `e5a5c08` |
| 9 | [Stage 3A demo and corpus sub-gate — evidence step](09-stage-3a-evidence.md) | 8 | landed | Step 9 PR (conditional on merge); [Stage 3A evidence](stage-3a-evidence.md) |

Stage 3B — interfaces, generics, conversion, and iteration.

| Step | One-PR slice | Requires | Status | PR / merge evidence |
| --- | --- | --- | --- | --- |
| 10 | Freeze Stage 3B contracts, including the `iter.map` review (G7) and index schema (G9) — specification prerequisite | 9 | not started | — |
| 11 | Interfaces and `impl`: `defint`, abstract and default members, nested placement and ownership, same-module rejection, completeness per block identity, target overlap, default-override rejection, static receiver dispatch, interface-bounded generics; `map` moved into the standard library over sorted arrays under `@map`, with the standard key conformances as ordinary implementations (per D17.1) | 10 | not started | — |
| 12 | Interface values: `any` and interfaces in type position, widening to an interface, dispatch through interface values, unions implementing interfaces | 11 | not started | — |
| 13 | Destination dispatch and conversion: factory members, `from`/`try-from`, `conversion-error`, redundant-conversion and ambiguous-destination checks | 12 | not started | — |
| 14 | Iteration: the `iter` contract and default methods, closed builtin conformance, adapter types, written algebraic laws for `equatable`/`ordered`/`hashable`/`iter` with conformance examples | 13 | not started | — |
| 15 | Resolved symbol/reference/index records and type-aware query metadata with canonical identities | 14 | not started | — |
| 16 | M3 demo and exit gate, including the M2 deferral sweep — evidence step | 15 | not started | — |

Steps 1, 4a, and 10 are specification prerequisites and Steps 9 and 16 are
evidence steps; they claim no language behavior. Guides for Steps 2–9 are
written in Step 1, the guide for Step 4b in Step 4a, and guides for Steps 11–16
in Step 10, because their content depends on the contracts those steps close.
Step 1 also records the [decision ledger](decision-ledger.md) and the
[M3 surface inventory](supported-surface.md). No step starts without its guide.

## Deliverable and gate coverage

| Roadmap obligation | Owning steps |
| --- | --- |
| Declared and anonymous records, enums, unions, tuples; wrapper types, arrays, flat maps, atoms | 2, 4, 6 |
| Declared and anonymous constructors, projection, lookups, `array.of`/`map.of`, variadic operands | 2, 4 |
| `where:` generics, inherited names, `types:`, closed `impl` targets | 3, 11 |
| `any`, interface values, default methods, `fn` types, `lambda`, nested `impl`, static dispatch, ownership/conflict rules | 11–13 |
| Unqualified nested names, path-based member references, flat member namespace, same-module `impl` rejection, type-keyed `impl` identity | 2, 11, 15 |
| Exhaustive `match`, `option`, `result`, `try`, unhandled failure | 4, 5, 7 |
| Declared and anonymous unions, non-unifiable members, no lifting or flattening | 6 |
| Structural type expressions, wrapper types, `intrinsic-type`, reserved `type-name` heads | 2 |
| Bound-agnostic unification behind every overlap rule | 3, 6, 11, 13 |
| Three widening relations, `as` ascription, `as` narrowing | 6, 12 |
| Destination-dispatched members, `from`/`try-from`, `conversion-error`, several applied targets per receiver | 13 |
| Destructuring patterns with shared irrefutability, no `(bind ...)` | 5 |
| `iter`, closed builtin conformance, adapter types | 14 |
| Function types with closed, initially empty effect rows | 3, 11 |
| Full resolved symbol/reference/index records | 15 |
| Core library including checked integer operations | 8, 13, 14 |
| Written algebraic laws for `equatable`, `ordered`, `hashable`, `iter` | 10, 14 |
| Demo gate: generic collection library with a nominal error type, an `iter` implementation, and an interface default method | 16 |
| Exit: type chapter has focused positive and negative cases | every behavior step; swept by 9 and 16 |
| Exit: stable diagnostics for ambiguous inference, shadowing, non-exhaustive matches, invalid `impl` placement, ignored fallible values | 1, 3, 5, 7, 10, 11 |
| Exit: interpreter independent of map/hash iteration | 4, 14, 16 |
| Exit: index and context output use canonical resolved identities | 15, 16 |
| Exit: every M2 deferral to M3 implemented or reassigned; no M3 form reports `@tool.unavailable` | 1 (inventory), every behavior step, 16 |

## M2 deferral inventory

The M2 [supported-surface inventory](../milestone-2/supported-surface.md),
[decision ledger](../milestone-2/decision-ledger.md) rows C1.3, C1.5, C1.6,
C1.7, C5.2, and C6.2, and the [availability audit](../milestone-2/availability-audit.md)
list the forms deferred to M3. Step 1 turns them into one M3 inventory that
gives each AST variant and ledger row an owning step, and adds a test that
fails when a variant has no disposition, as M2's inventory test does. The
`deffect`, nonempty-effect, and `@host` rows remain M4-owned.
