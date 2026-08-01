# S-expression migration status (issue #150)

Current status for the YAML → S-expression replacement: complete. The
historical notes below are retained as migration records, not current source
contracts.

The authoritative contract is
[`docs/decisions/s-expression-language.md`](../decisions/s-expression-language.md).
Its "Implementation and PR plan" defines nine steps; this file tracks them.

## Step status

| # | Step | State |
|---|------|-------|
| 1 | Reader, CST, spans, formatter | Done — #153, #157, #161, #166 |
| 2 | Lowering and compiler | Done — `src/surface_adapter.rs` bridges the typed AST into `src/lower.rs`'s existing `Value` shape |
| 3 | Macros and origins | Done — #165 |
| 4 | Language corpus | Done — #193 converted all modules; migrator has explicit `--write` opt-in |
| 5 | Projects and packages | Done — #158, #160 |
| 6 | LSP and rewrites | Done — typed S-expression semantic indexing powers live editor features |
| 7 | Generic editor removal | Done — legacy `src/code/` YAML editor stack removed |
| 8 | Output and embedding | Done — #154, #171 |
| 9 | Removal and documentation | Done — source contracts and migration metadata are S-expression-only |

### Archived Step 2 course correction (#150 surface-adapter)

The typed path (`typed_lower.rs`/`typed_body.rs`/`typed_program.rs`) requires
re-deriving every semantic rule `src/lower.rs` already implements and proves
against the live corpus. Measured readiness on that path stalled at
`materialized-valid 5/57`. `src/lower.rs`'s YAML coupling turned out to be
shallow -- an unqualified `use serde_yaml::Value` plus roughly 240 accessor
call sites, not a semantic dependency -- so `src/surface_adapter.rs` instead
adapts the typed AST into the exact `Value` shape `src/lower.rs` already
consumes. Chaining `syntax::parse -> ast::lower_document -> module_to_value
-> lower::lower_program` (with `lower_library`/`lower_tests` substituted for
files with no `main`, matching how a real build treats a library or test
suite) lowers 36/58 convertible corpus files completely, against the typed
path's 5/57. The remaining gap is concentrated in one known, documented
limitation (`impls:` for an interface declared in a different module than
its implementation), plus a handful of measurement-harness artifacts
(self-qualified entry mounting, `expect-error` tests, a missing prebuilt
`.wasm` fixture, and one project-level `@` import) that do not reflect real
build behavior. See the amendment in
`docs/decisions/s-expression-language.md` for the
contract exception this relies on. The adapter is internal, private,
single-direction, and now wired into `src/load.rs` after the typed path
reached parity.

### Archived dependency order

Earlier planning had step 6 running parallel to step 2. That is wrong. The
LSP cannot be repointed before the corpus is converted: every real workspace
file is still legacy YAML, `frontend::load_surface_program` rejects anything
that is not S-expression, so repointing `Workspace::build` would fail on all
real content — and the contract forbids the only workaround (no second source
parser, no content-based dialect selection). The actual order is:

    step 2 typed bodies  ->  step 4 corpus conversion (atomic cutover)
                         ->  step 6 LSP repoint
                         ->  steps 7/9 delete src/code/ and remaining YAML

Only the semantic *index* (#178) could land early, so that the eventual cutover
is a repoint rather than a rewrite.

## Merged so far

`#152` contract · `#153` reader/CST/printer · `#154` YAML output removed ·
`#155` generic types simplified · `#157` trailing labeled attributes ·
`#158` package metadata JSON · `#159` typed surface AST ·
`#160` manifests/locks · `#161` document-qualified AST identities ·
`#162` authoritative typed frontend graph · `#163` generic YAML editor removed ·
`#164` typed declaration lowering · `#165` program-wide macro expansion ·
`#166` staged formatter/diagnostics · `#167` typed executable bodies ·
`#168` staged typed docs/test readers · `#169` typed advanced type lowering ·
`#170` typed surface forms · `#171` YAML dropped as an embed format

## Frozen syntax decisions

- Generic types are direct call-like applications: `(option int64)`. There is no
  `(inst ...)` head. Type position makes it a generic application; expression or
  pattern head position stays a runtime call or enum constructor.
- Trailing labeled attributes spell configuration: `doc:`, `where:`, `defs:`,
  `impls:`, `tags:`, and peers. The reader emits `Atom::Label`. All positional
  operands precede labels, each label consumes exactly one following form, and
  duplicate/unknown/missing/misplaced labels are errors. This is **not** named
  arguments for ordinary calls.
- Governing rule: required, ordered, evaluated input is positional; optional,
  unordered configuration of the containing form is labeled.
- Primitives lose the `$` sigil. The spec spells them bare — `(add 1 1)`,
  `equal`, `not` — while the legacy table at `src/lower.rs:7340` still keys on
  `"$add"` and peers.
- **Primitive name resolution** (spec amended in #177, corrected in #180).
  Dropping the sigil makes `add` collidable with a user function, which `$add`
  never was. Rule: an *unqualified* call head matching a primitive name resolves
  to the primitive, and a *qualified* head (`module.add`) never does.
  Declaring a function named after a primitive is **permitted**.

  The first version of this rule rejected such declarations, on the stated
  grounds that they would be "permanently unreachable", and claimed the corpus
  had been verified free of collisions. Both were wrong. `option.and`,
  `option.or`, `result.and`, and `result.or` are real declarations reached
  through their qualified names, and the collision check had dismissed the grep
  hits as identifier substrings without opening the files. The rejection blocked
  typed signatures for those two stdlib modules and every module importing them
  — 36 of 58 corpus files. Fixing it moved `signature-valid` 22/58 to 43/58.

  Lesson worth keeping: a claim of "verified against the corpus" means the
  tiered validator was run, not that a grep looked clean.

## Architecture note (important, and easy to get wrong)

`src/lower.rs` is 9127 lines against `src/typed_lower.rs`'s 1619, which invites
the wrong conclusion that the typed path must reimplement it. It must not.
`typed_lower.rs` imports its *output* types from `crate::lower` (`TypeRef`,
`TypeAlias`, `ImplBody`, `ImplKey`), so the
typed modules are new **readers** producing the existing semantic IR. Legacy
lowering mentions `Yaml`/`Mapping` on only 27 of its lines; the YAML coupling is
the unqualified `use serde_yaml::Value` at `src/lower.rs:14` plus roughly 240
accessor sites (`as_str` 128, `as_mapping` 84, `as_sequence` 29).

Consequence: the remaining work is replacing the reading layer and sharing the
semantics, not rewriting the type system.

## Archived migration notes

### Step 2 — typed body lowering

Complete for every construct the corpus uses. Landed in order:

| Construct | PR |
|-----------|----|
| `Expr::Primitive`, all 22 `PrimitiveOp` variants | #177 |
| declarations named after primitives permitted | #180 |
| `Self` substituted before impl conformance | #182 |
| `Expr::EnumConstructor`, `Expr::If`, `Expr::PolicyNarrow`, `Statement::Spawn` (and `Statement::Join` validation) | #183 |
| wasm host-import bodies | #184 |
| interface method dispatch | #187 |
| `ExprKind::Convert` checked conversion | #189 |

Two details worth not relearning:

- `Convert` does not route through generic `PrimitiveOp` dispatch. The
  sigil-free primitive table intentionally excludes it, since `convert` has its
  own surface form, so it needs a dedicated `Expr::Primitive { op: Convert }`
  arm placed before the generic one. Its `fallback` is now `Spanned<Literal>`;
  it previously discarded the parser's span and so had no origin for
  `OriginCursor` to consume.
- Interface and inherent dispatch are the same mechanism. `error.error.kind`
  and `fs.writable.write-string` both go through `$interface` declarations.
  Receiver types come from a static pre-pass that uses `substitute_type` and
  **not** `normalize_type_ref` — normalizing inlines the nominal name away
  before `ImplKey` can use it.

Generic functions remain rejected at `src/typed_body.rs:147`.

Port from the legacy path rather than reimplementing, and reuse
`src/type_semantics.rs` for anything type-relational.

### Step 4 — corpus

All 62 `.vibra` corpus files are S-expressions. The migrator in
`tools/corpus-migrator` remains dry-run by default and supports an explicit
`--write` opt-in; it rewrites only successfully converted and
surface-validated files. `stdlib` is a submodule, so the completed corpus
conversion landed as a stdlib change plus a parent pointer bump.

**Converting the corpus IS the atomic cutover, not a preparatory step.** A
converted corpus is unreadable to the legacy YAML compiler, so `vibra test`
breaks the instant it lands unless the typed path fully compiles and runs the
corpus. This is why the formatter cutover was deliberately kept out of `main`.

**Readiness is measured in tiers** (#181, #185, #188). The validator reports
`surface-valid`, `signature-valid`, and `body-valid` as a funnel, each tier run
only on files passing the one above, with ranked per-tier failure reasons as the
remaining work-list. It previously reported a single conflated `typed-valid:
58/58` that measured surface parsing alone, which read as cutover readiness when
body readiness was 9%.

Read the failure reasons, not just the count. A count can stay flat while real
progress happens, because files advance to a deeper blocker — that is exactly
what #183 looked like at first glance.

Verified already correct: the migrator's `sym()` (line 1351) strips the `$`
sigil, so `$add` emits as bare `(add ...)`, matching the contract and #177.

### Step 6/7/9 — cutover seam

`src/load.rs` reads source only through the typed frontend and adapts its
surface AST for the internal lowering bridge. The legacy YAML editor model,
`yaml-edit` dependency, `yaml_subset`, and `.vibra.yaml` discovery have been
removed. `src/sexpr_semantic.rs` provides the editor semantic index.

## Acceptance gate scoreboard

Measured against current `main`:

| Gate | Current | Target |
|------|---------|--------|
| `E-YAML-*` diagnostics | removed | 0 |
| `.vibra.yaml` discovery | rejected by typed frontend | none |
| `serde_yaml` in `src/` | absent | absent |
| internal compatibility value bridge | typed AST -> data-only map | temporary |
| `yaml-edit` production dependency | absent | absent |
| `yaml` crate keyword | removed | removed |
| `src/yaml_subset.rs` | deleted | deleted |
| `src/code/` | deleted | deleted |

## Working rules

- Both suites must pass before any PR is claimed done: `cargo test` and
  `cargo run -- test` (currently 92 Vibra cases).
- Worktrees do not check out the `stdlib` submodule automatically. Run
  `git submodule update --init --recursive` in a fresh worktree, or the corpus
  inventory and language suite are wrong.
- No hidden legacy feature flag, and no dual-syntax supported state. The
  migration utility stays external and is never invoked by the compiler.
