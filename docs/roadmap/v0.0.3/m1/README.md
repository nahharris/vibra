# v0.0.3 M1: the whole language on native code

**Planned.** Widen the native slice of
[v0.0.2 M4](../../v0.0.2/m4/README.md) until every form of the supported
language lowers, verifies, steps, and runs as generated code on Windows, in
agreement with the interpreter.

Toolchain and target feasibility record: [feasibility.md](feasibility.md).

## Scope, in slices

The work packages are X3-10 to X3-19, X3-28, and X3-29 of the
[Nexus plan](../../nexus/plan-v0.0.3.md#x3-10-to-x3-19-x3-28-x3-29--the-whole-language).
Each slice extends the execution IR, its verifier, the runtime, the
lowering, the stepper, and the code generator together, and moves its rows
of the parity inventory.

1. **Scalars and arithmetic.** Saturating and checked operations,
   conversions, floats. No operation traps on user data.
2. **Aggregates and construction.** Tuples, records, variant types,
   wrappers, private representation, and both forms of `where`.
3. **Patterns and typed failure.** `match`, destructuring, `let … else`,
   `try`.
4. **Generic instantiation.** Type descriptors passed at run time.
5. **Calls.** Labelled operands, defaults, written operand order.
6. **Interfaces.** Static and interface-value dispatch, default members.
7. **Collections and text.** `List`, `Dict`, `Str`, `Bytes`, `Iter` and its
   adapters, the strict left `fold`.
8. **Module values.** Lazy, once, across modules.
9. **Tests and observation.** Canonical encoding of every value kind, the
   three assertions, the explicit error for a value that cannot be shown.
10. **Host operations.** All seventeen, behind grants and scope policies.
11. **Guards, completed.** On anonymous callables and interface members,
    and through conversion to a callable type.
12. **Plans and startup, completed.** Stored and returned plans, plans
    through interface values, every provider of the binding table.

A program that uses a form outside the slices landed so far is rejected as
a whole, by name. Nothing runs part of a program. A form the language
removed is not accepted through a fallback.

## Exit evidence

- The parity inventory lists every executable case once, with its status on
  the interpreter, the stepper, and Windows. Every cell of those columns is
  matched, with an unrestricted quota and with a quota of one. The Linux and
  macOS columns may still be pending; they are M2's.
- Expectations are written from the specification and are not regenerated
  from a candidate's output.
- The demo: one program with an indirect tail call across modules, a plan
  stored and awaited twice, an optional grant, a guard that returns its
  consequence, and a scoped file operation in the audit trace.
- Rejections before execution: invalid execution IR, a stale ABI or
  registry, an unknown symbol, a mismatched policy requirement.
- Two written audits: what the verifier guarantees to the generator, and
  every `unsafe` block of the bridge with the reason it is sound.
- Both suites pass for the claimed profile, with the commands, the commit,
  and the counts recorded.

## Not in this milestone

Linux and macOS, `vibra build`, an optimizer, and concurrency.
