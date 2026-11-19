# v0.0.5 M3: bounded exploration and release

**Planned.** The service of [M1](../m1/README.md), and whichever profiles of
[M2](../m2/README.md) are advertised, are explored over a stated state
space, qualified on a real driver, and released.

## Scope, in slices

1. **The exploration contract.** A versioned model, world, and schedule.
   Finite domains and seeds. A bound on states and depth. The fairness and
   environment assumptions. The code and policy identity. A status that
   says whether the space was exhausted or a part was unsupported.
2. **State identity that sees the future.** Two states are the same only if
   their pending operations and outcomes, queues, timers, model state,
   children and lifetimes, and code epochs are the same. States that look
   equal now and differ in what will happen are not merged.
3. **Counterexamples and replay.** The mutants of the revision and
   late-reply cases produce traces that replay. The correct fixture reports
   only the domain that was checked. Kernel and host events and origins
   agree between the interpreter and native code.
4. **Real qualification.** A corpus on the real driver. Long runs that
   measure backpressure, resource use, and cleanup. Startup and shutdown.
   The recovery campaign of the selected profile. A check that could not
   run stays a gap.
5. **Release audit.** Both suites and the schemas. The protected oracle and
   approval. What is trusted in the runtime, the drivers, and persistence.
   The exact supported profiles and their limits.

## Exit evidence

- The document-save case of the release gate passes, with every advertised
  failure and recovery path.
- Exploration is reported as bounded evidence. It is not a claim of
  correctness for every schedule.
- Redaction and the policy, source, and driver identities can be inspected.
- The record states the commands, commit, platform, counts, and gaps.
  Expectations are never regenerated from a candidate's output.

## Not in this milestone

Unbounded checking, a proof over all schedules, a static guarantee that a
handler replies or terminates, exactly-once external actions, distributed
recovery, and any widening of authority.
