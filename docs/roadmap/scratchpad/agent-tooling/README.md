# Agent tooling

## Beyond the local codebase

The [v0.0.2 codebase](../../v0.0.2/m1/README.md) starts with one local store
and explicit file import/export. Distributed object sync, multi-writer
merges, and retention policies require their own evidence. Use a fixture
where two agents publish divergent revisions of mutually recursive code.
Compare content transfer with exchanging source snapshots. Show that a
recipient can verify the graph, resolve conflicts explicitly, and keep the
source and nominal provenance used by diagnostics.

## Type-constrained decoding

A decoding service could report which continuations of a source prefix are
well-typed. It would build on the
[v0.0.2 M2 source-position query](../../v0.0.2/m2/README.md).
Measure the reduction in rejected generations against an unconstrained
baseline.
