# Step 28: startup-and-release

Kind: vertical. Slice 4, dynamic half: admission of a binding plan,
acquisition in the one startup order, the three outcomes per leaf, startup
reads through the `source` grant, the startup record, and release in
reverse order.
Depends on: 16, 17, 24.
Crates: `vibra-host`.

## Prerequisites

- Read: [programs
  chapter](../../../../spec/04-programs-and-packages.md#startup-order)
  "Startup order" whole; [runtime
  chapter](../../../../spec/06-runtime.md#entry-outcome) "Entry outcome",
  Startup; "Host world", "Recipes under a world"; "Audit trace", the
  startup event and the `@startup` phase; [tooling
  chapter](../../../../spec/05-tooling.md#command-payloads)
  `StartupRecord`; [questions](../spec-questions.md) Q13, Q19, Q21, Q24.

## Entry points

From step 14: `recipe::{BindingPlan, Leaf}`. From step 15:
`broker::Context`, `driver::{Driver, Acquire}`. From step 16:
`world::WorldDriver`. From step 17: `Context::startup_event`.

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-ir/src/external/host.rs:377` | `pub fn host_row` | the rows of the two startup reads |

Proposed: `vibra-host::startup::{admit, Admission, start, Started,
StartupRecord, BindingRecord, release}`.

## Files owned

- `crates/vibra-host/src/startup.rs`
- `crates/vibra-host/tests/it/startup.rs`

## Ordered tasks

1. `admit(plan, ceiling, driver)`: `plan.validate()` again; every
   leaf's interface and every startup row is in `ceiling`; every
   provider has a driver (`Driver::identity` is `Some`), else the
   admission result "unavailable". Invariant: admission acquires
   nothing.
2. `start`: process leaves in `plan.order()`, recomputed after each. Per
   leaf: resolve the resource (a startup read is a `Context::host` call
   on the `source` grant, phase `@startup`, no parent, its own
   invocation ordinal; a `source` bound absent is a refusal); call
   `Driver::acquire`; classify by the issuer's row.
3. Acquired: `Context::issue` with origin `{kind: @binding leaf}`, the
   recipe's policy and limits; append the `@startup` event with the
   projection. Refused optional: bind absent, event `@refused`, continue.
   Refused required: stop, `@grant.requiredUnavailable`. Failed: stop,
   `@grant.acquisitionFailed`, for an optional leaf too.
4. A startup read that returns an error the row does not list as a
   refusal is a failure; its bytes count against the `source` grant's
   limits (Q19).
5. `StartupRecord`: `status`, one `BindingRecord` per processed leaf in
   processing order with `recipeIdentity`, `providerIdentity`, `status`,
   and the projection; `failure` with the binding, code, and the recipe's
   span.
6. `release`: acquired leaves in reverse acquisition order, each
   `Driver::release` then a generation bump, status `released`. `Started`
   holds a guard whose drop releases, so every exit path releases.
   Release appends no event (Q21).

## Test matrix

Host tests over `WorldDriver`, and over the scripted driver where a
failure must be injected. `W` is the world of step 16; `M` the plan of
step 26's `M`.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| all acquired | positive | `M` under `W` (`args: ["/notes"]`) | status `acquired`; events: `@startup @args`, a `@host` `env.readArgs` of phase `@startup` with no parent, `@startup @notes`, `@startup @out` | 04 Startup order; 06: "configuration reads … before the entry are not hidden" |
| order by spelling | positive | leaves `@b`, `@a`, `@logs.reader`, no `after` | processed `@a`, `@b`, `@logs.reader` | 04: "smallest in UTF-8 byte order" |
| `V2-GRANT-startup-order-permuted` | positive | `@a` after `@z`; `@m` free; the dict written `@z @m @a` | processed `@m`, `@z`, `@a` | 07 Required coverage 12: "acquiring the first ready leaf makes a lexically earlier leaf ready" |
| refused optional | positive | `@out` optional, the driver has no stdout | `@out` `@refused` with no `failure`; startup continues; the entry argument is absent | 04: "A refused optional leaf is bound to `Option.none()` and the sequence continues" |
| refused required | negative | `@notes` required, `args: ["/missing"]` | status `failed`; `failure.code` `@grant.requiredUnavailable`, the recipe's span; `@out` has no event | 04: "A refused required leaf stops the sequence" |
| each directory refusal | negative | the path names nothing; names a file; index beyond the arguments; variable not present | each `@refused` | 04, the `@host.fs.directory` row |
| `V2-GRANT-startup-failed-optional` | negative | `@notes` optional; the scripted driver answers `Failed("io")` | status `failed`, `@grant.acquisitionFailed` | 04: "A failure stops the sequence for a required and an optional leaf alike" |
| `ATK-startup-failure-not-none` | negative | the same; inspect the entry arguments | none were built; no `Absent` for `@notes` | 04: "A failure never becomes an absence" |
| absent `source` | positive | `@args` optional and refused; `@notes` optional with `source: @args` | `@notes` `@refused` | 04: "A leaf whose `source` leaf was bound absent is refused as well" |
| absent `source`, required | negative | the same with `@notes` required | `@grant.requiredUnavailable` | the same |
| unlisted read error | negative | `@variablePath`; the variable's value is not Unicode | `@failed` (Q19) | 04: "Anything the row does not list as acquired or refused is a failure" |
| `V2-GRANT-startup-read-counts` | positive | `@args` with `limits: {invocations: 1u64}`; the entry then reads arguments | the entry's read is `limitExceeded` (Q19) | 04: "read once, at startup, through the `source` leaf's grant" |
| `V2-GRANT-startup-read-limit-fails` | negative | `@args` with `limits: {invocations: 1u64}` and two leaves that read through it | the second leaf is `@failed` (Q19) | the same |
| startup invocations | positive | two startup reads | invocations 1 and 2, no `parent` (Q13) | 06: "The startup reads of the recipes come first" |
| projection of a leaf | positive | `@notes` acquired | `issuedBy: {kind: @binding leaf: @notes}`, the policy's hash, `root` in issue order | 06 Audit trace |
| recipe identity | positive | `M` | each `recipeIdentity` equals step 14's function on the recipe | 05 `StartupRecord.bindings[]` |
| `ATK-admit-plan-outside-ceiling` | negative | a plan whose leaf interface is not in the ceiling given to `admit` | refused; `Driver::acquire` is never called | 03: "Check, run, and build MUST reject…" |
| `ATK-plan-provider-mismatch` | negative | a hand-built plan with `@host.io.stdout` for a `DirReadText` leaf | `admit` refuses | 03: "a recipe whose provider cannot issue the leaf's type" |
| admission without a driver | negative | a driver whose `identity(@host.fs.directory)` is `None` | "unavailable"; nothing acquired | 04: "rejected at admission, as `@tool.unavailable`, before any leaf is processed" |
| `V2-GRANT-release-order` | positive | three acquired leaves, then `release` | `Driver::release` called in reverse acquisition order | 06: "in reverse acquisition order" |
| release after a failed startup | positive | two acquired, the third fails | the two are released, in reverse | 06: "On every ending of the process (… or a failed startup)" |
| `V2-GRANT-release-status` | positive | a finished start and release | every acquired binding's status is `released` (Q21) | 05: "or `released` for an acquired leaf after its release" |
| `ATK-released-grant` | negative | a token kept after `release`, used on the same context | `InvalidHostValue` | 03: "or one released at shutdown, is unusable" |
| release on drop | positive | drop `Started` without calling `release` | the driver saw every release | 06: "every acquired grant is released" |
| `ATK-trap-during-startup` | negative | the scripted driver panics inside `acquire` under `catch_unwind` in the test | the leaves acquired so far are released (Q21) | the same |
| release appends nothing | positive | the trace before and after `release` | equal | 06: "Release runs no program code, appends no `@host` event" |

## Diagnostics and schema changes

Codes carried in the record: `@grant.requiredUnavailable`,
`@grant.acquisitionFailed`. `StartupRecord` is the tooling chapter's
record.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-host -p vibra-workspace --all-features
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0. The rows with a `V2-` id are host tests here; step 29
adds each as a `workspaceRun` case when Q17 names the document.

## Stays unavailable

No command starts a target until step 29.

## Excluded

The real drivers' refusals and failures (30, 31); the outcome of a run
(29).

## Completion evidence

- [ ] Each of the 26 rows is a test, named in the report.
- [ ] `Context::issue` has exactly two callers, `startup.rs` and
      `test_world.rs`.
- [ ] The validation commands exit 0.
