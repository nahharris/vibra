# Reproducible project dependencies

**Unscheduled.** How can Vibra admit multiple packages offline while making
the exact source graph and the source of external authority auditable?
The earlier dependency-first roadmap proposed this as a release gate.
[GitHub #409](https://github.com/nahharris/vibra/issues/409)

The motivating program is a diamond-shaped dependency graph with one shared
library. Resolve each alias to one explicit library target. Compare the
current embedded stdlib model with an ordinary pinned stdlib dependency
whose exact identity and external-declaration authority are anchored by the
toolchain. Keep the embedded model until the latter has a reviewed trust
contract. [Programs and packages](../../../spec/04-programs-and-packages.md#dependencies-and-lock)

Preserve the proposed work on exact Git revisions, local dependencies,
canonical lock and dependency edges, safe atomic vendor sync, project edit
plans, and version evolution for project, lock, and build data. Keep lock
identity platform-independent and join it to the checked input graph used by
the codebase and native compiler.

Before scheduling, show that repeated syncs are byte-identical, that check
and test work offline, and that stale locks, tampered files, path escapes,
and untrusted copied external declarations fail before lowering. A package
must never execute install or build scripts. The pre-alpha tooling and native
roadmaps scope their release fixtures to admitted inputs while this work is
unscheduled.
