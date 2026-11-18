# v0.0.3 work packages

Read [README.md](README.md#how-to-delegate-a-work-package) first. These are
the packages of [v0.0.3](../v0.0.3/README.md): the whole language on native
code (M1), three platforms (M2), and build with cutover (M3). Decisions (N*)
are in [decisions.md](decisions.md), and the commands and standing rules in
[validation.md](validation.md). Test names under **Validation** are names the
package must create; a package may add tests and may not rename or drop these.

Size is **S** (one sitting), **M** (one PR, several days of agent work), or
**L** (split as listed). "Agent" is the cheapest tier expected to succeed:
**mechanical** for a port or a table-driven change with an oracle to compare
against, **careful** where a wrong choice is not caught by a test.

| Package | Milestone | Depends | Size | Agent |
| --- | --- | --- | --- | --- |
| [X3-10](#x3-10-to-x3-19-x3-28-x3-29--the-whole-language) … X3-19 | M1 | X2-04 | L each | mechanical |
| [X3-28](#x3-10-to-x3-19-x3-28-x3-29--the-whole-language) | M1 | X3-15 | M | careful |
| [X3-29](#x3-10-to-x3-19-x3-28-x3-29--the-whole-language) | M1 | X3-15, X3-19 | M | careful |
| [X3-08](#x3-08--linux-and-macos-paths) | M2 | X2-04 | M | mechanical |
| [X3-20](#x3-20--platform-adapter-smoke) | M2 | X3-19, X3-08 | M | careful |
| [X3-21](#x3-21--limits-and-structured-failures) | M2 | X3-19 | M | careful |
| [X3-22](#x3-22--baseline-audits-and-m2-exit) | M2 | all of M1; X3-08, X3-20, X3-21 | M | careful |
| [X3-23](#x3-23--vibra-build-and-the-manifest) | M3 | X3-22; v0.0.2 M3 | M | careful |
| [X3-24](#x3-24--pinned-toolchains-and-reproducibility) | M3 | X3-23 | M | careful |
| [X3-25](#x3-25--packaging-and-install-smoke) | M3 | X3-24 | S | mechanical |
| [X3-26](#x3-26--cutover) | M3 | X3-24 | L | careful |
| [X3-27](#x3-27--release-audit) | M3 | X3-26 | S | careful |

Ids X3-01 to X3-07 belong to code that is already in the workspace
([README.md](README.md#what-exists-now)). They are not reused.

X3-08 depends only on the v0.0.2 slice and can run beside the M1 packages;
its platform columns fill as each M1 package lands.

## X3-08 · Linux and macOS paths

**Milestone:** M2. **Depends:** X2-04, and a CI runner per target (O7).
**Decisions:** N38.

**Tasks:** enable the `x86` and `arm64` features of `cranelift-codegen`
explicitly, as the feasibility record found necessary; the link driver and
linker discovery for each target; the runtime archive for each target; the
filesystem adapter for each target under the contract of N43.

**Validation:** the three link-driver tests, under their target's names, pass on
that target's runner; the fixture of X2-04 agrees with the interpreter on
each; the `linux` and `macos` columns of the inventory move for the cases
already matched on Windows; a target without a runner stays
`pending X3-08`. Emitting an object for a target from another machine is not
evidence that the target runs.

## X3-10 to X3-19, X3-28, X3-29 · The whole language

**Milestone:** M1. One package per feature, each split into the sub-items
listed, one PR each. Every package has the same shape and the same
validation, so it is stated once. The features are those of the rewritten
language; the "port from" column names code written for the earlier surface
that solves the same problem and is a starting point, not an oracle.

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
| X3-10 | Scalars and arithmetic | saturating integer operations at the extremes; checked operations, with `divChecked` and `remChecked` returning typed errors and no trapping form; integer conversions through `From` and `TryFrom`; floats and NaN canonicalization at observation; text conversion through `vibra-native::number` | `vibra-wasm/src/primitive.rs`; M4 guides 08a, 08c |
| X3-11 | Aggregates and construction | tuple and record; variant types and `Bool`; wrapper; widening to an interface value; field reads; private representation; a bare `where` leaving no check; `where … else` returning a `Result` with the constant error; validation on every other way a value comes into being | `lower.rs`, `types.rs`; M4 05b |
| X3-12 | Patterns and typed failure | `match` over every pattern kind, exact-length list patterns and constant patterns included; destructuring `let` and parameters; `let … else`; `try` with its expected type; `Never` | `pattern.rs`; M4 07, D2.15, D3.10 |
| X3-13 | Generic instantiation | type descriptors; generic functions and types; closures at their creator's type arguments; generic anonymous callables | `desc.rs`; M4 D9.2, D9.5 |
| X3-14 | Calls, completed | labelled operands and defaults; written operand order; function-value header completed | M4 06, D2.14 |
| X3-15 | Interfaces | static member selection; dispatch through an interface value, with the `Self` restrictions; destination-dispatched members; default members | `lower/dispatch.rs`, `lower/iterate.rs`, `lower/keys.rs`; M4 09 |
| X3-16 | Collections and text | `List`, `Dict` and key order with the later duplicate winning, `Str`, `Bytes`; lookups returning `Option`; `Iter` over each, the lazy adapters, and the strict left `fold`; native implementations beside their Vibra bodies | `vibra-native/src/arena.rs`; M4 08b, 08d, 10 |
| X3-17 | Module initialization | lazy once across modules; forward references; an initializer reached through a function value; the kept size exposed for the balance check (N35). Initializers are pure, so none performs a host call | M4 D2.13 |
| X3-18 | Assertions, tests, observation | observation of every value kind as `ObservedValue`; `assert.true`, `assert.false`, `assert.equal`; the test table and harness test runs with a `TestWorld`; the explicit observation error for a value that hides a function, plan, or grant | `canon_typed.rs` for cases only; M4 11, D8.4, D8.5 |
| X3-19 | Host operations | the seventeen operations of registry major 2 behind grants: standard streams; filesystem with scope policies; environment and arguments; clocks and random. The four shared error cases. The operand-list bound of N36 | `vibra-wasm-run/src/host.rs`; M4 16b–19; X2-03 |
| X3-28 | Guards, completed | guards on anonymous callables and interface members; a guarded callable converted to a callable type keeping its checks; a proven `… else` guard removed without a change of type; the tail-position table of the runtime chapter, row by row | X2-02 |
| X3-29 | Plans and startup, completed | plans stored in values and returned; plans through interface values with their rows; `requires … else` per `await`; record and tuple grant parameters; every provider of the binding table; reverse release after a failed startup | X2-02, X2-03 |

X3-10 to X3-13 are ordered. X3-14 to X3-19 depend on X3-13 and are otherwise
independent of each other, so they can run in parallel on separate branches;
the inventory file is the one shared file, and a rebase resolves it row by
row. X3-28 and X3-29 come last, because they combine the others.

**M1 exit.** Every executable case of the corpus is `matched` in the
`stepper` and `windows` columns, unbounded and with quota 1. The demo is one
program with an indirect tail call across modules, a plan stored and awaited
twice, an optional grant, a guard that returns its consequence, and a scoped
file operation in the audit trace.

## X3-20 · Platform adapter smoke

**Validation:** real process launches of the `system` launcher, on each
target's runner, asserting: an executable and a working directory whose paths
contain a space and a non-ASCII character; arguments after the program name
arrive through `EnvReadArgs.read` in order, including an empty one and a
non-ASCII one; stdout and stderr bytes are exactly what the program wrote,
with no newline translation on Windows; a file written by
`DirWriteText.write` under a directory grant chosen by an argument is read
back by the test, and a write outside that grant is a typed denial that
leaves no file; a required grant that cannot be acquired ends the process
before the entry; the application's exit code reaches the parent process
unchanged. Tests are named
`smoke_<target>_<aspect>` and a target without a runner is recorded as not
run.

## X3-21 · Limits and structured failures

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
- One conformance case per `@tool.*` build diagnostic (unsupported
  target, missing tool, link failure, incompatible artifact), with code,
  level, and span asserted.

## X3-22 · Baseline, audits, and M2 exit

**Kind:** evidence.

**Validation:**

- `native-parity.tsv` has **zero** `pending` cells in every column, and the
  runner's native line reports zero failed.
- The interpreter line reports zero failed, the Wasm line still matches the
  v0.0.1 corpus under the transition profile, and the repair scenario of the
  v0.0.2 release gate passes.
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

**Depends:** the snapshot, claim, witness, and
policy identities of v0.0.2 M3 (N48). No store or published revision is
involved.

**Validation:**

- `vibra build` and its JSON envelope validate against the published schema;
  the CLI and MCP agree on the facts.
- The manifest decodes with the project's `.vibon` data decoder and
  round-trips canonically.
- `a_source_change_moves_the_semantic_identity_and_the_artifact_hash`;
  `a_linker_flag_change_moves_only_the_artifact_identity`;
  `an_abi_bump_moves_only_the_artifact_identity`;
  `a_policy_change_is_visible_separately_from_both`.
- The manifest records the admission ceiling and the grant bindings as data.
  It contains no live grant, callable, or plan.
- `run`, `test`, and evaluation still execute the interpreter:
  `the_cli_does_not_depend_on_codegen_for_run_or_test` (a dependency test)
  and a behavior test that `run` works with no linker installed.

## X3-24 · Pinned toolchains and reproducibility

**Validation:** per target, `two_clean_offline_builds_from_different_paths_
are_byte_identical` for the executable and the manifest, run in CI from two
checkout directories of different lengths, with the network disabled after
`cargo fetch`. The pinned linker and SDK versions are in the target profile
and in the manifest, and a build with another version fails with the
incompatible-toolchain diagnostic instead of producing a different binary
silently.

## X3-25 · Packaging and install smoke

**Validation:** on a clean Windows machine image (a CI job with no Rust
toolchain), installing the package and running `vibra build` on the release
program of v0.0.2 produces an executable that runs with its arguments,
streams, scoped files, and exit code. A linker or SDK the user must install
is named by the installer, not discovered by a failure; the supported command and profile
inventory is published in the docs and matches `vibra --help`.

## X3-26 · Cutover

**Decisions:** N39. **Precondition:** X3-22 and X3-24
green on all three targets. If one target is not, this package does not
start and the release stays incomplete until the support plan is changed on
review. Removal is settled; only its date depends on readiness.

**Sub-items, one PR each:** (1) the specification cutover: charter,
`04`, `05`, `06`, `07`, profiles `wasm-v1` out and `nativeV1` in;
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

**Kind:** evidence. Records, in
`docs/roadmap/v0.0.3/m3/README.md`, the audit of artifact admission, the
closed host bridge, runtime limits, error mapping, and release metadata, each
with the test that evidences it, and the final counts of both suites per
target.
