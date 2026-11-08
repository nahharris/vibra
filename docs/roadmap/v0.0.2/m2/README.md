# Milestone 2 — agent-native workspace tooling

Outcome: an agent can understand and safely change a project without scraping
human terminal text.

Deliverables:

- complete versioned JSON output for all CLI and MCP command results;
- one `query <subject>` interface with metadata projections and relation
  expansions over one snapshot, covering every entity kind introduced through
  v0.0.2 M1, including implementations, unions, effect operations, and
  dependency units;
- application-kind metadata and completion for functions, constructors,
  tuple/record selectors, collection indices/keys, and patterns;
- explicit atom-value, entity-reference, and code-reference roles selected by
  source grammar or typed VIBON schema slots;
- transactional format, fix, rename, organize-imports, and project plans;
- CLI preview/write flows with atomic postcondition checking;
- MCP tools mirroring the shared engine; and
- read/write/test/run/sync action separation and workspace confinement.

Demo gate: through MCP alone, inspect a project, discover the expected type and
effects at a location through the same query interface, preview a cross-module
rename, reject a stale apply, and atomically apply a fresh plan. No tool starts
a shell, adds a target effect, or executes code unless its server action is
enabled.

A second, agent-loop demo exercises the purpose of the milestone: starting from
a failing test, an agent uses only query results, diagnostics with safe fixes,
and edit plans to repair the program, and every intermediate write is revision
checked.

Exit gate:

- tooling-v1 conformance passes against CLI and MCP renderings of the same
  workspace results;
- all writes are revision checked, reparsed, retyped, and atomic;
- read-only MCP cannot mutate or execute; and
- producer tests validate every rendered interchange document against its
  published JSON Schema, which is the consumer contract.
