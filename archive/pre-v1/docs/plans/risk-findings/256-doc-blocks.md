## Pre-implementation findings — documentation-block parse test

Surveyed every fenced code block in `docs/decisions/*.md` and ran each
Vibra-tagged block through the **real reader** (`vibra fmt --check`) to see what
the proposed test would actually report.

**Headline: the test is practical, cheap, and immediately finds three genuine
grammar-drift bugs in the decision docs. Recommend building it.**

---

### 1. Block inventory

`docs/decisions/` contains 3 files and **19 fenced blocks**.

| Fence tag | Count |
|---|---|
| ` ```ebnf ` | 7 |
| ` ```vibra ` | 9 |
| ` ```text ` | 2 |
| ` ```lisp ` | 1 |
| **Total** | **19** |

Per file:

| File | Blocks |
|---|---|
| `docs/decisions/s-expression-language.md` | 18 |
| `docs/decisions/effect-system.md` | 1 (` ```lisp `) |
| `docs/decisions/philosophy.md` | 0 |

The 7 `ebnf` blocks are grammar productions and the 2 `text` blocks are
pseudo-struct sketches — neither is Vibra source, and neither should be fed to
the reader. That leaves **10 candidate blocks**: 9 `vibra` + 1 `lisp`.

---

### 2. What the reader actually says today

All 10 candidates were extracted and run through `vibra fmt --check`.
**2 pass, 8 fail.**

| # | Location | Result | Diagnostic |
|---|---|---|---|
| 1 | `s-expression-language.md:95` | ✅ pass | — |
| 2 | `s-expression-language.md:122` | ❌ fragment | `E-SYN-007: `ready` is not a valid top-level form` |
| 3 | `s-expression-language.md:135` | ❌ fragment | `E-SYN-007: `identity` is not a valid top-level form` |
| 4 | `s-expression-language.md:163` | ❌ **stale** | `E-ATOM-003: visibility must be an atom; expected one of `@public`, `@private`` |
| 5 | `s-expression-language.md:231` | ✅ pass | — |
| 6 | `s-expression-language.md:395` | ❌ **stale** | `E-ATOM-003: `tags:` entry must be an atom such as `@name`` |
| 7 | `s-expression-language.md:421` | ❌ wrong grammar | `E-SYN-007: `project` is not a valid top-level form` |
| 8 | `s-expression-language.md:449` | ❌ fragment | `E-SYN-007: `embed` is not a valid top-level form` |
| 9 | `s-expression-language.md:553` | ❌ **stale** | `E-ATOM-003: syntax category must be an atom; expected one of `@expr-syntax`, … ` |
| 10 | `effect-system.md:16` (`lisp`) | ❌ pseudocode | `E-SYN-001: invalid symbol `...`` |

#### Complete modules vs fragments

- **Complete modules (2):** #1 and #5. Both are `def` / `defn` sequences that
  form a valid module and parse cleanly.
- **Fragments (3):** #2, #3, #8. These are deliberately expression-level —
  `(ready)`, `(identity true types: (bool))`, `(embed "assets/message.txt")` —
  illustrating call syntax and the `embed` *expression* (the prose at
  `s-expression-language.md:445` literally says "The `embed` **expression**").
  They can never parse as top-level modules and should not be expected to.
- **Different grammar (1):** #7 is a `project.vib` manifest. The prose at
  `s-expression-language.md:415-420` says it "uses the same lexer and scalar
  rules as every other `.vib` file" but has a required `(project ...)` root —
  i.e. it is a distinct root grammar, not a module.
- **Pseudocode (1):** #10 uses a literal `...` placeholder and the proposed
  `deffect` form; it documents a design, not compilable source.

#### The three genuine drift bugs

These are the reason to build the test. Each block is presented as correct Vibra
and is not.

**#4 — `s-expression-language.md:163`**
```vibra
(defn write-prefix (text str) void (do (io.stdout.print text)) visibility: private)
```
`private` must be the atom `@private`. Confirmed by the grammar
(`src/ast/surface.rs:2847` uses `visibility: @private`).

**#6 — `s-expression-language.md:395`**
```vibra
    tags: (language arithmetic)
    expect-error: (compile E-OP-002 "overflow")
```
Both entries need `@`-atoms. The live corpus writes
`tags: (@language @diagnostics)` and `expect-error: (@compile E-CAST-001 "…")`
— `tests/lang-diagnostics.vib:8-9`.

**#9 — `s-expression-language.md:553`**
```vibra
(macro unless (condition expr-syntax body expr-syntax) expr-syntax …)
```
Syntax categories must be `@expr-syntax`. The compiler has a dedicated test for
this exact mistake: `src/ast/surface.rs:3211` asserts
`module("(macro m (x expr-syntax) @expr-syntax (do x))")` fails with
`E-ATOM-003`. The decision doc documents the pre-`@` spelling.

All three look like fallout from the `@`-atom migration that the docs never
caught up with — precisely the drift class this test targets.

---

### 3. Intentionally invalid blocks: **none**

No block in `docs/decisions/*.md` is presented as a counter-example or
"don't do this". Every Vibra-tagged block is offered as correct usage. There is
no need for a `vibra-bad` / `should-fail` escape hatch in the initial version.

(Blocks #4, #6 and #9 are invalid, but *unintentionally* so — they are bugs, not
teaching examples.)

---

### 4. Verdict: build it

**Practical.** The corpus is tiny (19 blocks, 10 candidates, 3 files), the
harness already exists (`vibra fmt --check` is a pure reader + printer
round-trip and needs no project context), and the signal-to-noise is excellent:
the first run finds 3 real bugs and 0 false alarms once fragments are tagged.

**Required fence convention.** The current tags do not distinguish "module" from
"fragment", so a naive `all ```vibra blocks must parse` rule would fail on 8/10
blocks and get switched off. Proposed:

| Tag | Meaning | Test action | Blocks |
|---|---|---|---|
| ` ```vibra ` | Complete module | Reader must accept as-is | #1, #5, **+ #4, #6, #9 after fixing** |
| ` ```vibra-expr ` | Expression / statement fragment | Wrap in a synthetic `(defn __doc () void (do …))` then require the reader to accept | #2, #3, #8 |
| ` ```vibra-project ` | `project.vib` manifest root | Parse against the manifest root grammar | #7 |
| ` ```text ` | Pseudocode / not source | Skipped | #10, plus the 2 existing `text` blocks |
| ` ```ebnf ` | Grammar productions | Skipped | 7 existing |

Fail the test on any *unrecognised* tag, so a new `vibra`-ish tag can't silently
opt out of checking.

**Editing cost to make it pass: 9 blocks touched.**

| Change | Count | Blocks |
|---|---|---|
| Fix genuine grammar drift (content edit) | **3** | #4, #6, #9 |
| Retag module → `vibra-expr` | 3 | #2, #3, #8 |
| Retag module → `vibra-project` | 1 | #7 |
| Retag `lisp` → `text` | 1 | #10 |
| No change | 2 | #1, #5 |

Only 3 of those are real work; the other 6 are one-word fence retags. Landing
the 3 content fixes is worth doing on its own merits regardless of whether the
test ships.

**Scope note.** The plan says `docs/decisions/*.md`, which is 3 files. If the
intent is to stop *all* documentation from drifting, consider widening to
`docs/reference/` and `README.md` in a follow-up — but the convention above
should be settled on the small set first.

<sub>Method: blocks extracted with `awk` and each written to a standalone
`.vib` file, then `vibra fmt --check <file>` run against a `cargo build` of the
current tree (branch `agent/vib-source-call-order`).</sub>
