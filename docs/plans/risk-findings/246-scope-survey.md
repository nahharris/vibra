## Pre-implementation findings — shadowing as a hard error

Survey of the compiler's scope infrastructure and of every binding site in the
`.vib` corpus (`stdlib/src/*.vib`, `examples/*.vib`, `tests/*.vib` — 57 files,
**475 binder sites**), to size the migration before making shadowing an error.

**Headline: the corpus cost is one line. The risk is entirely in the
implementation shape, not the migration.**

---

### 1. Is there a lexical scope stack to reuse?

Yes — a clone-based lexical environment already exists on both lowering paths.
There is no named `ScopeStack` type; the discipline is "clone the parent
environment when entering a nested block". That is a real scope stack and it is
sufficient.

**`src/lower.rs` (live path)** — `locals: &mut HashMap<String, TypeRef>`, cloned
at every nested block:

| Construct | Clone site |
|---|---|
| `if` branches | `src/lower.rs:5542` (`let baseline = locals.clone();`) |
| `while` body | `src/lower.rs:5608` |
| `for` body | `src/lower.rs:5678` |
| `match` arm | `src/lower.rs:8852` (`let mut scoped_locals = locals.clone();`) |
| `spawn` task body | `src/lower.rs:5749`, `src/lower.rs:5824` |

Binders are installed with a plain `locals.insert(...)`, which **silently
overwrites**:

- `let` (call RHS) — `src/lower.rs:5882`
- `let` (expr RHS) — `src/lower.rs:5907`
- `for` binder — `src/lower.rs:5680`
- `(bind name)` in a pattern — `src/lower.rs:8616`

The only shadow rejections on this path are for affine task handles:
`src/lower.rs:5724` and `src/lower.rs:5792` (`E-TASK-003`).

**`src/typed_body.rs` (typed subset)** — same clone discipline
(`src/typed_body.rs:710`, `:722`, `:745`, `:765`) **and it already rejects
shadowing**:

- `src/typed_body.rs:612-614` — ``typed local `{var}` is already bound in `{context}` ``
- `src/typed_body.rs:766-769` — ``typed for binding `{var}` shadows an existing local``
- `src/typed_body.rs:842-846`, `:886-888` — `E-TASK-003` handle/join shadowing

So the rule is already written once, on the staged path. `match` is not covered
there (`src/typed_body.rs:833-835`: `typed match remains staged`), which is
exactly where the corpus's only shadow lives.

**`src/lsp.rs` — not reusable.** It has no lexical scope model at all; it works
off a flat `SemanticFact` symbol table (`src/lsp.rs:156-175`, `:392-404`,
`:514-536`).

> **Recommendation:** implement the check at the four `locals.insert` sites in
> `src/lower.rs` listed above, guarding on `!locals.contains_key(name) || name == "_"`.
> Reuse the existing clone-based env. No new infrastructure is required.

#### ⚠️ There is already a flat-seen-set collector in the tree — do not generalize it

`collect_binding_names` / `collect_binding_names_expr`
(`src/surface_adapter.rs:1958-2009`) walks a whole function body and pushes
**every** binder name into a flat `Vec<String>`, with no scope structure. It
feeds `E-ADAPT-048` (`src/surface_adapter.rs:1240-1251`), which rejects a local
that reuses a *parameter* name.

That is correct **only** because the parameter scope encloses the entire body,
so flatness is harmless there. Reusing this helper for general shadowing would
reject every sibling-scope reuse in section 3. It is the exact anti-pattern this
survey was asked to guard against, and it is already sitting in the codebase
looking reusable.

---

### 2. Actual shadowing in the corpus: **1 site**

| File:line | Form | Shadows |
|---|---|---|
| `tests/lang-values.vib:43` | `(let outer false)` inside a `match` arm | `(let outer true)` at `tests/lang-values.vib:41` |

That is the complete list. `stdlib/src/*.vib` (22 files): **0**.
`examples/*.vib` (3 files): **0**. `tests/*.vib` (32 files): **1**.

**The one site is a test that exists to assert shadowing works.**

```vibra
(test.scenario
  "match-arm-binding-does-not-leak"
  (test.case
    "match-arm-binding-does-not-leak"
    (let outer true)
    (let subject 1)
    (match subject 1 (do (let outer false)) _ (do))
    (test.assert outer)
    profile: @core
  )
)
```
— `tests/lang-values.vib:38-46`

Confirmed live today:

- `vibra test --filter match-arm-binding-does-not-leak` → `"status": "passed"`
- `vibra lint tests/lang-values.vib` → `0 errors, 0 warnings`

So shadowing is currently accepted **with no diagnostic at all**, and this test
pins that behaviour deliberately.

> **Migration cost: one test.** But it is a semantic decision, not a mechanical
> edit. The test's whole point is "an inner binding does not leak to the outer
> scope". Under the new rule that scenario becomes unexpressible. Either delete
> the test, or rewrite it to assert the *rejection* (e.g. rename the inner
> binding and add an `expect-error:` case for the shadowing form). Please decide
> which in the issue before implementing, so the intent isn't quietly lost.

---

### 3. Sibling-scope reuse — MUST stay legal: **16 pairs, 16 sites, 3 files**

Same name bound in two scopes where neither encloses the other. All are legal
today and all lint clean (`vibra lint stdlib/src/fs.vib stdlib/src/process.vib
tests/lang-iteration.vib` → 0 diagnostics).

| File | Name | Binding lines | Pairs |
|---|---|---|---|
| `stdlib/src/fs.vib` | `failure` (`bind`) | 191/195, 246/250, 270/274, 315/320/327/338 | 9 |
| `stdlib/src/process.vib` | `message` (`bind`) | 71/73/87/100 | 6 |
| `tests/lang-iteration.vib` | `number` (`for`) | 29/30 | 1 |
| **Total** | | | **16** |

Three concrete acceptance-gate cases:

1. **`stdlib/src/fs.vib:191` & `stdlib/src/fs.vib:195`** — two sibling `match`
   arms each binding `(bind failure)`. Scopes are created by the per-arm
   `locals.clone()` at `src/lower.rs:8852`.
2. **`stdlib/src/process.vib:71`, `:73`, `:87`, `:100`** — four sibling `match`
   arms in the same function, all binding `message`. The widest fan-out in the
   corpus; the best single regression test.
3. **`tests/lang-iteration.vib:29` & `tests/lang-iteration.vib:30`** — two
   *sequential* `for` loops in one `test.case`, both binding `number`:
   ```vibra
   (for number (range 3 -1 -1) (set count (add count 1)))
   (for number (range 0 0 1) (test.fail "empty range traversed"))
   ```
   Distinct from the `match` cases: these are consecutive statement-level
   scopes, not alternative branches, so they exercise a different code path
   (`src/lower.rs:5678`).

> **A flat seen-set implementation would reject all 16 and break two stdlib
> modules.** Land these as tests *before* the check. Note that
> `tests/lang-iteration.vib` already covers case 3 as a passing test, so a
> flat-set regression would be caught by `vibra test` — but `fs.vib` and
> `process.vib` would fail at *compile* time, which is a louder and more
> confusing failure. Add explicit sibling-scope cases so the intent is legible.

---

### 4. Macro-generated binders: **no textual collision risk**

Binders introduced by a macro template are **renamed**, not resolved by text.

`src/ast/typed_macro_expand.rs:1655` (in `hygienize_expr`, for `ExprKind::Let`)
and `src/ast/typed_macro_expand.rs:2034` (in `rename_binding`, for `for`/pattern
binders) both do:

```rust
let replacement = format!("{original}--macro-{hygiene_id}");
```

`hygiene_id` is bumped once per expansion (`src/ast/typed_macro_expand.rs:1123`).
Renaming is gated on the `generated` flag — true only when the node's `Origin`
resolves to a span inside the macro's own definition
(`src/ast/typed_macro_expand.rs:1622-1625`), so call-site-supplied fragments keep
their original names and macro-template binders cannot collide with them.

Two further reasons this is a non-risk today:

- `src/ast/typed_macro_expand.rs:197` rejects `@definition-syntax` and
  `@module-syntax` macros outright.
- **The corpus contains zero macros** — `grep -rn "(macro" --include=*.vib .`
  returns 0 matches repo-wide. (See the issue #250 survey.)

> **Recommendation:** the shadowing check should run on the **post-expansion**
> AST and compare `Name.value` (identity after hygiene), never raw source text.
> `src/typed_body.rs` already threads `node_origins`
> (`src/typed_body.rs:46`, `:1580`) so a diagnostic can point at the right
> post-expansion statement — there is an existing test for exactly this,
> `for_shadowing_selects_binding_statement_after_macro_source_validation`
> (`src/typed_body.rs:3979`). Follow that pattern.

---

### Summary

| Question | Answer |
|---|---|
| Scope stack exists? | **Yes** — clone-based env in `lower.rs` + `typed_body.rs`; reuse it |
| Must one be built? | **No** |
| Shadowing sites to migrate | **1** (`tests/lang-values.vib:43`) |
| Sibling-scope sites that must stay legal | **16** across 3 files (2 are stdlib) |
| Macro binder collision risk | **None** — hygienic renaming; 0 macros in corpus |

The migration is trivial. The one thing that can go wrong is implementing the
check with a flat seen-set — the codebase already contains such a collector
(`src/surface_adapter.rs:1958`) that looks tempting to reuse. Gate the PR on the
three sibling-scope cases in section 3.

<sub>Corpus figures from a purpose-built S-expression scope analyser
(`let` / `let-as` / `for` / `bind` / parameters; `_` exempt; scopes per `defn`,
`test.case`, `if` branch, `while` body, `for` body, `match` arm). Note
`let-as` has **zero** occurrences in the corpus — it appears only in docs and
in Rust unit tests (`src/typed_body.rs:3436`, `:3853`).</sub>
