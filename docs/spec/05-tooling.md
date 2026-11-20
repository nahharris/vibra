# Vibra CLI, MCP, and code tooling

Status: normative target
Implementation status: none of this chapter is implemented. A tool MUST
report which commands, profiles, and schema versions it actually
supports, as
[Conformance profiles](07-diagnostics-and-conformance.md#conformance-profiles)
requires.

## One workspace engine

The CLI, the MCP server, editor integrations, and the compiler MUST use one
workspace engine. That engine acquires source, parses it losslessly, resolves
names, checks types, guards, and effects, produces diagnostics, answers
queries, and plans edits. An adapter MAY present the engine's facts
differently. It MUST NOT contain a second checker or a second symbol index.

- **Explicit inputs.** Semantic phases consume immutable inputs they are
  given. They MUST NOT read files, dependencies, environment values, or the
  network themselves.
- **One source alone.** A single source given to the checker is one module of
  a one-module library workspace with the embedded standard library. It keeps
  the identity it was given, can import only modules captured in that
  workspace, and MUST receive the same verdict and spans as the same module
  in a project. No tool uses a weaker checker for snippets.
- **Files are the authority.** A content-addressed cache MAY hold derived
  results. Deleting it MUST NOT change source, approvals, identity, or
  execution. There is no staged codebase, no live redefinition, no
  publication step, and no second representation of source.
- **Formats.** JSON is the format of command and MCP responses. Persistent
  project, lock, approval, policy, and build documents are canonical `.vibon`
  data. A `.vib` file holds Vibra source. A loader chooses by extension, MUST
  NOT inspect content to choose, and MUST NOT accept a retired spelling.

## Command surface

```text
vibra [GLOBAL...] project init [DEST]
vibra [GLOBAL...] project inspect
vibra [GLOBAL...] project add target|dependency DATA [--write]
vibra [GLOBAL...] project remove target|dependency NAME [--write]
vibra [GLOBAL...] project sync
vibra [GLOBAL...] fmt PATH [--write]
vibra [GLOBAL...] check [TARGET]
vibra [GLOBAL...] lint [TARGET] [--write]
vibra [GLOBAL...] test [TEST]
vibra [GLOBAL...] run [TARGET] [-- ARG...]
vibra [GLOBAL...] eval SUBJECT [--arg VIBON]...
vibra [GLOBAL...] build [TARGET] [--target TRIPLE]
vibra [GLOBAL...] query SUBJECT [--include METADATA] [--expand RELATIONS]
vibra [GLOBAL...] edit fix SUBJECT [--write]
vibra [GLOBAL...] edit rename SUBJECT NEWNAME [--write]
vibra [GLOBAL...] edit imports PATH [--write]
vibra [GLOBAL...] mcp [SERVER-PERMISSION...]
vibra [GLOBAL...] help
```

This is the grammar of a process command line, not Vibra source.

**Names.** A code entity is written in camelCase or PascalCase everywhere,
the command line included: an argument that names one, such as an atom, a
profile, or a result, keeps the entity's spelling. Every other word of the
command line, a command or an option, may be kebab-case. A command name is
short, preferably one word: `edit imports` organizes imports.

| Operand | Meaning |
| --- | --- |
| `GLOBAL` | `--format human\|json` (default `human`) and `--workspace PATH` (default: the current directory), each at most once, before the command |
| `PATH`, `DEST` | a workspace-relative path, written with `/` in results, confined beneath the workspace root |
| `TARGET` | the atom naming one validated local target, such as `@app`; never a file path |
| `DATA` | one canonical VIBON value holding the complete typed target or dependency record |
| `NAME` | the atom naming a target or dependency alias |
| `SUBJECT` | a [query subject](#workspace-queries) |
| `NEWNAME` | one valid identifier |
| `TEST` | a [test selector](#tests-and-selection) |

- An unknown command or option, a repeated single option, and an extra
  positional are invalid input. So are a command line with no command and a
  command group, `project` or `edit`, with no subcommand of the group
  after it; the response then carries the empty command name.
- Invalid input is answered in the format recognized before the fault:
  `--format json --format human help` is refused in JSON, and
  `--format xml help` in the human format. It carries no diagnostic. In the
  human format it is one line on standard error, whose wording is not a
  contract.
- Every token after `mcp` is a server permission operand, whatever it
  begins with; none is read as an option.
- A known command the tool does not yet support is **unavailable**, which is
  a different outcome from an unknown spelling. The support inventory MUST
  name every such command. A well-formed line of such a command is
  unavailable whatever its operands hold: its typed operands are not
  validated.
- `help`, `--help`, and `-h` in the command position are equivalent and take
  no arguments. Help shows this hierarchy with the supported inventory and
  MUST NOT present an unavailable command as working. In JSON it returns the
  usage text and the supported commands, profiles, and schemas.
- There are no aliases such as `init`, `show`, or `find`.
- Without `TARGET`, `check` and `lint` cover every local target and the
  `@tests` unit, and the `check` payload's obligations include those of the
  test modules. `check @app` covers that target only, not `@tests`. A target
  whose kind the tool's profile does not support still decodes with the
  project: it is reported `@tool.unavailable` at its target record, with the
  status `unsupported`, and the other targets are checked. `run` and
  `build` need the project's only binary target; with none or several and no
  selector, the command is invalid input with no source diagnostic. A missing
  target, or a library given to `run` or `build`, is invalid input. So is a
  `TARGET` that names no local target, as in `check @nope`: the result is
  `@command.invalidInput`, with no diagnostic.
- `edit fix` needs exactly one applicable safe fix. `edit rename` needs one
  resolved renameable identity, and checks collisions before planning. A
  subject that matches nothing, or several things, is invalid input. No tool
  guesses an edit.

### Program arguments, execution and evaluation

**Arguments.** Only `run` accepts `--`, after its optional target. Every
token after it is a program argument, in order, including another `--` or a
token that begins with `-`. `--` with nothing after it supplies no
arguments. The argument operation observes exactly these values. An argument
that is not Unicode is that operation's typed error, not malformed command
input. A test takes no program arguments; its world supplies them.

**Run and test** execute on the reference interpreter and MUST NOT need a
linker.

**Run** validates the target's binding recipes, acquires its grants in the
[startup order](04-programs-and-packages.md#startup-order) the programs
chapter defines, and then invokes the entry. Startup work and its authority
are reported separately from the entry's body. A tool permission flag never
approves a program's resource policy.

**Eval** calls one checked pure module function, named by `SUBJECT`, with
`--arg` values that are checked against its parameter types.

- It MUST NOT run a procedure or a plan, make a grant, or use ambient
  authority.
- An argument that cannot have the required type is an input error.
- A result that cannot be shown is an explicit tool outcome, not a trap.
- Each call runs against a fresh snapshot. Eval is not a REPL and redefines
  nothing.
- An unproved static obligation is an error for eval as for check; eval
  cannot postpone it to run time.

**Limits.** Every executing command runs under published, versioned limits
on work, memory, and output. Its result names the engine, the limit profile,
the limits reached, and whether the observation is complete. Reaching a tool
limit is an **unknown** outcome. It MUST NOT be reported as a pass, as a
false guard, or as an error value of the program. A scheduler quota only
suspends and resumes; it is never a program result. The compiler's own
constant evaluation uses the same interface: a required constant or
obligation that is not established within the limits fails its check. The
published limit profile is `m1Default`. It allows one compile-time
evaluation 10,000,000 reductions, a reduction being the evaluation of one
checked node.

### Initialization and project operations

**Init.** `DEST` defaults to the workspace. It MUST be absent, or an empty
confined directory whose parents exist. Init creates only `project.vibon`,
`src/`, `tests/`, and one binary module, and refuses rather than overwrite.
`created` lists the paths in the order made. The generated module is current
source, takes no grants, and has an entry `proc` that returns a successful
`ExitCode`. The project has version `0.1.0`, no dependency, one binary
target, and an empty `grants` dict. The field names and the entry signature
are those of the [programs chapter](04-programs-and-packages.md#targets),
not of a template.

The default name of the package and of its target is derived from the
destination's name. Split it into runs of ASCII letters and digits and
lower-case each run. The name is the first run followed by each later run
with its first letter upper-cased, so `my-app` gives `myApp`. With no run
the name is `hello`. If the name starts with a digit, `project` is put
before it: `project2048`.

**Inspect** returns the captured project, its targets, each dependency's
source and state, the supported operations, and diagnostics. It fetches
nothing.

**Add and remove** take typed records, never a shell fragment or a text
patch. Each previews a plan and applies it only with `--write`. An invalid
or ambiguous identity is rejected. The edit keeps comments where it can and
lays the file out with the
[source-preserving formatter](04-programs-and-packages.md#decoded-values-and-canonical-bytes)
of the programs chapter.

**Sync** is the only command that acquires dependencies from the network.
It accepts exact HTTPS Git revisions and confined local dependencies,
verifies every input, runs no dependency code and no lifecycle script,
exports no Git metadata, and publishes the vendor tree and lock only after
everything validates. It rejects vendor content that conflicts. A dependency
cycle is an error whatever the revisions.

### Formatting

`fmt PATH` formats source or VIBON according to the extension. For VIBON it
is the source-preserving formatter: it keeps comments, spellings, and the
written order of entries. It previews by default; `--write` applies.

| Outcome | `changed` | `written` | `text` |
| --- | --- | --- | --- |
| preview | input bytes differ from the formatted bytes | `false` | the formatted text |
| written | `true` | `true` | `null` |
| diagnostics prevent writing | as computed | `false` | the planned text |

- The formatter MUST NOT rename an identifier, drop or add an `as`, or
  reorder operands, labelled arguments, field initializers, or dict pairs. A
  name-case convention is a diagnostic, never a reason to rename.
- Source the parser recovered from ambiguously is kept byte for byte. The
  formatter does not repair by guessing.
- A captured module is formatted with a semantic check of the candidate text
  in place of the captured bytes. A file that is not a module is formatted
  by syntax alone. When checking is unavailable, formatting proceeds from
  exact syntax facts; no incomplete fact authorizes a semantic edit.

### Tests and selection

A test selector is the test module's atom, `::`, and the test's name as one
canonical string literal: `@tests.math::"adds one"`. It contains no other
whitespace and its module begins `@tests.`. A malformed selector is invalid
input (exit 2, no diagnostics).

1. Tests are discovered in canonical module order, then declaration order.
   With no selector, a missing or empty test root is an empty passing suite.
2. Project, source-graph, and required parse errors are reported first, with
   zero counts and no items. Without a selector every test module is parsed;
   with one, its module is.
3. A selector that then names no module or no test is invalid input.
4. Every selected test is checked before any runs. An ordinary error in the
   selection invalidates all of it: every item is `@test.invalid` and none
   runs. Warnings do not block.
5. A valid construct the tool does not support marks `@test.unavailable`
   only the items whose declarations reach it. The others may run.
6. Each test runs in a fresh fake world with its declared ceiling and no
   real grant.

| Item result | Meaning |
| --- | --- |
| `@test.passed` | the body completed with no failed assertion |
| `@test.assertionFailed` | an assertion was false, or an operand could not be observed |
| `@test.trap` | a toolchain defect trap |
| `@test.invalid` | the selection had static errors |
| `@test.unavailable` | the item needs an unsupported construct |

`selected` counts selected items, `passed` only `@test.passed`, and `failed`
is their difference. An item that ended unknown or exhausted is never
counted as passed. When items differ, the suite result follows this
precedence: trap, operational exhaustion, unavailable, test failed, ok. A
diagnostic appears once in the envelope and is attributed to every item it
affects. An item keeps the audit events it completed before a later failure.

## Machine contracts

### Versioned envelope and results

The contracts of this chapter use JSON Schema major version 2, with
identifiers `urn:vibra:schema:v2:<document-kind>`. A producer MUST validate
every document it emits against its published schema, including rules that
relate fields.

JSON is the only place `null` appears: it is the **JSON projection** of an
absent optional field of a VIBON document, and of an unavailable fact.
Every VIBON document a response carries is embedded as its canonical text.
Every structured response is a closed object with these fields:

```json
{
  "schemaVersion": 2,
  "toolVersion": "version",
  "command": "check",
  "workspaceRevision": "sha256:...",
  "status": "passed",
  "result": "@command.ok",
  "diagnostics": [],
  "limits": {"profile": "id", "reached": [], "complete": true},
  "payload": {}
}
```

- `workspaceRevision` is `null` only when no snapshot was taken: help, a
  command-line error, or failed discovery. It is never invented.
- `status` is `passed`, `refuted`, `unknown`, or `unsupported`. It describes
  the observation requested. Passing sampled tests is `passed` for that run,
  not a universal claim about the program. The result fixes it, as the table
  below states, with two exceptions: a reached limit and a bare guard that
  is not established make it `unknown`.
- `command` is the words that select the command, joined by one space:
  `check`, `project init`, `edit imports`. An operand is never part of it,
  so `project add target` has the name `project add`.
- `limits.profile` names the published limit profile, with its units, defaults,
  and engine version. `reached` lists the names of the limits reached, in order;
  the memory limit is `"memory"`. `complete` is `false` after truncation or
  exhaustion.
- When output would exceed its limit, the tool emits a bounded, valid
  document with status `unknown`. It never emits truncated JSON and never
  silently omits facts.

| Result | Exit | Status | Meaning |
| --- | --- | --- | --- |
| `@command.ok` | 0 | `passed` | the requested operation completed and succeeded |
| `@command.diagnostics` | 1 | `refuted` | static diagnostics prevent admission |
| `@command.testFailed` | 1 | `refuted` | at least one selected test failed |
| `@command.invalidInput` | 2 | `unknown` | invalid command, selector, or typed input |
| `@command.operationalFailure` | 3 | `unknown` | tool, host, or startup failure or exhaustion |
| `@command.unavailable` | 4 | `unsupported` | a known operation outside the supported profile |
| `@command.trap` | 5 | `unknown` | a registered toolchain or runtime defect |
| `@command.programFailed` | 6 | `unknown` | the program returned a non-zero `ExitCode` |

- A bare guard that is disproved has status `refuted`; one that is not
  established has status `unknown`. Both prevent admission.
- **Exit code of a program.** `run` records the program's value in
  `programResult` and `applicationCode`, and maps any non-zero code to
  `@command.programFailed`, exit 6. A built executable returns the code
  itself. A non-zero code is not a trap, a diagnostic, or a startup failure.
  After a startup failure `programResult` is `null`.
- **Streams.** In JSON format the tool writes exactly one envelope to
  standard output and its logs to standard error. `run` captures the
  program's output into the payload; bytes that are not UTF-8 show as U+FFFD
  there, and the audit trace keeps the exact bytes. In human format the
  program's bytes go straight through and diagnostics go to standard error.
- **Invalid input.** Before a command's typed inputs are validated, the
  response has the recognized command name, or an empty one, and an empty
  payload. After validation it has that command's payload, with unavailable
  facts `null`.

### Command payloads

Each payload is a closed object. A field described as optional is a required
field that may be `null`. `SpanDocument` and `DiagnosticDocument` are defined
in the
[diagnostics chapter](07-diagnostics-and-conformance.md#diagnostic-documents-and-spans).

| Command | Payload fields |
| --- | --- |
| `help` | `usage`: string; `capabilities`: `CommandCapability[]` |
| `project init` | `workspace`: string; `created`: string[] |
| `project inspect` | `project`: VIBON text or null; `targets`: `TargetSummary[]`; `dependencies`: `DependencySummary[]` |
| `fmt` | `path`: string; `changed`: boolean; `written`: boolean; `text`: string or null |
| `check` | `accepted`: boolean; `obligations`: `ObligationRecord[]` |
| `lint` | `accepted`: boolean; `plan`: `EditPlan` or null |
| `query` | `subject`: string; `resolvedKind`: string or null; `identity`: `Identity` or null; `metadata`: object; `expansions`: object |
| `eval` | `subject`: `Identity` or null; `value`: canonical VIBON text or null; `observationError`: `ObservationError` or null |
| `run` | `target`: atom string; `programResult`: canonical value text or null; `applicationCode`: integer 0–255 or null; `stdout`: string; `stderr`: string; `auditTrace`: `AuditEvent[]`; `startup`: `StartupRecord`; `trap`: `TrapRecord` or null |
| `test` | `selected`, `passed`, `failed`: non-negative integers; `tests`: `TestItem[]` |
| `build` | `target`: atom string; `triple`: string; `executable`: confined path or null; `manifest`: canonical build VIBON text or null; `semanticIdentity`: digest or null; `artifactIdentity`: digest or null |
| `project add`, `project remove`, every `edit` | `plan`: `EditPlan` or null; `applied`: boolean; `resultingRevision`: digest or null |
| `project sync` | `dependencies`: `DependencySummary[]`; `lock`: confined path or null; `written`: boolean |

| Record | Fields |
| --- | --- |
| `CommandCapability` | one per line of the command surface, in its order: `command`, the command name; `operations`: string[], the operations a supported command offers, its own name for `help`; `profiles`: string[], the conformance profiles that cover it; `supported`: boolean; `schemaIds`: exact published identifiers of the documents it emits. The three lists are empty for an unsupported command |
| `TargetSummary` | `name`; `kind`; `root`; `entry` (nullable); `bindingStatus`: `exact`, `recovered`, or `unavailable` |
| `DependencySummary` | `alias`; `kind`; `source`; `target` (nullable); `status`: `captured`, `verified`, `unavailable`, or `refuted`; `identity` (nullable) |
| `TestItem` | `name`, the selector; `result`, the `@test.*` atom; `failure` (nullable); `trap` (nullable); `auditTrace`: `AuditEvent[]`, the test's events; `diagnostics`: string[], the codes of the envelope's diagnostics attributed to the item; `status`, that of the command result the item's result corresponds to: `passed` for `@test.passed`, `refuted` for `@test.assertionFailed` and `@test.invalid`, `unknown` for `@test.trap`, and `unsupported` for `@test.unavailable` |
| `TestItem.failure` | `kind`: `assertion` or `observation`; `assertion`, `expected`, `actual`: canonical text or null; `primarySpan`: `SpanDocument`; `observationError`: null for an assertion |
| `TrapRecord` | `trapCode`; `origin` (nullable) |
| `ObservationError` | `code`: `unsupportedValue`; `type` and `origin`: nullable |
| `StartupRecord` | `status`: `notStarted`, `acquired`, or `failed`; `bindings`; `auditTrace`; `failure`: null or `{binding, code, origin}`, where `code` is `@grant.requiredUnavailable` for a refused required leaf and `@grant.acquisitionFailed` for a failed acquisition |
| `StartupRecord.bindings[]` | one per processed leaf, in processing order: `name`; `recipeIdentity`: `sha256:` and the SHA-256 of `vibraRecipeIdentityV2`, a zero byte, and the canonical bytes of the recipe with its policy normalized; `providerIdentity`: `{provider, version}`; `status`: `acquired`, `refused`, or `failed`, the three outcomes of the [startup order](04-programs-and-packages.md#startup-order), or `released` for an acquired leaf after its release; `grantProjection` (nullable) |

A summary never executes an acquisition. A grant projection is data that
describes a grant; it cannot be turned back into one, and its schema and
`AuditEvent` belong to the [runtime chapter](06-runtime.md#audit-trace). A
function, a plan, or a grant is never encoded by inventing a string for it.
A failed command keeps the startup events that completed.

A human test report keeps selectors and result atoms, prints each item that
did not pass with its diagnostic, failure, or trap, and ends with the
passed, failed, and selected counts. Its exact wording is not a contract.

### Snapshot, claim and witness identity

**Workspace revision.** `workspaceRevision` is `sha256:` and 64 lower-case
hexadecimal digits. It hashes captured input bytes, never timestamps or
absolute paths, and deliberately not canonical bytes: an edit that changes
only a comment or a spelling is a new revision. The hashed bytes are, in
order:

1. the ASCII text `vibraWorkspaceRevisionV2`;
2. one zero byte;
3. the number of records, as a big-endian 64-bit integer;
4. the records, sorted by the UTF-8 bytes of `(kind, sourceId)`.

A record is three byte strings, each preceded by its length as a big-endian
64-bit integer: the kind, the project-relative source identifier, and the
exact input bytes. The kinds are `project`, `lock`, `policy`, `approval`,
`source`, `dependency`, and `toolchain`. The single `toolchain` record has
the identifier `@toolchain` and holds canonical VIBON naming the exact
grammar, checker, standard library, driver, and policy profile identities.
Every admitted input contributes a record; an absent optional file
contributes none. Identifiers are unique within a kind.

A query answers from one captured revision. It never rereads the disk and
never mixes revisions. Configuration that exists only at run time is
recorded in startup evidence, not in the snapshot.

Test vector (raw bytes, no trailing newline): `project/project.vibon` holds
`project`; `source/src/main.vib` holds
`fn f returns answer: Str do return "ok" endfn`; `toolchain/@toolchain`
holds `toolchain`. The revision is
`sha256:5fdc01744c45544c5765d4ec386b7a45bb990bc4b4c0ca415493546c7f36c8ec`.
The vector tests the framing only; its contents are not a valid project.
Host tests MUST cover it and an edit of equal length.

The `toolchain` record is a `@toolchain.v2` document:

| Field | Type | Meaning |
| --- | --- | --- |
| `format` | `@toolchain.v2` | required |
| `version` | `Str` | the toolchain release |
| `grammar` | `Str` | the source and VIBON grammar identity |
| `checker` | `Str` | the checker identity |
| `stdlib` | `Str` | `sha256:` and the SHA-256 of the canonical bytes of the embedded [`@stdlibManifest.v2`](04-programs-and-packages.md#the-standard-library) |
| `drivers` | `Str` | the driver set identity |
| `policy` | `Str` | the policy profile identity |
| `registry` | record `major: U32`, `minor: U32` | the host registry the toolchain implements |

```vibon
{
  format: @toolchain.v2
  version: "0.0.2"
  grammar: "sourceV2"
  checker: "staticV2"
  stdlib: "sha256:65c20520d3bde9ee2d53288d20df389fec9bce119c15722e95628d3005cf220f"
  drivers: "systemV2"
  policy: "scopePolicyV1"
  registry: {major: 2u32 minor: 0u32}
}
```

**Claim and witness.** Each declaration has two further hashes, each
`sha256:` and 64 lower-case hexadecimal digits of the SHA-256 of a framing
text, one zero byte, and the canonical bytes of a record.

| Identity | Framing text | Record | Covers |
| --- | --- | --- | --- |
| claim | `vibraClaimIdentityV2` | `@claim.v2` | public signatures with their binders and bounds, labelled defaults, nominal anchors, representations and their visibility, guard predicates and consequences, implementations with their choices, and the approved bindings and policies of every target the declaration is the entry of |
| witness | `vibraWitnessIdentityV2` | `@witness.v2` | the checked bodies of one recursive group and the witnesses of everything they resolve to |

A `@claim.v2` record has these fields, in this order:

| Field | Type | Meaning |
| --- | --- | --- |
| `format` | `@claim.v2` | required |
| `declaration` | `Str` | the declaration's identity |
| `kind` | atom | a declaration kind atom of the [type chapter](02-type-system.md#declared-types) |
| `access` | atom | `@public` or `@private` |
| `representation` | optional atom | `@public` or `@private`, for a type |
| `signature` | optional [signature record](06-runtime.md#open-signatures) | for a callable, a type, or a module value: its binders with their bounds, its receiver slot, and its type |
| `defaults` | dict from `Str` label to `Str` | the canonical source text of the constant default of each labelled parameter; written even when empty |
| `guards` | list of `{kind: Atom predicate: Str consequence: Str}` in written order | `consequence` optional; `kind` one of `@requires`, `@ensures`, `@where`; each text the canonical source of the expression |
| `implementations` | list of [implementation records](06-runtime.md#open-signatures) | every implementation written in the declaration, sorted by the canonical bytes of `contract`, then of `receiver` |
| `targets` | list of target facts sorted by `target` | one per binary target whose `entry` is this declaration; written even when empty |

A **target fact** is `{target: Atom effects: E bindings: B}`: the target's
name; its admission ceiling `E`, a list of entity reference records sorted
by `path`; and `B`, a list of `{leaf: Atom recipe: Str}` sorted by the
UTF-8 bytes of the leaf's spelling, where `recipe` is the recipe identity
of [Command payloads](#command-payloads). The recipe identity covers the
provider, the resource, the normalized policy, the limits, and `after`, so
changing what a target binds or permits changes its entry's claim.

A `@witness.v2` record covers one **recursive group**: a maximal set of
declarations each of whose bodies reaches every other through resolved
references. A declaration that is not recursive is a group of one. The
group is hashed as one unit, so no witness depends on itself, and every
member has the group's witness identity.

| Field | Type | Meaning |
| --- | --- | --- |
| `format` | `@witness.v2` | required |
| `members` | list of `{path: Atom declaration: Str body: Str}` sorted by the UTF-8 bytes of the spelling of `path` | every member, by name: its canonical path, its identity, and the canonical source text of the whole declaration |
| `dependencies` | list of `{declaration: Str witness: Str}` sorted by `declaration` | one per declaration outside the group that a member's body resolves to, with the witness identity of that declaration's own group |

A dependency is pinned by its witness, so a change to the body of anything
a declaration resolves to, however far, changes the declaration's witness.
The groups form an acyclic graph, because a cycle of references is one
group.

Test vectors. Both hash the record's canonical bytes: one line, with no
trailing newline. The records are shown laid out for reading. The vectors
test the framing and the serialization only; their identities are
placeholders and their lists are not those of a real program. The claim
record below has 436 canonical bytes and hashes to
`sha256:ee7305039b18bffa7745b5a96ed3790419bb8d26782a75558a3c5a354004302a`:

```vibon
{
  format: @claim.v2
  declaration: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
  kind: @proc
  access: @public
  signature: {
    binders: []
    typeInfo: {
      kind: @proc
      parameters: []
      labels: [:]
      result: {
        kind: @declared
        path: @std.process.ExitCode
        seed: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
        arguments: []
      }
      effects: {roots: []}
    }
  }
  defaults: [:]
  guards: []
  implementations: []
  targets: []
}
```

Its canonical bytes begin `{format: @claim.v2 declaration: "sha256:213e`
and end `implementations: [] targets: []}`. The witness record below has
234 canonical bytes and hashes to
`sha256:241dbcd0700fd2be1dab30d65e0fc9ed4c1d28eed647399a21969fd70f235e33`:

```vibon
{
  format: @witness.v2
  members: [
    {
      path: @hello.main.main
      declaration: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
      body: "proc main returns code: ExitCode do return ExitCode(0u8) endproc"
    }
  ]
  dependencies: []
}
```

An approval separately protects the test bodies it selected, their helper
code and data, the selection, and the runner, tool, driver, and policy
versions. A manifest MUST distinguish the code under repair from the
protected test code: pinning every body would make repair impossible.
Changing a guard, a consequence, a test, or an implementation's choice is
never a witness-only change. A report says which category changed and
against which approved revision.

A declared type's identity follows the logical anchor of the
[type chapter](02-type-system.md#declared-types). Editing a body does not
change it. A rename or move changes it, and the edit preview says so.

## Workspace queries

`query SUBJECT` accepts `path:offset` (a UTF-8 byte offset into a confined
source), a canonical entity atom, a registered diagnostic code, or
`@workspace`. It returns compact metadata by default.

| Option | Values, comma-separated |
| --- | --- |
| `--include` | `syntax`, `context`, `source`, `signature`, `type`, `effects`, `visibility`, `diagnostics`, `diagnostic`, `index` |
| `--expand` | `references`, `members`, `calls`, `applications`, `effectWitnesses`, `declarations` |

`calls` holds the edges that execute a function. `applications` also holds
constructions, projections, collection lookups, and the building of plans.

Query results are projections of the one checker's facts for one snapshot,
never a second computation. They report the grammar slot, visible locals and
imports, expected and observed types, whether the expected type was written,
constructors and fields, generic bounds, applicable operands, static
obligations, grant and driver summaries, diagnostics and fix capability, and
the revision. For a callable of unknown body the operation row is still
known exactly; which resource it holds may be unknown. Unknown never means
"no authority".

### Structural position and semantic envelope

A position query selects the smallest non-empty syntax node that contains
the offset. The offset MUST be on a character boundary and may be the end of
the file. A zero-width recovery node at the offset wins; ties go to the
deepest node, then source order. An offset outside the source or inside a
character is rejected.

**Structural record** (`sourcePositionQuery`, version 2):
`schemaVersion`, `offset`, `sourceId`, `span`, `mode` (`source` or `data`),
`syntaxKind`, `category`, `status`, `permittedForms`, `permittedLabels`. The
categories are `module`, `declaration`, `type`, `pattern`, `expression`,
`declarationAttribute`, `effectRow`, `dataField`, `trivia`, and
`recovery`. `null` means unavailable; an empty array means known to be
empty. Structural facts do not imply the source checked.

**Semantic envelope** (`workspacePositionQuery`, version 2):
`schemaVersion`, `toolVersion`, `workspaceRevision`, `sourceId`, `offset`,
`nodeId`, `structural`, `role`, `context`, `identity`, `expectedType`,
`observedType`, `visibleLocals`, `visibleImports`, `declarationCandidates`,
`application`, `pattern`, `effects`, `obligations`.

- `nodeId` is `<sourceId>#<start>-<end>`. A consumer joins facts by
  `(workspaceRevision, sourceId, nodeId)`, never by spelling.
- Each semantic field is `{status, value}`. The status is `exact`,
  `recovered`, or `unavailable` (with a `null` value). A complete fact stays
  `exact` beside an unrelated broken module. An incomplete fact is
  `recovered`; it is not an exact lower bound.
- **Roles:** `@atomValue`, `@entityReference`, `@codeReference`,
  `@literal`, `@localBinding`, `@discard`, `@declaration`, `@application`,
  `@unknown`. A role comes from the grammar or schema slot, not from how the
  token looks: `@std.io` is an atom value in an expression and an entity
  reference in an import. A `performs` entry resolves a name to an
  interface. A project entry, driver, or policy reference resolves only
  because its typed data slot says so.
- **Contexts:** `module`, `initializer`, `function`, `proc`, `parameter`,
  `anonymousFunction`, `letValue`, `letElseFallback`, `returnOperand`,
  `branch`, `argument`, `result`, `contractPredicate`,
  `contractConsequence`, `trivia`, `recovery`.
- **Locals.** A `let` opens no nested scope. The visible locals at a
  position are the earlier successful `let`s of the same sequence; a `let`'s
  own names are not visible in its initializer or its `else`. `_` has the
  discard role and no identity, definition, references, or rename.
- **Identities** are `{kind, canonical}`. A declaration uses its canonical
  logical identity. A local uses `binder:<sourceId>:<start>-<end>`, valid
  within one revision only. Lists of locals are in lexical order, imports in
  written order, and candidates in identity order. A private entity is not a
  candidate where it is not visible.

**Type document:** the JSON projection of the
[canonical type encoding](06-runtime.md#types). At a position inside an
open declaration it is the projection of the
[open type encoding](06-runtime.md#open-signatures), whose binder
references index the binders of the enclosing declaration's signature
record; there is no other spelling of a type parameter. A declared type is
referenced by its path and identity, never expanded; an interface value
carries its fixed `choices`; a callable carries its `labels` and, for a
`proc` or `intent`, its complete `effects`.

**Application fact:** `{kind, callee, calleeType, positional, labels,
resultType, dispatch}`.

- `kind` is `@function`, `@procPlan`, `@constructor`, `@contract`,
  `@recordProjection`, or `@collectionLookup`.
- Operands are in written order. `callee` is `null` without one resolved
  declaration.
- A constructor's `resultType` is `T` for a bare `where` and
  `Result(T E)` for `where … else`, whatever the checker can fold.
- `dispatch` is `null` or `{contract, member, receiver, selection,
  destination}`. `selection` is `static`, `default`, `dynamic`, or
  `closed`. `destination` is `true` for a member with no receiver slot that
  was chosen by the expected type.
- Applying a `proc` member builds a plan and performs nothing. An `await` is
  reported separately, with the plan's type, its complete row, and its
  witness.

**Pattern fact:** `{kind, scrutineeType, bindings, constructor}`, where
`kind` is `@wildcard`, `@binding`, `@literal`, `@constant`, `@variant`,
`@record`, `@tuple`, `@wrapper`, or `@list`, `bindings` is the ordered list
of identities and types, and `constructor` is the resolved identity or
`null`.

**Obligation record:** `{kind, declaration, predicateIdentity, source,
status, ruleIds, dependencies, counterexample}`, the JSON projection of the
[obligation record](07-diagnostics-and-conformance.md#persistent-observations)
of `@types.v2`, field for field. `kind` is `requires`, `ensures`, or
`where`. `declaration` is the identity of the declaration that contains the
site and `predicateIdentity` the identity of the guarded declaration (the
record's `predicate`). `source` is the span document of the site (the
record's `span`). `status` is `passed`, `refuted`, `unknown`, or
`unsupported`. `ruleIds` is the record's `rules`, and `dependencies` its
sorted identities of the guarded declaration and of every declaration whose
checked body or value the discharge read. `counterexample` is the canonical
VIBON text of the refuting values or `null`. A guard with `else` is
recorded in the declaration's metadata with its consequence; having a
consequence never makes its predicate `passed`.

### Effect and authority projections

The `effects` value is `{enclosing, application, target, authority}`, plus
`witnesses` under `--expand effect-witnesses`.

| Field | Content |
| --- | --- |
| `enclosing` | `null` or `{identity, kind, ceiling, performed}`; `kind` is `function`, `proc`, `operation`, `anonymousFunction`, or `test`. An operation's ceiling includes its owner. |
| `application` | `null` or `{immediate, latent, awaitSite}`. Building a plan performs none of its latent row. |
| `target` | `null` or `{entry, bindings, possibleOperations, startupOperations}`: the reviewed recipes and drivers and the operations they allow, never a blanket permission. |
| `authority` | the grant projections known at this position and, explicitly, what is unknown. It grants nothing. |
| `witnesses` | `{root, hops}` per root, in identity order. A hop is `{span, callee}`; `callee` is `{kind, identity}` with kind `function`, `proc`, `operation`, `functionValue`, or `contractMember`. |

A witness is the same chain, with the same tie-breaks, that
`@effect.outsideCeiling` reports. For each `await` a report shows the
operation, its delivery class, the driver bound for the target, and the
policy in plain terms. A static summary names operation interfaces; it never
invents which resource a run will touch.

### Index records

`query @workspace --include index --expand declarations` emits a canonical
`@index.v2` VIBON document. Its JSON form is `urn:vibra:schema:v2:index`,
with the same order and facts, atoms keeping their `@`, and absent optional
fields projected as `null`. The index is for retrieval; embedding, ranking,
and storage are not compiler services.

| Field | Type | Meaning |
| --- | --- | --- |
| `format` | `@index.v2` | required |
| `revision` | `Str` | the workspace revision |
| `declarations` | list of declaration records sorted by the UTF-8 bytes of `id` | every declaration of the local package |
| `implementations` | list of index implementation records sorted by the canonical bytes of `implementation.contract`, then of `implementation.receiver` | every implementation of the local package |
| `references` | list of reference records sorted by `source`, then `start`, then `end` | every resolved reference |

| Record | Fields |
| --- | --- |
| declaration | `id` (`Str`, the identity); `kind` (a declaration kind atom of the [type chapter](02-type-system.md#declared-types)); `module` (atom); `owner` (optional `Str`, the owner's identity; absent at top level); `access` (`@public` or `@private`); `source` (a span record); `text` (`Str`, the formatted source); `binding` (optional record `provider: Atom`, `wireSymbol: Str`, present for an external declaration); `facts` (optional, absent when not established) |
| `facts` | in this order: `signature` (optional [signature record](06-runtime.md#open-signatures); absent for a module, an interface, and an associated type); `ceiling` (optional row, the written `performs`; present for a `proc` and a test); `effects` (optional row, the performed row; present with `ceiling`); `errors` (list of open type encodings, the ordinary result error types, in written order, written even when empty); `applications` (list of application facts sorted by `source`, written even when empty); `contracts` (list of `{kind: Atom predicate: Str consequence: Str}` in written order, with `consequence` optional, distinguishing bare guards from guards with a consequence; written even when empty); `claimIdentity` (`Str`, required); `witnessIdentity` (`Str`, required) |
| application fact | in this order: `source` (a span record); `kind` (`@function`, `@procPlan`, `@constructor`, `@contract`, `@recordProjection`, or `@collectionLookup`); `callee` (optional `Str` identity, absent without one resolved declaration); `calleeType` (an open type encoding); `positional` (list of open type encodings, the operand types in written order); `labels` (dict from `Str` label to open type encoding); `resultType` (an open type encoding); `dispatch` (optional dispatch fact) |
| dispatch fact | in this order: `contract` (`Str`, the interface's identity); `member` (`Str`, the member's identity); `receiver` (an open type encoding); `selection` (`@static`, `@default`, `@dynamic`, or `@closed`); `destination` (`Bool`, `true` for a member with no receiver slot chosen by the expected type) |
| index implementation | in this order: `implementation` (an [implementation record](06-runtime.md#open-signatures): its binders, its positional interface key `contract`, its `receiver`, and its `choices`); `source` (a span record); `text` (`Str`); `members` (list, in written order, of `{member: Str source: S signature: G text: Str}`, where `member` is the contract member's identity, `S` a span record, and `G` a signature record over the implementation's binders) |
| reference | `from` (`Str`, the identity of the enclosing declaration); `written` (`Str`); `source` (a span record); `to` (optional `Str`, absent when it depends on failed checking) |

A span record is `{sourceId: Str start: U64 end: U64}`. A row is
`{roots: R}`, with `R` a list of
[entity reference records](06-runtime.md#types). Every open type encoding
inside a `facts` record refers to the binders of that record's `signature`.
A `facts` record always holds its two identities, so it is never empty.

The example's `id` is the declaration identity test vector of the
[type chapter](02-type-system.md#declared-types), which matches its module
and name. The record has no `facts`:

```vibon
{
  format: @index.v2
  revision: "sha256:5fdc01744c45544c5765d4ec386b7a45bb990bc4b4c0ca415493546c7f36c8ec"
  declarations: [
    {
      id: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
      kind: @proc
      module: @hello.main
      access: @private
      source: {sourceId: "src/hello/main.vib" start: 0u64 end: 68u64}
      text: "proc main returns code: ExitCode do return ExitCode(0u8) endproc"
    }
  ]
  implementations: []
  references: []
}
```

- An implementation is identified by the applied interface and the receiver
  type together, and a member also by its contract member. Two
  implementations for different applications of a generic interface never
  collapse into one, and a changed choice changes the implementation's
  observation and the receiver's claim.
- A module declaration has no `facts.signature` and its whole formatted
  text. The index covers the local package; a dependency or standard-library
  entity appears only as a reference target unless expanded.
- A rejected body never shows exact rows or proofs.

A diagnostic code used as a query subject reads the compiler's registry. It
never loads or runs a declaration.

## Transactional edit plans

The supported edits are canonical format, safe diagnostic fix, rename, and
organize imports. Project add and remove and module scaffolding use the same
plan. An edit names a typed operation, a target identity, and a new name or
typed record. Arbitrary syntax patterns, free text replacement, and patch
languages are not compiler operations.

**`EditPlan`, version 2:** `{schemaVersion, toolVersion, workspaceRevision,
operation, target, documents, edits, resultingDiagnostics, postconditions}`.
`target` is an identity or `null`. `documents` lists each affected input as
`{path, revision}`. An edit has a confined path, a half-open UTF-8 byte
span, replacement text, the expected syntax identity, and a source
fingerprint. Edits are ordered by path and span and do not overlap. A rename
records any change of nominal or claim identity in `postconditions`.

A mutating command previews unless `--write` is given; over MCP, writing
needs the write permission and a plan returned earlier. Applying a plan:

1. verify the workspace revision and every document revision;
2. stage every edit in memory;
3. parse and check the affected modules again;
4. format, without reordering operands;
5. validate the postconditions;
6. publish all files, or none.

A plan that is stale, ambiguous, overlapping, ill-typed, mismatched, or that
changes a policy without approval fails with nothing written. A backend that
cannot write several files atomically MUST refuse before writing. An
interrupted application is never reported as applied.

Rename rejects discards, generated nodes, keywords, private-access
violations, collisions, and shadowing. A field is renamed only where
resolution binds a token to that field; an equal atom or string is left
alone. Edits reach local modules and never vendored dependencies. Organizing
imports changes neither resolution nor operand order. No fix or format may
alter a protected claim, test, or policy outside its approval workflow.

## MCP server

`vibra mcp` serves the same engine over a versioned stdio MCP server rooted
at the workspace. Its tools mirror the CLI: `vibra.query`, `vibra.eval`,
`vibra.edit.renamePlan`, `vibra.edit.applyPlan`, `vibra.test`,
`vibra.run`, `vibra.project.sync`. The server publishes its schemas and
capability inventory, and no tool does anything the engine cannot.

By default the server reads only and executes no project code.

| Flag | Permits |
| --- | --- |
| `--allow-write` | applying a validated plan |
| `--allow-eval` | bounded pure evaluation |
| `--allow-test` | tests in isolated worlds |
| `--allow-run` | running an admitted target |
| `--allow-sync` | dependency acquisition and vendor writes |

- A permission makes no grant, widens no resource scope, and replaces no
  target approval. A target gains no filesystem or network authority because
  MCP may run it.
- The server accepts no executable path, shell fragment, arbitrary
  subcommand, or environment override.
- Paths are confined to the workspace. Escapes through a parent, a symbolic
  link, or a junction, absolute paths outside it, and writes to vendored
  code are rejected.
- Applying needs the exact plan and revision returned earlier.
- A disconnect or cancellation is an explicit incomplete outcome. It does
  not show that an action already dispatched did not happen.

## Schemas, delivery and exclusions

The toolchain MUST publish, and validate its own output against, version 2
JSON Schemas for command results, diagnostic and span documents, structural
and semantic queries and their expansions, the index, edit plans and
results, test reports, registry inspection, and MCP envelopes. Persistent
project, lock, build, policy, and approval documents are typed VIBON with
decode and round-trip tests, and have no second JSON form. Unknown input
fields and versions are rejected. A change of meaning or of fields is a new
major version.

Read queries, bounded eval, and revision-checked edits belong to the
language release. The MCP server and editor adapters follow in a named
milestone of the [roadmap](../roadmap/README.md). `build` produces native
executables for the qualified targets; `run`, `test`, and `eval` stay on
the interpreter. A tool reports what it supports at every stage.

Excluded:

- store stage, status, and publish workflows (`project sync` only acquires
  dependencies);
- a persistent REPL and live redefinition;
- arbitrary patch languages and unrestricted source evaluation;
- renames that keep a nominal identity automatically;
- a second checker or index in an adapter;
- making grants from tool permissions;
- choosing a backend for `run` or `test`;
- unversioned machine output and implicit shell or foreign-function
  execution;
- any claim that passing samples prove a guard for all inputs.
