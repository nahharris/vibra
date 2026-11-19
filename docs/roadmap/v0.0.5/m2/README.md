# v0.0.5 M2: journal and recovery profiles

**Planned.** [M1](../m1/README.md) supplies an explicit state machine with
outcomes and correlation. This milestone adds recovery for the workloads
that choose it. A workload takes the least profile it needs; journaling is
never universal.

## The three profiles

| Profile | What it keeps | What it claims |
| --- | --- | --- |
| none | Nothing beyond the ordinary audit trace and typed outcomes | No recovery |
| ring | A bounded diagnostic history, which overwrites and so loses records, explicitly | Diagnosis after a failure, not recovery after a crash |
| durable | A versioned, retained record of named state and operations | A recovery protocol with stated flush, crash, replay, and reconciliation points |

## The specification comes first

Before any code, the specification defines: the record version; the
identities of statement, code, policy, and driver that a record carries;
redaction and who may read a record; stable references to an invocation and
its correlation; the atomic and flush boundaries; the order of recovery; and
what an unsupported upgrade does.

A live grant, an arena offset, a callable, or a plan is never persisted as
authority. After a restart, grants are acquired again through the approved
binding recipes.

## Scope, in slices

1. **The record of each profile.** The ring's bounded overwrite, or the
   durable append and flush. A malformed record and a record of an
   incompatible version are refused. Secrets are redacted, with cases.
2. **A crash campaign.** A crash before submission, after a possible
   submission, after the reply, and before and after the durable commit.
   The three results stay distinct: not submitted, known, and unknown.
3. **Recovery and reconciliation.** A restart with pinned identities,
   legitimate reacquisition of authority, and explicit reconciliation of an
   external outcome. No blind retry.
4. **Upgrade and admission failure.** Mismatched code, policy, or schema,
   or a required grant that is unavailable, is rejected. A record is never
   replayed under a changed meaning, and a nominal type is never migrated
   implicitly.

## Exit evidence

- Independent crash and record cases qualify each advertised profile, on
  the models and on the real native persistence adapter.
- Whether each operation is safe to repeat is stated per operation. `@once`
  is a delivery class within a run; it is not exactly-once across a crash.
- The durability assumptions about the state and the operating system are
  recorded. They are not inferred from a process having exited normally.
- Both suites and the service and approval regressions pass.

A profile the workload does not need may stay explicitly unsupported. Its
absence is not a claim of recovery.

## Not in this milestone

Distributed transactions or consensus, event sourcing of everything, replay
of every effect, persisted bearer tokens, and silent recovery across
arbitrary upgrades.
