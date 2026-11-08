# Milestone 1 — reproducible projects and dependencies

Outcome: real multi-package projects build and test offline from exact inputs.

Deliverables:

- a toolchain-anchored trust contract for the standard library: which exact
  revisions a toolchain trusts to declare `@compiler` and `@host` externals,
  how a lock records that trust for offline verification, the diagnostics for
  an untrusted revision that declares externals, and how it replaces the
  embedded standard-library input;
- typed `project inspect/add/remove` edit plans;
- exact-revision Git and local dependencies, each alias binding one `@lib`
  target of its package;
- safe atomic vendor sync and canonical `@project-lock.v1`
  `project-lock.vibon` record data;
- workspace hashing, source graph invalidation, and offline enforcement;
- standard library delivered as an ordinary pinned dependency under that
  trust contract;
- deterministic build-plan `.vibon` data independent of the eventual backend;
- a specified evolution rule for `@project.v1`, `@project-lock.v1`, and
  `@build.v1` stating whether a later toolchain may add an optional field
  or target kind under a minor version or must bump the format major.

Delivery order: the trust contract is a specification prerequisite. It is
written into the programs-and-packages chapter before any dependency delivery
is implemented, because an ordinary dependency otherwise has no way to carry
the external-declaration authority the charter reserves for the toolchain.

Demo gate: sync a diamond-shaped exact dependency graph, disconnect the
network, then check and test it reproducibly. A changed vendor file or stale
lock fails before compilation.

Exit gate:

- path, symlink/junction, vendor, hash, and revision attacks have negative
  tests;
- dependencies execute no install or build scripts;
- clean repeated syncs are no-ops with byte-identical locks;
- a dependency that is not a trusted standard-library revision cannot declare
  an external binding, including through a copied declaration or alias; and
- project changes use the same revision-checked plan engine as source changes.
