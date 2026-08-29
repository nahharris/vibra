# Milestone 1 step plan

Status: active
Milestone: [Milestone 1 — reader, formatter, and conformance spine](../v1.md)
Execution model: [`../execution.md`](../execution.md)
Integration branch: `m1`

Milestone 1 makes an incomplete `.vib` file parseable, diagnosable,
formattable, and structurally queryable. This document records how that work
is cut into steps and what has landed.

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

### D5 — The dependency direction is a test, not a convention

`vibra-conformance` carries a host-language test that reads every workspace
manifest and fails when a crate depends on something the architecture boundary
does not permit. `vibra-conformance` is the crate that legitimately depends on
everything, so workspace-wide structural invariants live there.

## Steps

Steps 1, 3, and 11 carry no language behavior and are exempt from the
vertical-slice rule under `execution.md`; step 1 and step 3 are infrastructure
steps and step 11 is an evidence step. Every other step widens the accepted
language and carries its tree nodes, formatter rules, diagnostics, schemas,
and conformance cases in the same change.

| # | Step | Kind | Status |
| --- | --- | --- | --- |
| 1 | Workspace, pinned toolchain, CI, and crate skeleton | infrastructure | landed |
| 2 | Spans, line index, diagnostic model, closed code and level registry, and their JSON contract | vertical | not started |
| 3 | Conformance corpus layout, manifest decoding, profile dispatch, and runner | infrastructure | not started |
| 4 | Reader spine: minimal lexer, lossless recovery CST, document-mode selection, minimal formatter | vertical | not started |
| 5 | Literal surface: EDN characters, numeric suffixes, floats, `void`, booleans, string escapes | vertical | not started |
| 6 | Name surface: qualified kebab symbols, labels, atom names, discards | vertical | not started |
| 7 | VIBON document grammar, decoder, and canonical VIBON formatting | vertical | not started |
| 8 | Declaration AST: native top-forms, nested methods, nested `impl`, attributes, flat parameters | vertical | not started |
| 9 | Expression and pattern AST: general application, `as` in both head positions, control forms, retired-form rejection | vertical | not started |
| 10 | Structural source-position query metadata | vertical | not started |
| 11 | Fuzz campaign, specification-example classification, and exit-gate evidence | evidence | not started |

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

## Exit-gate coverage

| Exit-gate clause | Steps | Evidence |
| --- | --- | --- |
| Reader positive/negative/recovery corpus passes | 4–9, verified in 11 | pending |
| Every syntax example is classified and exercised | 11 | pending |
| Formatter round-trip and idempotence, including tolerant labelled/variadic normalization | 4–9, verified in 11 | pending |
| Unicode byte and display spans pass | 2, verified in 11 | pending |
| Fuzz campaign finds no panic or non-idempotent accepted input | 11 | pending |
