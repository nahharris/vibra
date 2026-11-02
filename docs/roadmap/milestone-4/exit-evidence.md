# M4 exit evidence

Captured 2026-11-02 for [Step 22](22-m4-exit.md), on `m4` at `103f301` (the merge
of Step 21b, PR #382). This step claims no new language behavior. It records the
demo gate, the exit-gate clauses, the deliverable coverage, the inventory sweep,
the corpus gate, the integration of every step, and the open items that M4 hands
on. Stage 4A is recorded in [its own report](stage-4a-evidence.md).

Its own PR adds the demo as `examples/m4-demo` with a process test and a
harness test (no language behavior changes), and records one defect that the
gate found and that was fixed first:

- **Step 21b is a defect the exit gate found.** The demo sweep tried a
  continuation-passing loop, a valid program built from closures, and the
  checker refused it with `@tool.unavailable` (`indirect call-flow analysis did
  not converge`); on the CLI's default thread it aborted with a host stack
  overflow. A closure's call-flow summary carried the summaries of its captured
  values, so each wrap of the continuation nested one level deeper, the domain
  had no finite height, and the analysis ran out of its budget or of stack. Step
  6 had met the same refusal and routed around it without an owner. The gate did
  not record it as passing: [Step 21b](21b-call-flow.md) (PR #382, merge
  `103f301`) made the domain finite by construction (a summary names a closure
  by identity, and what a closure expression captures is one joined record per
  expression), removed the budget and `IrError::CallFlowDidNotConverge`, and
  added three cases that run a hundred-thousand-closure chain in both backends.
  Ledger [D35.1](decision-ledger.md). The exit clause "interpreter-v1
  conformance passes for the full language" is evidenced after that fix, with the
  cases below as part of it.

## Integration

Every step is in the integration head by its merge, checked with `gh pr list
--state merged --base m4`, not by its README row. Steps 1 to 11 are in the
[Stage 4A report](stage-4a-evidence.md#integration).

| Step | PR and merge | CI run on the merged head |
| --- | --- | --- |
| 1 | [#354](https://github.com/nahharris/vibra/pull/354), `e7d2e08` | [37085988282](https://github.com/nahharris/vibra/actions/runs/37085988282) |
| 2 | [#355](https://github.com/nahharris/vibra/pull/355), `3289ee7` | [37115960861](https://github.com/nahharris/vibra/actions/runs/37115960861) |
| 2b | [#357](https://github.com/nahharris/vibra/pull/357), `388dfe1` | [37158655486](https://github.com/nahharris/vibra/actions/runs/37158655486) |
| 3 | [#356](https://github.com/nahharris/vibra/pull/356), `8a04c08` | [37155420689](https://github.com/nahharris/vibra/actions/runs/37155420689) |
| 4 | [#358](https://github.com/nahharris/vibra/pull/358), `704b7ad` | [37174529962](https://github.com/nahharris/vibra/actions/runs/37174529962) |
| 5a | [#359](https://github.com/nahharris/vibra/pull/359), `fc374df` | [37186067873](https://github.com/nahharris/vibra/actions/runs/37186067873) |
| 5b | [#360](https://github.com/nahharris/vibra/pull/360), `fb7e1e1` | [37202425526](https://github.com/nahharris/vibra/actions/runs/37202425526) |
| 6 | [#361](https://github.com/nahharris/vibra/pull/361), `e86d93e` | [37206772606](https://github.com/nahharris/vibra/actions/runs/37206772606) |
| 7 | [#362](https://github.com/nahharris/vibra/pull/362), `cc73408` | [37220864041](https://github.com/nahharris/vibra/actions/runs/37220864041) |
| 8a | [#363](https://github.com/nahharris/vibra/pull/363), `a450155` | [38297037394](https://github.com/nahharris/vibra/actions/runs/38297037394) |
| 8b | [#364](https://github.com/nahharris/vibra/pull/364), `705d297` | [38663888335](https://github.com/nahharris/vibra/actions/runs/38663888335) |
| 8c | [#365](https://github.com/nahharris/vibra/pull/365), `d51cc34` | [38825039628](https://github.com/nahharris/vibra/actions/runs/38825039628) |
| 8d | [#366](https://github.com/nahharris/vibra/pull/366), `17fe1ea` | [38851967289](https://github.com/nahharris/vibra/actions/runs/38851967289) |
| 9 | [#367](https://github.com/nahharris/vibra/pull/367), `c1217c1` | [38884251748](https://github.com/nahharris/vibra/actions/runs/38884251748) |
| 10 | [#368](https://github.com/nahharris/vibra/pull/368), `20db8c6` | [38983691557](https://github.com/nahharris/vibra/actions/runs/38983691557) |
| 11 | [#369](https://github.com/nahharris/vibra/pull/369), `1f55ad6` | [39022116218](https://github.com/nahharris/vibra/actions/runs/39022116218) |
| 12 | [#370](https://github.com/nahharris/vibra/pull/370), `d567003` | [39043263954](https://github.com/nahharris/vibra/actions/runs/39043263954) |
| 13 | [#371](https://github.com/nahharris/vibra/pull/371), `437e928` | [39935899219](https://github.com/nahharris/vibra/actions/runs/39935899219) |
| 14 | [#372](https://github.com/nahharris/vibra/pull/372), `fbe53c1` | [39947834315](https://github.com/nahharris/vibra/actions/runs/39947834315) |
| 15 | [#373](https://github.com/nahharris/vibra/pull/373), `f14aa66` | [39993989643](https://github.com/nahharris/vibra/actions/runs/39993989643) |
| 16a | [#374](https://github.com/nahharris/vibra/pull/374), `746e3d3` | [40010071608](https://github.com/nahharris/vibra/actions/runs/40010071608) |
| 16b-1 | [#375](https://github.com/nahharris/vibra/pull/375), `0fa74b0` | [40084090113](https://github.com/nahharris/vibra/actions/runs/40084090113) |
| 16b-2 | [#376](https://github.com/nahharris/vibra/pull/376), `1a175ba` | [40089980267](https://github.com/nahharris/vibra/actions/runs/40089980267) |
| 17 | [#377](https://github.com/nahharris/vibra/pull/377), `0551f21` | [40146123746](https://github.com/nahharris/vibra/actions/runs/40146123746) |
| 18 | [#378](https://github.com/nahharris/vibra/pull/378), `121f453` | [41339914023](https://github.com/nahharris/vibra/actions/runs/41339914023) |
| 19 | [#379](https://github.com/nahharris/vibra/pull/379), `461502e` | [41345121614](https://github.com/nahharris/vibra/actions/runs/41345121614) |
| 20 | [#380](https://github.com/nahharris/vibra/pull/380), `be86296` | [41349533875](https://github.com/nahharris/vibra/actions/runs/41349533875) |
| 21 | [#381](https://github.com/nahharris/vibra/pull/381), `40aec94` | [41444777087](https://github.com/nahharris/vibra/actions/runs/41444777087) |
| 21b | [#382](https://github.com/nahharris/vibra/pull/382), `103f301` | [41453263351](https://github.com/nahharris/vibra/actions/runs/41453263351), still in progress when this was captured; the pull request's own run [41449055836](https://github.com/nahharris/vibra/actions/runs/41449055836) passed every job on its head, and the merged tree is that head plus the merge commit. Re-read the merged run before merging `m4` |

Each run above passed all its jobs (the archive boundary and the build and test
jobs on Ubuntu, Windows, and macOS, and, from Step 4, the conformance corpus in
both backends on the same three), except as the last row says. Step 22's own run
is the PR's.

## Demo gate

The demo is [`examples/m4-demo`](../../../examples/m4-demo), one binary target.
Its manifest declares

```text
effects: (array @std.env.read @std.fs.read @std.io.stdout)
```

and its entry `main` declares `effects: (env.read fs.read io.stdout)`. It reads
`env.read.args`, takes the first argument as a path, reads that file with
`fs.read.text`, quotes each line with `> `, and writes the result with
`io.stdout.text`. It uses only the public standard library. Both sources are in
canonical format (`vibra fmt PATH` prints each unchanged).

### Accepted, through `vibra run` against the real filesystem

From `examples/m4-demo`, with `alpha\nbeta\n` in the file named:

```bash
vibra check
```

`check accepted`, exit 0.

```bash
vibra run src/app -- target/m4demo-in.txt
```

Standard output `> alpha\n> beta\n`, exit 0. The path is chosen at run time:
the program has no path in its source. A missing file is the `err` of
`fs.read.text`, which the entry returns through `try`:

```bash
vibra run src/app -- /nonexistent/file.txt
```

Standard error `(record type: ... variant: @err payload: (record kind: @enum
type: @std.fs.fs-error variant: @not-found))`, exit 6
(`@command.program-failed`, Step 19), with no trap and no diagnostic.

`vibra --format json run src/app -- PATH` returns `@command.ok`, `stdout`
`"> alpha\n> beta\n"`, `trap` null, and the audit trace of three events in
order:

| Event | `op` | argument | result |
| --- | --- | --- | --- |
| 1 | `env.read-args` | none | `ok`, the array holding the path argument |
| 2 | `fs.read-text` | `@std.fs.path` holding the path | `ok`, `"alpha\nbeta\n"` |
| 3 | `io.stdout-text` | `"> alpha\n> beta\n"` | `ok` |

The process test `vibra-cli/tests/process_m4_demo` repeats this with the actual
binary on every CI run (5 tests).

### Rejected, with the witness

The same source, with one root omitted. No grant, prompt, or runtime effect check
participates: the checker refuses it before any backend runs, `vibra run` of the
rejected source performs nothing (empty trace, empty standard output), and the
diagnostic is the one `check` reports.

| Omitted from | Diagnostic and position | Witness |
| --- | --- | --- |
| the function ceiling `(env.read io.stdout)` | `@effect.outside-ceiling` at the call of `fs.read.text` in `src/app/main.vib`; exit 1 | note `performs @std.fs.read via @std.fs.read.text` |
| the target array, `@std.fs.read` | `@effect.outside-ceiling` at the `entry:` value in `project.vibon`; exit 1 | related span at the call, `performs @std.fs.read via @std.fs.read.text` |

The process test and the harness test cover each of the three roots in both
places (6 variants), each with exactly one diagnostic and its witness.

### Both backends

The host test
[`demo_gate_m4_step22`](../../../crates/vibra-conformance/tests/demo_gate_m4_step22.rs)
runs the checked program under a host world that supplies the argument
(`/in/note.txt`) and the file, in the reference interpreter and, lowered, in the
Wasm backend, with `cargo test --locked --offline -p vibra-conformance --test
demo_gate_m4_step22 -- --nocapture`:

- the canonical results are equal;
- the audit traces are equal as events and, rendered with `trace_to_vibon`, as
  bytes, and the standard output that the `io.stdout-text` events carry is
  `"> alpha\n> beta\n"` in both;
- a different world (another argument and file) gives different output from the
  same checked program, so the path is read at run time;
- a rejected variant yields no program for a backend to run.

The corpus case `V1-RUNTIME-env-demo-args-name-a-file` holds the same shape (a
world supplies the argument and file; trace and result expected, matched in
both backends), `V1-RUNTIME-fs-demo-path-from-stdin` the stdin variant, and the
rejections `V1-EFFECT-host-function-ceiling-omits-fs-read`,
`V1-EFFECT-host-target-array-omits-fs-read`, `-env-read`, `-stdout`,
`V1-EFFECT-demo-gate-function-ceiling-omits`, and
`V1-EFFECT-demo-gate-target-array-omits` (an additive root, two witness hops).

## Exit-gate clauses

Case IDs run in both backends (or, for a static case, in the checker); host tests
are in `crates/vibra-conformance/tests` unless a crate is named.

| Clause | Evidence |
| --- | --- |
| An additive root declared on an effect operation reaches the performed row, the rejection diagnostic, and the target array of every transitive caller, and `@effect.outside-ceiling` names the call witness for each missing root | Performed row: `V1-EFFECT-additive-root-accepted`, `V1-EFFECT-host-additive-row`, `V1-EFFECT-recursion-closes`, `V1-RUNTIME-effect-operations`, `effects_m4_step14`. Rejection and witness: `V1-EFFECT-witness-three-hops`, `-shortest-chain`, `-equal-chains`, `-function-value`, `-contract-member`, `-chapter-example`, `V1-EFFECT-imported-root-witness`, `V1-EFFECT-missing-two-roots` (one witness per missing root), `V1-EFFECT-offending-callee-no-cascade`. Target array: `V1-EFFECT-target-array-additive-accepted`, `-omits-both`, `-entry-over-own-ceiling`, the demo-gate pair and `V1-EFFECT-host-target-array-omits-*`; `target_consent_m4_step15` (vibra-workspace). Ledger D23.1, D23.2, D24.1, D14 to D15 |
| Every host operation has one registry entry, owner effect, typed signature, and audit-event shape | `host_registry_snapshot_m4_step16` (vibra-ir): the 17 rows, their signatures, owners, closed error enums, and the checked-in snapshot, failing on a changed, removed, or added-without-`since` row; `@host` checked against the registry for symbol, owner, and signature (`V1-EFFECT-host-owner-mismatch`, `V1-EFFECT-host-signature-mismatch`); one audit shape, `audit_events_m4_step16` (vibra-host); every row has a corpus case in `V1-RUNTIME-io-*`, `-fs-*`, `-env-*`, `-time-*`, `-random-*` and a host test (`fs_operations_m4_step17`, `env_clock_random_m4_step18` in vibra-host); `host_single_source_m4_step16b` (no operation, provider call, or event built outside `vibra-host`). Ledger D17, D18, D19.1 |
| No compiler-generated ambient host read exists outside the registry | `ambient_reads_m4_step16`: no crate that checks, lowers, or executes a program reads ambient state, the system provider alone does, and the scan itself is tested; `host_single_source_m4_step16b` |
| Success, typed host error, propagation, and trap have interpreter/Wasm parity | Success: `V1-RUNTIME-io-write-text-and-bytes`, `-io-stdin-text-twice`, `-fs-read-file-and-text`, `-fs-write-then-read`, `-env-read-var`, `-time-wall-from-the-world`, `-random-seeded-bytes`, `V1-RUNTIME-env-demo-args-name-a-file`. Typed host error: `V1-RUNTIME-io-invalid-text`, `-fs-errors-are-values`, `-fs-invalid-text`. Propagation: `V1-RUNTIME-io-error-propagates-with-try`, `-fs-error-propagates-with-try`, `-env-error-propagates-with-try`, `V1-RUNTIME-entry-outcome-own-err`. Trap and host event: `V1-RUNTIME-entry-outcome-trap-holding-err`, `-entry-outcome-memory-holding-err`, `trap_origins_m4_step11` and `tests_traps_m4_step11` (every registered trap code with its origin); `host_boundary_m4_step16b` (hand-built IR through both backends); `entry_outcome_m4_step19`. All matched (parity inventory). Ledger D21, D30 |
| Deep non-tail recursion has the same specified outcome in both backends | `V1-RUNTIME-activation-depth`, `-activation-depth-counter` (a hundred thousand deep, a result), `-activation-exhaustion`, `-activation-exhaustion-discarded-call`, `-workspace-run-activation-exhaustion`, `-workspace-test-activation-exhaustion` (`@runtime.memory-exhausted`), and, since Step 21b, `V1-RUNTIME-continuation-chain-non-tail` (a hundred thousand activations and closures at once, a result); `activations_m4_step3`, `calls_m4_step6`. Re-run on the exit head, unchanged. Ledger D1.1 to D1.4 |
| A tail-recursive loop allocating a fresh compound value per iteration holds a bounded live arena in the Wasm backend | `calls_m4_step6::a_tail_loop_that_allocates_each_round_holds_a_bounded_live_arena` (seven loop kinds at 10,000 and 100,000 rounds), the Stage 4A demo's walk, `V1-RUNTIME-continuation-chain` and `-mutual` (a tail loop whose live set is the chain itself, released with bounded stack). Ledger D3.2 |
| Interpreter-v1 conformance passes for the full language, and the Wasm backend matches it on every executable case | The corpus gate below: 176 interpreter-v1 cases pass; 168 of 168 executable cases matched, 0 not lowered. The Step 21b fix is part of this evidence |

## Deliverable coverage

Each M4 deliverable of [`v1.md`](../v1.md#milestone-4--webassembly-spine-static-effects-and-host-operations)
and the owning step's evidence. The Stage 4A rows are evidenced in
[their report](stage-4a-evidence.md#stage-4a-exit-clauses), re-run here.

| Deliverable | Evidence |
| --- | --- |
| Unoptimized Wasm lowering of the complete pure language | Stage 4A report; `lowers_every_form_m4_step11`; 168 of 168 matched |
| Native implementations written once; body/native differential joined to the harness | Stage 4A report; `natives_m4_step10` |
| Instance-owned arena and opaque indices; no value index or instance identity in typed IR or build output | Stage 4A report; `typed_ir_identity_m4_step4` (**forward-compatibility obligation 1**). The audit events and the host world carry values only (`audit_events_m4_step16`). There is no build output before M7, which inherits the rule |
| Specified arena reclamation rule | Stage 4A report; `arena_runtime_m4_step5a`, `calls_m4_step6` |
| Interpreter/Wasm differential harness over the entire executable corpus | `differential_m4_step4`, `parity_inventory_m4_step4` (168 rows, all `matched`) |
| `deffect` operations, default-empty ceilings, transitively closed rows, imported-symbol effect references, effect metadata in general queries | Step 14: `V1-EFFECT-*` (51 cases), `effects_m4_step14`; Step 20: `V1-TOOL-index-effects`, `V1-TOOL-workspace-position-effects`, `-effect-witnesses`, `-effects-recovery`, `effect_metadata_m4_step20` (vibra-workspace) |
| Required binary-target effect arrays and static admission of the entry | Step 15: `V1-EFFECT-target-array-*`, `target_consent_m4_step15`, `init_consent_m4_step15` (the `project init` template) |
| Typed `@host` externals and the closed `vibra_v1` registry in both backends; **the registry version rule** | Steps 16a to 18; `host_registry_snapshot_m4_step16`. **Forward-compatibility obligation 2:** the registry is `1.N` and ships `1.0`, every row `since: 0`; adding a row is `1.N+1` and the snapshot test passes an added row only with a greater `since`; changing a symbol, signature, owner, or event shape requires `vibra_v2`; a build records the exact required symbols and the derived minor, the greatest `since` among them (`derived_registry_minor`; the build record is M7's). Ledger D19.1, D19.2 |
| Whole-value console, filesystem, environment-read, clock, and random operations, and `env.read.args` | Steps 16b to 18: the `V1-RUNTIME-io-*` (7), `-fs-*` (9), `-env-*` (5), `-time-*` (3), `-random-*` (2) and `-world-sets-env-args-clock-random` cases; `process_m4_console`, `process_m4_filesystem`, `process_m4_arguments` (vibra-cli) |
| Deterministic in-memory host world, one schema, and ordered audit traces with **one versioned encoding that tolerates new event kinds** | `host_world_m4_step16`, `host_world_corpus_m4_step16`; **forward-compatibility obligation 3:** `audit_events_m4_step16::an_event_of_an_unknown_kind_is_preserved_as_opaque_data` and `a_trace_is_the_record_of_its_events_and_keeps_unknown_kinds` (an unknown `kind` is kept as opaque data, an unknown field in a known kind is rejected), one `@audit-trace.v1` record produced by one function for both backends. Ledger D18.1, D18.2 |
| Effect rows in typed IR, query schemas, and build metadata that **do not assume a closed literal set** | **Forward-compatibility obligation 4:** a row is a record everywhere, `EffectRow { roots, tail: None }` in typed IR, `(record roots: ...)` in VIBON, `{"roots": [...]}` in JSON, the canonical `fn` encoding, the position envelope, and the index (`effect_rows_m4_step14` in vibra-fmt, `effect_metadata_m4_step20`); `tail` is the absence of a row variable, so a variable is an additive change. Rows are not assumed closed in the checker either: they are sets closed by a worklist, not enumerations. Ledger D25.1. Build metadata is M7's and inherits the record |
| Source-position metadata showing performed, enclosing, and target rows | Step 20: `V1-TOOL-workspace-position-effects`, `-effect-witnesses` |
| The M3 reassigned forms | Abstract contract member with its own generics, labelled operands and written `types:` arguments, a dict variadic tail on a contract member: Steps 2 and 9, `contract_members_m4_step2`, `interfaces_m4_step9`, `V1-RUNTIME-contract-member-generics`, `-contract-member-labelled`, `-contract-member-dict-tail`, `-contract-member-form-values`. The command result and exit code for an entry that returns `err`: Step 19, `V1-RUNTIME-entry-outcome-own-err`, exit 6 (shown above) |
| A specified outcome for deep non-tail recursion, written before the Wasm backend | Steps 1, 3, 6; the exit clause above |
| Effectful iteration that walks with `iter.next` and tail recursion | Step 16b-2: `V1-RUNTIME-io-effectful-iter-walk`, `examples/m4-console` (`console.walk.write-all`), `process_m4_console` |
| Recorded interpreter and unoptimized-Wasm performance baseline | Step 21: [the baseline](performance-baseline.md) and `performance-baseline.vibon`, ten programs in both backends; `performance_baseline_m4_step21` and `perf-baseline --check` on the exit head, below |

## Inventory sweep

`m4_contract_inventory` fails when an AST variant has no M4 disposition, when a contract row has no
owner, or when a guide is not linked; it passes on the
exit head. Every row of the [M4 surface inventory](supported-surface.md) and every
inherited row (I1 to I19 of the [M3 deferral inventory](README.md#m3-deferral-inventory),
plus the Stage 4B contract rows) is implemented in both backends, assigned to a
named later milestone, or static by design:

| Row | Disposition |
| --- | --- |
| `Declaration::Deffect`, nonempty `Attribute::Effects`, `Attribute::External` with `@host` | Implemented (Steps 14, 16b-2) |
| The four contract-member forms and the pre-M4 C16 effect-row rows | Implemented (Steps 2, 9, 14) |
| The entry that returns `err` (G16) | Implemented (Step 19) |
| The conformance chapter's effectful walk over `iter.next` | Implemented (Step 16b-2) |
| The 17-row registry, its versioning, the ABI, the audit record, the world, program arguments, the effect diagnostics, the manifest `host` list, the effect fields of queries, the baseline | Implemented (Steps 13 to 21) |
| Continuation chains through the call-flow analysis | Fixed (Step 21b) |

**No M4-owned form reports `@tool.unavailable`.** To check that no valid program
reaches it, every emission was listed and given its owner; the remainder are
trust boundaries, later milestones, single-source or bootstrap limits, or
internal-consistency checks that a checked program does not reach:

| Emission | Owner |
| --- | --- |
| Ordinary dependency delivery, `ordinary dependency delivery is unavailable in M2` (`V1-PROJECT-graph-static-dependency`) | M5 |
| The public `query` command, and `build` | `Action::Unavailable` in the CLI: `query` is M6, `build` is M7 |
| `native:`, `role:`, `external:` (`@compiler`, and `@host` outside the embedded library), and the intrinsic-type body outside the embedded standard library (`V1-RUNTIME-external-untrusted`, `V1-TYPE-NAMES-library-attributes`, `V1-TYPE-NOMINAL-intrinsic-type-user`) | A trust boundary, by design; M5 turns it into the toolchain-anchored trust contract |
| The standard-library import and `@std.assert` without verified bootstrap provenance; the exact-import limit of the single-source `@std.text` adapter (`V1-TYPE-INFER-bootstrap-extra-import`); a source with no value or function (`V1-TYPE-NAMES-binding-empty-module`); `test` outside the `@tests` unit; assertions outside it | Toolchain or single-source limits; the workspace path checks the same programs |
| `this declared type names a type outside the available M3 profile` | A cascade from a type already reported, not an independent refusal |
| `checked IR construction failed: ...` | The IR constructor's consistency checks. The one that could be reached by a valid program, the call-flow one, was removed in Step 21b. `IrError::RecursiveCall` cannot be raised (its `unresolved` set is never filled); `GlobalInitializerCycle` has its own error diagnostic before it (`V1-PROJECT-workspace-check-*-initializer-cycle`, `V1-TYPE-INFER-initializer-cycle-closure-global`) |
| The Wasm `Form`s `module-size` and `parameters` | `module-size` is a program of more than 2^32 functions, globals, or constant bytes; `parameters` is an entry with parameters, which the checker already refuses (`@project.invalid-entry-signature`); neither is a valid program. `lowers_every_form_m4_step11` holds `NotLowered` to hand-built IR only, and the runner reports 0 not lowered over all 168 executable cases |

A scratch probe on the exit head also ran a record holding a continuation, a
factory function that wraps a continuation, and 50,000 rounds of each through
`vibra run` (exit 0).

## Corpus gate

The full corpus reports 572 passed, 0 failed, and 0 unavailable, by profile:

| Profile | Passed | Failed | Unavailable |
| --- | --- | --- | --- |
| reader-v1 | 83 | 0 | 0 |
| static-v1 | 293 | 0 | 0 |
| interpreter-v1 | 176 | 0 | 0 |
| tooling-v1 | 20 | 0 | 0 |

By backend, over the 168 executable cases (the interpreter cases that expect
acceptance: 110 `interpret`, 34 `workspace-run`, and 24 `workspace-test`):

| Backend | Passed or matched | Failed | Unavailable or not lowered |
| --- | --- | --- | --- |
| interpreter | 168 | 0 | 0 |
| Wasm | 168 matched | 0 | 0 not lowered |

**The parity inventory's final state.** `conformance/parity.tsv` holds 168 rows,
every one `matched` and none `not-lowered`; `parity_inventory_m4_step4` holds the
table to the corpus and the runner. Compared with Stage 4A (131 rows), the 37
additions are Step 14 (4), 16b-2 (7), 17 (9), 18 (11), 19 (3), and 21b (3).

## Open items for later

Recorded plainly; none is fixed here, and none blocks the gate. They are the
"existing exceptions" backlog of the maintainer's 2026-10-03 rulings, with no new
exception introduced by M4.

**Coherence backlog**

- `(import option ...)` aliases: import aliases spelled as prelude names are
  accepted (21 files) although prelude names need no import and cannot be
  shadowed.
- Prelude types and values reach modules by two mechanisms: types through the role
  shortcut, values through the always-loaded `@std.bool` mechanism.
- Two spellings of an effect root appear in the position facts and the index
  rows, and nothing yet states which is canonical for a consumer.
- The resolver hard-codes the `assert.true`, `assert.false`, and `assert.equal`
  declarations; "unhardcode assert" is scheduled for later.
- The `tuple` reserved-spelling special case in `is_reserved_value_spelling`:
  `tuple` is reserved for static methods by dotted path, a reason that stopped
  applying when `tuple.of` was removed (D11.1); the reserved lists also no longer
  match the specification's classes after D17.1. Awaiting a ruling.
- The float and `void` literal-pattern exclusion (accepted for now: floats are
  deferred, `void` patterns are niche).
- The position query reports `@literal` for `(bool.true)`, where the tooling
  chapter's pattern-kind list has `@variant`: the internal boolean lowering leaks
  into tooling output.
- Reserved-spelling diagnostics span the name in the AST layer and the whole
  declaration in the resolver and checker.

**Other findings**

- A labelled default on a generic-typed parameter is checked without an expected
  type.
- Single-source `interpret` takes the first function as the entry; an entry with a
  parameter surfaces as `InvalidBody`.
- The `array.fold` body slices the array every step, so a fold run by the Wasm
  module is quadratic (about 40 seconds for 10,000 elements in a debug build); a
  standard-library rewrite for later, noted by the [baseline](performance-baseline.md).
- Debug `cargo test` takes minutes (the parity inventory and the project suite
  are the longest), which matters for CI cost.
- The call-flow analysis is now context-insensitive over a closure expression's
  creation sites (D35.1). It is sound and can only over-approximate call edges; a
  program whose extra edge would form a spurious initializer cycle is not known,
  and none is in the corpus.

Closed by this gate: the Step 6 finding that the checker reported
`@tool.unavailable` on a continuation closure passed through a loop (Step 21b).

## Commands

Run from the repository root with the pinned toolchain (Rust 1.96.1),
`CARGO_INCREMENTAL=0`, `--locked --offline`, on the code head `eef3139` (the later commit changes only this document's table and one README link).

| Command | Exit | Result |
| --- | --- | --- |
| `cargo fmt --all --check` | 0 | clean |
| `cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings` (`RUSTFLAGS=-D warnings`) | 0 | clean |
| `cargo test --locked --offline --workspace --all-targets --all-features` | 0 | 1,444 passed, 0 failed |
| `cargo doc --locked --offline --workspace --no-deps --all-features` (`RUSTDOCFLAGS=-D warnings`) | 0 | clean |
| `cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases` | 0 | 572 passed, 0 failed, 0 unavailable; Wasm 168 matched, 0 failed, 0 not lowered |
| `cargo test --locked --offline -p vibra-conformance --test evidence_step11 --test m4_contract_inventory --test m3_contract_inventory --test diagnostic_registry --test parity_inventory_m4_step4 --test architecture_boundary` | 0 | all pass |
| `cargo run --locked --offline -p vibra-conformance --bin m1-fuzz -- --profile ci-smoke` | 0 | 6 targets, 96 cases pass |
| `cargo run --locked --offline -p vibra-conformance --bin perf-baseline -- --check` | 0 | the exact metrics match `performance-baseline.vibon` |
| `git diff --check origin/m4 HEAD` | 0 | clean |
| `vibra check`, `vibra run src/app -- PATH` in `examples/m4-demo` | 0, 0 | `check accepted`; `> alpha`, `> beta` |

A first run of the workspace tests stopped at `m4_contract_inventory`, which
requires the README to link the Step 16b guide with a plain link; the rewritten
README header had dropped it. The link is restored and the whole list above is
the second run.

## Verdict

The M4 exit gate is met. The demo gate holds in both backends and through
`vibra run`, with its rejections and witnesses; every exit-gate clause maps to
cases, host tests, and ledger rows; every deliverable and each
forward-compatibility obligation has evidence; every `@tool.unavailable` that
remains is owned by a later milestone, a trust boundary, or a toolchain limit;
and the corpus has 0 failed, 0 unavailable, and 0 not lowered over 168
executable cases. The gate found one defect, a valid continuation-passing
program that the checker refused, and it was fixed first as Step 21b rather than
recorded as passing. The open items above are the maintainer's backlog. The
Step 22 pull request is conditional on its merge, and the merge of `m4` into
`main` is the maintainer's.
