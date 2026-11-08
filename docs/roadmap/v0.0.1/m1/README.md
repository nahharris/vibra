# Milestone 1 — reader, formatter, and conformance spine

Outcome: an incomplete `.vib` file can be parsed, diagnosed, formatted, and
queried structurally.

Deliverables:

- new Rust workspace and CI without archived dependencies;
- UTF-8 lexer, lossless recovery CST, spans, native top-form AST nodes, and
  nested interface-implementation nodes;
- explicit `.vib` source and `.vibon` data document modes over the shared
  lexer, with no content sniffing or extension fallback;
- `void`, EDN-style Unicode character literals, decimal numeric literals, and
  exact `i8`-through-`i64`, `u8`-through-`u64`, `f32`, and `f64` suffixes;
- complete qualified-kebab symbol grammar, derived labels and atoms, and
  discard semantics;
- general lists with arbitrary head forms, flat pattern/type parameters,
  flat record/enum pairs, flat union member lists, flat match arms, and flat
  declaration syntax;
- the `as` reserved form in expression and pattern head position, recognized
  before the general application and constructor-pattern productions;
- canonical, idempotent formatter;
- named atom diagnostic data model, built-in level registry, and initial
  CLI/MCP JSON Schemas;
- spec-rule-addressed conformance runner; and
- structural source-position query metadata for grammar categories and allowed
  labels.

Exit gate:

- reader positive/negative/recovery corpus passes;
- every syntax example is classified and exercised;
- formatter round-trip and idempotence properties pass, including tolerant
  labelled/variadic normalization;
- Unicode byte and display spans pass; and
- a configured fuzz campaign finds no panic or non-idempotent accepted input.
