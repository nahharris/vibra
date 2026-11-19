# v0.0.2 M0: native contract

This record is the "Native contract" item of [M0](README.md). It compares
the code at and below checked IR with the active specification: the checked
IR, the reference interpreter, the host world and natives, and the native
pipeline. Each item has one status:

- **kept**: the code already does what the specification requires;
- **changed**: the construct stays, and the concrete change is named;
- **removed**: the construct has no place in the specification and leaves
  with its tests;
- **new**: the specification requires something the code does not have.

Code is cited as `path:line` at commit `c6521c76`. A path that starts with
the file name only (`lib.rs:92`) belongs to the crate the row names. A
specification claim names its chapter and heading: "06 Host calls" is the
heading "Host calls" of the runtime chapter. The chapters are
[02](../../../spec/02-type-system.md), [03](../../../spec/03-effects.md),
[04](../../../spec/04-programs-and-packages.md),
[05](../../../spec/05-tooling.md), [06](../../../spec/06-runtime.md), and
[07](../../../spec/07-diagnostics-and-conformance.md). The Nexus design is
[design.md](../../nexus/design.md), and its v0.0.2 packages are in
[plan-v0.0.2.md](../../nexus/plan-v0.0.2.md).

Where the specification is silent or contradicts itself, the entry points
to a gap of [section 6](#6-specification-gaps). The specification is
corrected before a package relies on the answer.

## Crate status

| Crate | Status | In one line |
| --- | --- | --- |
| `vibra-ir` | changed | Unions, variadics, trapping division, and module-path effect roots leave; `Array` becomes `List`; callables split into `Fn`, `Proc`, and `Intent`; guards, plans, grants, interface labels, and the v2 encodings are new |
| `vibra-interp` | changed | Executes guards, plans, and grants; runs startup and returns an `ExitCode`; loses the two removed traps and the `err`-variant classification |
| `vibra-host` | changed | Registry-major-2 operations with a grant first, camelCase error cases plus four broker cases, `@hostWorld.v2`, `@auditTrace.v2`; a broker, policies, narrowing, startup, `TestWorld`, and a held-directory filesystem adapter are new |
| `vibra-native` | kept | The shared algorithms stay; `array` names become `list`, and the `array.of` and `dict.of` rows go |
| `vibra-lower` | changed | The slice survives; removed forms and the division trap leave; guards, plans, `await`, and grant operands are new forms |
| `vibra-exec` | changed | Structure and verifier stay; `ObjectKind::Array` becomes `List`, `Union` leaves, only the two defect traps remain, and the plan kind and grant operand are new |
| `vibra-nexus` | changed | Context, frames, steps, and store stay; `Context::host` gains the dispatch order of 06 Host calls, admission gains the entry and binding checks, and the ABI version moves |
| `vibra-nexus-eval` | kept | Changes only when a new instruction arrives (X2-02) |
| `vibra-codegen` | changed | The `entry_outcome` input is replaced by the entry contract; otherwise kept |
| `vibra-nexus-bridge` | changed | Build kinds, descriptor entry field, launcher exit mapping, startup, and the harness observation change |
| `vibra-build` | changed | `entry_outcome` leaves; the build carries the binding recipes |
| `vibra-actors` | kept | Untouched in v0.0.2; it depends on no other crate |

## 1. Checked IR

`vibra-ir` is the contract between the checker and every executor. The
table has one row per construct family. Paths are under `crates/vibra-ir/src/`.

| Family | Code | Status | Concrete change | Specification |
| --- | --- | --- | --- | --- |
| Functions | `CheckedFunction` `lib.rs:2779`; `new_external` `lib.rs:2829`; `new_host` `lib.rs:2878` | changed | Add the callable kind (`fn` or `proc`), the optional `requires … else` and `ensures … else` guards, and the result binding the `ensures` reads (section 1.1). External and native bodies stay | 02 Guards; 03 Operation interfaces |
| Effect owner of a function | `with_effect_owner` `lib.rs:3163` | changed | The owner is an operation interface's declaration identity, not a module path | 03 Operation interfaces |
| Module values | `CheckedGlobal` `lib.rs:2705` | kept | Initializers stay pure and evaluate on first read | 03 Effect ceilings; 06 Module-value initialization |
| Test assertions | `TestAssertion` `lib.rs:763` | kept | Assertions stay ordinary typed calls with no registry symbol | 06 Test assertions |
| Declared types | `TypeDefinition` `nominal.rs:81`; `TypeBody` `nominal.rs:47` | changed | A variant case has named fields or none, so the single payload slot of `Enum` (`nominal.rs:51`) becomes a field list. `Union` (`nominal.rs:57`) is removed. New: the invariant (`where P`, optionally `else E` with a constant `E`), the representation visibility, and the role table of the standard library | 02 Declared types; 02 Type invariants; 02 Language core and standard library |
| Type identity | `TypeId` `nominal.rs:17` (numeric id and path) | changed | Carries the declaration identity of the logical anchor, which every encoding writes as `seed` (G28) | 02 Declared types; 06 Types |
| Interfaces and implementations | `Implements` `lib.rs:2809`; `ClosedContract` `lib.rs:1353` | changed | An interface with a `proc` member is an operation interface and an effect root. Associated types reach IR only inside interface value types (section 1.6). The closed contracts stay as dispatch shortcuts | 02 Interfaces; 02 Associated types; 03 Operation interfaces |
| Scalar types | `Type` `lib.rs:40` | kept | `Void`, `Never`, `Char`, `Atom`, the integers, and the floats are compiler-owned | 02 Language core and standard library |
| Role types | `Type::Bool` `lib.rs:42`, `Type::Str` `lib.rs:50`, `Type::Bytes` `lib.rs:52`, `Type::Dict` `lib.rs:94` | changed | `Bool`, `Str`, `Bytes`, `Dict`, `Option`, and `Result` are declared library types that play roles. The IR names them as `Declared` or `Applied` of the role's declaration and looks the role up; executors keep their direct string, bytes, and dict representations under 06 Representation latitude | 02 Language core and standard library; 06 Representation latitude |
| Sequence type | `Type::Array` `lib.rs:92` | changed | Renamed `List` (section 1.10) | 02 Language core and standard library |
| Structural types | `Type::Tuple`; `Type::Record`; `Type::AtomSingleton` `lib.rs:56`; `Type::Enum` `lib.rs:84` | changed | Tuple, record, and atom singleton stay. The anonymous variant type `Enum` is removed: structural types are only tuples and records | 02 Structural types |
| Callable types | `Type::Function` `lib.rs:78`; `FunctionSignature` `lib.rs:563` with `variadic` `lib.rs:568` and `effects` `lib.rs:573` | changed | Split into `Fn` (no row), `Proc` (row), and `Intent` (result and row). The variadic tail is removed. Labelled parameters stay; defaults belong to values | 02 Callable types; 06 Types |
| Interface value types | `Type::Interface` `lib.rs:99`; `Type::Any` `lib.rs:102` | changed | Gains the associated types fixed by label, in canonical label order. `Any` stays | 02 Interface values |
| Untagged unions | `Type::Union` `lib.rs:96` | removed | No union type exists | 02 Widening; 06 Types ("No variadic or union descriptor exists") |
| Bindings and values | `Literal`, `Sequence`, `Variable`, `Captured`, `Let` `lib.rs:1110`, `Global`, `Function`, `Closure` `lib.rs:1081` | kept | `let … else` stays a checker desugaring into `Match` | 06 Evaluation; 06 Tail calls |
| Calls | `Call` `lib.rs:1179`; `CallTarget` `lib.rs:1303` | changed | The tail flag is false for a call whose value an `ensures … else` still checks. Contract targets stay | 06 Tail calls |
| Plans | none | new | Applying a `proc` builds an `Intent`; `await` runs it (section 1.4) | 03 Cold plans and performed rows; 06 Cold plans and invocation identity |
| Constructions | `Record` `lib.rs:1202`; `Variant` `lib.rs:1212`; `Wrap` `lib.rs:1223`; `Tuple` `lib.rs:1243` | changed | Record fields are evaluated in written order: the code uses declaration order for a declared constructor (`lib.rs:1205`). A variant takes named fields. A construction of a type with `where … else` has result `Result(T E)` (section 1.1) | 02 Type invariants; 06 Callable contract execution |
| Sequence and dict literals | `Array` `lib.rs:1263`; `Dict` `lib.rs:1273` | changed | `Array` becomes `List`. The dict literal stays; it depends on the `@dict` role | 06 Character, sequence and dictionary operations |
| Projections | `Project` `lib.rs:1232`; `Lookup` `lib.rs:1286`; `TupleProject` `lib.rs:1252` | changed | Field projection and lookup stay. Tuple index projection is removed | design §2 "From checked IR" (N53) |
| Control | `If`; `Match` `lib.rs:1122`; `Return` `lib.rs:1161`; `Try` `lib.rs:1149` | changed | A `try` propagation is a candidate of the `ensures … else` check like any return | 06 Callable contract execution |
| Widening | `Widen` `lib.rs:1135` | changed | Only to an interface value type and from an atom singleton to `Atom`; the union member discriminant is removed | 02 Widening |
| Defaults | `Default` `lib.rs:1040` | kept | Defaults of labelled parameters | 02 Callable types |
| Patterns | `Pattern` `pattern.rs:12`; `Variant` `pattern.rs:26`; `Array` `pattern.rs:39`; `Member` `pattern.rs:41`; `MatchArm` `pattern.rs:148` | changed | Wildcard, binding, literal, record, tuple, and wrapper patterns stay. A variant pattern takes named fields; `Array` becomes `List`; `Member` (the union `as` pattern) is removed | 02 Declared types; design §2 "From checked IR" |
| Effect rows | `EffectRoot` `effect.rs:18`; `RowTail` `effect.rs:64`; `EffectRow` `effect.rs:68`; encoding `effect.rs:148` | changed | A root is an operation interface identity. `RowTail` is removed. The row is written as the brace record of 06 Types (section 1.9, G29) | 03 Effect ceilings; 06 Types |
| Compiler intrinsics | `External` `lib.rs:1015`; `CompilerIntrinsic` `external.rs:314`; `IntegerOp` `external.rs:146`; `REGISTRY_VERSION` `external.rs:28` | changed | Remove `IntegerOp::Div` and `Rem` (`external.rs:171`, `external.rs:173`), `ArrayOf` and `DictOf` (`external.rs:359`, `external.rs:361`), and the variadic signatures (`external.rs:672`). Rename the `array.*` rows. Equality, order, formatting, and parsing stay only as implementations behind `Equatable`, `Ordered`, `From`, and `TryFrom`. New: the dict iteration primitive (G30) and the narrowing rows (G17). The registry version changes (G4) | 06 Compiler intrinsic registry |
| Host calls | `Host` `lib.rs:1029` | changed | The first argument is the grant, and the row is a registry-major-2 row | 06 Host registry |
| Host registry | `HOST_ROWS` `external/host.rs:229`; `HostRow` `external/host.rs:154` | changed | Section 3 lists the row changes | 06 Host registry; 06 Registry versioning |
| Native rows | `NATIVE_ROWS` `external/native.rs:252` | changed | The `ArrayOf` row (`external/native.rs:323`) leaves; `ObjectKind::Array` (`external/native.rs:112`) becomes `List` | 06 Native implementations |
| Arena kinds | `arena.rs:68` to `arena.rs:90` | changed | New kinds for `Proc`, `Intent`, and sealed authority; `ARRAY` (`arena.rs:78`) becomes `LIST`; `UNION` (`arena.rs:88`) leaves (G32) | 06 Value arena |
| Trap codes | `TrapCode` `boundary.rs:154` | changed | Checked programs have two traps (section 1.3) | 06 Traps |
| Observed values | `ObservedValue` `observed.rs:16`; `Array` `observed.rs:51`; `Union` `observed.rs:55`; `is_err_variant` `observed.rs:77`; `canonical_observation` `observed.rs:182` | changed | `Array` becomes `List`; `Union` and `is_err_variant` are removed; the encoding becomes `@value.v2` | 06 Canonical value encoding |
| Type encoding | `canonical_type` `lib.rs:5921`; `canonical_function_signature` `lib.rs:5987`; `CheckedModuleSet::canonical_vibon` `lib.rs:3262` | changed | S-expression records become the brace records of 06 Types under `@type.v2`. A declared type gains `seed` and `arguments`; the anonymous variant (`lib.rs:5950`) and union (`lib.rs:5954`) records go; the atom singleton tag becomes `@atomSingleton` (`lib.rs:5961`); a signature loses its variadic tail and, for `Fn`, its row | 06 Types |
| Program | `CheckedProgram` `lib.rs:3426`; `for_entry` `lib.rs:3470`; `IrError` `lib.rs:3548` | changed | The entry is a `proc` with grant-shaped parameters and an `ExitCode` result (section 1.7); the binding recipes travel beside the program (section 1.8) | 04 Targets |

### 1.1 Guards with consequences

**Specification.** 02 Guards with `else`; 06 Callable contract execution;
design "Guards" (N44).

**Code.** None. `CheckedFunction` (`crates/vibra-ir/src/lib.rs:2779`) has
no guard field, and `crates/vibra-ir/src/` has no `requires`, `ensures`, or
invariant anywhere.

**New.**

- `CheckedFunction` gains `requires: Option<Guard>` and
  `ensures: Option<Guard>`. A `Guard` holds the predicate and the
  consequence, both certified expressions. Only a guard with `else`
  reaches IR; a bare guard does not (section 1.2).
- The `ensures` guard names the slot the candidate is bound to, the
  declared result name, so the consequence can read it.
- On entry `C` is evaluated once. False: `V` is evaluated once and
  returned, and the body and the `ensures` are both skipped.
- Every return of the body, including one propagated by `try`, is a
  candidate. `C` is evaluated once on it; false evaluates `V` with the
  candidate still bound, and `V` is not checked again.
- A type definition with `where P else E` records the predicate (a
  certified `Fn({R} Bool)`) and the constant `E`. A construction of such a
  type, by `Record`, `Tuple`, or `Wrap`, has the result type `Result(T E)`:
  the raw representation is assembled in written order, `P` is applied,
  and the result is `Result.ok(value: …)` or `Result.err(error: E)`. A bare
  `where` leaves the construction's type unchanged and adds nothing.
- A call whose result an `ensures … else` still checks is not in tail
  position (06 Tail calls); the checker sets `tail: false` on that `Call`
  (`crates/vibra-ir/src/lib.rs:1179`).
- The checker, not an executor, removes a guard it proves. The result type
  stays.

**Gaps.** Guards on contract members and their implementations (G22); a
guard on the entry (G23); `await` in tail position (G21).

### 1.2 Bare guards

**Specification.** 02 Bare guards; 06 Callable contract execution ("Bare
guards leave no code").

**Code.** None, which is what the specification requires.

**Required.** No IR node or field represents a bare `requires`, `ensures`,
or `where`. Its absence is the contract: no executor can check what is not
there, and an unproved obligation is a diagnostic, never a run-time check.
The IR test is the one design §2 states for lowering, one level earlier:
the checked IR of a function with a discharged bare `requires` equals that
of the same function without it, origins aside.

Fully specified, except for a bare `requires` on the entry (G23).

### 1.3 The two defect traps

**Specification.** 06 Traps; 06 Integer operations; 06 What has no encoding;
06 Test assertions.

**Code.** `TrapCode` (`crates/vibra-ir/src/boundary.rs:154`) has four codes,
including `UnobservableFunction` (`boundary.rs:158`) and `DivisionByZero`
(`boundary.rs:163`), mapped to `RuntimeUnobservableFunction` and
`RuntimeDivisionByZero` (`boundary.rs:195`, `boundary.rs:197`), which the
diagnostics registry defines (`crates/vibra-diagnostics/src/registry.rs:422`,
`crates/vibra-diagnostics/src/registry.rs:424`). The interpreter raises
both (`crates/vibra-interp/src/lib.rs:277`,
`crates/vibra-interp/src/lib.rs:282`,
`crates/vibra-interp/src/machine.rs:1291`), and the lowering emits the
division trap (`crates/vibra-lower/src/lib.rs:1111`).

**Changed.** A checked program has exactly two traps,
`@runtime.invalidCheckedProgram` and `@runtime.invalidHostValue`.

- `divChecked` and `remChecked` return `Result(T ArithmeticError)`; there
  is no plain division.
- A result that contains a function is rejected statically where a value
  must be observed. One hidden behind an interface value is
  `@tool.unsupportedValue`, a tool outcome.
- The native trap type has the two codes.
- The two removed diagnostic codes leave the registry, which 07 owns.

Fully specified.

### 1.4 Plans and `await`

**Specification.** 03 Cold plans and performed rows; 06 Cold plans and
invocation identity; 06 Value arena; design "Plans" (N45).

**Code.** One callable type with a row on every function
(`crates/vibra-ir/src/lib.rs:78`, `lib.rs:573`); no plan, no `await`; the
interpreter's `Callable` (`crates/vibra-interp/src/value.rs:281`) has only
named and lambda functions.

**New.**

- The types `Proc({A} R {Row})` and `Intent(R {Row})`. `Fn` has no row.
- A function's kind, `fn` or `proc`. A `proc` body is entered only by an
  `await`.
- An application node for a `proc`: it evaluates the callee and the
  operands in written order and builds an `Intent` value that holds them,
  with the type arguments. It enters no body, runs no guard with `else`,
  dispatches nothing, and consumes no limit. Its target is a direct, an
  indirect (a `Proc` value), or a contract target (a `proc` member,
  including one through an interface value).
- An `await` node over an `Intent` operand, with a tail flag. Each `await`
  is a new invocation from the same plan: the procedure's `requires … else`
  and `ensures … else` run in it. The checker admits `await` only in a
  `proc` body or a test body.
- A host-bound member keeps today's shape: the standard-library wrapper
  built by `new_host` (`crates/vibra-ir/src/lib.rs:2878`) has a `Host` body,
  and awaiting its plan runs that body.
- At run time `Fn`, `Proc`, and `Intent` are three kinds. A plan has no
  consumed bit and no memoized result.

**Gaps.** `await` in tail position (G21); invocation ordinals in the trace
(G9); row identity and order (G29).

### 1.5 Grant values and sealed interfaces

**Specification.** 03 Capability values; 03 Narrowing; 03 Brands; 06 Value
arena; 06 Host registry; 06 Host calls; 06 What has no encoding.

**Code.** None. A host row has no grant parameter
(`crates/vibra-ir/src/external/host.rs:154`), and a provider method takes a
string path (`crates/vibra-host/src/provider.rs:129`).

**New.**

- The type of a grant is the operation interface it permits, so a grant
  parameter has an interface value type such as `DirReadText`. Its run-time
  value is a sealed authority: a target kind (`HostResource` here), the
  provider and world, the scope and rights, the accounting lineage, and the
  lifetime and generation. Source cannot build one.
- The first operand of every `Host` node is that grant.
- A grant has no encoding, no equality, and no dict key. The checker
  rejects a type statically known to contain one wherever an encoding or a
  comparison is required.
- A brand, `type LogDir of DirWriteText` with a private representation, is
  an ordinary wrapper over a grant and needs no new node.
- The narrowing members `restrict`, `sub`, and `limit` return a narrower
  grant without performing anything (G17).

**Gaps.** How a sealed authority implements its interface for dispatch
(G18); the narrowing rows (G17); policy roots (G15); limit accounting
(G16).

### 1.6 Associated types

**Specification.** 02 Associated types; 02 Interface values; 06 Generic
instantiation.

**Code.** None. `Type::Interface(TypeId, Vec<Type>)`
(`crates/vibra-ir/src/lib.rs:99`) holds positional arguments only.

**Answer (recommended, G24).** Associated types reach IR in one place and
are erased everywhere else.

- An interface value type keeps the associated types it fixes by label:
  `Iter(Item: U8)` and `Iter(Item: I32)` are different types, so
  `Type::Interface` gains a label-to-type list in canonical label order.
- A projection through a concrete type, `Once.Item`, is resolved by the
  checker to the chosen type and never reaches IR.
- A projection through a type parameter, `I.Item` under `given {I: Iter}`,
  is elaborated by the checker into an implicit type parameter of the
  generic callable, supplied at each instantiation from the
  implementation's choice. Generic activations already receive type
  descriptors at run time (06 Generic instantiation; design X3-13), so IR
  needs no projection type and no run-time lookup of a choice.
- `Implements` (`crates/vibra-ir/src/lib.rs:2809`) needs no field for the
  choices.

### 1.7 The `ExitCode` entry

**Specification.** 04 Targets; 06 Entry outcome; 05 Versioned envelope and
results (the item "Exit code of a program"); 06 Native execution ("An old
descriptor's guessed error-variant flag MUST NOT decide how `ExitCode`
behaves").

**Code.** The entry takes no parameter: the lowering refuses one
(`crates/vibra-lower/src/lib.rs:227`), and the interpreter fills only
defaults (`crates/vibra-interp/src/lib.rs:496`). Failure is guessed from a
variant named `err`: `ObservedValue::is_err_variant`
(`crates/vibra-ir/src/observed.rs:77`), the interpreter
(`crates/vibra-interp/src/lib.rs:416`), the build
(`crates/vibra-build/src/pipeline.rs:70`), the descriptor
(`crates/vibra-nexus-bridge/src/abi.rs:333`), and the launcher
(`crates/vibra-nexus-bridge/src/launcher.rs:138`).

**Changed.**

- The entry is a module-level `proc`. Every parameter is a bare name whose
  type is a grant shape: an operation interface value type, `Option` of
  one, or a record whose fields are grant shapes. The result is `ExitCode`
  from `@std.process`, a wrapper over `U8`.
- The program's outcome is that `U8`. Every `err`-variant classification
  is removed.
- `CheckedProgram::for_entry` (`crates/vibra-ir/src/lib.rs:3470`) asserts
  the entry shape that project checking established.

**Gaps.** A built executable's exit status for outcomes that are not an
application code (G7); a guard on the entry (G23).

### 1.8 Startup grant acquisition order

**Specification.** 03 Target admission; 04 Targets; 06 Entry outcome;
design §7.

**Code.** None. No recipe reaches IR or the build, and the launcher starts
the entry at once (`crates/vibra-nexus-bridge/src/launcher.rs:131`).

**New.** Project checking produces, beside the checked program, the
validated binding recipes of the target: for each leaf (`@notes`,
`@logs.reader`), its provider, resource, policy, limits, and `after`
names. The recommended carrier is a separate `Bindings` value, so that
`CheckedProgram` stays independent of the project file. The interpreter's
run and the build both take it, and the native descriptor carries the same
data (design §6: "the driver and policy requirements of the target's
bindings"). Acquisition follows a topological order of `after`.

**Gaps.** The tie order is contradictory (G1). Release (G2), provider
refusal (G5), recipes under a host world (G6), and the startup record (G8)
are underspecified.

### 1.9 Removal of the old effect roots and entry signature

**Specification.** v0.0.2 [README](../README.md) "Scope and priorities"
("Its source surface, project record, entry signature, and effect roots are
replaced; nothing accepts both"); 03 Operation interfaces; 03 Effect
ceilings.

**Code.** `EffectRoot { path, id }` (`crates/vibra-ir/src/effect.rs:18`)
names module-path roots, and one root owns several rows: `@std.fs.read`
owns three of `HOST_ROWS` (`crates/vibra-ir/src/external/host.rs:229`),
and the test at `effect.rs:229` writes a row of module paths. `RowTail`
(`effect.rs:64`) is an empty enum standing for an absent row variable.
Every function type carries a row (`crates/vibra-ir/src/lib.rs:573`).

**Removed.** Module-path roots, `RowTail`, rows on `Fn`, and the old entry
(no parameters, a `Void` or `err`-variant result). No reader of the old
`@types.v1` document remains.

**Changed.** A root is an operation interface's declaration identity, and
every host operation has its own owner interface.

Fully specified, except the identity and order of roots (G29).

### 1.10 `@array` becomes `@list`

**Specification.** 02 Language core and standard library ("the one generic
immutable sequence `List(T)`"); 06 Types ("`List` has the wire tag
`@list`"); 06 Values (`kind: @list`).

**Code, every place the name occurs.**

- vibra-ir: `Type::Array` (`crates/vibra-ir/src/lib.rs:92`), `Expr::Array`
  (`lib.rs:1263`), `Pattern::Array` (`pattern.rs:39`),
  `ObservedValue::Array` (`observed.rs:51`), the encoding (`lib.rs:5938`),
  `HostType::Array` (`external/host.rs:41`), the `array.*` intrinsics
  (`external.rs:511`, `external.rs:518`), `kind::ARRAY` (`arena.rs:78`),
  and `ObjectKind::Array` (`external/native.rs:112`).
- Below it: `ObjectKind::Array` (`crates/vibra-exec/src/lib.rs:130`),
  `KIND_ARRAY` (`crates/vibra-nexus-bridge/src/abi.rs:197`),
  `Values::build_array` (`crates/vibra-nexus/src/host.rs:109`), and
  `HostValues::build_array` (`crates/vibra-host/src/operation.rs:83`).

**Changed.** Renamed everywhere.

Fully specified.

## 2. Interpreter

`vibra-interp` is the oracle of every other executor. Paths are under
`crates/vibra-interp/src/`.

| Area | Code | Status | Concrete change |
| --- | --- | --- | --- |
| Run entry points | `run` `lib.rs:371`; `run_with_budget` `lib.rs:377`; `run_with_host` `lib.rs:388` | changed | Take the target's bindings and a provider world, run startup (section 1.8), build the entry's plan, and return the application code, the startup record, and the trace |
| Outcome | `Execution` `lib.rs:83` with `program_failed` (set at `lib.rs:416`) | changed | `program_failed` is replaced by the `ExitCode` value; a startup failure is its own outcome with `@grant.requiredUnavailable` |
| Entry frame | `entry_frame` `lib.rs:496` | changed | Fills each grant parameter from startup, building records of grants for bundle parameters and `Option.none()` for a refused optional grant |
| Tests | `run_test_with_host` `lib.rs:452` | changed | A fresh world per test, with the sealed `TestWorld` root grant when the test takes one (G19, G20) |
| Errors | `RuntimeError` `lib.rs:260`; `program_trap` `lib.rs:320` | changed | `UnobservableFunction` (`lib.rs:277`) and `DivisionByZero` (`lib.rs:282`) are removed; `invalidHostValue` is raised for a bad grant |
| Values | `RuntimeValue` `value.rs:239`; `Union` `value.rs:272` | changed | `Array` becomes `List`; `Union` is removed; a sealed authority value is new, a reference into the broker |
| Callables | `Callable` `value.rs:281` | changed | Split into `Fn`, `Proc`, and `Intent`; a plan holds its callee, operands, and type arguments |
| Continuations | `Kont` `machine.rs:109`; `exit` `machine.rs:369` | changed | New continuations for the `requires … else` branch, the `ensures … else` check over the candidate, `await`, and construction under `where … else`. `exit` routes every return, including a `try` propagation (`machine.rs:887`), through the `ensures … else` check |
| Widening | `unary` `machine.rs:844`; `Widen` `machine.rs:858` | changed | Only to interface values; the union branch is removed |
| Host calls | `host_call` `machine.rs:1249` | changed | Calls the shared dispatch boundary of 06 Host calls (section 3). An operand mismatch (`machine.rs:1258`) stays `invalidCheckedProgram`; a stale or foreign grant is `invalidHostValue`; `perform` failure is no longer always memory (`machine.rs:1259`); the event is still appended after the result (`machine.rs:1260`) |
| Intrinsics | `external` `machine.rs:1266` | changed | The zero-divisor trap (`machine.rs:1291`) is removed; checked operations build their `Result` |
| Folding | `Kont::Fold` `machine.rs:135` | kept | Stays the accelerator of a left fold, held to the Vibra body of `Iter.fold` |
| Calls and tail calls | `call` `machine.rs:1388`; `transfer` `machine.rs:1607`; `enter_callee` `machine.rs:1694` | kept | The tail rule follows the checker's flag; a call before an `ensures … else` check is not a tail call |
| Contract dispatch | `contract_call` `machine.rs:1456`; `dispatch` `machine.rs:1583` | changed | A sealed authority receiver dispatches to its host-bound member (G18) |
| Module values | `GlobalState` `machine.rs:66` | kept | Unread, evaluating, ready |
| Assertions | `invoke_test_assertion` `machine.rs:1734` | kept | Observation of a hidden function is `@tool.unsupportedValue` |
| Memory | `MemoryBudget` `lib.rs:52` | kept | Exhaustion stays the host event `@runtime.memoryExhausted` |
| Observation | `is_err_variant` use at `lib.rs:416` | changed | Results are encoded as `@value.v2` (section 1, "Observed values") |

## 3. Host world and natives

### vibra-host

Paths are under `crates/vibra-host/src/` unless they name another crate.

| Area | Code | Status | Concrete change | Specification |
| --- | --- | --- | --- | --- |
| Operation set | `HostCall` `operation.rs:133`; `HostCall::read` `operation.rs:172`; kebab symbols `operation.rs:214`, `operation.rs:227` | changed | The 17 operations of registry major 2, with the grant first and a `RelativePath` (a list of names) in place of a path string. The wire symbols are spelled as G3 settles | 06 Host registry |
| Row table | `HOST_REGISTRY_MINOR` `crates/vibra-ir/src/external/host.rs:20`; `HostType` `crates/vibra-ir/src/external/host.rs:25`; `HostErrorType` `crates/vibra-ir/src/external/host.rs:78`; `HostRow` `crates/vibra-ir/src/external/host.rs:154`; `HOST_ROWS` `crates/vibra-ir/src/external/host.rs:229`; `registry_snapshot` `crates/vibra-ir/src/external/host.rs:390` | changed | One owner interface per row, a delivery class (`@once` for all 17), `since` 0, a grant parameter, `RelativePath`, `List(Str)` results, and the snapshot heading of major 2.0 in place of the major-1 heading (`crates/vibra-ir/src/external/host.rs:392`) | 06 Host registry; 06 Registry versioning |
| Value access | `HostValues` `operation.rs:59`; `build_array` `operation.rs:83`; `read_path` | changed | `build_list`; a `RelativePath` reader; a grant reader that answers the broker reference or `invalidHostValue` | 06 Host calls |
| Error cases | `provider.rs:62` to `provider.rs:95` (`"not-found"` `provider.rs:70` and the rest); `HostErrorType` names `crates/vibra-ir/src/external/host.rs:100` | changed | camelCase case names, plus `denied`, `limitExceeded`, `unsupported`, and `outcomeUnknown` on each of the five types | 06 Error types |
| Provider | `HostProvider` `provider.rs:115`, string paths `provider.rs:129` to `provider.rs:139`; `perform` `operation.rs:347` | changed | `perform` becomes step 4 of 06 Host calls: the adapter bound to the grant's target, which reports "known not submitted", "completed", and "may have run" distinctly. A filesystem adapter receives the held directory and the `RelativePath` | 06 Host calls |
| System provider | `SystemProvider` `system.rs:21`; `file_bytes` `system.rs:183`; `fs_error_at` `system.rs:114` | changed | The file code checks metadata on an ambient path and then reopens it by name (`system.rs:186`, `system.rs:191`), and probes ancestors (`system.rs:122`): driver rules 1 to 5 forbid all of it. It is replaced by the held-directory, no-follow adapter of the M0 feasibility record, which answers `FsError.unsupported` where it cannot keep the rules | 03 Filesystem policies; 06 Closed operation semantics |
| World | `HostWorld` `world.rs:42`; `decode` `world.rs:76`, which accepts only the format `host-world.v1` (`world.rs:187`); `WORLD_FIELDS` `world.rs:22` | changed | `@hostWorld.v2` in brace VIBON: the same eight fields, with the defaults and path rules of 06 Host world. Its directories are reached only through fake grants | 06 Host world |
| Audit trace | `TRACE_FORMAT` `event.rs:31`; `HostEvent` `event.rs:35`; `to_vibon` `event.rs:84`; `to_json` `event.rs:105` | changed | `@auditTrace.v2`: `kind`, `invocation`, `parent`, `op`, `driver`, `origin`, `grant`, `args`, `result` (G9 to G12) | 06 Audit trace |
| Unknown events | `OpaqueEvent` `event.rs:46` | kept | A consumer keeps an unknown kind opaque | 06 Audit trace |
| Broker | none | new | One per world: resources, policies, counters, accounting lineage, generations. It reserves before an action, charges progress, and never refunds an unknown outcome | 03 Limits; 06 Host calls |
| Policies | none | new | `FsPolicy`, the `@scopePolicy.v1` decoder, normalization, the node and depth bounds, and evaluation of a request | 03 Scope policies |
| Narrowing | none | new | `restrict`, `sub`, and `limit` over the broker (G17) | 03 Narrowing |
| Dispatch boundary | `Context::host` in `crates/vibra-nexus/src/context.rs:634`; `host_call` in `crates/vibra-interp/src/machine.rs:1249` | new | One function both executors call: validate the grant, check rights, enforce the policy and reserve limits, call the adapter, then let the caller advance and append | 06 Host calls |
| Startup | none | new | Recipe evaluation, acquisition, refusal to `Option.none()`, release, and the startup record, shared by the interpreter and the launcher (G1, G2, G5, G8) | 04 Targets; 06 Entry outcome |
| Test world | none | new | The `TestWorld` issuer and its six members (G19, G20) | 04 Test worlds and grants |

**What grants and scope policies change.** Every operation now has an
authority operand that the runtime validates before anything else, so an
operation can no longer be reached from a path string. A filesystem request
is checked against the whole policy chain, then performed on the object the
adapter opened. A reservation precedes every byte. Policy refusals and
limits become typed results (`denied`, `limitExceeded`, `unsupported`)
instead of being impossible, and a fake world answers them exactly as a
real one does. The in-memory world stops being a provider that any program
can reach and becomes the resource behind fake grants.

### vibra-native

| Area | Code | Status | Concrete change |
| --- | --- | --- | --- |
| Value access | `Source` `crates/vibra-native/src/lib.rs:62`; `Sink` `lib.rs:85` | kept | The algorithms know no representation |
| Sequence algorithms | `copy`, `concat`, `append`, `slice`, `compare`, `length` (`lib.rs:121` to `lib.rs:194`) | kept | They back `Str`, `Bytes`, and `List` operations, held to their Vibra bodies |
| Numbers | `integer_to_str` `number.rs:42`; `parse_integer` `number.rs:82`; `parse_float` `number.rs:111` | kept | They back the `From` and `TryFrom` implementations of 06 Integer operations and 06 Floating-point operations |
| UTF-8 | `encode` `utf8.rs:10`; `decode` `utf8.rs:92` | kept | Unchanged |
| Arena dispatch | `call` `arena.rs:538` | changed | Dispatches over the revised native rows: `array` names become `list` (`lib.rs:51`, `lib.rs:118`), and the `array.of` and `dict.of` rows go |

`vibra-native` performs no host operation and stays that way (06 Native
implementations, "No user natives").

## 4. Native pipeline

| Crate | Kept as is | Changed, and by whom |
| --- | --- | --- |
| `vibra-lower` | Whole-program reachability, the ownership rules, deterministic identifiers, origin numbering (`crates/vibra-lower/src/lib.rs:395`), the `form` list and `NotLowered` (`lib.rs:111` to `lib.rs:168`), and host lowering (`lib.rs:719`, `lib.rs:1238`) | X2-01: `WIDEN` (`lib.rs:145`) narrows to interfaces; `TUPLE_PROJECT` (`lib.rs:159`) is removed; `ARRAY` (`lib.rs:161`) becomes `List`; `LABELLED_OR_VARIADIC_PARAMETER` (`lib.rs:131`) keeps labelled only; the division trap (`lib.rs:1111`, documented at `lib.rs:21`) is removed; new refused forms name guards with `else`, plan building, `await`, and a host call with a grant. X2-03: `ENTRY_WITH_PARAMETERS` (`lib.rs:129`, checked at `lib.rs:227`) goes when grant parameters lower |
| `vibra-exec` | Blocks, slots, ownership, terminators, the five checkpoints (`crates/vibra-exec/src/lib.rs:357`), and the verifier | X2-01: `ObjectKind::Array` (`lib.rs:130`) becomes `List`, `ObjectKind::Union` (`lib.rs:138`) is removed, and `Terminator::Trap` (`lib.rs:427`) names only the two defect traps. X2-02: the plan kind and a call through a plan. X2-03: the grant as the first operand of `Host` (`lib.rs:399`), an entry with parameters (`lib.rs:478`), and the registry minor of major 2 (`lib.rs:518`) |
| `vibra-nexus` | Context, frames, the step protocol and quota (`crates/vibra-nexus/src/lib.rs:102`), module values (`context.rs:593`), the store (`store.rs:242`), and integer arithmetic | X2-03: `ABI_VERSION` (`lib.rs:47`) moves; `admit` (`lib.rs:165`) checks the registry major, the entry contract, and the binding requirements; `Context::host` (`context.rs:634`) validates the grant and calls the shared boundary, and a failed `perform` is no longer always exhaustion (`context.rs:650`); `result_is_variant` (`context.rs:880`) is removed; `build_array` (`host.rs:109`) becomes `build_list`. X2-02: the plan kind |
| `vibra-nexus-eval` | The stepper, its ABI requirements (`crates/vibra-nexus-eval/src/lib.rs:117`), and its host terminator (`lib.rs:280`) | X2-02 adds the plan instructions |
| `vibra-codegen` | Cranelift emission, the descriptor layout derived from the bridge types (`crates/vibra-codegen/src/descriptor.rs:1`), and the host terminator (`crates/vibra-codegen/src/function.rs:494`) | X2-03: `Inputs::entry_outcome` (`descriptor.rs:31`, written at `descriptor.rs:186`) is replaced by the entry contract. X2-02: the plan instructions |
| `vibra-nexus-bridge` | The step ABI, the routines, table admission, and the process entry (`crates/vibra-nexus-bridge/src/entry.rs:34`) | X2-01: `KIND_ARRAY` (`abi.rs:197`) becomes `List`, `KIND_UNION` (`abi.rs:205`) is removed. X2-03: `VibraProgram::entry_outcome` (`abi.rs:333`) and `entry_err_variant` (`admission.rs:214`) are replaced by the entry contract; the launcher's exit codes (`launcher.rs:23` to `launcher.rs:36`) and `run_admitted` (`launcher.rs:122`) gain startup before `generated.start` (`launcher.rs:131`) and return the `ExitCode` (G7); the process arguments (`entry.rs:39`) reach the program only through an `EnvReadArgs` grant. X2-04: the harness observation (`harness.rs:304`, S-expression nodes at `harness.rs:87`) becomes the versioned observation document |
| `vibra-build` | The link driver, toolchain discovery, and `build_executable` | X2-03: `entry_outcome` (`crates/vibra-build/src/pipeline.rs:70`) and `inputs_for` (`pipeline.rs:61`) are removed; the build carries the bindings |
| `vibra-actors` | Everything: the kernel has no dependency and is outside v0.0.2 | Endpoint grants (`ActorEndpoint`, 06 Host calls "Two kinds of target") arrive with the concurrency release |

### Reusable as is

- The execution IR's structure, ownership discipline, and verifier; the
  checkpoint set of 06 Native execution is exactly the code's five.
- The Nexus context, frames on a cell stack, the dispatcher and its
  unobservable quota, module-value states, and reference-counted store.
- The stepper, Cranelift generation, the single unsafe bridge, the static
  archive, and the Windows link driver.
- The ordering in `Context::host`: operation, reply, `resume`, then the
  event (`crates/vibra-nexus/src/context.rs:650` to `context.rs:659`). It is
  the tail of the order 06 Host calls requires, so the new checks go in
  front of it.

### What must change first

1. Before X2-01: the IR removals and renames, and the two-trap code type
   (packages N1 and N2 of section 7). X2-01 cannot rebaseline against an IR
   that still has `Union`, `TupleProject`, and the variadic tail.
2. Before X2-02: the callable kinds, plans, and guards in IR and on the
   interpreter (N4, N5), because the interpreter is the oracle.
3. Before X2-03: registry major 2, the broker and its dispatch boundary,
   the filesystem adapter, the v2 world and trace, and startup with the
   `ExitCode` entry (N7 to N11), plus answers to G1, G2, G5 to G13, and
   G16.
4. Before X2-04: the v2 encodings (N3), which the observation document
   uses.

## 6. Specification gaps

Each entry names where the specification is, the question the code must
answer, and a recommended answer. Coverage of the call-outs of section 1:

| Call-out | Gaps |
| --- | --- |
| Guards with `else` | G21, G22 |
| Bare guards | fully specified, except G23 |
| The two defect traps | fully specified |
| Plans and `await` | G9, G21, G29 |
| Grants and sealed interfaces | G15 to G18 |
| Associated types | G24, G25 |
| `ExitCode` entry | G7, G23 |
| Startup acquisition order | G1, G2, G5, G6, G8 |
| Old effect roots and entry signature | fully specified, except G29 |
| `@array` to `@list` | fully specified |

### G1. Startup tie order

- **Where.** 06 Entry outcome says ties are broken "in the order written in
  the project file". 03 Target admission and 04 Targets say "in the order
  of their names", and design §7 follows them.
- **Question.** Which order breaks ties among grants that `after` leaves
  unordered?
- **Recommendation.** By name: the UTF-8 bytes of the leaf's binding key
  (`@logs.reader`), the same order whatever the file's layout. Correct
  06 Entry outcome to match. A written order would make reformatting the
  project file change behavior.

### G2. When and how grants are released

- **Where.** 04 Targets releases grants "in the reverse order after the
  entry returns". 06 Entry outcome and 03 Target admission mention release
  only after a failed startup. Neither says what happens after a defect
  trap or memory exhaustion, or whether release is traced.
- **Question.** On which endings is release performed, in what order, and
  does it appear in the startup record or the trace?
- **Recommendation.** Release every acquired grant in reverse acquisition
  order on every ending: return, defect trap, memory exhaustion, and failed
  startup. A release appends no `@host` event and is listed in the startup
  record (G8) with its status. Release never runs program code.

### G3. Host wire symbols

- **Where.** 06 Host registry says major 2 "keeps their wire symbols", and
  03 Standard operation inventory says "The wire symbols are those of
  v0.0.1". Both tables then list camelCase symbols (`io.stdinText`,
  `fs.readFile`). The v0.0.1 symbols are kebab-case
  (`crates/vibra-ir/src/external/host.rs:231`: `io.stdin-text`).
- **Question.** Which spelling does registry major 2 use?
- **Recommendation.** The camelCase spellings of the tables. Major 2
  changes every row's signature anyway, so nothing is gained by keeping a
  second spelling. Reword both sentences: each major-2 symbol is the
  camelCase spelling of its v0.0.1 symbol, one to one.

### G4. Compiler intrinsic registry version and spelling

- **Where.** 06 Compiler intrinsic registry says "the registry maps each to
  a fixed wire symbol such as `i32.add-checked`" and "Changing the casing
  of a source name MUST NOT rename a wire string". It names no version. The
  code's registry is `vibra_v1` (`crates/vibra-ir/src/external.rs:28`), and
  rows leave (`Div`, `Rem`, `ArrayOf`, `DictOf`) and change (`array.*`).
- **Question.** What is the registry's version, and are its wire symbols
  kebab-case or camelCase?
- **Recommendation.** State a new registry version, since rows are removed.
  Spell its symbols in camelCase, like the host registry after G3
  (`i32.addChecked`, `list.append`), and keep the casing rule as a
  stability rule for later minors. If Hannah prefers to keep kebab-case
  symbols, say so in the host registry too, so that the two registries
  follow one rule.

### G5. Refused and failed acquisition, per provider

- **Where.** 06 Entry outcome and 04 Targets bind an optional grant to
  `Option.none()` when it is "refused in the way its provider specifies";
  the provider table of 04 Targets specifies no refusal. The `index` of an
  `@argumentPath` has no stated base.
- **Question.** For each provider, which acquisition failures are a refusal
  (none for an optional leaf, `@grant.requiredUnavailable` for a required
  one) and which are errors? Where does an argument index start?
- **Recommendation.** For `@host.fs.directory`: a missing argument or
  variable, a path that does not exist, is not a directory, or cannot be
  opened is a refusal; a path that is not Unicode is a refusal; a malformed
  recipe is a build error. Stream, clock, random, argument-list, and
  variable-set providers never refuse. `index` is zero-based over the
  program arguments without the executable path.

### G6. Binding recipes under a host world

- **Where.** 06 Host world says conformance execution uses deterministic
  worlds "with fake sealed grants", and that a world "describes inputs";
  04 Targets defines recipes over real resources. Nothing maps a recipe to
  a world. The M4 gate needs the interpreter, the stepper, and the linked
  executable to agree on one fixture with startup.
- **Question.** How does a run under a world, in the interpreter or the
  harness launcher, acquire the entry's grants?
- **Recommendation.** The same recipes, resolved against the world:
  `@fixedPath` is read as a canonical virtual path (a relative one under
  `/`), `@argumentPath` indexes the world's `args`, `@variablePath` reads
  its `env`, and the other providers bind the world's streams, clocks, and
  random source. Policies and limits apply unchanged. A missing virtual
  directory is a refusal under G5.

### G7. The exit status of a built executable

- **Where.** 06 Entry outcome: a built executable "returns that `U8` to the
  operating system", and memory exhaustion, startup failure, and tool
  failure "MUST NOT appear as a returned application code". 05 Versioned
  envelope and results maps codes only for `vibra run`. The launcher uses
  3, 5, and 6
  (`crates/vibra-nexus-bridge/src/launcher.rs:23` to `launcher.rs:29`),
  which collide with application codes, and marks the admission code
  provisional (`launcher.rs:36`).
- **Question.** What does a built executable exit with after a defect trap,
  memory exhaustion, a failed startup, or a refused admission?
- **Recommendation.** On Windows, where an exit status has 32 bits, use
  four fixed values above 255, written in 06 Entry outcome, and write the
  diagnostic to standard error. Application codes 0 to 255 pass unchanged.
  Linux and macOS have only eight bits and are not v0.0.2 targets; record
  their rule as open for the native release.

### G8. The startup record and its trace

- **Where.** 06 Audit trace: "Startup acquisition has its own phase and
  correlations", with no fields. 05 Command payloads defines
  `StartupRecord` with `status`,
  `bindings`, `auditTrace`, and `failure`, and each binding with
  `recipeIdentity`, `providerIdentity`, `status`, and `grantProjection`,
  but no binding status values and no identity format.
- **Question.** What are the binding statuses, the identity formats, and
  the events of the startup trace?
- **Recommendation.** Binding status: `acquired`, `refused` (bound to
  none), `failed`, `released`. `recipeIdentity` is the hash of the
  normalized recipe, in the format of G28; `providerIdentity` is the
  provider atom and the toolchain version. The startup trace holds the
  `@host` events of reads a recipe performs (an argument or a variable),
  with `parent` none, numbered in the same invocation sequence as the run
  (G9).

### G9. Invocation and parent ordinals

- **Where.** 06 Audit trace: `invocation` is "the invocation's ordinal
  within the run", and `parent` the parent invocation's ordinal. 06 Cold
  plans and invocation identity makes each `await` an invocation. Neither
  says which invocations are counted or what a parent is in a sequential
  run.
- **Question.** Are the ordinals counted over host events only or over
  every `await`? What is a host event's parent?
- **Recommendation.** Every `await` is an invocation, numbered from 1 in
  the order it starts; the launcher's run of the entry's plan is the first
  invocation of the run, after the startup reads of G8. A host event's
  `invocation` is the ordinal of the `await` that ran its host-bound
  member, and its `parent` is the invocation whose body performed that
  `await`. Executors agree because evaluation order is fixed.

### G10. Audit origins across executors

- **Where.** 06 Audit trace: `origin` is "the source-origin ordinal".
  06 Traps uses the program's origin table. The lowering numbers origins in
  order of first use (`crates/vibra-lower/src/lib.rs:395`); the interpreter
  has no table. The M4 gate compares traces across three executors.
- **Question.** Which table gives the ordinal, so that the interpreter and
  native code write the same number?
- **Recommendation.** Define a canonical origin table of the checked
  program: every source origin of the reachable program, sorted by source
  identity, then start, then end, numbered from 1. Both the interpreter and
  the lowering use it. The alternative, writing the span itself instead of
  an ordinal, is also sound and avoids the table.

### G11. Driver, grant projection, and the JSON form

- **Where.** 06 Audit trace: `driver` is "the identity and version of the
  bound driver, as data"; `grant` is "a safe projection … its policy,
  rights, and root accounting". "A JSON envelope … MUST NOT fall back to
  untyped event strings." The code's JSON puts VIBON text in strings
  (`crates/vibra-host/src/event.rs:105`).
- **Question.** What are the exact shapes, and is a per-field VIBON string
  in JSON an "untyped event string"?
- **Recommendation.** `driver: {provider: @host.fs.directory version: "…"}`,
  with `@hostWorld.v2` as the provider of a fake grant. `grant:
  {leaf: @notes policy: "<policy hash>" rights: [@read] root: 1u64}`, where
  `root` is a run-local number of the acquired root grant. In JSON each
  field is its own member, and a value is canonical VIBON text inside a
  string; only a whole event as one string is forbidden.

### G12. Refusals before the adapter

- **Where.** 06 Audit trace: "A refusal before the adapter is called MUST
  NOT fabricate a completed host event", and future kinds "for attempts,
  refusals" need their own fields. 03 Delivery and typed outcomes defines
  `denied`, `limitExceeded`, and `unsupported` as "nothing was attempted".
- **Question.** Does a trace then show no trace of a denial at all?
- **Recommendation.** Yes, for v0.0.2: such a call appends nothing, and
  conformance compares the typed result the program received. Say so
  explicitly in 06 Audit trace, and leave a refusal kind to the release
  that needs it.

### G13. Create or replace, and the order of the checks

- **Where.** 06 Host calls requires the policy to be enforced (step 3)
  before the adapter is called (step 4). 06 Closed operation semantics
  makes a write's action, create or replace, depend on whether the target
  exists, which only the adapter learns after it opens the directory.
  Filesystem rule 2 of 03 Filesystem policies checks the name the
  filesystem reports, which is also known only after opening.
- **Question.** Where are the action-dependent and name-dependent checks
  made, and how are their refusals reported?
- **Recommendation.** Steps 2 and 3 check everything the operands decide:
  the prefix, the requested name against the name test, and that the
  policy permits at least one of the possible actions. The adapter then
  enforces the right for the action it found and the name it opened, and
  reports a failure as `denied` from the adapter. That is a completed call
  and is traced (06 Audit trace, "A typed denial returned by the adapter").

### G14. Filtered listing

- **Where.** 06 Closed operation semantics, `DirList.entries`: "A filtered
  listing applies its per-entry visibility rule before it exposes any
  name." 03 Exclusions excludes "filtered directory listing".
- **Question.** Is there a filtered listing?
- **Recommendation.** No. Remove the sentence from 06: a directory rule
  permits listing a directory whole or not at all.

### G15. Policy roots for bundle leaves

- **Where.** 03 The wire form: "`root` names the entry parameter whose
  recipe holds the directory". 04 Targets binds recipes to leaves, and a
  bundle parameter has one leaf per field (`@logs.reader`).
  `ScopeError.foreignRoot` (03 Narrowing) has no stated trigger.
- **Question.** What does `root` name for a field of a bundle, and when is
  `foreignRoot` returned?
- **Recommendation.** `root` names the leaf. A policy whose rules name a
  leaf other than the grant's own is `foreignRoot`, both in a recipe (a
  build error) and in `restrict` given a policy decoded from data.

### G16. Limit accounting

- **Where.** 03 Limits counts "invocations and payload bytes in and out, in
  encoded bytes". 04 Targets gives recipe limits as `U64` bounds "for the
  life of the process"; 03 Narrowing gives `GrantLimits` as `Option(U64)`
  fields. 06 Closed operation semantics requires a limit to be reserved
  before an operation could exceed it.
- **Question.** What counts as input and output bytes for each row, and how
  is a read of unknown size reserved?
- **Recommendation.** Input bytes are bytes the program receives (file
  contents, standard input, a variable's value, random bytes, argument
  text); output bytes are bytes it sends (written contents, stream
  writes); an invocation is every call that passed reservation. A read
  first learns the size on the opened object, reserves it, and reads at
  most that many bytes; a size over the remaining limit is
  `limitExceeded` before any content is read. An omitted recipe limit is
  unbounded, the same as `Option.none()`.

### G17. The narrowing members

- **Where.** 03 Narrowing declares `restrict`, `sub`, and `limit` as pure
  members of every standard operation interface. They are neither host
  rows (06 Host registry) nor rows of 06 Compiler intrinsic registry, yet
  each derives a broker lineage.
- **Question.** How does the standard library bind them?
- **Recommendation.** Three generic rows of the compiler intrinsic
  registry, `grant.restrict`, `grant.sub`, and `grant.limit`, generic over
  the grant type and implemented by the broker. They perform no host
  operation and append no event. List them in 06 Compiler intrinsic
  registry, so the closed table stays closed.

### G18. How a sealed authority implements its interface

- **Where.** 03 Operation interfaces binds host members with
  `external @host` inside the owning interface. 03 Tests and model
  profiles lets a package implement an operation interface in Vibra. A
  value of type `DirReadText` can therefore hold a sealed authority or a
  user type, and 02 Dispatch calls "the implementation … of the held type".
- **Question.** What is the held type of a sealed authority, and which code
  runs when `DirReadText.read` is called on one?
- **Recommendation.** A sealed authority is implemented by its interface's
  own `external @host` members: dispatch on an authority receiver selects
  the host-bound member, and dispatch on any other held value selects its
  `impl`. State this in 03 Operation interfaces; the checked IR then needs
  no hidden carrier type.

### G19. How `TestWorld` is bound

- **Where.** 04 Test worlds and grants makes `TestWorld` an operation
  interface with six procedures. 06 Host registry has 17 rows and "No
  symbol escapes the closed table". Nothing gives `TestWorld` a symbol, an
  owner row, a delivery class, or an audit meaning.
- **Question.** Are the `TestWorld` members host rows, and do their calls
  appear in the trace?
- **Recommendation.** A separate closed test-world registry with one row
  per member (`testWorld.directory` and so on), bound with `external @host`
  in `@std.test`, refused by admission for every target, since no recipe
  can name `TestWorld`. Their calls reach no external resource and append
  no `@host` event.

### G20. `TestWorld` semantics

- **Where.** 04 Test worlds and grants.
- **Questions and recommendations.**
  - *Policy of a fake grant.* Unstated. Recommend: each grant of
    `directory` permits its own action everywhere in its new directory,
    with no limits; a test narrows it with `restrict`, `sub`, and `limit`.
  - *Nested paths.* `directory` takes `Dict(RelativePath Str)`. Recommend:
    a multi-name path implies its parent directories.
  - *Standard input.* A test world takes `stdin` from its `@hostWorld.v2`
    document, and `streams` also takes `stdin: Str`. Recommend: `streams`
    makes new streams whose input is its argument, and the world's `stdin`
    feeds only an entry run under that world (G6).
  - *Output.* `output` returns "what the test has written so far".
    Recommend: everything written through every stream grant the test's
    world issued, in write order, per stream.
  - *Repeated calls.* Recommend: each `clocks`, `random`, and `environment`
    call returns grants over the world's single clock, random state, and
    environment, so two calls share state; only `directory` and `streams`
    make new resources.

### G21. `await` in tail position

- **Where.** 06 Tail calls lists the tail positions and does not mention
  `await`; design "Plans" says an `await` "may be a tail call". 03 Cold
  plans and performed rows writes an effectful walk as "a recursive `proc`
  over `Iter.next`", which needs constant space.
- **Question.** Must an `await` with nothing after it replace its
  activation?
- **Recommendation.** Yes: add `return await p` to the table of 06 Tail
  calls under the same rule as a call. Building a plan creates no
  activation and is not a call.

### G22. Guards on contract members and implementations

- **Where.** 02 Guards and callable types: "Each implementation is checked
  under the contract's `requires` and MUST establish the contract's
  `ensures`." It does not say who evaluates a contract's `requires … else`
  or `ensures … else`, or whether an implementation may add guards of its
  own.
- **Question.** Where does a contract guard with `else` run, and are
  implementation guards allowed?
- **Recommendation.** A contract member's guards with `else` run on every
  call of every implementation, as part of the member: the checker copies
  them into each implementation's checked function, so static and dynamic
  dispatch behave alike. An implementation may not add a guard; it states
  its own checks in its body.

### G23. Guards on the entry

- **Where.** 06 Entry outcome: the launcher "builds the entry's plan and
  runs it under the same contract … rules". A bare `requires` on a `proc`
  is discharged where its plan is built (02 Guards and callable types), but
  the launcher has no checker, and a predicate cannot mention a grant
  (02 Predicates).
- **Question.** May the entry have a `requires`?
- **Recommendation.** A bare `requires` on the entry is
  `@project.invalidEntrySignature`, because no call site exists to
  discharge it. A `requires … else` and either form of `ensures` are
  allowed and behave as on any `proc`.

### G24. Associated types in IR

- **Where.** 02 Associated types and 02 Interface values define them;
  06 Generic instantiation passes type descriptors at run time; no chapter
  says whether associated types survive checking.
- **Question.** Do associated types reach checked IR?
- **Recommendation.** As section 1.6: only as the labelled arguments of an
  interface value type; every projection is resolved by the checker or
  elaborated into an implicit type parameter.

### G25. Encoding of interface value types, `Any`, and interface values

- **Where.** 06 Types lists every type record "exactly" and has none for an
  interface value type or `Any`. 06 Test assertions and 06 What has no
  encoding give an observation error for a value "that hides one behind an
  interface value", which implies that other interface values are
  observable.
- **Question.** How are these types and values encoded?
- **Recommendation.** Type records `{type: @interface interface: T
  associated: {Item: T}}`, where the inner `T` is the interface's declared
  type record, and `{type: @any}`. A value of an interface value type is
  encoded as `{kind: @interface value: v}`, where `v` is the held value's
  own complete observation (`format` omitted). The held type is visible to
  tools, never to source, which 02 Interface values requires of source
  only.

### G26. The type record of a scalar

- **Where.** 06 Types: scalars "use the fixed wire atoms `@i8` through
  `@u64` …", and "Every type record has exactly these fields", but the
  table has no scalar row.
- **Question.** Is a scalar type written as the bare atom or as a record?
- **Recommendation.** The bare atom (`type: @u8`), as in the code's
  encoding; add the sentence to 06 Types.

### G27. The payload of a variant

- **Where.** 06 Values: a variant has "`payload`: the encoded fields, only
  for a case that has fields". A case has named fields (02 Declared types).
- **Question.** What is the payload's shape?
- **Recommendation.** A record of the case's fields in declaration order,
  with no `kind` and no `type`: `payload: {radius: 1.5f64}`.

### G28. The declared identity, `seed`

- **Where.** 02 Declared types: the identity is "the hash of its logical
  anchor" and includes the identity scheme version. 06 Types writes it as
  `seed`. No chapter fixes the hash, the framing of the anchor, the string
  form, or the version's spelling. 05 Snapshot, claim and witness identity
  fixes a framing and SHA-256 for the snapshot revision.
- **Question.** What exactly is `seed`?
- **Recommendation.** `sha256:` followed by lowercase hex of SHA-256 over
  the anchor fields (package and unit, module path, owner path, kind, name,
  scheme version `@identity.v1`), framed the way 05 frames the entries of
  a revision. Give the rule and one test vector in 02.

### G29. Identity and order of effect roots

- **Where.** 03 Effect ceilings: `roots` is "a list of interface
  identities, sorted by their UTF-8 bytes". 06 Types: "resolved canonical
  identity atoms, sorted by identity bytes". The code sorts by path and
  then by a numeric id (`crates/vibra-ir/src/effect.rs:18`).
- **Question.** Is a root written as the interface's path atom or as its
  identity, and is the order that of the path or of the identity?
- **Recommendation.** Write each root as the interface's canonical path
  atom, and sort by the UTF-8 bytes of that path. Paths are unique within a
  snapshot, and a row stays readable. Make the two chapters say the same.

### G30. The dict iteration primitive

- **Where.** 06 Character, sequence and dictionary operations: "the pure
  primitive behind `Dict`'s nested `Iter` implementation", with no name and
  no signature.
- **Question.** What is its row?
- **Recommendation.** `dict.next`, `Fn({Dict(K V)} Option({{K V} Dict(K V)}))`:
  the first entry in key order and the remainder.

### G32. Arena kind codes

- **Where.** 06 Value arena says arena layout is private. The native code
  and the runtime read kinds from `crates/vibra-ir/src/arena.rs:68`.
- **Question.** Must native kind codes keep the historical numbers?
- **Recommendation.** No. The kinds are private to the toolchain, and Nexus
  gives itself the kinds it needs, including `List`, `Proc`, `Intent`, and
  sealed authority. No specification change is needed; this records the
  decision.

## 7. Suggested package cut

The packages below adapt the IR, the interpreter, and the host; the native
packages of [plan-v0.0.2.md](../../nexus/plan-v0.0.2.md) follow them. "Beside
the front end" says whether a package can run while the new reader and
checker are written.

1. **N1. IR removals and renames.** Remove `Union`, the anonymous `Enum`,
   union widening, `Pattern::Member`, `TupleProject`, the variadic tail,
   `ArrayOf`, `DictOf`, integer `Div` and `Rem`, `RowTail`, and the `err`
   classification; rename `Array` to `List` everywhere. Crates: `vibra-ir`
   and every consumer that must keep building (`vibra-interp`,
   `vibra-lower`, `vibra-exec`, `vibra-nexus`, `vibra-nexus-bridge`,
   `vibra-native`, and the current checker). Depends: M0. Beside the
   front end: no; it lands first, and the new checker targets the result.
2. **N2. Two defect traps.** The native trap type with two codes; the
   registry rows of the removed codes deleted; fixtures rewritten (X2-01
   item 3). Crates: `vibra-ir`, `vibra-interp`, `vibra-lower`, `vibra-exec`,
   `vibra-diagnostics`, `vibra-conformance`. Depends: N1. Beside: yes.
3. **N3. Encodings and identities v2.** `@type.v2` and `@value.v2` brace
   records, `seed`, rows, `ObservedValue`, and the interpreter's and
   harness's observation. Crates: `vibra-ir`, `vibra-interp`, `vibra-host`,
   `vibra-nexus-bridge`. Depends: N1; G25 to G29; the brace VIBON writer of
   the front end. Beside: yes, sharing the VIBON module.
4. **N4. Callable kinds, plans, and `await`.** `Fn`, `Proc`, `Intent`;
   plan building and `await` in IR; three value kinds and invocation in the
   interpreter. Crates: `vibra-ir`, `vibra-interp`. Depends: N1; G21.
   Beside: yes; tested on hand-built IR.
5. **N5. Guards with consequences and `where … else`.** IR fields,
   construction results, interpreter continuations, the `try` path, and the
   tail flag. Crates: `vibra-ir`, `vibra-interp`. Depends: N1, N4; G22,
   G23. Beside: yes.
6. **N6. Interface values and associated types.** Labelled interface value
   types and the elaboration contract for projections. Crates: `vibra-ir`,
   `vibra-interp`. Depends: N1; G24, G25. Beside: yes, agreed with the
   checker.
7. **N7. Host registry major 2.** Rows, owners, delivery class, grant
   parameter, `RelativePath`, `List`, camelCase error cases plus the four
   broker cases, `HostCall`, `HostValues`, provider signatures, the 2.0
   snapshot. Crates: `vibra-ir`, `vibra-host`, `vibra-interp`,
   `vibra-nexus`. Depends: N1; G3. Beside: yes.
8. **N8. Broker, grants, policies, narrowing, and the dispatch boundary.**
   The sealed authority, the broker, `FsPolicy` and `@scopePolicy.v1`,
   `restrict`, `sub`, `limit`, and the ordered checks of 06 Host calls in
   one function. Crates: `vibra-host`, `vibra-ir`, `vibra-interp`.
   Depends: N4, N7; G13, G15 to G18. Beside: yes.
9. **N9. Windows filesystem adapter.** The held-directory, no-follow
   adapter from the M0 feasibility record, replacing the ambient-path code
   of the system provider. Crates: `vibra-host`. Depends: N8 and the
   feasibility record. Beside: yes.
10. **N10. Host world v2 and audit trace v2.** The `@hostWorld.v2` decoder,
    fake resources behind fake grants, `@auditTrace.v2` events, and the JSON
    form. Crates: `vibra-host`, `vibra-interp`, `vibra-nexus-bridge`.
    Depends: N3, N8; G9 to G12. Beside: yes.
11. **N11. Startup and the `ExitCode` entry.** Bindings beside the checked
    program, acquisition, refusal, release, the startup record, the
    application code, and the `vibra run` mapping to
    `@command.programFailed`. Crates: `vibra-host`, `vibra-interp`,
    `vibra-cli`. Depends: N8, N10; G1, G2, G5 to G8, G23; project
    checking of `@project.v2` recipes from the front end. Beside: partly; it
    needs the recipe checker.
12. **N12. `TestWorld`.** The issuer, its six members, and the test runner's
    root grant. Crates: `vibra-host`, `vibra-interp`, `vibra-workspace` (its
    test runner).
    Depends: N8, N10; G19, G20. Beside: yes.


The native track then runs as planned: X2-01 after N1 and N2; X2-02 after
N4 and N5; X2-03 after N7 to N11; X2-04 after N3 and N10.
