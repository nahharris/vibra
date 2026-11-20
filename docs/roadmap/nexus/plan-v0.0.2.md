# v0.0.2 work packages

Read [README.md](README.md#how-to-delegate-a-work-package) first. These four
packages are the native part of [v0.0.2 M4](../v0.0.2/m4/README.md): one
slice of the rewritten language, lowered, verified, stepped, generated,
linked on Windows, and compared with the interpreter. Decisions (N*) are in
[decisions.md](decisions.md), and the commands and standing rules in
[validation.md](validation.md). Sizes and agent tiers are as in the
[v0.0.3 plan](plan-v0.0.3.md).

| Package | Depends | Size | Agent |
| --- | --- | --- | --- |
| [X2-01](#x2-01--rebaseline-against-the-new-contracts) | v0.0.2 M0 | M | careful |
| [X2-02](#x2-02--guards-and-plans-in-the-slice) | X2-01; v0.0.2 M1 for guards, M2 for plans | M | careful |
| [X2-03](#x2-03--grants-startup-and-the-entry) | X2-01; v0.0.2 M2 | L | careful |
| [X2-04](#x2-04--slice-exit) | X2-02, X2-03; v0.0.2 M3 | S | careful |

X2-01 needs only the frozen contracts and can start as soon as M0 exits.
X2-02 and X2-03 need the interpreter to run the same forms first, because
the interpreter is the oracle. The gate of M4 is claimed after M3's (N40).

---

## X2-01 · Rebaseline against the new contracts

**Kind:** evidence and a mechanical change. **Decisions:** N41, N53.

**Read first:** the native contract comparison that v0.0.2 M0 recorded; the
[runtime chapter](../../spec/06-runtime.md) whole; [design.md](design.md) §2
and §6.

**Deliver:**

1. A table in [v0.0.2 M4](../v0.0.2/m4/README.md), one row per crate of the
   native track and per test file: kept unchanged, changed (what and by
   which package), or removed.
2. The checked-IR nodes the rewritten checker produces, mapped to the rows of
   [From checked IR](design.md#from-checked-ir). The table there is corrected
   in the same change. Rows for removed forms are deleted from `vibra-lower`
   with their tests.
3. The trap codes `DivisionByZero` and `UnobservableFunction` leave the
   native track: every hand-built fixture that ends in one is rewritten
   around a defect trap or a typed error (N44).
4. The existing lowering slice passes again on the rewritten checked IR, for
   the forms that survive unchanged.
5. The native track depends on `vibra-diagnostics`, `vibra-ir`,
   `vibra-native`, and `vibra-host`, and the five `-v1` crates it was
   compiled against are deleted: `vibra-diagnostics-v1`, `vibra-syntax-v1`,
   `vibra-ir-v1`, `vibra-host-v1`, and `vibra-native-v1`, with their rows of
   the architecture test and `NATIVE_TRACK_V1_CRATES`.

**Validation:**

- `every_lowered_program_verifies` and `lowering_is_deterministic` pass on
  the rewritten checked IR.
- `a_form_outside_the_slice_is_not_lowered_and_names_the_form` covers every
  new node the slice does not lower yet: guards with consequences, plan
  construction, `await`, a host call with a grant.
- No test of the native track names a removed trap code or a removed form.
  The search that shows it is recorded.
- `architecture_boundary::` passes with no new dependency.

**Out of scope:** lowering anything new.

## X2-02 · Guards and plans in the slice

**Decisions:** N44, N45. **Read first:**
[callable contract execution](../../spec/06-runtime.md#callable-contract-execution);
[tail calls](../../spec/06-runtime.md#tail-calls); design
[Guards](design.md#guards) and [Plans](design.md#plans).

**Deliver**, each as the four-part change of
[design.md](design.md#verification-rules) where it adds an instruction:

1. `requires C else V` and `ensures C else V` on a `fn`.
2. The plan object kind; applying a `proc`; `await`.
3. A `proc` with `requires … else`, whose check runs on each `await`.

**Validation**, each compared on the stepper against the interpreter,
unbounded and with quota 1:

- `a_bare_guard_lowers_to_no_instruction`: the lowered program of a function
  with a discharged bare `requires` equals that of the same function without
  it.
- `a_false_requires_returns_its_consequence_and_skips_the_body_and_the_ensures`:
  the body contains a host write that must not appear in the trace.
- `a_false_ensures_returns_its_consequence_evaluated_with_the_candidate`,
  and the consequence is not checked again.
- `a_call_before_an_ensures_check_is_not_a_tail_call`, and
  `a_call_with_nothing_after_it_still_is`.
- `building_a_plan_runs_nothing`: no host event, no guard evaluation, no
  frame pushed.
- `one_plan_awaited_twice_runs_twice`: two host events from one plan value.
- `a_plan_called_as_a_function_is_a_toolchain_defect`, built by hand in
  execution IR, since checked source cannot express it.
- The generator emits everything the stepper runs:
  `every_instruction_the_stepper_runs_is_generated` still passes.

**Out of scope:** `where … else` (X3-11), guards on interface members and
through callable conversion (X3-28), plans through interface values (X3-29).

## X2-03 · Grants, startup, and the entry

**Decisions:** N42, N43, N46. Split into three PRs, in this order.

**Read first:** [capability values](../../spec/03-effects.md#capability-values);
[targets](../../spec/04-programs-and-packages.md#targets);
[host calls](../../spec/06-runtime.md#host-calls) and
[entry outcome](../../spec/06-runtime.md#entry-outcome); design
[Grants](design.md#grants) and §7.

**Deliver:**

1. **The grant value and the broker.** A sealed value kind in the store; a
   broker per world that owns resources, policies, and counters; the checks
   of `Context::host` in the specified order. One operation of registry
   major 2 is bound: `fs.readText` through a `DirReadText` grant, with the
   Windows adapter whose feasibility v0.0.2 M0 recorded and the fake
   directory of the test world.
2. **Startup.** The launcher validates recipes, acquires in dependency
   order, binds a refused optional grant to none, and releases in reverse.
3. **The entry.** The descriptor's entry contract replaces the
   `entry_outcome` field; `ExitCode` is returned to the operating system by
   the `system` launcher and recorded by the `harness` launcher;
   `ABI_VERSION` is bumped and an older descriptor is refused.

**Validation:**

- `a_request_inside_the_policy_reads_the_file`, and
  `a_request_outside_the_policy_is_a_typed_denial_with_no_adapter_call`.
- `a_grant_from_another_world_is_a_toolchain_defect`, and the same for a
  stale generation. Both are built by hand; checked source cannot express
  them.
- `a_narrowed_grant_shares_its_parents_limit`: two children exhaust one
  parent counter, and the next request is `limitExceeded`.
- `yielding_around_a_host_call_does_not_repeat_it`, with quota 1.
- `a_refusal_before_the_adapter_appends_no_completed_event`.
- `a_required_grant_that_cannot_be_acquired_fails_startup_before_the_entry`,
  with the grants acquired before it released in reverse order.
- `a_refused_optional_grant_is_none`.
- `a_cyclic_or_missing_binding_never_reaches_the_launcher`: it is a build
  error.
- `a_descriptor_of_the_previous_abi_is_refused_before_startup`.
- One test per exit path of the `system` launcher: a zero code, a non-zero
  code, a defect trap, memory exhaustion, a startup failure. None is
  confused with another.
- On Windows through the real adapter: the attack cases of the
  [scope policies](../../spec/03-effects.md#scope-policies) that concern a
  read (a link, a reparse point, a name that differs in case, a replaced
  root).
- No `unsafe` outside the bridge.

**Out of scope:** the other sixteen operations (X3-19); Linux and macOS
adapters (X3-08, X3-20).

## X2-04 · Slice exit

**Kind:** evidence. **Decisions:** N47, N49.

**Deliver** the fixture of the M4 gate, its corpus cases, the versioned
observation document of the `harness` launcher, the parity inventory of the
rewritten corpus, and the evidence in
[v0.0.2 M4](../v0.0.2/m4/README.md).

The fixture is one program that contains a guard with a consequence, a
discharged bare guard, a plan built once and awaited twice, one scoped file
read through a bound grant, startup acquisition, and an `ExitCode` result.

**Validation (each maps to one clause of the M4 gate):**

| Clause | Evidence |
| --- | --- |
| Three-way agreement | The interpreter, the stepper, and the linked Windows executable agree on result, exit code, and audit trace, from one expectation written from the specification, unbounded and with quota 1 |
| No repeated submission | The quota-1 run has exactly the host events of the unbounded run |
| Refusals | A program mutated after verification is rejected by `verify`; a descriptor of another ABI and one naming an unknown symbol are refused by the launcher. None performs a host operation |
| Inventory | The rewritten corpus has an inventory with the columns and rules of N14. The record states matched and pending counts per column and per owning package |
| Gap inventory | Per feature and platform, what the native path does not support yet |
| Handoff | [design.md](design.md#11-what-v004-extends-the-m1-handoff) reviewed against the as-built ABI and corrected in the same change |

**Not claimed:** Linux and macOS, the whole language, `vibra build`.
