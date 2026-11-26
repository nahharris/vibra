# v0.0.2 M2: operations and authority

**Exited.** Procedures, effect ceilings, grants, and the entry contract, on
the interpreter, with fake and real drivers. The exit record is
[exit.md](exit.md).

The design is in the [architecture](architecture.md). What the plan assumes
where the specification is silent is in the
[specification questions](spec-questions.md). Each step has a guide under
[steps/](steps/) that is the whole task of the agent that implements it.

## Scope, in slices

1. **Procedures and plans.** `proc`, `performs`, `Proc` and `Intent` types,
   plan construction, `await`, exact rows, guards on procedures.
2. **Operation interfaces.** The seventeen host operations of v0.0.1 as one
   nominal interface each, with their error types, under the new registry
   major version.
3. **Grants.** Sealed grant values, required and optional parameters,
   narrowing by selecting a component and by pure restriction, shared
   accounting, typed denial.
4. **Policy and binding.** Policy documents, the `@project.v2` target with
   its admission ceiling and grant bindings, build-time validation, startup
   acquisition in dependency order, reverse release.
5. **Entry.** The entry signature, `ExitCode`, and the outcome mapping of
   `vibra run`.
6. **Test worlds.** `TestWorld`, fixture-scoped fake grants, the
   `@hostWorld.v2` document, the completed-call audit trace with grant
   projections.
7. **Real drivers.** Console, filesystem, environment, clock, and random on
   Windows, with the file-scope adapter of M0.

## Exit evidence

- The release program of [v0.0.2](../README.md#release-gate) runs under
  `vibra run`.
- Rejected at compile time: a callback whose row exceeds its parameter's
  row; an operation outside a ceiling; an entry row outside the target's
  admission ceiling; a missing or cyclic binding.
- Denied or absent at run time, as values: a request outside a grant's
  policy; a refused optional grant; an exhausted shared limit.
- Attack cases of the effects chapter for file scopes, run on Windows
  through the real adapter and in the fake world.
- No grant is obtainable from data, from a row alone, or outside a test
  world by test code.

## The model-writing pilot

After the gate above, freeze and run the registered model-writing pilot: two
surface arms that differ only in whether an ordinary `let` ends with
`endlet`, ten tasks, two model families, graded by this milestone's checker
and test world. Reference solutions and negative controls pass before any
model call. The result is a dated observation about that one choice on those
tasks and models. If it favors the other arm under its registered rule, the
change returns to the specification. The
[observation of 2026-11-26](pilot/observation.md) answers that the default
stays: `let … be … endlet` remains the specified form.

## Not in this milestone

Network operations, concurrency, and native execution.

## Step table

A step is `planned`, `in progress`, or `landed`. The status column is edited
on the integration branch when a step merges, never on a step's own branch.
"Can run in parallel with" lists the steps that are neither before nor after
this one in the dependency graph; no two of them own one file.

| Id | Name | Kind | Depends on | Can run in parallel with | Crates touched | Files owned | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 01 | [seams](steps/01-seams.md) | infrastructure | — | — | `vibra-conformance`, `vibra-host`, `vibra-workspace` | [list](steps/01-seams.md#files-owned) | landed |
| 02 | [host-registry-2](steps/02-host-registry-2.md) | infrastructure | 01 | 13, 18 | `vibra-ir`, `vibra-host`, `vibra-interp`, `vibra-types` | [list](steps/02-host-registry-2.md#files-owned) | landed |
| 03 | [ir-procedures](steps/03-ir-procedures.md) | infrastructure | 02 | 13, 14, 18 | `vibra-ir`, `vibra-interp`, `vibra-types` | [list](steps/03-ir-procedures.md#files-owned) | landed |
| 04 | [checker-effects-seam](steps/04-checker-effects-seam.md) | infrastructure | 03 | 06, 13, 14, 15, 16, 17, 18 | `vibra-types` | [list](steps/04-checker-effects-seam.md#files-owned) | landed |
| 05 | [proc-and-plan](steps/05-proc-and-plan.md) | vertical | 04 | 06, 13, 14, 15, 16, 17, 18 | `vibra-types`, `conformance/cases` | [list](steps/05-proc-and-plan.md#files-owned) | landed |
| 06 | [interp-plans-and-await](steps/06-interp-plans-and-await.md) | vertical | 03 | 04, 05, 13, 14, 15, 16, 17, 18 | `vibra-interp` | [list](steps/06-interp-plans-and-await.md#files-owned) | landed |
| 07 | [procedures-run](steps/07-procedures-run.md) | vertical | 05, 06 | 13, 14, 15, 16, 17, 18 | `vibra-types`, `vibra-workspace`, `conformance/cases` | [list](steps/07-procedures-run.md#files-owned) | landed |
| 08 | [operation-interfaces](steps/08-operation-interfaces.md) | vertical | 07 | 13, 14, 15, 16, 17, 18 | `vibra-types`, `conformance/cases` | [list](steps/08-operation-interfaces.md#files-owned) | landed |
| 09 | [ceilings](steps/09-ceilings.md) | vertical | 08 | 13, 14, 15, 16, 17, 18 | `vibra-types`, `conformance/cases` | [list](steps/09-ceilings.md#files-owned) | landed |
| 10 | [callable-rows](steps/10-callable-rows.md) | vertical | 09 | 13, 14, 15, 16, 17, 18 | `vibra-types`, `conformance/cases` | [list](steps/10-callable-rows.md#files-owned) | landed |
| 11 | [review-rows](steps/11-review-rows.md) | evidence | 10 | 13, 14, 15, 16, 17, 18 | none | [list](steps/11-review-rows.md#files-owned) | landed |
| 12 | [fix-rows](steps/12-fix-rows.md) | evidence | 11 | 13, 14, 15, 16, 17, 18 | `vibra-ir`, `vibra-interp`, `vibra-types`, `conformance/cases` | [list](steps/12-fix-rows.md#files-owned) | landed |
| 13 | [fs-scope-complete](steps/13-fs-scope-complete.md) | infrastructure | 01 | 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30 | `vibra-fs-scope` | [list](steps/13-fs-scope-complete.md#files-owned) | landed |
| 14 | [policy-and-recipe-data](steps/14-policy-and-recipe-data.md) | infrastructure | 02 | 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 18, 19, 20 | `vibra-host` | [list](steps/14-policy-and-recipe-data.md#files-owned) | landed |
| 15 | [broker](steps/15-broker.md) | vertical | 03, 14 | 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 18, 19, 20 | `vibra-host`, `vibra-conformance` | [list](steps/15-broker.md#files-owned) | landed |
| 16 | [host-world](steps/16-host-world.md) | vertical | 15 | 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 17, 18, 19, 20 | `vibra-host` | [list](steps/16-host-world.md#files-owned) | landed |
| 17 | [audit-trace](steps/17-audit-trace.md) | vertical | 15 | 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 16, 18, 19, 20 | `vibra-host` | [list](steps/17-audit-trace.md#files-owned) | landed |
| 18 | [stdlib-policy-types](steps/18-stdlib-policy-types.md) | vertical | 01 | 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15, 16, 17, 20 | `stdlib`, `conformance/cases` | [list](steps/18-stdlib-policy-types.md#files-owned) | landed |
| 19 | [stdlib-operations](steps/19-stdlib-operations.md) | vertical | 02, 12, 18 | 13, 14, 15, 16, 17, 20 | `stdlib`, `vibra-types`, `conformance/cases` | [list](steps/19-stdlib-operations.md#files-owned) | landed |
| 20 | [interp-host-dispatch](steps/20-interp-host-dispatch.md) | vertical | 06, 12 | 13, 14, 15, 16, 17, 18, 19 | `vibra-interp` | [list](steps/20-interp-host-dispatch.md#files-owned) | landed |
| 21 | [test-world](steps/21-test-world.md) | vertical | 16, 17, 19, 20 | 13 | `stdlib`, `vibra-host`, `vibra-types`, `vibra-workspace`, `vibra-schema`, `vibra-conformance`, `conformance/cases` | [list](steps/21-test-world.md#files-owned) | landed |
| 22 | [grant-values](steps/22-grant-values.md) | vertical | 21 | 13 | `vibra-types`, `conformance/cases` | [list](steps/22-grant-values.md#files-owned) | landed |
| 23 | [review-grants](steps/23-review-grants.md) | evidence | 22 | 13 | none | [list](steps/23-review-grants.md#files-owned) | landed |
| 24 | [fix-grants](steps/24-fix-grants.md) | evidence | 23 | 13 | `vibra-host`, `vibra-interp`, `vibra-types`, `stdlib`, `conformance/cases` | [list](steps/24-fix-grants.md#files-owned) | landed |
| 25 | [entry-and-admission](steps/25-entry-and-admission.md) | vertical | 24 | 13, 28, 30, 31 | `vibra-types`, `vibra-workspace`, `conformance/cases` | [list](steps/25-entry-and-admission.md#files-owned) | landed |
| 26 | [binding-recipes](steps/26-binding-recipes.md) | vertical | 14, 25 | 13, 28, 30, 31 | `vibra-workspace`, `conformance/cases` | [list](steps/26-binding-recipes.md#files-owned) | landed |
| 27 | [local-dependencies](steps/27-local-dependencies.md) | vertical | 26 | 13, 28, 29, 30, 31, 32, 33 | `vibra-workspace`, `conformance/cases` | [list](steps/27-local-dependencies.md#files-owned) | landed |
| 28 | [startup-and-release](steps/28-startup-and-release.md) | vertical | 16, 17, 24 | 13, 25, 26, 27 | `vibra-host` | [list](steps/28-startup-and-release.md#files-owned) | landed |
| 29 | [run-in-world](steps/29-run-in-world.md) | vertical | 20, 21, 26, 28 | 13, 27, 30, 31 | `vibra-workspace`, `vibra-conformance` | [list](steps/29-run-in-world.md#files-owned) | landed |
| 30 | [system-drivers](steps/30-system-drivers.md) | vertical | 24, 28 | 13, 25, 26, 27, 29 | `vibra-host` | [list](steps/30-system-drivers.md#files-owned) | landed |
| 31 | [system-fs-driver](steps/31-system-fs-driver.md) | vertical | 13, 30 | 25, 26, 27, 29 | `vibra-host` | [list](steps/31-system-fs-driver.md#files-owned) | landed |
| 32 | [cli-run](steps/32-cli-run.md) | vertical | 29, 30, 31 | 27, 33 | `vibra-cli`, `vibra-schema` | [list](steps/32-cli-run.md#files-owned) | landed |
| 33 | [dual-run-and-attacks](steps/33-dual-run-and-attacks.md) | vertical | 29, 31 | 27, 32 | `vibra-workspace` | [list](steps/33-dual-run-and-attacks.md#files-owned) | landed |
| 34 | [review-drivers](steps/34-review-drivers.md) | evidence | 27, 32, 33 | — | none | [list](steps/34-review-drivers.md#files-owned) | landed |
| 35 | [fix-drivers](steps/35-fix-drivers.md) | evidence | 34 | — | `vibra-fs-scope`, `vibra-host`, `vibra-workspace`, `vibra-cli` | [list](steps/35-fix-drivers.md#files-owned) | landed |
| 36 | [release-program](steps/36-release-program.md) | vertical | 35 | — | `examples`, `vibra-cli` | [list](steps/36-release-program.md#files-owned) | landed |
| 37 | [m2-exit](steps/37-m2-exit.md) | evidence | 36 | — | `vibra-conformance`, `conformance/cases` | [list](steps/37-m2-exit.md#files-owned) | landed |
| 38 | [pilot-freeze](steps/38-pilot-freeze.md) | evidence | 37 | — | none | [list](steps/38-pilot-freeze.md#files-owned) | landed |
| 39 | [pilot-references](steps/39-pilot-references.md) | evidence | 38 | — | `examples` | [list](steps/39-pilot-references.md#files-owned) | landed |
| 40 | [pilot-runs](steps/40-pilot-runs.md) | evidence | 39 | — | none | [list](steps/40-pilot-runs.md#files-owned) | landed |
| 41 | [pilot-observation](steps/41-pilot-observation.md) | evidence | 40 | — | none | [list](steps/41-pilot-observation.md#files-owned) | landed |

### How the steps run

Two lanes, and at most two implementation steps at a time.

- **Static lane:** the checker, the library, the project file, the
  command. Steps 04, 05, 07 to 12, 18, 19, 22, 25 to 27, 32.
- **Dynamic lane:** the checked IR, the interpreter, the host, the
  file-scope adapter. Steps 02, 03, 06, 13 to 17, 20, 28, 30, 31, 33.

Steps 01, 21, 29, and 36 join the lanes: nothing else runs beside the first
and the last, and beside 21 and 29 only a step that the table lists.
The three review gates (11, 23, 34) read a merged tree and write findings;
the fix step after each owns the files the findings can touch, so nothing
that shares those files runs beside it: step 20 edits the interpreter
files that step 12 repairs, and starts after it. A step that uses a
function or a fixture of another step depends on that step: 30 on 28 for
admission, 32 on 31 for the directory driver.

A suggested order that respects the table, one pair per line:

| Static lane | Dynamic lane |
| --- | --- |
| 01 | — |
| 18 | 02 |
| — | 03 |
| 04 | 06 |
| 05 | 14 |
| 07 | 15 |
| 08 | 16 |
| 09 | 17 |
| 10 | 13 |
| 11, 12 | — |
| 19 | 20 |
| 21 | — |
| 22 | — |
| 23, 24 | — |
| 25 | 28 |
| 26 | 30 |
| 27 | 31 |
| 29 | — |
| 32 | 33 |
| 34, 35 | — |
| 36, 37 | — |
| 38 to 41 | — |

## Slices and steps

| Slice | Steps |
| --- | --- |
| 1. Procedures and plans | 03, 04, 05, 06, 07, 09, 10; gate 11, 12 |
| 2. Operation interfaces | 02, 08, 18, 19 |
| 3. Grants | 14, 15, 20, 22; gate 23, 24 |
| 4. Policy and binding | 14, 26, 27, 28 |
| 5. Entry | 25, 29, 32 |
| 6. Test worlds | 16, 17, 21 |
| 7. Real drivers | 13, 30, 31, 33; gate 34, 35 |
| The release program and the exit | 36, 37 |
| The model-writing pilot | 38, 39, 40, 41 |
| Seams that serve every slice | 01, 04 |

| Step | Slice or purpose |
| --- | --- |
| 01 | seams: rule files per step, the two crate edges, module declarations |
| 02 | 2: the registry of major 2 and the test-world registry |
| 03 | 1: the checked IR of procedures and its validation |
| 04 | 1, and the seam of every checker step |
| 05 | 1: named `proc`, plans, `await` |
| 06 | 1: plans and `await` in the interpreter |
| 07 | 1: procedures run under a test |
| 08 | 2, and the rows of 1 |
| 09 | 1: ceilings and witnesses |
| 10 | 1: callable types with rows |
| 11, 12 | the review gate of slice 1 and its fixes |
| 13 | 7: the file-scope adapter completed |
| 14 | 3 and 4: policy and recipe data |
| 15 | 3: the broker |
| 16 | 6: the fake world |
| 17 | 6: the audit trace |
| 18 | 2: `@std.policy`, `@std.process` |
| 19 | 2: the five operation modules |
| 20 | 3: grants in the interpreter |
| 21 | 6: `TestWorld` |
| 22 | 3: grant values in source |
| 23, 24 | the review gate of slices 2, 3, and 6 and its fixes |
| 25 | 5: the entry and the admission ceiling |
| 26 | 4: binding recipes |
| 27 | 4: local dependencies |
| 28 | 4: startup and release |
| 29 | 5: the run |
| 30 | 7: streams, environment, clocks, random |
| 31 | 7: the directory driver |
| 32 | 5: `vibra run` |
| 33 | 7: the dual run and the attack runs |
| 34, 35 | the review gate of slices 4, 5, and 7 and its fixes |
| 36 | the release program |
| 37 | the exit record |
| 38 to 41 | the pilot: freeze, references and controls, runs, observation |

## Exit clauses

| Exit clause | Steps that produce it | Recorded by |
| --- | --- | --- |
| the release program runs under `vibra run` | 29, 31, 32, 36 | 36, 37 |
| rejected: a callback whose row exceeds its parameter's row | 10 (`V2-EFFECT-callback-row-wider`), 36 | 37 |
| rejected: an operation outside a ceiling | 09 (`V2-EFFECT-outside-ceiling-direct`), 36 | 37 |
| rejected: an entry row outside the admission ceiling | 25 (`V2-EFFECT-entry-outside-ceiling`), 36 | 37 |
| rejected: a missing or cyclic binding | 26 (`V2-GRANT-binding-missing`, `V2-GRANT-binding-cycle`), 36 | 37 |
| a value: a request outside a grant's policy | 22 (`V2-GRANT-denied-outside-policy`), 29, 36 | 37 |
| a value: a refused optional grant | 28, 29 (`V2-RUNTIME-run-optional-absent`), 36 | 37 |
| a value: an exhausted shared limit | 22 (`V2-GRANT-limit-siblings-share`), 36 | 37 |
| file-scope attack cases through the real adapter | 13, 31, 33 | 33, 37 |
| file-scope attack cases in the fake world | 22 (prefix, suffix, correlation, listing), 29 | 37 |
| no grant from data | 03 (`ATK-ir-grant-constant`), 17 (`ATK-trace-decode-grants-nothing`), 22 (`V2-GRANT-no-constructor`), 36 | 37 |
| no grant from a row alone | 21 (`V2-GRANT-test-without-world`), 36 | 37 |
| no grant outside a test world by test code | 21 (`ATK-test-runner-no-system-driver`, `ATK-test-world-op-on-leaf`) | 37 |
| the pilot's dated observation | 38, 39, 40 | 41 |

## Plan validation

The plan is checked by a script over the guides, the architecture, the
questions, the owner table, and the tree. It reports:

- 41 guides, each with every required section;
- every dependency points to an earlier step, so the graph is acyclic;
- 108 pairs of steps can run in parallel, and no pair owns one file;
- every diagnostic and result code cited is in the registry of the
  diagnostics chapter or the result tables of the tooling chapter;
- every `path:line` of an entry-point table exists and holds the symbol
  named beside it;
- every package named with `-p` is known to `cargo metadata --offline`,
  and no guide runs the test suite of the whole workspace;
- every case and attack id is a row of exactly one guide and names no
  case that the corpus already has;
- each of the 31 invariants of the authority argument has at least one
  attack case, and each such id is a row of the step the table names;
- every case id that a question binds is a row of a guide;
- every relative link and its anchor resolve.

The quoted sentences in the "Specification" column of the guides were
compared with the chapters by the same script: of 668 quoted fragments, two
are not found verbatim, both because the fragment itself contains a quote
mark that the table escapes.
