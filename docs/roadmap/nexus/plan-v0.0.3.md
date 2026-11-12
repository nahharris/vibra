# v0.0.3 work packages

Read [README.md](README.md#how-to-delegate-a-work-package) first. Goal IDs
(G3.*) are in [goals.md](goals.md), decisions (N*) in
[decisions.md](decisions.md), and the commands and standing rules in
[validation.md](validation.md). Test names under **Validation** are names the
package must create; a package may add tests and may not rename or drop these.

Size is **S** (one sitting), **M** (one PR, several days of agent work), or
**L** (split as listed). "Agent" is the cheapest tier expected to succeed:
**mechanical** for a port or a table-driven change with an oracle to compare
against, **careful** where a wrong choice is not caught by a test.

| Package | Milestone | Depends | Size | Agent |
| --- | --- | --- | --- | --- |
| [X3-S1](#x3-s1--transition-specification) | M1 | — | M | spec: draft only |
| [X3-S2](#x3-s2--toolchain-and-target-feasibility) | M1 | — | S | careful |
| [X3-01](#x3-01--execution-ir-text-form-and-verifier-hardening) | M1 | — | S | mechanical |
| [X3-02](#x3-02--store-parity-with-the-m4-arena) | M1 | — | M | mechanical |
| [X3-03](#x3-03--lowering-slice) | M1 | X3-01 | M | careful |
| [X3-04](#x3-04--stepper-in-the-conformance-harness) | M1 | X3-03 | M | careful |
| [X3-05](#x3-05--bridge-archive-and-launcher) | M1 | X3-S1 (ABI) | M | careful |
| [X3-06](#x3-06--code-generation-for-the-slice) | M1 | X3-01, X3-05 | M | careful |
| [X3-07](#x3-07--windows-link-driver) | M1 | X3-S2, X3-06 | M | careful |
| [X3-08](#x3-08--linux-and-macos-paths) | M1 | X3-07 | M | mechanical |
| [X3-09](#x3-09--m1-demo-and-exit) | M1 | all above | S | careful |
| [X3-10](#x3-10-to-x3-19--parity-by-feature) … X3-19 | M2 | X3-09 | L each | mechanical |
| [X3-20](#x3-20--platform-adapter-smoke) | M2 | X3-19 | M | careful |
| [X3-21](#x3-21--limits-and-structured-failures) | M2 | X3-19 | M | careful |
| [X3-22](#x3-22--baseline-audits-and-m2-exit) | M2 | X3-10…21 | M | careful |
| [X3-23](#x3-23--vibra-build-and-the-manifest) | M3 | X3-22, H1 | M | careful |
| [X3-24](#x3-24--pinned-toolchains-and-reproducibility) | M3 | X3-23 | M | careful |
| [X3-25](#x3-25--packaging-and-install-smoke) | M3 | X3-24 | S | mechanical |
| [X3-26](#x3-26--cutover) | M3 | X3-24 | L | careful |
| [X3-27](#x3-27--release-audit) | M3 | X3-26 | S | careful |

X3-01, X3-02, X3-S1, and X3-S2 have no dependency and can start at once.
X3-03/04 (lowering) and X3-05/06/07 (native path) are two independent lines
until X3-09 joins them.

---

## X3-S1 · Transition specification

**Kind:** specification prerequisite. **Goals:** G3.R4, G3.M1.1, H2.
**Decisions it lands:** N3, N8 (wording), N10, N14, O5, O6.

**Read first:** charter; `06-runtime.md` whole; `07` "Conformance profiles"
and "Differential execution"; `04` "Build products"; `05` "Execution backend
of `run` and `test`"; [design.md](design.md) §2–§9.

**Deliver** a draft PR of specification text, for Hannah's review, that adds
the native contract beside the Wasm one without removing anything:

1. `06-runtime.md`: a "Native execution" section stating that a native
   executable is checked IR lowered to verified execution IR and linked with
   Nexus; that execution may be suspended and resumed at checkpoints with no
   observable effect; the runtime ABI version and that it is independent of
   the registry version and the build format; the origin table as the
   source-origin mapping; admission (ABI, registry minor, symbols) before
   execution. "Activations and memory" reworded from "the arena" to the
   instance's own storage.
2. `07`: a `native-v1` profile; the transition rule that an executable case
   is compared against every executor a runner has; the native parity
   inventory and that a pending cell fails the M2 gate.
3. `04` and `05`: native build products and the `build` command shape, marked
   as landing in M3.
4. The diagnostic registry: codes for an unsupported target, a missing link
   tool, a link failure, and an incompatible artifact, each with its level.
5. The roadmap gate text of v0.0.3 updated where the specification now says
   it.

**Validation:**

- Every MUST added has a conformance case named or an explicit review-only
  note, as the charter requires.
- No sentence of the Wasm contract is weakened or removed (`git diff` shows
  additions and the one rewording of "the arena").
- `cargo test -p vibra-conformance --test it -- diagnostic_registry::
  evidence::` passes with the new codes registered.
- Hannah approves. Until then X3-05's ABI and X3-04's inventory are
  provisional.

**Out of scope:** removing Wasm text (X3-26); any actor contract (X4-S1).

## X3-S2 · Toolchain and target feasibility

**Kind:** evidence. **Goals:** G3.M1.2, G3.M1.13, H4. **Decisions:** N12, N13,
O7.

**Deliver** `docs/roadmap/v0.0.3/m1/feasibility.md` recording, per target:
the Cranelift ISA flags and CPU baseline; that `cranelift-object` 0.136.x
emits a linkable object (a one-function spike, kept under `fuzz/` or deleted,
never in `crates/`); the exact linker, its discovery method, the system
libraries a Rust `staticlib` needs on that target (`rustc --print
native-static-libs`); how the archive is built; the CI runner that qualifies
it.

**Validation:**

- On Windows: a hand-linked `.exe` from a Cranelift object plus a Rust
  `staticlib` runs and exits 0; the record gives the full `link.exe` command
  line and how `link.exe` was found without `PATH`.
- The same for Linux and macOS on a CI runner, or an explicit "not run" with
  the reason. A not-run row keeps X3-08 blocked.
- The record states whether any target needs a Cranelift feature the pinned
  version lacks.

## X3-01 · Execution IR text form and verifier hardening

**Goals:** G3.M1.3. **Decisions:** N4.

**Tasks:** (1) a deterministic text form of a `Program` (`Display`), used in
test failures and snapshots; (2) a property test that builds random programs
from a seed, mutates one field, and asserts `verify` either accepts or
returns an error without panicking; (3) a test that every `VerifyErrorKind`
variant is produced by some test.

**Validation:**

- `cargo test -p vibra-exec` passes, including `text_form_is_deterministic`,
  `verify_never_panics_on_mutated_programs` (at least 10,000 mutations, seed
  fixed in the test), and `every_verify_error_kind_has_a_rejected_program`.
- No change to rules V1–V6 or to the existing tests.

## X3-02 · Store parity with the M4 arena

**Goals:** G3.M1.5, G3.M2.4. **Decisions:** N7, O9.

**Read first:** `crates/vibra-wasm/src/layout.rs` (the layout chapter);
M4 ledger rows D2.4, D2.10, D3.5–D3.8; `crates/vibra-native/src/arena.rs`.

**Tasks:** (1) port every M4 arena host test that does not concern value IDs
to `vibra-nexus::store`, keeping the measured constants; (2) implement
`vibra_native::Source`/`Sink` for the store by handing
`vibra_native::arena` a view of the store's bytes, and add `vibra-native` to
Nexus's dependencies and to the architecture table; (3) expose a live-size
and peak counter for the baseline.

**Validation:**

- `the_largest_size_class_is_allocated_or_exhausts_and_nothing_above_it_exists`,
  `a_block_that_exactly_fills_the_limit_runs_and_one_byte_under_does_not`,
  `an_allocating_loop_has_a_live_size_that_does_not_grow_with_its_iterations`
  (equal live size at 10,000 and 100,000 iterations) pass.
- `the_native_arena_reads_a_store_value`: a `str` built by the store is read
  by `vibra_native::arena` and equals its text, with no copy of the layout
  constants outside `vibra_ir::arena`.
- `architecture_boundary::` passes with the widened `vibra-nexus` row.

## X3-03 · Lowering slice

**Goals:** G3.M1.3. **Decisions:** N4, N9.

**Read first:** [design.md](design.md#from-checked-ir); `06-runtime.md`
"Evaluation", "Tail calls", "Module-value initialization";
`crates/vibra-wasm/src/lower.rs` for evaluation order and count placement.

**Deliver** `vibra-lower` with `lower(&CheckedProgram) -> Result<Program,
LowerError>` for the rows of the design table marked X3-03. A form outside
the slice is `LowerError::NotLowered(&'static str)` naming the form: never a
partial program.

**Validation:**

- `lower` output always passes `vibra_exec::verify`:
  `every_lowered_program_verifies` over every checked source the slice admits.
- `lowering_is_deterministic`: two lowerings of one program are equal, and
  equal across two processes.
- For each form of the slice, a host test that checks the lowered program
  with the stepper against `vibra_interp::Interpreter::run` on result and
  audit trace, unbounded and with quota 1.
- `a_tail_call_lowers_to_a_tail_transfer_for_every_kind_of_callee` (direct,
  closure, function value in a parameter, module function as a value).
- `a_form_outside_the_slice_is_not_lowered_and_names_the_form`.
- `architecture_boundary::` passes with a `vibra-lower` row of
  `["vibra-exec", "vibra-ir"]` and no other dependency.

**Out of scope:** intrinsics, aggregates, patterns, generics.

## X3-04 · Stepper in the conformance harness

**Goals:** G3.M1.12, H5. **Decisions:** N6, N14.

**Tasks:** (1) a harness executor that lowers, verifies, admits, and steps an
executable case with a fresh copy of its host world under the runner's memory
limit, twice (unbounded, quota 1); (2) `conformance/native-parity.tsv` with
one row per executable case and the columns `stepper`, `windows`, `linux`,
`macos`; (3) the host test that holds table and corpus together; (4) a
`native backend:` count line in the runner's report.

**Validation:**

- `native_parity_inventory::the_table_names_every_executable_case_once`.
- `native_parity_inventory::a_matched_cell_that_fails_fails_the_run` and
  `…::a_pending_cell_names_a_package_of_the_plan`.
- `native_parity_inventory::a_cell_that_matches_but_is_marked_pending_fails`,
  so a package cannot forget to move a row.
- The runner's exit status is unchanged for the interpreter and Wasm lines:
  the baseline counts of [validation.md](validation.md#baseline) hold.
- The stepper column is `matched` for every case X3-03 can lower and `pending
  X3-1n` for the rest, with no third value.

## X3-05 · Bridge, archive, and launcher

**Goals:** G3.M1.4, G3.M1.6. **Decisions:** N10, N11, N13, N18.

**Read first:** [design.md](design.md#6-generated-code-and-the-bridge) and §7;
`crates/vibra-nexus/src/context.rs`.

**Deliver** `vibra-nexus-bridge`: `crate-type = ["staticlib", "rlib"]`, its
own `[lints]` table that allows `unsafe_code` and keeps every other workspace
lint; one `extern "C"` routine per `Context` transfer and per instruction;
`VibraProgram` and its admission; the launcher `main` with the `system`
profile, and the `harness` profile behind a cargo feature.

**Validation:**

- `architecture_boundary::only_the_bridge_contains_unsafe`: a scan of every
  other crate's sources finds no `unsafe` token, and every other manifest
  inherits the workspace lints.
- Every `unsafe` block has a `// SAFETY:` comment, enforced by
  `clippy::undocumented_unsafe_blocks = "deny"` in the bridge's lint table.
- `the_bridge_drives_the_demo`: a Rust-written `VibraStep` table (no
  Cranelift) implementing the foundation's `demo` program through the
  `nexus_*` routines observes exactly what the stepper observes, unbounded
  and with quota 1.
- `a_program_built_for_another_abi_is_refused_by_the_launcher` and
  `…_naming_an_unknown_symbol_…`: exit code and stderr asserted, and no host
  operation performed.
- The `system` launcher maps outcomes to the exit codes of the tooling
  chapter (0, trap, 3 for the host event, 6 for `program-failed`); one test
  per code.
- `cargo build -p vibra-nexus-bridge --release` produces the archive, and the
  record lists its native static library requirements.

## X3-06 · Code generation for the slice

**Goals:** G3.M1.4. **Decisions:** N2, N11, N12.

**Deliver** `vibra-codegen` with `emit(&Program, &Target) -> Result<Object,
EmitError>` for every instruction and terminator the stepper executes at that
point: one native function per execution-IR function (a switch on `resume`),
the `vibra_program` descriptor, the constant and origin data sections.

**Validation:**

- `emit` takes only a program that passed `verify` (a typed wrapper,
  `Verified<'_>`, so the compiler enforces it).
- `emission_is_byte_identical_across_runs` and `…_across_processes`.
- `the_object_defines_exactly_the_expected_symbols`: `vibra_program` and
  local functions only; undefined symbols are a subset of the `nexus_*` list.
- `no_generated_function_calls_another_generated_function`: the object's
  relocations from code target only `nexus_*` routines (N2).
- `only_codegen_depends_on_cranelift` in the architecture test; Cranelift
  version equals the one in `Cargo.lock` before the change.
- An instruction the generator does not implement is `EmitError`, never a
  silent omission: `every_instruction_the_stepper_runs_is_generated` iterates
  the instruction enum.

## X3-07 · Windows link driver

**Goals:** G3.M1.4, G3.M1.13.

**Deliver** in `vibra-build`: MSVC and SDK discovery, the link invocation,
and a harness helper that builds an executable from a checked program.

**Validation:**

- `the_linker_is_found_without_path`: with `PATH` emptied the driver still
  finds `link.exe`; with the discovery sources removed it reports the
  missing-tool diagnostic of X3-S1.
- `a_linked_demo_runs_as_an_exe`: the M1 fixture is lowered, generated,
  linked, launched as a process, and its observation document equals the
  stepper's.
- `the_output_path_may_contain_spaces_and_non_ascii`.
- The link command line is recorded in the test's failure output and matches
  the feasibility record of X3-S2.

## X3-08 · Linux and macOS paths

**Goals:** G3.M1.13, G3.R1. **Depends:** a CI runner per target (O7).

**Validation:** the three tests of X3-07, under their target's names, pass on
that target's runner; the `linux` and `macos` columns of the inventory move
for the M1 fixture's cases; a target without a runner stays `pending X3-08`.

## X3-09 · M1 demo and exit

**Goals:** G3.M1.7–G3.M1.12. **Kind:** evidence.

**Deliver** the fixture `examples/nexus-m1/` (two modules, an indirect tail
call from one into the other, one `io.stdout.text`, and a variant that ends
in `@runtime.division-by-zero`), its corpus cases, and the M1 evidence in
`docs/roadmap/v0.0.3/m1/README.md`.

**Validation (each maps to one gate clause):**

| Clause | Evidence |
| --- | --- |
| G3.M1.8 demo | On Windows, the linked fixture's result, trap code, origin, and audit trace equal the interpreter's, from one expectation. The runtime chapter gives `@runtime.division-by-zero` no origin, so the expected origin of that variant is none; an origin that maps to a span is evidenced by a second variant, a test that ends in `@runtime.unobservable-function` |
| G3.M1.9 | The same run with quota 1 has exactly one `io.stdout-text` event and the yield count is at least the number of checkpoints |
| G3.M1.10 | With the loop count at 10,000 and 100,000, peak frame count is equal and peak live size differs by at most 64 bytes |
| G3.M1.11 | Three refused inputs: a program mutated after verification is rejected by `verify`; an object with another `abi` is refused by the launcher; a descriptor naming an unknown symbol is refused. None performs a host operation |
| G3.M1.12 | `native-parity.tsv` committed; the README states matched and pending counts per column and per owning package |
| G3.M1.7 | [design.md](design.md#11-what-v004-extends-the-m1-handoff) reviewed against the as-built ABI and corrected in the same PR |

---

## X3-10 to X3-19 · Parity by feature

**Goals:** G3.M2.1, G3.M2.2. One package per feature, each **L** and split
into the sub-items listed, one PR each. Every package has the same shape and
the same validation, so it is stated once.

**Shape of each PR:** extend the execution IR if the feature needs a new
instruction (the four-part rule of
[design.md](design.md#verification-rules)); implement the runtime routine in
Nexus; lower the checked forms; execute it in the stepper; generate code for
it; move the inventory rows.

**Validation of each PR:**

1. Every inventory row the PR claims is `matched` in the `stepper` column and
   in every platform column whose runner exists, unbounded and with quota 1.
2. No row moves from `matched` to `pending`.
3. For a registry row: the shared sample vectors of
   `vibra_ir::external::vectors` pass through the Nexus routine,
   `every_vector_of_<family>_matches`, so the routine is held to the same
   table as the interpreter and the Wasm backend.
4. A rejection case: the nearest invalid program still fails in the checker
   with its existing diagnostic, and nothing reaches the lowering.
5. A balance check: after each of the feature's host tests the context's live
   size equals what its module values keep.
6. The pre-merge commands pass.

| Package | Feature | Sub-items, in order | Port from |
| --- | --- | --- | --- |
| X3-10 | Scalars and arithmetic | integer unchecked (saturating, `div`/`rem` trap with origin); integer checked; conversions; floats and NaN canonicalization at observation; `to-str` and `parse` through `vibra-native::number` | `vibra-wasm/src/primitive.rs`; M4 guides 08a, 08c |
| X3-11 | Aggregates | tuple and record; enum and `bool`; wrapper; union and `Widen`; projection | `lower.rs`, `types.rs`; M4 05b |
| X3-12 | Patterns and typed failure | `match` over every pattern kind; destructuring `let`, parameters, lambdas; `let-else`; `as` narrowing; `try`; `never` | `pattern.rs`; M4 07, D2.15, D3.10 |
| X3-13 | Generic instantiation | type descriptors; generic functions and types; closures at their creator's type arguments; generic `lambda` | `desc.rs`; M4 D9.2, D9.5 |
| X3-14 | Calls, completed | labelled operands and defaults; variadic tails; function-value header completed | M4 06, D2.14 |
| X3-15 | Contracts and interfaces | static member selection; dispatch through an interface value; closed contracts (`key.compare`, `key.equal`, `iter.next`); destination-selected members | `lower/dispatch.rs`, `lower/iterate.rs`, `lower/keys.rs`; M4 09 |
| X3-16 | Collections and text | `array.*`; `dict.*` and key order; `str.*` and `bytes.*`; lookups; native bodies and `array.fold` (which runs its Vibra body, as a module does) | `vibra-native/src/arena.rs`; M4 08b, 08d, 10 |
| X3-17 | Module initialization | lazy once across modules; forward references; an initializer that performs a host call; an initializer reached through a function value | M4 D2.13 |
| X3-18 | Assertions, tests, observation | observation of every value kind as `ObservedValue`; `assert.true/false/equal`; the test table and harness test runs; `@runtime.unobservable-function` | `canon_typed.rs` for cases only; M4 11, D8.4, D8.5 |
| X3-19 | Host operations | console; filesystem; environment and arguments; clocks and random; `@command.program-failed` entry outcome | `vibra-wasm-run/src/host.rs`; M4 16b–19 |

X3-10 to X3-13 are ordered. X3-14 to X3-19 depend on X3-13 and are otherwise
independent of each other, so they can run in parallel on separate branches;
the inventory file is the one shared file, and a rebase resolves it row by
row.

## X3-20 · Platform adapter smoke

**Goals:** G3.M2.3.

**Validation:** real process launches of the `system` launcher, on each
target's runner, asserting: an executable and a working directory whose paths
contain a space and a non-ASCII character; arguments after the program name
arrive through `env.read.args` in order, including an empty one and a
non-ASCII one; stdout and stderr bytes are exactly what the program wrote,
with no newline translation on Windows; a file written by `fs.write.text` is
read back by the test; each exit code of the tooling chapter. Tests are named
`smoke_<target>_<aspect>` and a target without a runner is recorded as not
run.

## X3-21 · Limits and structured failures

**Goals:** G3.M2.4, G3.M2.5, G3.M2.6.

**Validation:**

- `non_tail_recursion_a_hundred_thousand_deep_completes_natively` on a thread
  whose stack is 256 KiB, per target.
- `a_recursion_with_no_base_case_is_the_host_event_natively`, exit code 3,
  and the same case with a larger limit completes.
- `yield_everywhere`: the whole executable corpus with quota 1 equals the
  unbounded run on result, trap, origin, and audit trace, on the stepper and
  on each target. Zero differences is the criterion.
- `a_long_primitive_charges_its_work`: concatenating two arrays of 100,000
  elements charges at least 100,000 units and the run still yields no later
  than the next checkpoint.
- One conformance case per structured diagnostic of X3-S1 (unsupported
  target, missing tool, link failure, incompatible artifact), with code,
  level, and span asserted.

## X3-22 · Baseline, audits, and M2 exit

**Goals:** G3.M2.7–G3.M2.10. **Kind:** evidence.

**Validation:**

- `native-parity.tsv` has **zero** `pending` cells in every column, and the
  runner's native line reports zero failed.
- The interpreter and Wasm lines equal the baseline, and the v0.0.2 agent
  workflow fixture (where it exists by then) passes.
- `docs/roadmap/v0.0.3/m2/performance-baseline.md` records, per target, the
  same metrics as the [M4 baseline](../v0.0.1/m4/performance-baseline.md) for
  the same programs, with hardware and commit. No threshold is set.
- Two written audits in the M2 README: the runtime boundary (every bridge
  routine, what it validates, why each `unsafe` block is sound) and the
  compiler boundary (what `verify` guarantees to the generator, and each
  place the generator relies on it).
- The M2 demo fixture runs on all three targets with forced yields and its
  trap variant.

---

## X3-23 · `vibra build` and the manifest

**Goals:** G3.M3.1, G3.M3.2. **Blocked on:** H1 (the semantic digest of
v0.0.2 M5). If H1 has not landed, this package first lands the digest behind
the interface M5 specified, in a separate PR reviewed as M5 work.

**Validation:**

- `vibra build` and its JSON envelope validate against the published schema;
  the CLI and MCP agree on the facts.
- The manifest decodes with the project's `.vibon` data decoder and
  round-trips canonically.
- `a_source_change_moves_the_semantic_identity_and_the_artifact_hash`;
  `a_linker_flag_change_moves_only_the_artifact_identity`;
  `an_abi_bump_moves_only_the_artifact_identity`.
- `run`, `test`, and evaluation still execute the interpreter:
  `the_cli_does_not_depend_on_codegen_for_run_or_test` (a dependency test)
  and a behavior test that `run` works with no linker installed.

## X3-24 · Pinned toolchains and reproducibility

**Goals:** G3.M3.3.

**Validation:** per target, `two_clean_offline_builds_from_different_paths_
are_byte_identical` for the executable and the manifest, run in CI from two
checkout directories of different lengths, with the network disabled after
`cargo fetch`. The pinned linker and SDK versions are in the target profile
and in the manifest, and a build with another version fails with the
incompatible-toolchain diagnostic instead of producing a different binary
silently.

## X3-25 · Packaging and install smoke

**Goals:** G3.M3.4.

**Validation:** on a clean Windows machine image (a CI job with no Rust
toolchain), installing the package and running `vibra build` on the M1
fixture produces an executable that runs; the supported command and profile
inventory is published in the docs and matches `vibra --help`.

## X3-26 · Cutover

**Goals:** G3.M3.5, G3.R5. **Precondition:** X3-22 and X3-24 green on all
three targets. If one target is not, this package does not start, Wasm stays,
and the release decision goes to Hannah (G3.R5).

**Sub-items, one PR each:** (1) the specification cutover: charter,
`04`, `05`, `06`, `07`, profiles `wasm-v1` out and `native-v1` in;
(2) conformance switches to interpreter/native and the Wasm differential
retires; (3) `vibra-wasm`, `vibra-wasm-run`, Wasmtime,
`wasm-encoder`, and `wasmparser` leave the workspace, and `vibra_ir::boundary`
keeps only what Nexus uses under a name without "WebAssembly"; (4) packaging
and README references.

**Validation:**

- `rg -i 'wasm|wasmtime'` over `crates/`, `Cargo.toml`, `.github/`, and
  `docs/spec/` returns only historical references the PR lists one by one.
- `Cargo.lock` has no `wasmtime`, `wasm-encoder`, or `wasmparser`.
- v0.0.1 evidence under `docs/roadmap/v0.0.1/` is unchanged, and the reusable
  fixtures still run as native differential cases.
- Both suites pass on the support matrix.

## X3-27 · Release audit

**Goals:** G3.M3.6, G3.M3.7. **Kind:** evidence. Records, in
`docs/roadmap/v0.0.3/m3/README.md`, the audit of artifact admission, the
closed host bridge, runtime limits, error mapping, and release metadata, each
with the test that evidences it, and the final counts of both suites per
target.
