# Milestone 1 step plan

Status: exit gate evidenced; integration PR #283 ready for review
Milestone: [Milestone 1 — reader, formatter, and conformance spine](../v1.md)
Execution model: [`../execution.md`](../execution.md)
Integration branch: `m1`

Milestone 1 makes an incomplete `.vib` file parseable, diagnosable,
formattable, and structurally queryable. This document records how that work
is cut into steps and what has landed.

## Start here

The implementation baseline used for Step 11 was refreshed on 2026-09-05 at
`origin/m1` commit `9d3759f9da98c7349b7b861cf1c92a90afb9709f` (merge of PR
#293). Steps 1–10 were present there; Step 11 is the executable exit-gate
audit on top of that head. Re-fetch and verify the current integration head in
future sessions. The Step 11 evidence and final PR CI are recorded below; the
integration branch remains separate from `main` until PR #283 is reviewed.

Read [the implementation guides](implementation.md) and
[validation and evidence requirements](validation.md) before choosing work.
The guides distinguish grammar validation from resolution/type checking and
identify contracts that need specification work before dependent code.

## Fixed design decisions

These constrain every later step in the milestone. Each was taken
deliberately; a later session that wants to change one changes this document
and says why.

### D1 — One crate per architecture node

The workspace creates only the nodes Milestone 1 needs:

```text
vibra-diagnostics   spans, line index, code and level registry, diagnostic model
vibra-syntax        lexer, lossless recovery CST, document modes, AST
vibra-fmt           canonical formatter
vibra-schema        versioned CLI and MCP JSON contracts
vibra-conformance   corpus runner and workspace-level invariants
```

Later milestones add `vibra-resolve`, `vibra-types`, `vibra-ir`,
`vibra-interp`, `vibra-workspace`, `vibra-cli`, and `vibra-mcp`.

The reason is enforcement, not tidiness. The roadmap's architecture boundary
forbids dependency arrows from language semantics into CLI, MCP, filesystem
UI, or a backend. Cargo enforces that mechanically once the nodes are separate
crates, and a convention inside one crate does not.

### D2 — The lossless CST is hand-rolled

Vibra's surface is uniform: trivia, atoms, lists, and an error node. A
purpose-built tree is small, adds no dependency to the most foundational
crate, and lets trivia attachment and span rules be built to the
specification rather than around a general-purpose API. `rowan` and `cstree`
were considered and rejected as more machinery than an S-expression reader
needs.

### D3 — Conformance cases are directories with a neutral manifest

Each case is a directory containing its inputs, a `case.toml` manifest
carrying the rule ID and expected diagnostics with explicit spans, and any
expected-output snapshots. The manifest format is deliberately not VIBON: the
corpus is the oracle for the reader and the VIBON decoder, so a defect in
either must not be able to corrupt the expectations that would catch it.

### D4 — Milestone 1 ships no user-facing `vibra` binary

The roadmap places `vibra project init`, `fmt`, `check`, `test`, and `run` in
Milestone 2. Milestone 1 delivers library crates, the published JSON schemas,
and an internal conformance runner. Nothing is advertised on a command surface
that Milestone 2 has not built, as roadmap rule 3 requires.

### D5 — Published JSON schemas are identified by URN, provisionally

Schemas are identified as `urn:vibra:schema:v1:<name>`. Every `$ref` is
internal, so validating a document needs no resolution and no identifier has
to be fetchable.

This is deliberately provisional. The conventional choice is an HTTPS
identifier under a domain the project controls, and the intended home is
`vibra.harrisonn.dev`. Until that subdomain exists, an HTTPS identifier would
name a location nothing serves, so the URN states only what is true today.

Switching to HTTPS later is a contract break, not a refactor. It must happen
in one change across every published schema, and it must happen before v1 is
released, while the only consumers are inside this repository. A session that
finds the subdomain live should make that change rather than adding a schema
under the old scheme.

The `v1` in an identifier is the **schema's** major version, not the
language's. The charter versions machine schemas independently of the source
language after 1.0, so a schema `v2` can exist under language v1 and the two
must not be assumed to move together.

### D6 — The dependency direction is a test, not a convention

`vibra-conformance` carries a host-language test that reads every workspace
manifest and fails when a crate depends on something the architecture boundary
does not permit. `vibra-conformance` is the crate that legitimately depends on
everything, so workspace-wide structural invariants live there.

### D7 — The Step 3 manifest is neutral TOML with explicit snapshots

Cases live below `conformance/cases/` in directories named by their stable
case IDs. Each directory contains `case.toml`, any declared source/project/data
inputs, and optional expected-output snapshots. The manifest records `id`, a
normative `rule`, and one of the closed profiles from the diagnostics chapter.
Inputs and snapshots are case-relative paths; the loader rejects traversal,
absolute paths, missing files, symlink escapes, and directory/manifest ID
mismatches.

`[expect]` records acceptance, ordered diagnostics, and optional formatting,
resolved-identity, type, effect, interpreter, Wasm, and artifact observations.
Expected diagnostics use the closed registry's atom code and fixed level plus
an explicit half-open byte span. This keeps the corpus oracle independent of
both the VIBON decoder and future execution backends.

The internal runner dispatches cases to the closest registered capable profile.
An absent capable handler is an `unavailable` result, not a silently skipped
case. Later backend milestones register handlers through the public
`ProfileHandler` interface; this step adds no language behavior.

### D8 — Reader conformance runs through a dedicated internal entrypoint

Step 4 registers the real syntax/formatter handler for `reader-v1` and exposes
it through the `vibra-conformance` crate's internal runner binary. CI invokes
that binary as a separate conformance job, so checked-in cases are executed
independently of the Rust host-language test suite. The binary is an internal
CI adapter and does not add a user-facing `vibra` command; any failed or
unavailable case returns a nonzero exit status. The handler maps the manifest's
`source`, `project`, and `data` roles to their respective loaders, and the
entrypoint rejects an empty corpus or one with no `reader-v1` cases. Host
language tests use synthetic temporary corpora, so the checked-in cases are
loaded only by this entrypoint and its CI job.

Step 4 regression coverage MUST include mixed lexer/parser diagnostic ordering
and byte-preserving formatting of opaque quoted leaves with CR/CRLF interiors
in both document modes. Literal validation and canonical escaping remain in
step 5; canonical VIBON values remain in step 7. These cases contribute to the
reader and formatter exit gates, whose complete evidence remains in step 11.

### D9 — Terminated malformed strings have a distinct reader diagnostic

Step 5 uses `@syntax.invalid-string-literal` for an invalid escape or Unicode
scalar in a terminated quoted leaf, reporting the complete leaf span. An
unterminated quoted leaf, including one ending in an unfinished escape, keeps
the existing `@syntax.unmatched-delimiter` recovery diagnostic only. This
distinction prevents a recovery marker from being mistaken for a second
lexical failure and preserves the Step 4 byte-preserving contract.

### D10 — Malformed name candidates have a lexical diagnostic

Step 6 validates nonliteral leaves with one shared symbol/label/atom/discard
grammar. A malformed candidate emits `@syntax.invalid-name` over its complete
token span; literal-family diagnostics take precedence. The validator does not
assign contextual roles or resolve names, and valid names retain their exact
source spelling.

### D11 — Generic VIBON ordering is deterministic and schema-independent

Generic records retain source field order because no schema supplies a field
order. Generic maps sort keys by the canonical encoded value produced by the
data formatter, using the complete canonical bytes as the tie-breaker. A
duplicate record label emits `@data.duplicate-field`; a duplicate map key emits
`@data.duplicate-key`. Typed adapters may supply explicit record order and atom
roles without resolving references.

### D12 — Declaration and expression structure is an internal contextual view

Steps 8–9 add an owned `SourceAst` view over the lossless source CST. It is
constructed only for a source root containing a recognized native declaration
head; arbitrary reader fragments remain accepted by the syntax-only reader.
Declaration, type, expression, pattern, and application-shape errors are
appended to document diagnostics for those recognized roots. Written
applications retain raw spans and accept optional authoritative binding facts;
no semantic resolution or public JSON AST schema is added.

### D13 — Structural queries own the source facts and schemas adapt them

Step 10 exposes `query_position` from `vibra-syntax` over the existing
lossless CST. The result owns the selected span, CST kind, grammar category,
fact status, and ordered permitted forms/labels; it never resolves an atom or
reparses source text. `vibra-schema` adds the deliberate one-way dependency on
`vibra-syntax` and publishes `source-position-query` with explicit
exact/recovered/unavailable status and null-versus-empty continuation facts.
The internal reader-v1 corpus observes these results through dedicated query
snapshots. No CLI or MCP surface is added.

### D14 — Exit evidence is executable and bounded

Step 11 keeps the specification-example inventory in
`syntax-examples.tsv`, with a host test that checks stable fence and inline
digests, rejects stale rows, and losslessly exercises source/data fragments.
The fuzz campaign is an in-tree deterministic harness configured by
`fuzz/m1.toml`; CI runs a short smoke profile while the gate records the
separate six-target campaign. Harness budgets and generated-input limits are
test limits only and do not change the language contract.

## Steps

Steps 1, 3, and 11 carry no language behavior and are exempt from the
vertical-slice rule under `execution.md`; step 1 and step 3 are infrastructure
steps and step 11 is an evidence step. Every other step widens the accepted
language and carries its tree nodes, formatter rules, diagnostics, schemas,
and conformance cases in the same change.

| # | Step | Kind | Status |
| --- | --- | --- | --- |
| 1 | Workspace, pinned toolchain, CI, and crate skeleton | infrastructure | landed |
| 2 | Spans, line index, diagnostic model, closed code and level registry, and their JSON contract | vertical | landed |
| 3 | Conformance corpus layout, manifest decoding, profile dispatch, and runner | infrastructure | landed |
| 4 | Reader spine: minimal lexer, lossless recovery CST, document-mode selection, minimal formatter | vertical | landed (PR #286, `50aa40a`) |
| 5 | Literal surface: EDN characters, numeric suffixes, floats, `void`, booleans, string escapes | vertical | landed (PR #288, `af73e22`) |
| 6 | Name surface: qualified kebab symbols, labels, atom names, discards | vertical | landed (PR #289, `80a13ca`) |
| 7 | VIBON document grammar, decoder, and canonical VIBON formatting | vertical | landed (PR #290, `aec8a3d`) |
| 8 | Declaration AST: native top-forms, nested methods, nested `impl`, attributes, flat parameters | vertical | landed in PR #291 (merge `3057737d8c595409fe977a3eafb37436e4c7dfd7`) |
| 9 | Expression and pattern AST: general application, `as` in both head positions, control forms, retired-form rejection | vertical | landed in PR #292 (merge `b7f94f85f74de20f4a4a48aeb30a54b823bc72b1`) |
| 10 | Structural source-position query metadata | vertical | landed in PR #293 (merge `9d3759f9da98c7349b7b861cf1c92a90afb9709f`) |
| 11 | Fuzz campaign, specification-example classification, and exit-gate evidence | evidence | landed in PR #294; CI run [34001568235](https://github.com/nahharris/vibra/actions/runs/34001568235); merge verification recorded in #283 |

Detailed guides: [5–6: literals and names](05-06-leaves.md),
[7: VIBON](07-vibon.md), [8–9: contextual AST](08-09-ast.md),
[10: structural queries](10-queries.md), and
[11: exit-gate audit](11-evidence.md).

Steps 5 and 6 can be developed as separate complete slices using their shared
guide. Steps 8 and 9 share grammar infrastructure, but remain separate steps:
Step 8 exposes declaration structure with lossless body/pattern references;
Step 9 validates and exposes their expression and pattern interiors. Neither
step claims resolved or typed facts.

## Deliverable coverage

Every Milestone 1 deliverable maps to at least one step. A deliverable spread
across steps is complete only when its last step lands.

| Roadmap deliverable | Steps |
| --- | --- |
| New Rust workspace and CI without archived dependencies | 1 |
| UTF-8 lexer, lossless recovery CST, spans, native top-form AST nodes, nested interface-implementation nodes | 2, 4, 5, 6, 8 |
| Explicit `.vib` and `.vibon` document modes over the shared lexer | 4, 7 |
| `void`, EDN character literals, decimal numerics, exact numeric suffixes | 5 |
| Qualified-kebab symbol grammar, derived labels and atoms, discard semantics | 6 |
| General lists with arbitrary heads and every flat list form | 8, 9 |
| The `as` reserved form in expression and pattern head position | 9 |
| Canonical, idempotent formatter | 4, 5, 6, 7, 8, 9 |
| Named atom diagnostic data model, level registry, initial JSON schemas | 2 |
| Spec-rule-addressed conformance runner | 3 |
| Structural source-position query metadata | 10 |
| Executable Milestone 1 exit-gate evidence | 11 |

## Exit-gate coverage

| Exit-gate clause | Steps | Evidence |
| --- | --- | --- |
| Reader positive/negative/recovery corpus passes | 4–9, verified in 11 | 73 passed, 0 failed, 0 unavailable locally and in CI run [34001568235](https://github.com/nahharris/vibra/actions/runs/34001568235) |
| Every syntax example is classified and exercised | 11 | `syntax-examples.tsv`; inventory host test passes for 42 fences and 1,408 inline spans locally and in CI run [34001568235](https://github.com/nahharris/vibra/actions/runs/34001568235) |
| Formatter round-trip and idempotence, including tolerant labelled/variadic normalization | 4–9, verified in 11 | existing formatter suite plus six-target roundtrip property; CI run [34001568235](https://github.com/nahharris/vibra/actions/runs/34001568235) is green |
| Unicode byte and display spans pass | 2, verified in 11 | `LineIndex` derives one-based scalar columns; covered for astral scalars, combining marks, interior offsets, and CRLF, plus a property over a multiline Unicode document. Full verification in step 11. |
| Fuzz campaign finds no panic or non-idempotent accepted input | 11 | `fuzz/m1.toml`: 6 targets × 128 iterations = 768 passed after the deep-data repair; CI smoke is green in run [34001568235](https://github.com/nahharris/vibra/actions/runs/34001568235) |
