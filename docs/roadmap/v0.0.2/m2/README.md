# v0.0.2 M2: operations and authority

**Planned.** Procedures, effect ceilings, grants, and the entry contract, on
the interpreter, with fake and real drivers.

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
change returns to the specification.

## Not in this milestone

Network operations, concurrency, and native execution.
