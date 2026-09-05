# Milestone 1 implementation map

Status: process guidance; the specifications remain authoritative.

## Existing code to understand before changing it

Paths below are relative to the repository root.

| Area | Entry points | Preserve |
| --- | --- | --- |
| Source and spans | `crates/vibra-diagnostics/src/{span,line_index,diagnostic,registry}.rs` | Half-open UTF-8 byte spans; one-based Unicode-scalar display columns; closed fixed-level registry |
| Reader | `crates/vibra-syntax/src/reader.rs` | `lex_bytes`, explicit `parse_source`/`parse_data`, `Document`, `CstNode`, token/trivia bytes, error markers |
| Formatter | `crates/vibra-fmt/src/lib.rs` | `format_document`; recovered documents unchanged; iterative layout; independent opening/closing delimiter decisions |
| Corpus | `crates/vibra-conformance/src/{manifest,corpus,profile,runner,reader}.rs` | Safe case-relative input loading, real `ReaderV1Handler`, failed/unavailable distinction |
| Machine data | `crates/vibra-schema/src/diagnostic.rs`, `schemas/v1/` beneath that crate | Exact atom codes; schema version; producer/consumer validation |
| Existing regressions | `crates/*/tests/`, `conformance/cases/` | Independent host and language suites; exact diagnostic order and formatter bytes |

Read the code, not just this table. New modules suggested by the guides do not
yet exist. Keep syntax dependent only on diagnostics. Formatter consumes syntax;
schemas adapt internal facts to wire data. A new schema-to-syntax dependency
needs a deliberate architecture-table update, never a reverse syntax-to-schema
dependency. `crates/vibra-conformance/tests/architecture_boundary.rs` enforces
the allowed edges.

## Shared implementation strategy

Keep one source-owning lossless CST. Add literal classification and contextual
AST views over its nodes; avoid a second parser that rescans strings with its
own spans. Preserve raw spellings even when a leaf also has a decoded value.
Represent invalid or missing children explicitly. A parent with an ambiguous
child cannot masquerade as a fully valid AST node.

Use small cursor helpers for consuming a required form, checking a list head,
collecting a flat pair/triple sequence, and reporting a missing operand.
Each helper returns either validated structure or an explicit error result.
Guard deep inputs across parsing, traversal, formatting, debug output, and drop;
the existing iterative reader must not gain an unbounded recursive second pass.

Derive formatter and query facts from the same grammar structure. Neither
surface should maintain a separate list of accepted declaration attributes.
Keep grammar categories distinct: module, declaration body, type, expression,
pattern, parameters, and data. The same list head can have different meanings
in those contexts.

## Contract decisions to close before dependent implementation

These are observed gaps or delivery dependencies, not permission to choose a
language policy in Rust. Make a specification change with positive/negative
examples, registry/schema updates, and affected roadmap guidance first.

| Decision | Owner | Required resolution |
| --- | --- | --- |
| Malformed strings, invalid names, wrong arities, duplicate attributes, invalid data shapes | Steps 5–9, before each affected slice | Audit the closed diagnostic table: several rejection rules have no dedicated code or precise span policy. Specify the mapping; do not reuse unmatched-delimiter for unrelated errors. |
| Generic VIBON canonical ordering | Step 7 | Define ordering across permitted map keys, duplicate-key treatment, and record ordering when no typed schema supplies field order. Do not silently use Rust hash iteration, lexicographic source text, or source collection semantics. |
| Signature-dependent operand normalization | Step 9 | M1 has no resolver, but its gate includes labelled/variadic normalization. Define the formatter input contract for supplied binding facts and how the reader profile exercises it, or revise the normative delivery allocation explicitly. Never infer a signature from a callee's spelling. |
| Structural metadata wire representation and boundary behavior | Step 10 | Align the initial schema with the tooling contract, including EOF, trivia, invalid offsets, and exact/recovered/unavailable facts. Do not publish guessed future semantic fields. |

Unblocked work can proceed while a separate contract is unresolved. Record the
blocked checklist rows explicitly; do not mark their containing step complete.
The entire exit gate remains pending until every dependency is closed.

## Scope boundaries

M1 validates written grammar and provides structural facts. It does not resolve
imports, infer numeric types, range-check source numerics, check exhaustiveness,
unify union members, execute data, or implement a CLI/MCP server. A lexically
valid `-1u8` is not a reader error. A parsed application is not proof that its
callee is applicable. Syntax acceptance is never advertised as full-v1 validity.

Do not weaken old corpus inputs when adding stricter grammar. If an old fixture
was intentionally only a reader fragment, preserve a focused host regression
at that layer and make its corpus wrapper valid for the new module grammar.
Explain each migration and retain the original behavior assertion.
