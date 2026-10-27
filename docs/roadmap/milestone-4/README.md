# Milestone 4 step plan

Status: in progress. Stage 4A, the WebAssembly spine, is complete on `m4`: Steps 1
to 11 have merged, and Step 12 records the [Stage 4A evidence](stage-4a-evidence.md)
(landed, PR #370, merge d567003). Stage 4B, static
effects and host operations (Steps 13 to 22), has its contracts frozen by
[Step 13](13-stage-4b-contracts.md) (landed, PR #371, merge 437e928), and
its implementation has begun: [Step 14](14-effect-rows.md) (landed, PR #372,
merge fbe53c1) and [Step 15](15-target-consent.md) (landed, PR #373, merge
f14aa66) have merged, [Step 16a](16a-host-infrastructure.md) (landed, PR #374, merge
746e3d3) has merged, [Step 16b-1](16b-host-console.md) (the host-call plumbing in
both backends) has landed (PR #375, merge 0fa74b0),
[Step 16b-2](16b-host-console.md#16b-2-the-io-operations) (the `io` operations in both
backends) has landed (PR #376), [Step 17](17-filesystem.md) (the filesystem
operations in both backends) has landed on its branch, conditional on its pull
request merging, [Step 18](18-env-clock-random.md) (the environment, clock, random, and program-argument operations) has landed on its branch, conditional on its pull request merging, and Steps 19 to 22 have not started.

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

- **One provider trait, two providers, one world (D20, Stage 4B).** A run is
  given a host provider through one trait. The system provider (proposed crate
  `vibra-host`) serves `run`, and the in-memory host world serves `test` and the
  conformance harness. There is no mode switch in the language or in the IR, and
  the world is a VIBON document (`@host-world.v1`) that a case supplies. The
  provider crate is the only crate through which a running program reaches the
  ambient filesystem, environment, clock, or random source. A host test scans the
  crates that check, lower, or execute a program for those APIs and names the
  tooling crates that read a project, a command line, or a corpus (D31.1).
- **Each host operation is written once (D30.2, Stage 4B).** The operation is
  Rust behind a value-access trait. The interpreter implements the trait over its
  values, and the Wasm runner through the exported accessors and builders, never
  through `vibra_v1_memory`. The audit event is built by that one code, so a trace
  cannot differ between backends. The registry table of 17 rows lives in
  `vibra-ir` beside the compiler registry (a checked-in snapshot guards it), and
  the emitter depends on its symbols and signatures only.
- **Rows are records before they are anything else (D25).** `EffectRow { roots,
  tail: None }` is the IR shape from Step 14, so a row variable is one more
  field. Nothing assumes a closed set of roots.

These are implementation constraints, not language rules. Observable decisions
are closed in the owning specification by Step 1 (Stage 4A) or Step 13
(Stage 4B) before dependent code.

## Performance baseline

Step 13 fixes the baseline's definition (ledger D29.1, gap G23), and Step 21
records it. M7 compares against it, so it must be reproducible.

The baseline is a named list of about ten programs, kept in the corpus or in
`examples/` and named in `milestone-4/performance-baseline.vibon`:

1. the Stage 4A deep tail-recursive walk;
2. the M3 demo library's tests;
3. non-tail recursion to depth 10^5;
4. a tail loop of 10^6 iterations;
5. `array.fold`;
6. dict insertion;
7. text concatenation;
8. float `to-str`;
9. the value nested 5,000 levels deep (build, compare, release; this also settles
   the debug-build cost of ledger D14.2); and
10. the Stage 4B effectful walk over `iter.next`, in a host world.

For each program and each backend the file records **exact metrics**: module
bytes (Wasm only), the allocation count, the peak live arena size
(`vibra_v1_live_size`, Wasm only; the interpreter's own measure beside it), and
fuel, which is Wasmtime fuel for a module (metering is enabled by the measuring
runner only) and the reduction count of the interpreter. A CI host test
regenerates the exact metrics and fails on any difference from the file, so a
change that moves one is reviewed as a baseline change. **Wall-time medians** over
a stated number of runs are recorded as information beside them and never fail a
build. The file is `@perf-baseline.v1` VIBON data, written by a measuring tool and
never by hand.

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
deliverables. None may be settled in implementation or tests. Step 1 closed
the Stage 4A items (G1–G11), and Step 13 closed the Stage 4B items (G12–G24),
each by the ledger rows its last column names.

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
| G12 | The host registry has no rows. The effects chapter reserves ten roots in five modules and shows two example operations; no closed symbol list, signature, or error type exists, and `path` and `fs-error` are named but never declared. "Whole-value" console input is not defined. | Steps 16–18 implement the registry and need every row before code, as M3's library steps did. | Closed by [D17.1–D17.6](decision-ledger.md#stage-4b-contracts-step-13); implemented by Steps 16–18 |
| G13 | An audit event is an opaque string in `@audit-trace.v1` and in the CLI's `auditTrace`. The roadmap requires one versioned event encoding shared by both backends that tolerates new event kinds, and a deterministic event shape per registry entry. | No host operation can be implemented without its event, and the demo gate compares traces byte for byte. | Closed by [D18.1–D18.3](decision-ledger.md#stage-4b-contracts-step-13); implemented by Step 16 |
| G14 | There is no rule for whether adding a registry entry is a minor registry version, or how a build records the registry version it requires. `vibra_v1` is the only identity. | It is a named deliverable and a forward-compatibility obligation; the Wasm import module name depends on it. | Closed by [D19.1–D19.2](decision-ledger.md#stage-4b-contracts-step-13); implemented by Step 16 (the snapshot test) and M7 (the build record) |
| G15 | Tests "use deterministic providers by default" and may be given "recorded responses", but no source form, project field, or harness input supplies them. The sentence "an unconsumed failure or unrecorded dependency on a nondeterministic provider fails the test" names no result atom. A corpus case has no input kind for host responses, standard input, a filesystem image, an environment, a clock, or random bytes. | Effectful tests and every Stage 4B corpus case depend on it. | Closed by [D20.1–D20.4](decision-ledger.md#stage-4b-contracts-step-13); implemented by Steps 16–18 |
| G16 | [M3's G21](../milestone-3/README.md#contract-gaps-found-while-planning): an entry that returns `err` ends `run` with `@command.ok`. M4 adds host failures at the entry, which need the same decision: a result atom and an exit code. | Reassigned to M4 by the M3 exit evidence. | Closed by [D21.1](decision-ledger.md#stage-4b-contracts-step-13); implemented by Step 19 |
| G17 | Several effect conditions have no diagnostic: an unknown or repeated root in a target's `effects` array ("project errors"), a binary target with no `effects` field, a `@host` declaration outside a `deffect`, a `@host` operation with an additive row, a registry entry whose owner is not the enclosing root, and `effects:` written on a `deffect`. The call-witness shape that `@effect.outside-ceiling` must report is not defined, and neither is whether `@contract.unused-effect` applies to a target array. | The exit gate names the witness, and each rejection needs a stable code, level, and span before its case is written. | Closed by [D23.1–D23.2](decision-ledger.md#stage-4b-contracts-step-13); implemented by Steps 14, 15, and 16 |
| G18 | A performed row is computed over the function-call graph, but a call through a function value or an interface value has no statically known callee. The row such a call contributes, from the function type's row or the contract ceiling, is implied and not stated. | M3 made an unknown call target stand for every escaping function; effects need the stated rule instead, or the performed row is not least. | Closed by [D24.1](decision-ledger.md#stage-4b-contracts-step-13); implemented by Step 14 |
| G19 | Effect rows in typed IR, query schemas, and build metadata must not assume a row is a closed literal set. No representation rule exists. | It is a forward-compatibility obligation that is cheap now and breaking later. | Closed by [D25.1](decision-ledger.md#stage-4b-contracts-step-13); implemented by Steps 14 and 20 |
| G20 | `never` has no terminating source in v1 other than `return`. The pre-M4 revision left a host `exit` or a pure intrinsic as an M4 decision; the charter excludes processes. | Decide once, so the standard effect inventory is closed. Recommendation: add none. | Closed by [D26.1](decision-ledger.md#stage-4b-contracts-step-13); no step adds a terminating source |
| G21 | `@stdlib-manifest.v1` is a closed record with `compiler`, `native`, and `assertions` lists and no list for `@host` symbols, and the embedded module set has no `io`, `fs`, `env`, `time`, or `random` module. | Host declarations need the same closed authority as `@compiler` ones. | Closed by [D27.1](decision-ledger.md#stage-4b-contracts-step-13); implemented by Steps 16–18 |
| G22 | The position envelope and `@index.v1` have no fields for performed, enclosing, and target effect rows, effect-operation witnesses, or effect roots and operations as indexed entities. `V1-TOOL-index-unavailable` pins their absence in the M3 form, which Step 20 replaces. | Steps 14 and 20 emit them. | Closed by [D28.1](decision-ledger.md#stage-4b-contracts-step-13); implemented by Step 20 |
| G23 | The performance baseline has no definition: which programs, which measures, where the numbers are recorded, and whether CI checks them. | M7 compares against it, so it must be reproducible. | Closed by [D29.1](decision-ledger.md#stage-4b-contracts-step-13); implemented by Step 21 |
| G24 | Found while closing G12 and the demo gate: no host operation can receive program arguments and the CLI has no way to pass them, but the demo gate reads "a path chosen at runtime". | The registry's `env.read.args` and the command grammar of the tooling chapter must agree, and a closed grammar cannot grow in an implementation step. | Closed by [D22.1](decision-ledger.md#stage-4b-contracts-step-13); implemented by Step 18 |

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
| 7 | [Patterns and typed failure](07-patterns-failure.md): `match` with every pattern kind, destructuring bindings, `let-else`, `as` narrowing, `try`, and `never` | 6 | landed, PR #362, merge cc7340823bc5d261cabe0d06a384c41a6ea9d169 | PR #362 |
| 8a | [Integer and `char` primitive rows](08a-integer-primitives.md), lowered before emission and held to shared sample vectors | 7 | landed, PR #363, merge a450155 | PR #363 |
| 8b | [Collections, text, bytes, and dict](08b-collections.md): arrays, variadic tails, checked lookups, the `array.*` and `dict.*` rows, and `str`, `bytes`, and `dict` through their standard-library bodies | 8a | landed, PR #364, merge 705d297e6c4bf7ccf261cba0166d66ed79158c00. It does not deliver the module-run body of `array.fold`, so `V1-RUNTIME-library-roles` is not lowered in it; Step 8d owns both (ledger D10.5) | PR #364 |
| 8c | [Number text, floats, and NaN](08c-number-text-floats.md): the integer and float `to-str` and `parse` natives, written once in Rust and called by both backends, the float arithmetic rows, and NaN canonicalization | 8b | landed, PR #365, merge d51cc34. 11,539 shared vectors, 20 new native imports, 14 inline float rows; `V1-RUNTIME-registry-floats` and `V1-RUNTIME-registry-integers` are matched | PR #365 |
| 8d | [A native's checked body in typed IR](08d-native-bodies.md), so a module runs the body of a native that applies a function value (`array.fold`), which the typed IR does not carry today (ledger D10.5): the checker checks the body of a builtin member in both paths, and the IR holds it beside the wrapper the interpreter runs | 8b | landed, PR #366, merge 17fe1ea093ca5edf109a9b82f5e716c65d128415. `array.fold` carries one checked body, which a module runs; `V1-RUNTIME-library-roles` is matched | PR #366 |
| 9 | [Interfaces](09-interfaces.md): static dispatch, interface values, default members, destination dispatch and conversion, `iter` with its adapters, and the Step 2 forms | 8c, 8d | landed, PR #367, merge c1217c13c46d24f4eb7a6fa4fd708b80bf8224ca. Run-time dispatch from type descriptors, interface values as pairs, the closed conformances, the key unit, and `iter`; all 22 owned rows are matched | PR #367 |
| 10 | [Natives and the joined differential](10-natives.md): the native import module `vibra_native_v1`, the single Rust source of G7, and the body/native differential joined to the interpreter/Wasm harness | 9 | landed, PR #368, merge 20db8c624032235a94ee88c0b28f149f1d3eb397. The single-source audit as a test (three interpreter copies of native algorithms removed), the manifest and import-table checks, 112 four-path samples of the 19 natives plus the looping rows and `iter.next` in both backends, and the native defect path end to end | PR #368 |
| 11 | [Tests and traps in the Wasm backend](11-tests-traps.md): `workspace-test` observations, the `vibra_v1_test` export and the `vibra_v1_failure`, `vibra_v1_failure_expected`, and `vibra_v1_failure_actual` exports, assertion outcomes, and every trap code with its origin | 10 | landed, PR #369, merge 1f55ad6658923f0c63df1af2277c640fdaf3292d. The test module and `vibra_v1_test`, the three assertions and their failure exports, canonical equality by one iterative walk of the arena, the origin table, trap and origin parity for every registered code, and the three forms Step 9 left unlowered; the Wasm backend reports 0 not-lowered rows (130 of 130 cases matched) | PR #369 |
| 12 | [Stage 4A demo and corpus sub-gate](12-stage-4a-evidence.md) — evidence step | 11 | landed, PR #370, merge d567003e5a0b428812225d60b3c25f2ba64b4a95. The [Stage 4A evidence](stage-4a-evidence.md): the demo in both backends with a deep tail-recursive walk, the corpus sub-gate (131 of 131 executable cases matched, 0 not lowered), and the exit-clause map; two contract fixes (`expect.host_event` on `workspace-test`, the tail-call wording for widened results) | PR #370 |

Stage 4B — static effects and host operations, each in both backends.

| Step | One-PR slice | Requires | Status | PR / merge evidence |
| --- | --- | --- | --- | --- |
| 13 | [Freeze Stage 4B contracts](13-stage-4b-contracts.md): the host registry, audit-event encoding, registry versioning, the test host world, entry outcomes, effect diagnostics, and effect metadata — specification prerequisite | 12 | landed, PR #371, merge 437e928. The 17-row `vibra_v1` registry and its closed error enums, the `@audit-trace.v1` event record, registry versioning, the `@host-world.v1` test world, program arguments, `@command.program-failed`, the witness shape, indirect-call and row rules, the manifest `host` list, the effect fields of the envelope and the index, the performance baseline, and the Wasm `@host` ABI; G12–G24 closed by [ledger D17–D32](decision-ledger.md#stage-4b-contracts-step-13); two registry codes, the `@command.program-failed` atom, the event-record and trap-code schema changes; guides for Steps 14–22 | PR #371, branch `claude/m4-step-13-stage-4b-contracts` |
| 14 | [Effect declarations and rows](14-effect-rows.md): `deffect`, operations with Vibra bodies and additive rows, row resolution, function-type rows, the row record in typed IR and the canonical `fn` encoding, transitively closed performed rows, indirect-call rows, `@effect.outside-ceiling` with call witnesses, `@effect.invalid-reference`, `@contract.unused-effect`, `effects:` on a `deffect`, and interface contract ceilings | 13 | landed, PR #372, merge fbe53c1. Rows are a record in typed IR and the canonical `fn` encoding, resolved in both checkers, closed over the function-call graph in `vibra-types`, and checked with the witness; the formatter orders a row by its roots; both backends run effect operations as ordinary functions (30 static, 4 executable `matched`, 1 rejected, and 1 tooling cases added; 509 of 509 passed, 135 of 135 matched) | PR #372, branch `claude/m4-step-14-effect-rows`, based on `origin/m4` at `437e928` (PR #371 merged) |
| 15 | [Target consent and effectful tests](15-target-consent.md): the required `effects` array of a binary target, its resolution diagnostics, static admission of the resolved entry, effectful test ceilings, and the `project init` template | 14 | landed, PR #373, merge f14aa66. Each atom of a binary target's `effects` array resolves to one canonical effect root in `vibra-resolve`, beside the entry, with the ordinary resolution codes and `@effect.duplicate-root`; `vibra-types` admits the entry's performed row against the array before `check`, `test`, `run`, or `build` accepts the target (`@effect.outside-ceiling` at the `entry:` value with the witness as related spans, and `@contract.unused-effect` at an unused atom); a test is consent to its own row; the formatter writes the array in canonical order; the `project init` template and its chapter keep `effects: (array)` (16 cases added, none re-expected; 525 of 525 passed, 135 of 135 matched) | PR #373, branch `claude/m4-step-15-target-consent`, merged into `m4` at `f14aa66` |
| 16a | [Host provider infrastructure](16a-host-infrastructure.md) — infrastructure step: the provider trait and crate, the system provider, the in-memory host world with its `@host-world.v1` decoder and defaults, the registry table of 17 rows with its checked-in snapshot, the audit-event codec that preserves unknown kinds, the manifest `host` list and the `0.3.0` package, the corpus `host-world` input, and the ambient-read test | 15 | landed, PR #374, merge 746e3d30fd20e6e440142a0123b91451ceab9553. The new crate `vibra-host` holds the `HostProvider` trait (a method for each of the 17 rows, answering `Done<T, E>` so that memory exhaustion is the host event and never a registry error), the `HostValues` value-access trait with the typed `HostCall`, `perform`, and `build_reply`, the `@host-world.v1` decoder and `WorldProvider` (SplitMix64 vectors for seeds `0` and `1234567`), the `SystemProvider`, and the `AuditEvent` with its VIBON and JSON codecs and the trace record. The 17-row table and the checked-in snapshot sit in `vibra-ir` (`external::host`, `crates/vibra-ir/host-registry.snapshot`); the manifest gains the `host` list, checked at load for symbol, owner, and signature, and the package is `0.3.0`; a `workspace-run` or `workspace-test` case may name a `host-world` document, rejected with the data diagnostics at the document; `ambient_reads_m4_step16` names the tooling crates. No operation runs and no `@host` external is accepted (no case added; 525 of 525 passed, 135 of 135 matched) | Branch `claude/m4-step-16a-host-infrastructure`, based on `origin/m4` at `f14aa66` (PR #373 merged) | PR #374 |
| 16b-1 | [The host boundary](16b-host-console.md#16b-1-the-host-call-plumbing): the typed IR call target, the scalar-only ABI over the arena (the `vibra_v1` imports, the seven builders, and the adoption of a result), both backends' implementations of the value-access trait, the audit events in both backends, the world and system providers wired into `run`, `test`, and the conformance handlers (the system provider's output sink chosen by the command), and the `auditTrace` objects; proved with hand-built IR. No source can call an operation: `@host` stays `@tool.unavailable` | 16a | landed, PR #375, merge 0fa74b027456740a23ae78240d421e38e53e0e13. 19 builder and ownership tests, 7 two-backend host-call tests, and the single-source audit (no operation, no provider call, no event built outside `vibra-host`; the runner's host path never touches `vibra_v1_memory`). The corpus is unchanged (525 passed, 135 matched) | Branch `claude/m4-step-16b-host-console`, based on `origin/m4` at `746e3d3` (PR #374 merged) | PR #375 |
| 16b-2 | [The `io` operations](16b-host-console.md#16b-2-the-io-operations): `external: @host` checked against the registry (symbol, owner, signature) with its diagnostics, `@std.io` and the six manifest symbols, the effectful walk over `iter.next`, and the corpus cases of the brief, `matched` in both backends | 16b-1 | landed, PR #376, merge 1a175ba. `@host` is checked against the registry for its symbol, owner, and signature, each reported on its own; `@std.io` binds the six rows; the parser reports the invalid-attribute cases at the attribute. 7 executable cases (write text and bytes, whole stdin as text and as bytes read twice, `invalid-text`, a host error propagated with `try`, a test over the world's stdin, and the effectful `iter.next` walk), all matched in both backends, and 4 negatives; 4 cases and 5 tests that asserted `@host` unavailable are re-expected. The corpus is 536 cases (83 reader, 287 static, 150 interpreter, 16 tooling); the Wasm backend matches 142 of 142 | Branch `claude/m4-step-16b2-host-io`, based on `origin/m4` at `0fa74b0` (PR #375 merged) | PR #376 |
| 17 | [Filesystem operations](17-filesystem.md) under `fs.read`, `fs.write`, and `fs.metadata`, with `path` and the typed host errors | 16b | landed, PR #377, merge 0551f21. `@std.fs` binds the six rows (`path`, `fs-error`, `fs-kind`, `fs-metadata`, and the roots `read`, `write`, and `metadata`); one implementation of each operation in `vibra-host` serves the system provider and the world provider in both backends. 9 executable cases (all matched in both backends) and 3 negatives; the system provider maps a path below a file to `not-a-directory` on every platform. The corpus is 548 cases (83 reader, 290 static, 159 interpreter, 16 tooling); the Wasm backend matches 151 of 151 | Branch `claude/m4-step-17-filesystem`, based on `origin/m4` at `1a175ba` (PR #376 merged) | PR #377 |
| 18 | [Environment reads, clocks, random bytes, and program arguments](18-env-clock-random.md): `env.read`, `time.now`, `random.generate`, `vibra run TARGET -- ARG...`, and the world's environment, clock, and generator | 17 | landed, conditional on its pull request merging. `@std.env`, `@std.time`, and `@std.random` bind the five rows; the manifest lists the five symbols (the package stays `0.3.0`). `vibra run TARGET [-- ARG...]` passes the arguments after the first `--` to the system provider verbatim; the closed grammar, the help text, and the tooling chapter agree. 11 executable cases (all matched in both backends) and 3 negatives (a target array that omits each root). The corpus is 562 cases (83 reader, 293 static, 170 interpreter, 16 tooling); the Wasm backend matches 162 of 162 | Branch `claude/m4-step-18-env-clock-random`, based on the head of Step 17 (PR #377, not yet merged); PR to be recorded |
| 19 | [Entry outcomes](19-entry-outcomes.md): `@command.program-failed` and exit code 6 for an entry that returns `err` (per G16), the host failure at the entry, and the parity sweep over success, typed host error, propagation, and trap | 18 | not started | — |
| 20 | [Effect metadata](20-effect-metadata.md): performed, enclosing, and target rows at a source position, effect-operation witnesses, and index records for effect roots and operations | 19 | not started | — |
| 21 | [Interpreter and unoptimized-Wasm performance baseline](21-performance-baseline.md) on the named programs — evidence step | 20 | not started | — |
| 22 | [M4 demo and exit gate](22-m4-exit.md), including the M3 deferral sweep — evidence step | 21 | not started | — |

Steps 1 and 13 are specification prerequisites, Steps 4 and 16a are
infrastructure steps, and Steps 12, 21, and 22 are evidence steps; they claim no
language behavior. Step 3 claims its behavior for the interpreter only; the Wasm
backend meets the same outcome in Step 6. Guides for Steps 2–12 are written in
Step 1 and guides for Steps 14–22 in Step 13, because their content depends on
the contracts those steps close. Step 1 also records the decision ledger, the
M4 surface inventory, and M4 validation. No step starts without its guide.

Step 13 changed the Stage 4B decomposition in two places, and the reasons are
recorded here and in each guide. Step 16 became 16a and 16b. The decisions of
2026-10-17 make the host world a schema with a decoder, a corpus input, and two
providers behind one trait, together with a 17-row registry, a checked-in snapshot,
and an event codec that survives unknown kinds. That is the harness every operation
of Steps 16b–18 is tested through, and carrying it with the ABI, the first typed
`@host` declaration, and the console in one change would give a change whose
review seam is not a vertical slice. Step 16a lands the harness first and claims
no language behavior; Step 16b lands the first operations in both backends, as the
stage rule requires. Step 16b then split in two, because the delivery rule says host
operations land "each in both backends in the same change": 16b-1 lands the boundary
in both backends and proves it with hand-built IR, so no operation is reachable
from source until 16b-2 makes the `io` operations callable in both at once. Step 18 grew by the program arguments: `env.read.args` has no
value without `vibra run TARGET -- ARG...`, and the closed command grammar now
carries it (ledger D22.1), so the CLI change, the system provider's arguments, and
the world's `args` land with the operation that reads them. No other step moved.

Step 1 split two planned steps, and the split is recorded here, in the guides, and
in the ledger. Step 5 became 5a and 5b: the arena, its reference counting, the
handle table, and the host accessors are one self-contained contract with its own
proof (bounded live size, bounded-stack release, exhaustion), and carrying every
compound form with it would give one change that cannot be reviewed against that
proof. Step 8 became 8a, 8b, 8c, and 8d: its primitive registry is about two hundred
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
| Unoptimized Wasm lowering of the complete pure language: mandatory tail calls, union discriminants in written order, checked lookups, `@compiler` externals lowered before emission | 4–10 (5a–8d); NaN and number text in 8c ([Stage 4A evidence](stage-4a-evidence.md#stage-4a-exit-clauses)) |
| Native implementations lowered from the single source; body/native differential joined to the interpreter/Wasm harness | 1, 8a, 8b, 8c, 8d, 10 ([Stage 4A evidence](stage-4a-evidence.md#stage-4a-exit-clauses)) |
| Instance-owned value arena and opaque-index discipline; no index or instance identity in typed IR or build output | 1, 5a, 16b ([Stage 4A evidence](stage-4a-evidence.md#stage-4a-exit-clauses)) |
| Specified arena reclamation rule; bounded host memory for a long tail-recursive program | 1, 5a, 6 ([Stage 4A evidence](stage-4a-evidence.md#stage-4a-exit-clauses)) |
| Interpreter/Wasm differential harness over the entire executable corpus, through the conformance harness | 1, 4, 11, 12 ([Stage 4A evidence](stage-4a-evidence.md#stage-4a-exit-clauses)) |
| Native `deffect` operations, default-empty ceilings, transitively closed performed rows, imported-symbol effect references | 14 |
| Effect metadata in general queries; source-position metadata for performed, enclosing, and target rows | 20 |
| Required binary-target effect arrays and static admission of the entry | 15 |
| Typed `@host` externals and the closed `vibra_v1` registry in both backends; the registry versioning rule | 13, 16a, 16b |
| Whole-value console, filesystem, environment-read, clock, and random operations | 16b–18, and the program arguments of 18 |
| Injected deterministic test providers and ordered audit traces; one versioned audit-event encoding | 13, 16a, 16b, 18 |
| Effect rows in typed IR, query schemas, and build metadata that do not assume a closed literal set | 13, 14, 20 |
| The forms M3 reassigned: the three contract-member forms, and the result of an entry that returns `err` | 2, 9, 19 |
| A specified outcome for deep non-tail recursion, written before the Wasm backend | 1, 3, 6 ([Stage 4A evidence](stage-4a-evidence.md#stage-4a-exit-clauses)) |
| Effectful iteration examples that walk with `iter.next` and tail recursion | 16b |
| Recorded interpreter and unoptimized-Wasm performance baseline | 21 |
| Stage demo: the M3 demo library's tests agree in both backends; a deep tail-recursive walk grows neither stack nor arena | 12 ([Stage 4A evidence](stage-4a-evidence.md#stage-4a-exit-clauses)) |
| Demo gate: a target declaring `@std.fs.read` reads, transforms, and writes; the same source is rejected when a ceiling omits a root; byte-identical stdout and audit traces in both backends | 22 |
| Exit: an additive root reaches the performed row, the rejection, and the target array of every transitive caller; the witness is named for each missing root | 14, 15 |
| Exit: every host operation has one registry entry, owner effect, typed signature, and audit-event shape | 13, 16a–18 |
| Exit: no compiler-generated ambient host read exists outside the registry | 16a (the ambient-read test), 16b–18; audited by 22 |
| Exit: success, typed host error, propagation, and trap have interpreter/Wasm parity | 11, 19 |
| Exit: deep non-tail recursion has the same specified outcome in both backends | 3, 6 ([Stage 4A evidence](stage-4a-evidence.md#stage-4a-exit-clauses)) |
| Exit: a tail-recursive loop allocating a fresh compound value per iteration holds a bounded live arena in the Wasm backend | 6 ([Stage 4A evidence](stage-4a-evidence.md#stage-4a-exit-clauses)) |
| Exit: interpreter-v1 conformance passes for the full language, and the Wasm backend matches it on every executable case | every behavior step; swept by 12 and 22 ([Stage 4A evidence](stage-4a-evidence.md#stage-4a-exit-clauses)) |

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
| `Attribute::External` with `@host` | Step 16b |
| An abstract contract member with its own generic parameters | Step 2; Wasm in Step 9 |
| Labelled operands and written `types:` arguments on a contract member call | Step 2; Wasm in Step 9 |
| A dict variadic tail on a contract member | Step 2; Wasm in Step 9 |
| A contract member with its own generics or labelled parameters as a function value | Step 2; Wasm in Step 9 |
| M3's G21 (G16 here): the command result for an entry that returns `err` | Step 19 |
| Pre-M4 C16: effect rows of a `let` value, a `let-else` fallback, and a `return` operand | Step 14 |
| The conformance chapter's effectful walk over `iter.next` | Step 16b |

Step 13 adds the Stage 4B contract rows. They are not forms an earlier milestone
deferred but contracts that the steps below implement, and the inventory test
requires an owner for each, so that a closed contract cannot lose its
implementation:

| Stage 4B contract row | Owner |
| --- | --- |
| The `vibra_v1` host registry of 17 rows, its versioning rule, and its checked-in snapshot (ledger D17, D19) | Step 16a (table, snapshot); Steps 16b–18 (rows) |
| The `@host` ABI: borrowed parameters, an owned result, and the seven builders (D30) | Step 16b |
| The `@audit-trace.v1` event record and the CLI `AuditEvent` objects (D18) | Step 16a (codec); Step 16b (events) |
| The `@host-world.v1` test world, the system provider, and the corpus `host-world` input (D20) | Step 16a; fields in Steps 16b–18 |
| Program arguments, `vibra run TARGET -- ARG...` (D22) | Step 18 |
| `@command.program-failed` and exit code 6 (D21) | Step 19 |
| The effect diagnostics, the witness, the indirect-call rule, and the row record (D23–D25) | Step 14; Step 15 for the target array |
| The manifest `host` list, the `0.3.0` package, and the five effect modules (D27) | Step 16a (shape); Steps 16b–18 (modules) |
| The `effects` fact of the position envelope and the effect fields of the index (D28) | Step 20 |
| The performance baseline and its CI check (D29) | Step 21 |
