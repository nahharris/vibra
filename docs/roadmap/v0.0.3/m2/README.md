# v0.0.3 M2: three platforms

**Planned.** [M1](../m1/README.md) supplies the whole language on native
code, qualified on Windows. This milestone qualifies real processes on all
three platforms. It changes no language semantics.

## Scope

The work packages are X3-08 and X3-20 to X3-22 of the
[Nexus plan](../../nexus/plan-v0.0.3.md).

1. **Linux and macOS paths.** Code generation for both architectures, the
   link driver and runtime archive for each target, and the filesystem
   adapter for each under the scope-policy contract. X3-08 depends only on
   the v0.0.2 slice and may run beside M1.
2. **Real processes.** On each target's own runner: executable and working
   paths with spaces and non-ASCII characters; arguments, including an empty
   one and a non-ASCII one; exact standard output and error bytes; standard
   input; scoped file reads and writes; startup outcomes; the application's
   exit code.
3. **Full parity.** Canonical results, encoding errors, typed failures,
   assertions, origins, and completed audit events, under fresh equivalent
   worlds. A quota of one equals an unrestricted quota.
4. **Memory.** 100,000 frames of non-tail recursion on a small native stack.
   Equal peak activation at 10,000 and 100,000 tail transfers. Release of a
   deep value without native recursion. A stable live size in an allocating
   loop. Memory exhaustion as the host event, never a `Result`.
5. **Grants and drivers.** The same vectors through the fake and the real
   drivers. A grant from another world, a stale one, and a forged one are
   rejected. Narrowing never widens a scope or resets a shared limit. No
   directory handle is kept between invocations. A submission whose outcome
   is unknown is not retried.
6. **Baseline and audits.** Per target, with hardware, commit, and profile:
   build and start time, steady execution, peak live and frame memory,
   bridge call counts, and the work a long primitive charges. No speedup
   threshold is set. The compiler, runtime, `unsafe`, and driver audits are
   brought up to date.

## The parity inventory

Every executable case has one row, with its status per executor and
platform. A cell marked matched that fails, fails the run. A case cannot
leave the table because the corpus changed. A pass on the stepper is not a
pass on a platform. A platform without a runner stays pending.


## Exit evidence

- Zero pending and zero failed executable cases on all three platforms.
- Both suites pass, with current schemas and source maps, and the approval
  and repair regressions of v0.0.2.
- Expectations were reviewed against the specification, not regenerated.
- The record states which host assumptions were tested and which are
  trusted.
- No expectation of this profile is a trap caused by user data. Tests that
  inject defects are kept separate.

Yielding must not repeat a host action. That is not a promise of
exactly-once external behavior or of durability across a crash.

## Not in this milestone

Remote targets, cross-compilation as a product, an optimizer, and
concurrency. [M3](../m3/README.md) packages what is qualified here.
