# Milestone 2 — executable pure core

Execution plan: [M2 steps and implementation guides](execution.md).
Integration branch: `m2`, created from `main` after the M1 merge. Each numbered
step is planned to land through one PR into `m2`. Steps 1–8 were integrated
directly before that process was enforced; Steps 9–14 each have an individual
step PR. The Step 14 report records that history explicitly. Step 1 closed the
explicit subset, bootstrap, diagnostic, and machine-contract prerequisites
before dependent implementation. The exact implemented and deferred surface,
its evidence, and the current integration state are recorded in the
[M2 step plan](execution.md) and its
[Step 14 exit report](14-exit-evidence.md). Valid deferred forms
use the canonical `@tool.unavailable` result rather than a profile-specific
dialect.

Outcome: `vibra project init`, `fmt`, `check`, `test`, and interpreter-backed
`run` work for pure programs.

Deliverables:

- `@project.v1` record decoder, source graph, explicit atom-entity import
  resolver, `project.vibon` discovery, and namespaces;
- unit-rooted atom path resolution shared by imports and the `entry`
  declaration reference, enforced leaf-or-directory module layout, non-nested
  target roots, and decode/resolve phase separation;
- primitive types including `char` and the fixed-width numerics, `def`,
  functions, `lambda`, immutable bindings/values, general application with the
  `fn` category, direct local-name patterns, `do`, `if`, recursion with
  mandatory tail calls, first-class function paths, and closed `@compiler`
  externals;
- resolver-enforced no-shadowing and explicit visibility;
- binding and wildcard discards that create no identity and may repeat;
- rejection of every provider other than `@compiler` and `@host`, including
  source-level Wasm FFI;
- rejection of retired `while`, `for`, `break`, and `continue` forms (`return`
  was retired at M2 and is specified by [`pre-m4/README.md`](../pre-m4/README.md));
- typed IR and reference interpreter;
- pure core standard library and test assertions; and
- type-aware source-position metadata for visible names and expected primitive
  types.

Demo gate: initialize and run a pure multi-module program, then run its tests
from a clean checkout with no network or host operations.

Exit gate:

- static and interpreter conformance profiles pass for the implemented subset;
- no compiler phase depends on CLI/MCP representations;
- pure execution emits no host event and cannot observe ambient state through
  conforming source;
- tail-position recursion does not grow language-level stack in the reference
  interpreter; and
- unsupported forms scheduled for later milestones receive explicit
  availability diagnostics rather than being misclassified as generic
  applications.
