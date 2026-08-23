# Vibra v1 CLI, MCP, and code tooling

Status: normative target
Implementation status: not started

## One workspace engine

The CLI, MCP server, editor integration, and compiler MUST use one workspace
engine for parsing, resolution, types, effects, diagnostics, queries, and edit
planning. A tool may render the result differently; it may not reimplement the
language or maintain a second symbol index.

Every machine response has a schema version and is deterministic for one
workspace snapshot. JSON is the sole general machine format. Human output is
for terminals; program stdout from `run` remains program-owned.

## V1 CLI

The stable v1 command surface is:

```text
vibra project init|inspect|add|remove|sync
vibra fmt
vibra check
vibra lint
vibra test
vibra run
vibra build
vibra effects
vibra index
vibra code context|symbol|references|rename|fix|organize-imports
vibra mcp
```

Commands that can change files preview by default. `--write` is required to
apply formatter, lint-fix, project-edit, rename, fix, and organize-imports
plans. `project init` and `project sync` are inherently mutating and must print
their planned destinations before mutation when run interactively; they reject
nonempty destinations or conflicting vendor content rather than overwriting.

All commands accept `--format human|json` where their output is structured.
JSON goes to stdout, diagnostics and operational logs go to stderr, and a
nonzero exit is classified by a stable result code. No command infers an
output format from a filename extension.

## Project operations

Project tooling edits `project.vib` through its typed project AST. `add` and
`remove` produce the same transactional plan format as source edits. They
preserve comments when possible and always finish with canonical formatting.

`sync` is the only v1 command that performs dependency network access. It
accepts only declared HTTPS Git sources and exact revisions, exports no Git
metadata, executes no dependency code, and atomically replaces the vendor tree
and lock only after all hashes validate.

## Context and index queries

`vibra code context <path>:<byte>` returns the smallest containing syntax node
and the information needed to produce a valid continuation:

- grammar category and permitted child forms or labels;
- expected and observed type;
- allowed effect ceiling and currently inferred row;
- visible lexical names and imported module aliases;
- resolved declaration candidates with canonical identities;
- applicable enum constructors, fields, interface methods, and generic bounds;
- diagnostics and safe fixes at the location; and
- the exact workspace revision used.

`symbol` and `references` operate on resolved identities, not token spelling.
They distinguish type, value, interface, effect root, effect operation, field,
variant, and lexical binder identities.

`vibra index` emits one normalized record per declaration: canonical source,
signature, effect row, visibility, call edges, referenced types, error types,
and source fingerprint. The exact checker remains the admission test for a
retrieved candidate; the index is not a substitute type system.

## Transactional edit plans

Supported v1 semantic changes are format, safe diagnostic fix, semantic rename,
and organize imports. Project add/remove and generated module scaffolding use
the same plan envelope. V1 does not expose arbitrary AST patterns, text search
and replace, or a general patch language as a trusted compiler operation.

An edit plan contains at least:

```json
{
  "schemaVersion": 1,
  "workspaceRevision": "sha256:...",
  "operation": "rename",
  "target": { "kind": "value", "id": "..." },
  "documents": [
    { "path": "src/main.vib", "revision": "sha256:..." }
  ],
  "edits": [],
  "resultingDiagnostics": []
}
```

Edits use half-open UTF-8 byte spans and include the expected syntax identity
and source fingerprint. Before writing, the engine MUST verify the workspace
and document revisions, apply every edit in memory, reparse and recheck every
affected module, format changed forms, and validate the requested postcondition.
It then replaces all files atomically. Stale, overlapping, ambiguous, or
ill-typed plans fail without partial writes.

Rename rejects generated-only nodes, keywords, visibility violations, name
collisions, and shadowing. A rename may touch imports and qualified references
across packages in the current workspace but never edits vendored dependencies.

## MCP server

`vibra mcp --workspace <root>` exposes the shared services over a versioned MCP
stdio server. Tool names mirror CLI concepts, for example
`vibra.code.context`, `vibra.code.rename-plan`, and
`vibra.code.apply-plan`.

The default server is read-only and does not execute project code. Independent
startup flags grant narrow server authority:

- `--allow-write` permits application of a validated edit plan;
- `--allow-test` permits the deterministic test runner;
- `--allow-run` permits project execution under no more than project grants;
  and
- `--allow-sync` permits exact-revision dependency fetch and vendor writes.

These flags grant MCP operations, not Vibra host authority. A program still
needs project/runtime grants and budgets. The server never accepts an
executable path, shell fragment, arbitrary CLI subcommand, environment entry,
or unvalidated filesystem path from a client.

All client paths resolve beneath the canonical workspace. Symlink and junction
escape, `..` escape, absolute paths outside the root, and writes to vendored
dependencies are rejected. An apply call must present the exact plan and
revision returned by a prior plan call.

## Schemas and errors

The v1 implementation publishes JSON Schemas for project inspection, lock,
diagnostic, context, symbol, reference, effect, index, edit plan/result, test
report, build metadata, host registry, and MCP result shapes.

Schema IDs and major versions are contracts. Unknown fields are rejected in
inputs and ignored only where an output schema explicitly permits forward
extension. Errors distinguish invalid input, diagnostics found, stale
workspace, permission denied, execution denied, command failure, resource
limit, and internal failure.
