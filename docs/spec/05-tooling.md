# Vibra v1 CLI, MCP, and code tooling

Status: normative target
Implementation status: not started

## One workspace engine

The CLI, MCP server, editor integration, and compiler MUST use one workspace
engine for parsing, resolution, types, effects, diagnostics, queries, and edit
planning. A tool may render the result differently; it may not reimplement the
language or maintain a second symbol index.

Every machine response has a schema version and is deterministic for one
workspace snapshot. JSON is the general CLI and MCP interchange format. Vibra
owns no persistent JSON file: project, lock, and build data use the canonical
`.vibon` data grammar. Human output is for terminals; program stdout from `run`
remains program-owned.

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
vibra query <subject> [--include <metadata>] [--expand <relations>]
vibra edit fix|rename|organize-imports
vibra mcp
```

Query subjects are a source position such as `src/main.vib:120`, an atom entity
reference such as `@std.fs.read`, a built-in diagnostic code such as
`@type.argument-mismatch`, or `@workspace`. Types, effects, context,
diagnostics, references, and index records are metadata projections from that
subject, not separate query kinds. V1 has no `query effects`, `query index`,
`vibra effects`, `vibra index`, or generic `vibra code` command.

Commands that can change files preview by default. `--write` is required to
apply formatter, lint-fix, project-edit, rename, fix, and organize-imports
plans. `project init` and `project sync` are inherently mutating and must print
their planned destinations before mutation when run interactively; they reject
nonempty destinations or conflicting vendor content rather than overwriting.

`vibra fmt` selects the source or VIBON formatter from the `.vib` or `.vibon`
extension before parsing. It never guesses from contents and rejects a
document whose grammar does not match its extension.

All commands accept `--format human|json` where their output is structured.
JSON goes to stdout, diagnostics and operational logs go to stderr, and a
nonzero exit is classified by a stable result atom. No command infers an output
format from a filename extension.

## Project operations

Project tooling edits `project.vibon` through its typed project-data AST. `add`
and `remove` produce the same transactional plan format as source edits. They
preserve comments when possible and always finish with canonical formatting.

`sync` is the only v1 command that performs dependency network access. It
accepts only declared HTTPS Git sources and exact revisions, exports no Git
metadata, executes no dependency code, and atomically replaces the vendor tree
and `project-lock.vibon` only after all hashes validate.

## Workspace queries

Every query returns one normalized envelope containing the subject, resolved
kind and canonical identity where applicable, exact/recovered/unavailable fact
status, and workspace revision. With no projection flags it returns the compact
core metadata appropriate to that subject. `--include` accepts a comma-separated
selection from `syntax`, `context`, `source`, `signature`, `type`, `effects`,
`visibility`, `diagnostics`, and `diagnostic`. `--expand` requests potentially
large relations: `references`, `members`, `calls`, `applications`,
`effect-witnesses`, or `declarations`. `calls` contains function-call edges;
`applications` also includes constructors, projections, and lookups.

A source-position query resolves the smallest containing syntax node. Its
available metadata includes the information needed to produce a valid
continuation:

- grammar category and permitted child forms or labels;
- token role, including `@atom-value`, `@entity-reference`, or
  `@code-reference`, and the grammar or VIBON schema slot that selected it;
- expected and observed type;
- for an application, its resolved kind, callee type or entity, operand
  contract, selector when static, and exact result type;
- allowed effect ceiling and currently computed performed row;
- visible lexical names and imported module aliases;
- resolved declaration candidates with canonical identities;
- applicable constructors, record-field atoms, tuple indices, collection
  key/index expectations, interface methods, and generic bounds;
- diagnostics and safe fixes at the location; and
- the exact workspace revision used.

Entity metadata and expanded references operate on resolved identities, not
token spelling. They distinguish module, type, value, function, method,
interface, effect root, effect operation, field, variant, and lexical binder
identities. An interface implementation and its members have no single-atom
identity and are reported as the contract member, applied interface target, and
receiver type together, as the type-system chapter defines. The applied target
is required: one receiver may implement a generic interface at several targets,
and omitting it would report two distinct members under one identity.
Discards have no identity, definition, references, or rename target; a query at
`-`, `@-`, or `-:` reports its discard role and enclosing context only.

Function and source-position metadata can include written-or-default ceilings
and performed rows. `--expand effect-witnesses` adds the call paths that
introduced those roots. `vibra query @workspace --expand declarations` emits
one normalized record per declaration: canonical source, signature, effect
row, visibility, call edges, referenced types, error types, and source
fingerprint. The exact checker remains the admission test for a retrieved
candidate; workspace metadata is not a substitute type system.

Every resolved application reports one of `@function`, `@constructor`,
`@tuple-projection`, `@record-projection`, or `@collection-lookup`. Projection
metadata includes the compile-time component or field identity; lookup
metadata includes the required key or index type. Only `@function` carries a
callee effect row or contributes a function-call edge. Completion after a
tuple callee proposes valid literal indices, completion after a record callee
proposes visible atom field selectors, and collection lookup proposes the
required operand type. Pattern completion uses the same resolved field,
component, and constructor identities.

Diagnostic codes are built-in query subjects. For example,
`vibra query @type.argument-mismatch --include diagnostic` returns the code's
fixed level, domain, summary, and fix capability from the compiler's diagnostic
registry. It does not load or execute a Vibra declaration.

Tooling MUST NOT infer reference role from atom shape. A query at `@std.io` in
an ordinary expression reports `@atom-value`; the same token in the module
locator of `(import io @std.io)` reports `@entity-reference` and the resolved
module. `io.stdout` in a source effect row reports `@code-reference`, the
lexical import alias, and the canonical nominal effect identity. An effect atom
inside a `project.vibon` target reports `@entity-reference` because that typed
schema slot requires one, as does its `entry` declaration reference. A slot
decides whether a token is a reference and which entity kind it must denote; the
code graph alone decides which entity its path denotes.

## Transactional edit plans

Supported v1 semantic changes are format, safe diagnostic fix, semantic rename,
and organize imports. Project add/remove and generated module scaffolding use
the same plan envelope. V1 does not expose arbitrary AST patterns, text search
and replace, or a general patch language as a trusted compiler operation.

An edit plan is a JSON response and contains at least:

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

Rename rejects generated-only nodes, discards, keywords, visibility
violations, name collisions, and shadowing. A record-selector atom in
`(value @field)` is a rename reference only after application resolution binds
it to that record's field identity; an ordinary equal atom value is untouched.
A rename may touch imports and qualified references across packages in the
current workspace but never edits vendored dependencies.

## MCP server

`vibra mcp --workspace <root>` exposes the shared services over a versioned MCP
stdio server. Tool names mirror CLI concepts, for example `vibra.query`,
`vibra.edit.rename-plan`, and
`vibra.edit.apply-plan`.

The default server is read-only and does not execute project code. Independent
startup flags authorize narrow server actions:

- `--allow-write` permits application of a validated edit plan;
- `--allow-test` permits the test runner;
- `--allow-run` permits execution of a statically valid project target; and
- `--allow-sync` permits exact-revision dependency fetch and vendor writes.

These flags govern MCP actions, not Vibra effects. `--allow-run` does not add
effect roots: the selected binary target supplies its complete static ceiling
and blanket execution consent. The server never accepts an executable path,
shell fragment, arbitrary CLI subcommand, environment entry, or unvalidated
tooling path from a client.

All tooling paths resolve beneath the canonical workspace. Symlink and
junction escape, `..` escape, absolute paths outside the root, and writes to
vendored dependencies are rejected. This confines compiler and edit operations;
it does not narrow filesystem paths used by a running target that declares a
filesystem effect. An apply call must present the exact plan and revision
returned by a prior plan call.

## Schemas and errors

The v1 implementation publishes JSON Schemas for CLI and MCP project
inspection, diagnostics, normalized query results and expansions, edit
plans/results, test reports, command results, external-registry inspection, and
MCP envelopes. Persistent project, lock, and build-data record schemas are
defined as VIBON data contracts and tested through their typed decoders, not
duplicated as normative JSON files.

Schema IDs and major versions are contracts. Unknown fields are rejected in
inputs and ignored only where an output schema explicitly permits forward
extension. Diagnostic and result atoms are serialized to JSON with their exact
spelling, for example `"@type.argument-mismatch"`. Errors distinguish invalid input,
diagnostics found, stale workspace, MCP action denied, dependency failure, host
operation failure, runtime trap, and internal failure.
