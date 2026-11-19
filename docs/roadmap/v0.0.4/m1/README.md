# v0.0.4 M1: libraries and tool adapters

**Planned.** A program depends on real code written by someone else, and an
agent reaches the toolchain without a shell.

It needs the snapshots, protected approval, and the query, evaluation, test,
and edit engine of [v0.0.2](../../v0.0.2/README.md). It does not need native
code: resolving a library and serving an adapter involve no code generation.

## Scope, in slices

1. **Exact dependencies.** Local and Git dependencies with a lock. Each
   package and unit has a canonical identity, and revisions are exact. A
   package or import cycle is rejected. A stale, missing, or tampered lock
   is refused. Paths stay inside the dependency. A second resolution offline
   gives the same result.
2. **One dependency worth having.** A real library is integrated through
   explicit types and conversions, with a stated boundary for its authority
   and for how its behavior is modelled in tests. Its failures are typed,
   and the driver and policy it runs under are recorded.
3. **Upgrade and repair.** The pinned revision changes. The tooling
   classifies what changed in public claims, nominal types, and authority. A
   stale approval is rejected. A real failing protected test is repaired,
   and the effort is recorded.
4. **A thin MCP adapter.** It returns the same pinned query, evaluation,
   test, and edit facts, statuses, and revision preconditions as the machine
   CLI. Malformed, stale, and denied requests, and bounded output, are
   covered.
5. **One editor client.** Over the same engine: go to a failure, inspect a
   callee with its effect row and driver, preview an edit that checks the
   revision, and run the selected tests again. A further client is added
   only with evidence that it is used.

## Exit evidence

- A real dependency is used, pinned, inspected, upgraded, and repaired under
  the supported offline and local story.
- The test oracle is protected: test bodies, fixtures, selection, and the
  runner are pinned for each run, separately from the candidate code.
- The record states the integration, conversion, and modelling costs and
  the features that are unsupported. It does not claim an ecosystem.
- Schemas, diagnostics, and source and version identities agree between the
  adapters and the CLI. No adapter checks source with its own checker.
- Both suites pass.

## Not in this milestone

A package registry, version-range solving, dependency lifecycle scripts, a
foreign function interface, automatic trust in a driver, a set of five
editors, live redefinition, and a publish workflow backed by a database.
