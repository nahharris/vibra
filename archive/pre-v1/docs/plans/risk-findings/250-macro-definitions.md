## Pre-implementation findings — macro-generated definitions and `vibra index`

The plan assumes `vibra index` must cope with definitions produced by macros,
which have no honest source text to emit.

**Headline: this is hypothetical, not a real problem today. Zero macros exist in
the corpus, and the expander explicitly refuses to expand definition- or
module-level macros at all. The plan can drop the macro-provenance machinery and
replace it with a few-line guard.**

---

### 1. Does the corpus contain macros that generate top-level definitions?

**No. The corpus contains no macros at all.**

```
$ grep -rn "(macro" --include=*.vib .
0
```

Zero matches repo-wide — not in `stdlib/src/*.vib` (22 files), not in
`examples/*.vib` (3), not in `tests/*.vib` (32), not in any other `.vib` file in
the tree. Every `(macro …)` string in the repository lives inside a Rust unit
test literal, e.g. `src/ast/surface.rs:2734`, `src/ast/surface.rs:2847`,
`src/ast/typed_macro_expand.rs:2763-2765`.

**Syntax categories the language defines** (`src/ast/surface.rs:1143-1154`):

| Category | Generates definitions? |
|---|---|
| `@expr-syntax` | no |
| `@type-syntax` | no |
| `@pattern-syntax` | no |
| `@definition-syntax` | **yes** |
| `@module-syntax` | **yes** |

The two that matter are **rejected by the only expander in the pipeline**:

> `src/ast/typed_macro_expand.rs:197`
> ```
> "@definition-syntax and @module-syntax macros are not supported by the typed expander"
> ```

So macro-generated top-level definitions are not merely absent from the corpus —
they are **unimplemented**. There is currently no way to write one that compiles.

---

### 2. Is there an origin-tracking mechanism, and is it queryable?

Yes, origin tracking exists and is queryable — but **not** as the `OriginId` /
arena the plan (and the decision doc) describe.

**Implementation** — `src/ast/surface.rs:55-74`:

```rust
pub enum Origin {
    Source(Span),
    DocumentSource { document: DocumentId, ast_id: AstId, span: Span },
    Expansion { call_site: Span, definition: Span, parent: Arc<Origin> },
    DocumentExpansion { ast_id: AstId, call_site: SourceLocation,
                        definition: SourceLocation, parent: Arc<Origin> },
}
```

It is an **inline `Arc`-linked chain stored on every AST node**, not an arena of
interned ids. Accessors at `src/ast/surface.rs:76-102`:

- `primary_span() -> Span`
- `document_id() -> Option<DocumentId>`
- `ast_id() -> Option<AstId>`

**It is populated on expansion.** Expanded nodes are stamped by
`annotate_generated_expr` / `annotate_generated_type` /
`annotate_generated_pattern` (`src/ast/typed_macro_expand.rs:2331`, called from
`:402`, `:720`, `:849`), each of which rewrites the node's `Origin` into the
`Expansion` / `DocumentExpansion` variant carrying both the call site and the
macro definition site.

**It is already consumed for diagnostics.** `src/typed_body.rs:46` keeps
`node_origins: HashMap<String, Vec<Origin>>` keyed by function, populated at
`src/typed_body.rs:1580` and walked by an `OriginCursor` so errors point at the
correct post-expansion statement. There is a dedicated regression test:
`for_shadowing_selects_binding_statement_after_macro_source_validation`
(`src/typed_body.rs:3979`).

#### ⚠️ Documented design and implementation have diverged

`docs/decisions/s-expression-language.md:590-603` specifies an arena-style model:

```text
Expansion {
  macro-symbol,
  call-site: OriginId,
  definition-site: SourceSpan,
  template-site: SourceSpan,
  parent: OriginId
}
```

The code has **no `OriginId` type and no arena** —
`grep -rn "OriginId\|OriginArena\|origin_arena" src/` returns nothing. It uses
`Arc<Origin>` parent chains and carries no `macro-symbol` or `template-site`.
Any plan written against the doc's field list will not compile. Either the doc
should be corrected or the divergence noted before `vibra index` builds on it.

---

### 3. Verdict: hypothetical — simplify the plan

Macro-generated definitions cannot occur today:

1. No `.vib` file in the repository defines a macro (0 occurrences).
2. `@definition-syntax` and `@module-syntax` expansion is explicitly unsupported
   (`src/ast/typed_macro_expand.rs:197`).
3. Consequently every definition `vibra index` will encounter has honest,
   directly-attributable source text.

**Recommendation:** drop the macro-provenance subsystem from the `vibra index`
plan. Replace it with a cheap, honest guard that costs a few lines and cannot
silently produce wrong records later:

- When emitting a definition record, inspect its `Origin`.
- For `Origin::Source` / `Origin::DocumentSource` (the only cases that occur
  today), emit the source text as normal.
- For `Origin::Expansion` / `Origin::DocumentExpansion`, set `generated: true`
  and emit the **macro definition span** rather than fabricating text.

That branch is dead code today but is the correct behaviour the day
definition-syntax macros land, and it fails visibly rather than emitting a
plausible-looking lie. It reuses an existing, already-queryable field — it is
not a subsystem.

**Suggested follow-ups (separate from #250):**

- Reconcile `docs/decisions/s-expression-language.md:590-603` with the actual
  `Origin` enum, or mark that section as a forward-looking design.
- If definition-syntax macros are ever implemented, revisit this survey — the
  `generated: true` guard is the hook that will need real content.

<sub>Method: `grep -rn "(macro" --include=*.vib .` over the whole repository;
syntax-category set read from `src/ast/surface.rs:1143-1154`; expander
restriction from `src/ast/typed_macro_expand.rs:197`; origin model read from
`src/ast/surface.rs:55-102` and `src/ast/typed_macro_expand.rs:2331`.</sub>
