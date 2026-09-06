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
| Malformed strings, invalid names, wrong arities, duplicate attributes, invalid data shapes | Steps 5–9, before each affected slice | Steps 5–6 close terminated malformed strings with `@syntax.invalid-string-literal` and malformed nonliteral leaves with `@syntax.invalid-name`, each over the complete token; unterminated leaves remain `@syntax.unmatched-delimiter`. Later slices must close their remaining mappings before implementation. Do not reuse unmatched-delimiter for unrelated errors. |
| Generic VIBON canonical ordering | Step 7 | Generic records retain source field order; generic map keys sort by the complete canonical encoded key bytes; duplicate labels/keys emit `@data.duplicate-field`/`@data.duplicate-key`. Typed adapters may supply explicit field order and atom roles. Do not use Rust hash iteration or source collection order implicitly. |
| Declaration/type contextual structure | Step 8 | The internal source AST is an owned view over the lossless CST for recognized native declaration roots. It preserves raw body/pattern nodes alongside contextual views, validates declaration/type context and fixed arities, maps errors to the closed registry, and adds no public JSON AST schema. Arbitrary nondeclaration reader fragments retain syntax-only acceptance. |
| Expression/pattern contextual structure | Step 9 | Recognized declarations expose written literals, names, applications, control forms, patterns, and flat match arms with half-open source spans. Reserved heads dispatch before generic fallback; retired forms use the closed diagnostic; semantic resolution and type checking remain later work. |
| Signature-dependent operand normalization | Step 9 | `BindingFacts` supplies fixed positional count, declaration-order labels, and an optional array/map variadic tail through `ApplicationBinding`. The formatter reorders only with an exact application span and authoritative facts, emits `@style.argument-order` when written order changes, and returns a binding error for duplicate/unknown/missing/extra operands. Without facts it preserves written order and never infers a signature from a callee spelling. |
| Structural metadata wire representation and boundary behavior | Step 10 | Closed by the `source-position-query` contract: EOF and UTF-8 boundary errors are explicit, recovery/trivia status is distinct, and `permittedForms`/`permittedLabels` use null for unavailable versus an empty array for a known empty set. No future semantic fields are published. |

Step 10 implements that decision with a syntax-owned iterative CST query and a
one-way schema adapter. The adapter publishes the fixed fields and vocabularies
in `crates/vibra-schema/schemas/v1/source-position-query.json`; the conformance
manifest accepts ordered `[[expect.queries]]` snapshots so the real reader-v1
handler checks the producer output independently of host-language tests.

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
