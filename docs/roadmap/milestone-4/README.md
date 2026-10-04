# Milestone 4 step plan

Status: in progress. Step 1, the Stage 4A contract freeze, has landed
(PR #354), and so have Step 2, the contract-member forms and the binder fix
(PR #355), Step 2b, the boolean constants (PR #357), and Step 3, deep non-tail recursion in the reference interpreter
(PR #356), and Step 4, the WebAssembly backend skeleton and the differential
harness (PR #358), and Step 5a, the value arena and its runtime (PR #359), and
Step 5b, the data forms and core lowering (PR #360), and Step 6, calls
(PR #361, merge e86d93e).
Step 7, patterns and typed failure, is proposed as `landed` by its pull request
and is conditional on that PR merging; no other implementation step has landed
Decision ledger: [decision-ledger.md](decision-ledger.md)
Surface inventory: [supported-surface.md](supported-surface.md)
Validation: [validation.md](validation.md)
Milestone: [Milestone 4 — WebAssembly spine, static effects, and host operations](../v1.md#milestone-4--webassembly-spine-static-effects-and-host-operations)
Execution model: [execution.md](../execution.md)
Integration branch: `m4`

M4 gives the typed IR its second consumer and the language its first contact
with a host. The roadmap delivers it in two ordered stages on `m4`: the
WebAssembly spine for the complete pure language, with no `@host` import, and
then static effects and host operations, each landing in both backends in the
same change. The reference interpreter stays the oracle throughout.

## Start here

`m4` is created from `origin/main` at
`2e7d257bb9e5d46a0454851a731706a9061bb788`. That commit contains the M3
integration merge ([PR #304](https://github.com/nahharris/vibra/pull/304),
`a72bf94`), its three follow-ups (#347, #348, #349), and the three
[pre-M4 specification changes](../pre-m4/README.md) (#350, #351, #352). M3's
exit evidence is in [its exit report](../milestone-3/exit-evidence.md).

Baseline at `2e7d257`: the independent corpus reports 82 reader, 224 static,
94 interpreter, and 14 tooling cases — 414 passed, 0 failed, 0 unavailable.
The 94 interpreter cases (67 `interpret`, 4 `workspace-run`, and 23
`workspace-test`) are the executable corpus the Wasm backend must match.
Steps 1 to 3 added cases, and at `8a04c08`, the head Step 4 branched from, the
corpus reports 82 reader, 230 static, 104 interpreter, and 15 tooling cases —
431 passed, 0 failed, 0 unavailable. Step 2b then added cases, and at `388dfe1`
the corpus reports 83 reader, 239 static, 107 interpreter, and 15 tooling cases
— 444 passed, 0 failed, 0 unavailable. Of the 107 interpreter cases, 100 expect
acceptance, and those are the executable cases the parity inventory covers; a
rejected case reaches no backend and has no row.

1. Read `AGENTS.md`, [the charter](../../spec/00-charter.md), the M4 section of
   [`v1.md`](../v1.md), this plan, and the chosen step's guide. Read the exact
   specification sections the guide names before editing code.
2. Fetch `origin/m4` and branch from its current head. Do not start from
   `main`, `m3`, or a previous step's unmerged branch.
3. Implement the earliest unfinished step whose predecessors have merged.
   Every step of Stage 4A lands before the first step of Stage 4B.
4. Run the [M4 validation commands](validation.md#before-merging-each-step)
   against `origin/m4` and record the [M4 handoff](validation.md#step-handoff).
5. The completing PR sets its row to `landed`, conditional on the merge. Record
   the PR and verified merge commit.

Keep one standing draft PR from `m4` to `main`. It stays draft until Step 22
evidences every exit-gate clause.

## Fixed implementation decisions

- **One typed IR, two consumers.** The Wasm backend consumes the same
  `vibra-ir` checked program the interpreter runs. It gets no private
  frontend, no second lowering from syntax, and no IR variant of its own. A
  shape the IR cannot express for both backends is fixed in the IR first.
- **The interpreter is the oracle, and a case has one expectation.** An
  executable corpus case keeps one expected result and one expected audit
  trace. The Wasm backend must reproduce them; no case carries a
  Wasm-specific expected output.
- **Parity shrinks monotonically.** From Step 4 every executable case has a
  Wasm disposition: matched, or not yet lowered with an owning step. Each
  Stage 4A step moves cases to matched and never back. A case still unmatched
  at Step 12 fails the stage sub-gate. A parity inventory test fails on a case
  with no disposition, as the surface inventory test does for AST variants.
- **The emitter has no engine dependency.** A new backend crate, `vibra-wasm`,
  lowers typed IR to module bytes and depends only on `vibra-ir`,
  `vibra-diagnostics`, and the encoder, as `vibra-interp` depends on the first
  two (it uses `vibra-ir` and the encoder alone, and its tests use
  `vibra-diagnostics`). The Wasm engine is reached only by the code that runs a
  module, `vibra-wasm-run`, and the emitter does not depend on the native-code
  crate either, only on the import names in `vibra-ir`. The boundary names and
  codes of the module (export names, the native import module, the status, trap,
  and failure codes) are written once in `vibra_ir::boundary`, which the
  emitter and the runner both read. The architecture boundary test changes in
  the PR that adds each crate; since Step 4 it also fails when any crate but
  the runner depends on Wasmtime, or any crate but `vibra-conformance` on the
  runner.
- **Unoptimized only.** No wrapper erasure, compact enum layout, or in-place
  update enters M4. The runtime chapter's representation latitude is M7's,
  after parity, measured against the baseline Step 21 records.
- **Emission is deterministic from the first module.** The same checked
  program yields byte-identical module bytes, and a host test asserts it from
  Step 4, so M7's byte-identical build gate inherits a property rather than
  retrofitting one.
- **No value index leaves an instance.** No arena index or instance identity
  appears in typed IR, in a canonical value encoding, in an audit event, or in
  any snapshot. A test over the IR's canonical form enforces it.
- **One registry table per provider.** The closed tables in `vibra-ir` drive
  the checker, the interpreter, and Wasm lowering. A host operation's
  signature, owner root, and audit-event shape are written once, and a Stage
  4B step lands an operation in both backends or not at all.
- **`vibra build` stays unavailable.** Wasm is executed only through the
  conformance harness in M4, as the roadmap states. Build products, custom
  sections, and source maps are M7's.
- **Standard library through the embedded input.** New standard-library
  modules for effects arrive through `stdlib/manifest.vibon`, and each change
  replaces the package version in place. No previous manifest shape, module
  set, or audit format is kept beside the new one.
- **Availability shrinks monotonically.** Each step moves forms from
  `@tool.unavailable` to supported with positive and negative cases, and never
  reclassifies a valid form as malformed. Every M4-owned form still
  unavailable at Step 22 fails the gate.
- **Activations live in the instance arena (D1).** The language rule is the
  outcome: no host stack per activation, depth bounded by memory, exhaustion a
  host event. The lowering is an implementation decision. The Wasm backend
  holds activation frames in the arena and runs a dispatcher loop, so a non-tail
  call pushes a frame and returns to the loop instead of nesting a Wasm call,
  and a tail call replaces the current frame. The interpreter holds its frames
  on the heap by the same plan (Step 3). Arena frames were chosen over a counted
  limit because a limit would make every non-tail call a counted, trapping
  operation and tie the lowering to the engine's stack, and because arena frames
  are the representation the post-v1 process model needs, where a process's
  stack is part of its heap and preemption counts function calls.
- **The arena is the module's own linear memory (D2).** It is managed by
  compiler-emitted code. Inside a module values refer to each other by offset,
  because nothing moves. Host-facing value IDs are 64-bit, index 0 is invalid,
  an ID is never reused within an instance, and a handle table makes each live
  ID hold a reference to its value. In Stage 4B the host reads and builds
  compound values through functions the module exports, taking and returning IDs
  and scalars, so no guest pointer crosses the `@host` boundary. Linear memory is
  exported once under the reserved name `vibra_v1_memory` for toolchain-owned
  native code only. A Stage 4A module imports only the pure native import module
  and has no `@host` import. An ID that is not an address
  lets a later ABI move values between instances and threads, as the beyond-v1
  obligations require, and lets the host read nothing it was not given.
- **Reclamation is precise reference counting (D3).** The specification states
  the rule only: liveness, unobservable release, a bounded live arena for an
  allocating tail loop, bounded-stack release, and that values cannot form
  cycles ([the premises are verified](decision-ledger.md#d34-the-no-cycle-premises)).
  The implementation is Perceus-style and non-atomic, because v1 and the post-v1
  plan are share-nothing with one instance per thread. M4 ships plain `dup` and
  `drop` with no elision. Borrow inference, count fusion, and in-place reuse of
  unshared values are M7 optimizations measured against the Step 21 baseline.
  Freeing walks an explicit worklist, so the engine stack stays bounded for any
  nesting depth.
- **Wasmtime with Cranelift, `wasm-encoder`, and `wasmparser` (D5).** The
  engine is Wasmtime for its mature Rust embedding API and its path to native
  host integration: GUI and GPU operations arrive later through Vibra's own host
  registry, and a Rust embedding reaches them without a second runtime. The
  compiler is Cranelift for its speed. Winch is excluded: tail calls are disabled
  under it, and an aarch64 Winch tail-call issue was opened on 2026-10-02 (as
  reported by the maintainer; not independently re-verified here). The baseline
  uses no tail-call instruction, but the exclusion keeps the engine choice from
  resting on a compiler with a known gap in a feature M7 may adopt.
  `wasm-encoder` emits and `wasmparser` validates, with exactly the baseline
  feature set. Only the code that runs modules may depend on Wasmtime. Step 4
  added the dependencies and the crates `vibra-wasm` (the emitter) and
  `vibra-wasm-run` (the runner, the only crate that depends on Wasmtime); see
  [Dependency evidence](#dependency-evidence).
- **Type arguments pass at run time (D9).** The Wasm backend passes type
  arguments as the interpreter does and emits one function per source function.
  Monomorphization is an M7 optimization. Typed IR carries what both backends
  need: a contract call holds the interface arguments and, from Step 2, the
  member's own type arguments in `where:` order (`member_types`), and each
  implementation or default function names the generic parameter it declares
  for each of them (`member_generics`); operands are in resolved parameter
  order, with a labelled operand left out already replaced by the contract's
  default and a dict tail already packed, so labelled operands and a dict tail
  add checking and no IR field; and a contract member used as a function value
  stays the closure M3 already made of it, whose body is a tail contract call
  carrying the same fields, so it adds no expression and no call target. See
  [ledger D9.1 and D9.3](decision-ledger.md).
- **Natives are written once, in Rust, and called by both backends (D10,
  "Bun style").** The Vibra runtime embeds Wasmtime, and where performance
  matters the toolchain runs native Rust and escapes from Wasmtime into the host
  process. This is not Rust compiled to Wasm. Each native implementation and each
  looping primitive row (integer and float `to-str` and `parse`) is toolchain-owned
  Rust in exactly one crate, proposed `vibra-native`. It is written once against a
  value-access interface (a trait over reading and building arena values) that both
  backends implement: the interpreter over its values, and the Wasm runner over the
  instance's linear memory. The interpreter calls the functions directly, and a
  module reaches them through the pure, versioned import module `vibra_native_v1`.
  The closed list of import names and signatures is a table in `vibra-ir`, beside
  the compiler registry. The emitter crate depends on that table's names and never
  on `vibra-native`; the runner crate, which already depends on Wasmtime, depends on
  `vibra-native` and supplies the imports. The Vibra body stays the meaning, and
  the harness holds native code to it. Natives read the instance's memory directly
  through the toolchain-reserved export `vibra_v1_memory`; no other host reads it,
  and the `@host` ABI stays scalar-only over IDs. A module that must run outside the
  Vibra runtime would instead use a self-contained form that runs the bodies; M7
  decides the shipped form, and M4 emits the native-import form only. See the
  [D10 options](decision-ledger.md#d10-the-single-source-for-natives-and-primitives).
- **The prelude is declared, not folded (D16).** The checker declares the
  values of the closed import-free vocabulary from the embedded `@std.bool`
  module, which is in every checked graph, so typed IR reads `true` and `false`
  as ordinary module-value reads. Folding module values that are constants is
  an M7 optimization applied to every constant or to none. A constant pattern is
  expanded by checking the constant's initializer as its own module checks it.
- **The NaN defence is on (D11).** The engine runs with NaN canonicalization
  enabled because it is unobservable, while the specification canonicalizes at the
  observation points.

These are implementation constraints, not language rules. Observable decisions
are closed in the owning specification by Step 1 (Stage 4A) or Step 13
(Stage 4B) before dependent code.

## Dependency evidence

Checked on 2026-10-02 against `rust-toolchain.toml`, which then pinned 1.94.1,
with `cargo info` on the crates.io index, and re-checked by Step 4 on 2026-10-03,
when it added the dependencies; the latest versions were unchanged.

| Crate | Version | Rust version | Licence |
| --- | --- | --- | --- |
| `wasmtime` | 49.0.2 (latest) | 1.96.0 | Apache-2.0 WITH LLVM-exception |
| `wasmtime` | 48.0.0 | 1.95.0 | Apache-2.0 WITH LLVM-exception |
| `wasmtime` | 47.0.4 | 1.94.0 | Apache-2.0 WITH LLVM-exception |
| `wasm-encoder`, `wasmparser` | 0.261.0 | 1.88.0 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |

**Decided (Hannah, 2026-10-02): use the latest versions.** Step 4 adopts the latest
Wasmtime, `wasm-encoder`, and `wasmparser` at the time it runs, and raises
`rust-toolchain.toml` to the Rust version they require, which is 1.96 for Wasmtime
49.0.2, in the same PR. Step 1 changed neither `rust-toolchain.toml` nor
`Cargo.toml`. The workspace is `MIT OR Apache-2.0`, and each crate's licence permits
linking it.

### Step 4 record

- **Versions and features.** The workspace declares `wasmtime` 49.0.2 with
  `default-features = false` and the features `std`, `runtime`, and `cranelift`;
  `wasm-encoder` 0.261.0 with `default-features = false` and `std`; and
  `wasmparser` 0.261.0 with `default-features = false` and `std`, `validate`,
  `features`, and `simd` (SIMD stays compiled in so that validation rejects it by
  its feature flag, which the baseline test then proves). With no optional engine
  feature, Wasmtime has no threads, garbage-collection, exception,
  function-reference, or reference-type proposal at all. The toolchain pin is
  1.96.1, the latest 1.96 patch release, and the workspace `rust-version` is 1.96.
- **Who depends on them.** `vibra-wasm-run` depends on `wasmtime` and `wasmparser`;
  `vibra-wasm` on `wasm-encoder`; and `vibra-wasm-run` has `wasm-encoder` as a
  development dependency for its hand-built test modules. Nothing else does.
- **Lockfile.** `Cargo.lock` gains 74 packages: the two new workspace crates and
  72 third-party packages, among them Wasmtime and its `wasmtime-internal-*` crates
  (49.0.2), the Cranelift crates (0.136.2), `regalloc2`, `gimli`, `object`, and
  `target-lexicon`. Wasmtime 49.0.2 itself depends on `wasm-encoder`, `wasmparser`,
  and `wasmprinter` 0.258.3, so the lock holds that version beside the 0.261.0 the
  workspace names; the choice of the latest 0.261.0 for the workspace's own use is
  Hannah's decision above, and it costs one extra compile of each of the two crates.
- **Licences.** Wasmtime, the Cranelift crates, and `target-lexicon` are
  Apache-2.0 WITH LLVM-exception; `wasm-encoder`, `wasmparser`, and `wasmprinter`
  are that licence OR Apache-2.0 OR MIT; the rest of the new packages are MIT OR
  Apache-2.0 or dual or permissive (`libm` and `generic-array` MIT, `foldhash`
  Zlib, `memchr`, `termcolor`, and `winapi-util` Unlicense OR MIT, `fnv`
  Apache-2.0 / MIT). None is copyleft, and each permits linking into a
  `MIT OR Apache-2.0` workspace. `cargo tree -p vibra-wasm-run --format "{p} {l}"`
  lists them.
- **Added build time.** On the maintainer's Windows machine, a clean
  `cargo test --locked --offline --workspace --all-targets --all-features --no-run`
  into a fresh target directory took 38 s at `8a04c08` on Rust 1.94.1 and 94 s at
  Step 4 on Rust 1.96.1, so the engine and the toolchain together add about 56 s
  to a clean build. The incremental cost is the crates that depend on the runner:
  `vibra-conformance` links Wasmtime, so a change to it relinks the engine.
- **Offline.** After one `cargo fetch --locked`, which needs the network (it exited
  0), and one `rustup toolchain install 1.96`, every command of the
  [validation list](validation.md#before-merging-each-step) ran with `--locked
  --offline` and passed, with no download.
- **Platforms.** The CI `check` job compiles and tests the workspace, the engine
  included, on Ubuntu, Windows, and macOS, and the `conformance` job runs the
  corpus in both backends on the same three. The Windows build is proven locally; the
  other two are proven by that CI run, and a failure of Wasmtime to build on one of
  them stops the step for a report and does not swap the engine.

## Contract gaps found while planning

The following were found while reviewing the specification against the M4
deliverables. None may be settled in implementation or tests. Step 1 closes
the Stage 4A items; Step 13 closes the Stage 4B items.

| ID | Gap | Why it blocks | Owner |
| --- | --- | --- | --- |
| G1 | The runtime chapter still says v1 defines no portable stack-depth limit and that exhausting the host stack is a host event outside parity, reported as `@runtime.host-stack-exhausted` and `@command.operational-failure`. The roadmap requires that rule replaced by either (a) a counted activation-depth limit both backends trap on identically, or (b) activations held in the arena. The interpreter's current bound is an implementation constant of 4,096 activations. | The choice decides how the Wasm backend lowers every non-tail call, whether it depends on the engine's stack, and which diagnostic, command result, and exit code three chapters name. It is a maintainer decision. | Closed by [D1.1–D1.4](decision-ledger.md); implemented by Steps 3 and 6 |
| G2 | The runtime chapter names an instance-owned value arena of opaque indices only in **WebAssembly boundary**. It does not say where compound values live, what an index denotes, which kinds exist, or how a host reads a compound value when no guest pointer crosses the boundary. Stage 4A allows no import, so the arena cannot be a set of host functions the module calls. | Every compound value in the Wasm backend depends on it, and the Stage 4B host ABI is designed against it. | Closed by [D2.1–D2.3](decision-ledger.md); implemented by Step 5a |
| G3 | The arena reclamation rule does not exist. The roadmap fixes its constraints: reclamation is unobservable apart from memory use, an index is never reused within an instance, and a tail-recursive loop that allocates per iteration holds a bounded live arena. Nothing states how wide an index is, what happens when the index space or the memory is exhausted, or that releasing a deeply nested value must not recurse on the engine stack. | The arena cannot be implemented first and given a rule later; the exit gate measures the live size. | Closed by [D3.1–D3.5](decision-ledger.md); implemented by Steps 5a and 6 |
| G4 | The conformance chapter lists "Wasm result and ordered audit trace where executable" but defines no operation, snapshot key, or reporting rule for it, and the closed operation list has only interpreter selectors. `wasm-v1` does not include `interpreter-v1`, so no declared profile runs both backends on one case. | The differential harness is a deliverable and the Stage 4A sub-gate; it needs a contract before it has a handler. | Closed by [D6.1–D6.4](decision-ledger.md); implemented by Step 4 |
| G5 | The tooling chapter does not say which backend `vibra run` and `vibra test` execute once Wasm exists. The roadmap reaches Wasm only through the conformance harness in M4. | Without a rule, a step could add a backend flag, or silently switch `run`, and the command contract would change by accident. Recommendation: both commands keep the reference interpreter in M4 and gain no option; M7 decides the shipped behavior. | Closed by [D7.1](decision-ledger.md) |
| G6 | The Wasm feature baseline is unstated: whether tail position lowers to the tail-call instructions or a trampoline, and which other proposals a v1 module may use. The workspace has no Wasm encoder or engine dependency, and the choice of engine bounds what G1 option (a) can prove about its stack. | Mandatory tail calls and the deep-recursion outcome are both lowered against it, and a dependency choice is a recorded milestone decision. It is a maintainer decision. | Closed by [D4.1–D4.4 and D5.1–D5.3](decision-ledger.md); the toolchain version is an open maintainer question |
| G7 | A native implementation must have "one source shared by the reference interpreter and the WebAssembly backend". Today each of the 19 natives is a Rust function in the interpreter's registry, which cannot be lowered into a module. The primitive rows have the same problem at a larger scale: float `to-str` and `parse`, integer `parse`, and the `char` conversions need code inside the module. | The roadmap deliverable says natives are lowered from that single source and that the body/native differential joins the interpreter/Wasm harness. The form of the source decides both. | Closed by [D10.1–D10.3](decision-ledger.md); implemented by Steps 8a, 8c, and 10 |
| G8 | The runtime chapter lets an implementation monomorphize or pass type arguments at run time, and the interpreter passes them. The typed IR cannot yet express an abstract contract member with its own generic parameters, which is why M3 reassigned it, with labelled operands, written `types:`, and a dict variadic tail on a contract member call. | The reassigned forms need one IR shape for both backends, and the Wasm instantiation strategy decides what that shape must carry. | Closed by [D9.1–D9.2](decision-ledger.md); implemented by Step 2, lowered in Step 9 |
| G9 | A trap has a stable code and a source origin when its span is known. Nothing says how an engine trap, such as an unreachable instruction or an out-of-bounds access, becomes a stable `@runtime.*` code, or how a Wasm trap recovers its source origin before M7's source maps exist. | The exit gate requires trap parity, and `@test.trap` records an origin. | Closed by [D8.1–D8.3 and D13.2](decision-ledger.md); implemented by Step 11 |
| G10 | WebAssembly leaves the payload of a NaN produced by an arithmetic instruction nondeterministic. The runtime chapter canonicalizes NaN in serialization and equality only. | A NaN that reaches `f64.to-str`, `compare-total`, a dict key order, or a result encoding must agree across backends. The rule must say where canonicalization happens. | Closed by [D11.1–D11.2](decision-ledger.md); implemented by Step 8c |
| G11 | The pre-M4 revision pinned, without fixing, that a local binder may be named `if`, `i32`, `let-else`, `return`, or `never`, which the specification forbids. An interpreter test over a value nested 5,000 levels deep took about seven minutes in a debug build, with the cause not investigated. | Neither is an M4 form, but the first would be hardened by a second backend and the second distorts the performance baseline. Each needs an owning step or an explicit reassignment. | Assigned by [D14.1–D14.2](decision-ledger.md): the binder fix to Step 2, the nested-value cost to Step 21 |
| G12 | The host registry has no rows. The effects chapter reserves ten roots in five modules and shows two example operations; no closed symbol list, signature, or error type exists, and `path` and `fs-error` are named but never declared. "Whole-value" console input is not defined. | Steps 16–18 implement the registry and need every row before code, as M3's library steps did. | Step 13 |
| G13 | An audit event is an opaque string in `@audit-trace.v1` and in the CLI's `auditTrace`. The roadmap requires one versioned event encoding shared by both backends that tolerates new event kinds, and a deterministic event shape per registry entry. | No host operation can be implemented without its event, and the demo gate compares traces byte for byte. | Step 13 |
| G14 | There is no rule for whether adding a registry entry is a minor registry version, or how a build records the registry version it requires. `vibra_v1` is the only identity. | It is a named deliverable and a forward-compatibility obligation; the Wasm import module name depends on it. | Step 13 |
| G15 | Tests "use deterministic providers by default" and may be given "recorded responses", but no source form, project field, or harness input supplies them. The sentence "an unconsumed failure or unrecorded dependency on a nondeterministic provider fails the test" names no result atom. A corpus case has no input kind for host responses, standard input, a filesystem image, an environment, a clock, or random bytes. | Effectful tests and every Stage 4B corpus case depend on it. | Step 13 |
| G16 | [M3's G21](../milestone-3/README.md#contract-gaps-found-while-planning): an entry that returns `err` ends `run` with `@command.ok`. M4 adds host failures at the entry, which need the same decision: a result atom and an exit code. | Reassigned to M4 by the M3 exit evidence. | Step 13; implemented by Step 19 |
| G17 | Several effect conditions have no diagnostic: an unknown or repeated root in a target's `effects` array ("project errors"), a binary target with no `effects` field, a `@host` declaration outside a `deffect`, a `@host` operation with an additive row, a registry entry whose owner is not the enclosing root, and `effects:` written on a `deffect`. The call-witness shape that `@effect.outside-ceiling` must report is not defined, and neither is whether `@contract.unused-effect` applies to a target array. | The exit gate names the witness, and each rejection needs a stable code, level, and span before its case is written. | Step 13 |
| G18 | A performed row is computed over the function-call graph, but a call through a function value or an interface value has no statically known callee. The row such a call contributes, from the function type's row or the contract ceiling, is implied and not stated. | M3 made an unknown call target stand for every escaping function; effects need the stated rule instead, or the performed row is not least. | Step 13 |
| G19 | Effect rows in typed IR, query schemas, and build metadata must not assume a row is a closed literal set. No representation rule exists. | It is a forward-compatibility obligation that is cheap now and breaking later. | Step 13 |
| G20 | `never` has no terminating source in v1 other than `return`. The pre-M4 revision left a host `exit` or a pure intrinsic as an M4 decision; the charter excludes processes. | Decide once, so the standard effect inventory is closed. Recommendation: add none. | Step 13 |
| G21 | `@stdlib-manifest.v1` is a closed record with `compiler`, `native`, and `assertions` lists and no list for `@host` symbols, and the embedded module set has no `io`, `fs`, `env`, `time`, or `random` module. | Host declarations need the same closed authority as `@compiler` ones. | Step 13 |
| G22 | The position envelope and `@index.v1` have no fields for performed, enclosing, and target effect rows, effect-operation witnesses, or effect roots and operations as indexed entities. `V1-TOOL-index-unavailable` pins their absence. | Steps 14 and 20 emit them. | Step 13 |
| G23 | The performance baseline has no definition: which programs, which measures, where the numbers are recorded, and whether CI checks them. | M7 compares against it, so it must be reproducible. | Step 13 |

## Steps

Stage 4A — the WebAssembly spine, with no `@host` import (only pure native imports). Stage demo: the M3 demo
library's tests produce identical results in both backends, including a deep
tail-recursive walk that grows neither stack nor arena.

| Step | One-PR slice | Requires | Status | PR / merge evidence |
| --- | --- | --- | --- | --- |
| 1 | [Freeze Stage 4A contracts](01-contracts.md) — specification/infrastructure prerequisite | M3 on `main`; this bootstrap | landed | PR #354, merge e7d2e08 |
| 2 | [Contract-member forms reassigned from M3, and the binder defect](02-contract-members.md): an abstract contract member with its own generic parameters, labelled operands and written `types:` arguments on a contract member call, a dict variadic tail on a contract member, and such a member as a function value, in typed IR and the interpreter (per G8), plus the reserved-binder fix (per G11) | 1 | landed | PR #355, merge 3289ee75c68f4d68419597e0e7dfecd28c64c742 |
| 2b | [Boolean constants and constant patterns](02b-boolean-constants.md): `true` and `false` become `@std.bool` values of the prelude with no source boolean literal, a pattern name that resolves to a constant module `def` is a value pattern, and every prelude name is reserved at a binder (per ledger D14.1 and D16) | 2 | landed | PR #357, merge 388dfe1 |
| 3 | [The specified outcome of deep non-tail recursion](03-activations.md) in the reference interpreter, replacing the host-event rule: heap activations, a memory budget, and `@runtime.memory-exhausted` as the host event, with `expect.host_event` (per G1) | 2 | landed | PR #356, merge 8a04c08 |
| 4 | [Wasm backend skeleton and differential harness](04-skeleton.md): the emitter crate `vibra-wasm`, the runner crate `vibra-wasm-run` with Wasmtime 49.0.2 (the latest; Cranelift only) behind it, `wasm-encoder` and `wasmparser` 0.261.0, the toolchain raised to Rust 1.96.1, a module that exports the six accessors that depend on no value kind and no test (the rest of the export table arrives in 5a and 11), the corpus contract of G4, the parity inventory and its test, deterministic emission, and a CI job — infrastructure step | 3 | landed | PR #358, merge 704b7ad |
| 5a | [The value arena and its runtime](05a-arena.md): linear-memory arena, reference counting, the handle table, the exported accessors that Step 4 left out (`vibra_v1_release`, `vibra_v1_variant`, `vibra_v1_length`, `vibra_v1_read_i32`, `vibra_v1_read_i64`, `vibra_v1_read_f32`, `vibra_v1_read_f64`, and `vibra_v1_read_id`), memory exhaustion, scalars and literals, and the canonical result observation | 4 | landed | PR #359, merge fc374df |
| 5b | [Data and core lowering](05b-core-lowering.md): declared and anonymous records, enums, tuples, wrappers, and unions with discriminants in written order, projection, module values, `let`, body sequences, `if`, `return`, and direct calls | 5a | landed, PR #360, merge fb7e1e1 | PR #360 |
| 6 | [Calls](06-calls.md): generic instantiation, function values, closures, indirect calls, a tail call to every kind of callee, the deep non-tail recursion outcome, and a bounded live arena across a long allocating tail loop | 5b | landed, PR #361, merge e86d93e | PR #361 |
| 7 | [Patterns and typed failure](07-patterns-failure.md): `match` with every pattern kind, destructuring bindings, `let-else`, `as` narrowing, `try`, and `never` | 6 | landed, conditional on its PR merging | Branch `claude/m4-step-07-patterns-failure`; PR and merge commit to be recorded when the PR merges |
| 8a | [Integer and `char` primitive rows](08a-integer-primitives.md), lowered before emission and held to shared sample vectors | 7 | not started | — |
| 8b | [Collections, text, bytes, and dict](08b-collections.md): arrays, variadic tails, checked lookups, the `array.*` and `dict.*` rows, and `str`, `bytes`, and `dict` through their standard-library bodies | 8a | not started | — |
| 8c | [Number text, floats, and NaN](08c-number-text-floats.md): the integer and float `to-str` and `parse` natives, written once in Rust and called by both backends, the float arithmetic rows, and NaN canonicalization | 8b | not started | — |
| 9 | [Interfaces](09-interfaces.md): static dispatch, interface values, default members, destination dispatch and conversion, `iter` with its adapters, and the Step 2 forms | 8c | not started | — |
| 10 | [Natives and the joined differential](10-natives.md): the native import module `vibra_native_v1`, the single Rust source of G7, and the body/native differential joined to the interpreter/Wasm harness | 9 | not started | — |
| 11 | [Tests and traps in the Wasm backend](11-tests-traps.md): `workspace-test` observations, the `vibra_v1_test` export and the `vibra_v1_failure`, `vibra_v1_failure_expected`, and `vibra_v1_failure_actual` exports, assertion outcomes, and every trap code with its origin | 10 | not started | — |
| 12 | [Stage 4A demo and corpus sub-gate](12-stage-4a-evidence.md) — evidence step | 11 | not started | — |

Stage 4B — static effects and host operations, each in both backends.

| Step | One-PR slice | Requires | Status | PR / merge evidence |
| --- | --- | --- | --- | --- |
| 13 | Freeze Stage 4B contracts: the host registry, audit-event encoding, registry versioning, test providers, entry outcomes, effect diagnostics, and effect metadata — specification prerequisite | 12 | not started | — |
| 14 | Effect declarations and rows: `deffect`, operations with Vibra bodies and additive rows, row resolution, function-type rows, transitively closed performed rows, `@effect.outside-ceiling` with call witnesses, `@effect.invalid-reference`, `@contract.unused-effect`, and interface contract ceilings | 13 | not started | — |
| 15 | Target consent and effectful tests: the required `effects` array of a binary target, static admission of the resolved entry, effectful test ceilings, and the `project init` template | 14 | not started | — |
| 16 | The host boundary and console: typed `@host` externals, the closed `vibra_v1` host registry, the scalar-only ABI over the arena, injected providers, the audit-event encoding, the `io` roots, and an effectful walk over `iter.next` | 15 | not started | — |
| 17 | Filesystem operations under `fs.read`, `fs.write`, and `fs.metadata`, with typed host errors | 16 | not started | — |
| 18 | Environment reads, clocks, and random bytes, with deterministic test providers | 17 | not started | — |
| 19 | Entry outcomes: the command result and exit code for an entry that returns `err` and for a host failure at the entry (per G16), and the parity sweep over success, typed host error, propagation, and trap | 18 | not started | — |
| 20 | Effect metadata: performed, enclosing, and target rows at a source position, effect-operation witnesses, and index records for effect roots and operations | 19 | not started | — |
| 21 | Interpreter and unoptimized-Wasm performance baseline on the conformance and demo programs — evidence step | 20 | not started | — |
| 22 | M4 demo and exit gate, including the M3 deferral sweep — evidence step | 21 | not started | — |

Steps 1 and 13 are specification prerequisites, Step 4 is an infrastructure
step, and Steps 12, 21, and 22 are evidence steps; they claim no language
behavior. Step 3 claims its behavior for the interpreter only; the Wasm
backend meets the same outcome in Step 6. Guides for Steps 2–12 are written in
Step 1 and guides for Steps 14–22 in Step 13, because their content depends on
the contracts those steps close. Step 1 also records the decision ledger, the
M4 surface inventory, and M4 validation. No step starts without its guide.

Step 1 split two planned steps, and the split is recorded here, in the guides, and
in the ledger. Step 5 became 5a and 5b: the arena, its reference counting, the
handle table, and the host accessors are one self-contained contract with its own
proof (bounded live size, bounded-stack release, exhaustion), and carrying every
compound form with it would give one change that cannot be reviewed against that
proof. Step 8 became 8a, 8b, and 8c: its primitive registry is about two hundred
rows, the collections need the integer rows, and number text and floats are their
own body of work. The order of 8a-8c follows the dependencies still: collections
use `u64` arithmetic, and the number-text natives build `str` values, so they
come after the collection layout.

The slices above are the planned decomposition, not a promise that each fits
one PR. A step that proves too large is split before delivery, as M3 split
Steps 4, 8, 11, 14, and 15, with the split recorded in this table, the guides,
and the ledger.

## Deliverable and gate coverage

| Roadmap obligation | Owning steps |
| --- | --- |
| Unoptimized Wasm lowering of the complete pure language: mandatory tail calls, union discriminants in written order, checked lookups, `@compiler` externals lowered before emission | 4–10 (5a–8c); NaN and number text in 8c |
| Native implementations lowered from the single source; body/native differential joined to the interpreter/Wasm harness | 1, 8a, 8c, 10 |
| Instance-owned value arena and opaque-index discipline; no index or instance identity in typed IR or build output | 1, 5a, 16 |
| Specified arena reclamation rule; bounded host memory for a long tail-recursive program | 1, 5a, 6 |
| Interpreter/Wasm differential harness over the entire executable corpus, through the conformance harness | 1, 4, 11, 12 |
| Native `deffect` operations, default-empty ceilings, transitively closed performed rows, imported-symbol effect references | 14 |
| Effect metadata in general queries; source-position metadata for performed, enclosing, and target rows | 20 |
| Required binary-target effect arrays and static admission of the entry | 15 |
| Typed `@host` externals and the closed `vibra_v1` registry in both backends; the registry versioning rule | 13, 16 |
| Whole-value console, filesystem, environment-read, clock, and random operations | 16–18 |
| Injected deterministic test providers and ordered audit traces; one versioned audit-event encoding | 13, 16, 18 |
| Effect rows in typed IR, query schemas, and build metadata that do not assume a closed literal set | 13, 14, 20 |
| The forms M3 reassigned: the three contract-member forms, and the result of an entry that returns `err` | 2, 9, 19 |
| A specified outcome for deep non-tail recursion, written before the Wasm backend | 1, 3, 6 |
| Effectful iteration examples that walk with `iter.next` and tail recursion | 16 |
| Recorded interpreter and unoptimized-Wasm performance baseline | 21 |
| Stage demo: the M3 demo library's tests agree in both backends; a deep tail-recursive walk grows neither stack nor arena | 12 |
| Demo gate: a target declaring `@std.fs.read` reads, transforms, and writes; the same source is rejected when a ceiling omits a root; byte-identical stdout and audit traces in both backends | 22 |
| Exit: an additive root reaches the performed row, the rejection, and the target array of every transitive caller; the witness is named for each missing root | 14, 15 |
| Exit: every host operation has one registry entry, owner effect, typed signature, and audit-event shape | 13, 16–18 |
| Exit: no compiler-generated ambient host read exists outside the registry | 16–18; audited by 22 |
| Exit: success, typed host error, propagation, and trap have interpreter/Wasm parity | 11, 19 |
| Exit: deep non-tail recursion has the same specified outcome in both backends | 3, 6 |
| Exit: a tail-recursive loop allocating a fresh compound value per iteration holds a bounded live arena in the Wasm backend | 6 |
| Exit: interpreter-v1 conformance passes for the full language, and the Wasm backend matches it on every executable case | every behavior step; swept by 12 and 22 |

## M3 deferral inventory

The M3 [surface inventory](../milestone-3/supported-surface.md) leaves three
rows to M4: `Declaration::Deffect`, nonempty `Attribute::Effects`, and
`Attribute::External` with `@host`. The M3
[exit evidence](../milestone-3/exit-evidence.md#reassigned-to-later-milestones)
reassigns four more forms, and the
[pre-M4 checklist](../pre-m4/01-bindings-return-never.md) defers its row C16,
the effect ceiling of a `let` value, a `let-else` fallback, and a `return`
operand, to the effect step.

Step 1 turns them into one M4 inventory that gives each AST variant and each
inherited row an owning step, and adds a test that fails when a variant has no
disposition, as M3's inventory test does. Ordinary dependency delivery stays
M5's and the public `query` command stays M6's.

| Inherited row | Owner |
| --- | --- |
| `Declaration::Deffect`; nonempty `Attribute::Effects`, including on a function type and an `iter` default callback | Step 14 |
| `Attribute::External` with `@host` | Step 16 |
| An abstract contract member with its own generic parameters | Step 2; Wasm in Step 9 |
| Labelled operands and written `types:` arguments on a contract member call | Step 2; Wasm in Step 9 |
| A dict variadic tail on a contract member | Step 2; Wasm in Step 9 |
| A contract member with its own generics or labelled parameters as a function value | Step 2; Wasm in Step 9 |
| M3's G21 (G16 here): the command result for an entry that returns `err` | Step 19 |
| Pre-M4 C16: effect rows of a `let` value, a `let-else` fallback, and a `return` operand | Step 14 |
| The conformance chapter's effectful walk over `iter.next` | Step 16 |
