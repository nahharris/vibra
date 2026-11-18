# v0.0.2 M1 architecture

Status: milestone design record, non-normative. The
[specification](../../../spec/00-charter.md) and the [v0.0.2
roadmap](../README.md) win over it. It records the decisions that constrain the
M1 steps in [README.md](README.md) and the reasons they were taken, as the
[execution model](../../execution.md#design-decisions) requires.

## Summary

The v0.0.1 code implements the previous S-expression surface end to end and
is green. The specification describes a different language. M1 builds the
new front end, checker, and interpreter as new crates that carry the final
crate names from their first commit, beside the old crates, which are renamed
with a `-v1` suffix in the first step and deleted in one cutover step near
the end of the milestone. Nothing accepts both languages at any moment: the
old pipeline reads only the old corpus, the new pipeline reads only the new
corpus, and the `vibra` binary is the new one from its first step. The
integration branch compiles and passes `cargo test --workspace` after every
merged step, because no step changes a type another live crate depends on.

## 1. Transition strategy

### What has to stay true at every moment

- `cargo test --locked --offline --workspace --all-targets --all-features`
  and the conformance job pass on the integration branch after every merge.
- The WebAssembly backend keeps a differential run until the native cutover
  of v0.0.3 ([runtime chapter](../../../spec/06-runtime.md#wasm-transition)).
- The native track (`vibra-exec`, `vibra-lower`, `vibra-nexus`,
  `vibra-nexus-eval`, `vibra-nexus-bridge`, `vibra-codegen`, `vibra-build`)
  keeps compiling, and package X2-01 of the [Nexus
  plan](../../nexus/plan-v0.0.2.md) can rebaseline it against the new checked IR
  while M1 is in flight.
- Many agents work in parallel, each in a worktree, merging into one branch.
  Two agents must never need to edit the same `lib.rs` for unrelated steps.

### The three candidates

**(i) Rewrite each crate in place, bottom-up, old corpus deleted up front.**
Deleting the corpus makes the conformance binary exit non-zero on day one
(`corpus is empty`, `crates/vibra-conformance/src/bin/conformance.rs:46`),
and rewriting `vibra-syntax` in place breaks the nine crates that import its
AST (`vibra-fmt`, `vibra-resolve`, `vibra-types`, `vibra-host`,
`vibra-workspace`, `vibra-cli`, `vibra-schema`, `vibra-conformance`) until
every one is rewritten. The branch is red for weeks, the native track loses
the host tests it is written against (`crates/vibra-conformance/tests/it/`
holds its lowering, differential, and linking tests in old syntax), and the
Wasm backend runs nothing until the new checker produces IR. Rejected: fails
every requirement above.

**(ii) New module trees inside the same crates, old modules deleted at a
cutover.** Compiles green, because new modules are additive. But every crate
then exports two APIs from one `lib.rs`; `vibra-ir` would hold two IRs and
`vibra-interp` two machines; the architecture test cannot tell the two
dependency graphs apart (the new interpreter must not depend on `vibra-host`
in M1, the old one does); every step touches the shared `lib.rs` and
`Cargo.toml`, so parallel steps conflict on the same lines; and
`cargo test -p vibra-types` cannot select one pipeline. Workable, but worse
for parallel agents than (iii) in every respect that matters here.

**(iii) New crates beside the old, cutover, delete.** Chosen, with one
refinement: **the old crates are renamed first**, so the new crates take the
final names from their first commit and the code that survives the milestone
is never renamed. Separate directories mean separate PRs never conflict,
`cargo test -p` selects one pipeline, the architecture test enforces the new
dependency graph on its own, and the cutover is `git rm` plus manifest edits
rather than a rename of everything that survives.

### The decision

1. **Step 01 renames the old world.** `vibra-diagnostics`, `vibra-syntax`,
   `vibra-fmt`, `vibra-resolve`, `vibra-types`, `vibra-ir`, `vibra-interp`,
   `vibra-workspace`, `vibra-schema`, `vibra-cli`, and `vibra-conformance`
   become `<name>-v1` (directory, package name, workspace dependency, every
   `use` path). The `vibra` binary becomes `vibra-v1`, `conformance/cases`
   becomes `conformance/v1/cases`, `stdlib/` becomes `stdlib-v1/` (the old
   `vibra-types` embeds it by path with `include_bytes!`,
   `crates/vibra-types/src/stdlib.rs:64`), `examples/` becomes
   `examples-v1/`, and `fuzz/` becomes `fuzz-v1/`. CI runs the renamed
   binaries. Nothing else changes; the diff is mechanical and the tests
   prove it.
2. **New crates carry the final names.** Each is created by the step that
   first needs it (see the crate map) and depends only on new crates plus
   the crates the map marks kept. The new `vibra-conformance` owns the
   architecture test for the whole workspace, old and new rows alike, from
   step 02.
3. **Two corpora, two runners, one CI job.** The v1 runner keeps running
   `conformance/v1/cases` with the v1 handlers, and on the Wasm backend
   until step 27 moves the backend to the new IR. The
   new runner runs `conformance/cases` with the `readerV2`, `staticV2`, and
   `interpreterV2` handlers as they land. Both lines are in the conformance
   job until the cutover.
4. **The cutover is step 28.** It deletes every `-v1` crate, `stdlib-v1/`,
   `examples-v1/`, `fuzz-v1/`, and `conformance/v1/`, removes the v1 lines
   from CI and the v1 rows from the architecture test, and updates the
   repository README and roadmap status lines. Its prerequisites are every
   language step of M1 (01 to 27) and the first PR of package X2-01, which
   moves the native track to the new `vibra-ir`. Until then the v1 pipeline
   is live and green, and the `vibra` binary, which is new from step 24, is
   the only command surface a user sees.
5. **What the Wasm backend needs.** It consumes checked IR, not source. The
   v0.0.1 corpus can run on it only while it reads the v1 IR, that is,
   until step 27. Step 27 ports `vibra-wasm` and `vibra-wasm-run` to the new
   `vibra-ir` fail-closed: a node the port does not lower is reported
   unsupported, never guessed. From step 27 the backend runs the pure
   executable cases of the new corpus it supports, under the
   `wasmTransitionV2` profile, and reports what it does not. No M1 gate
   requires the Wasm column; its counts are recorded. The sentence in the
   release README that the backend "keeps running the v0.0.1 corpus" is
   true until step 27 and is listed as [Q1](spec-questions.md) because it
   cannot be true afterwards without keeping a second IR in the backend.
6. **What the native track needs.** The native crates depend on
   `vibra-ir-v1` and `vibra-host` from step 01. Step 12 publishes the new
   `vibra-ir` with the complete M1 node set and a migration table from the
   old nodes. Package X2-01 moves the native track to it and re-homes the
   native host tests out of `vibra-conformance-v1`. If X2-01 has not landed
   when every other prerequisite of step 28 has, step 28 does the mechanical
   re-pointing itself and records the tests it had to drop as X2-01's
   remaining list; it never deletes the native crates.
7. **`vibra-host`, `vibra-native`, `vibra-wasm`, `vibra-wasm-run`** are kept
   crates. They depend on `vibra-ir-v1` and `vibra-syntax-v1` until step 27
   ports them: the host world and audit event documents move to the data grammar
   of the [programs
   chapter](../../../spec/04-programs-and-packages.md#vibon-data-documents) with
   their v1 field shapes, respelled `@hostWorld.v1` and `@auditTrace.v1`, and M2
   replaces them with the v2 documents. The new interpreter has no host
   dependency in M1, because the pure language performs no operation.

### Red by design

Nothing is red at any point. Two tests stay `#[ignore]`d until their
replacement exists in the new `vibra-conformance`: the registry membership
test (`crates/vibra-conformance/tests/it/diagnostic_registry.rs:65`) is
replaced in step 02, and the specification example inventory
(`crates/vibra-conformance/tests/it/evidence.rs:360`) in step 08. The
native parity inventory test (`native_parity_inventory.rs:141`) is X2-04's.
All three leave with `vibra-conformance-v1` at the cutover.

### Green after every step

| Step | `cargo test --workspace` green after it | Why |
| --- | --- | --- |
| 01 | yes | mechanical rename; every test target is rebuilt and passes |
| 02 | yes | new crates only; the architecture test lists every crate |
| 03 to 08 | yes | new crates and new cases; the v1 pipeline is untouched |
| 09 to 11 | yes | same |
| 12 to 14 | yes | new crates and `stdlib/`; nothing else reads them yet |
| 15 to 23 | yes | the checker grows inside new crates; each step's cases pass before merge |
| 24 to 26 | yes | the new `vibra` binary; the v1 binary is `vibra-v1` |
| 27 | yes | the kept runtime crates compile against the new IR; their tests are ported with them |
| 28 | yes | the cutover deletes code and its tests together; the new corpus and both suites pass |
| 29 | yes | evidence only |

## 2. Crate map

### Existing crates

| Crate | Decision | What changes |
| --- | --- | --- |
| `vibra-diagnostics` | rewritten (step 02; old → `-v1`, deleted in 28) | `ByteSpan`, `LineIndex`, `Position`, `Diagnostic`, `RelatedSpan`, `Fix`, `DocumentRevision` copied. The registry enum is regenerated from the table of the [diagnostics chapter](../../../spec/07-diagnostics-and-conformance.md#diagnostics-are-a-language-surface): camelCase atoms, one variant per row, levels from the table, fix capability `@none` for every error. |
| `vibra-syntax` | rewritten (step 03) | Lexer for the keyword-led grammar, lossless CST with recovery, typed source views, VIBON values, document mode by extension. No S-expression reader. |
| `vibra-fmt` | rewritten (step 03) | Canonical format of the source chapter; never reorders operands; returns recovered documents unchanged. No binding facts from the checker: the `BoundFormat` path of v1 (`crates/vibra-fmt/src/lib.rs:88`) has no successor because the formatter never reorders arguments. |
| `vibra-resolve` | rewritten (step 10) | Flat module scope, derived import names, anchor identities, namespaces, paths against field reads, acyclic imports. The neutral filesystem-free input shape of v1 (`crates/vibra-resolve/src/lib.rs:123`) is kept as a design. |
| `vibra-ir` | rewritten (step 12) | The checked IR of the new language: variant types with named fields, no unions, no variadics, guard consequences, `List` instead of `Array`, library roles bound to declared types. Compiler intrinsic registry with camelCase source names and the v1 wire symbols. Host registry rows as data for M2 and the port of step 27. |
| `vibra-types` | rewritten (steps 14 to 22) | Stdlib loader, declaration collection, nominal checks, local inference, bodies, coverage, interfaces, dispatch, associated types, widening, the certified fragment, the seven discharge rules, compile-time evaluation through the interpreter, lowering to checked IR. |
| `vibra-interp` | rewritten (step 13) | Heap activations, tail calls, lazy module values, memory budget, `@value.v2` observation, the compiler intrinsics, guard consequences, an evaluation entry for the checker. The design of v1 (`crates/vibra-interp/src/lib.rs:40`) is carried over; the code is not, because every value kind changes. |
| `vibra-workspace` | rewritten from kept modules (steps 09, 15, 23) | `confined_fs`, `discovery`, `snapshot` are copied and adapted; `project` decodes `@project.v2`; `source_graph`, `semantic`, `test_runner` are rewritten. `query`, `index`, `format_plan` are M3's. `codebase` is removed by M0 and does not return. |
| `vibra-schema` | rewritten (step 24) | Version 2 schemas: command result, diagnostic, span, test item, capability. Query, index, and edit-plan schemas are M3's. |
| `vibra-cli` | rewritten (steps 24 to 26) | `help`, `fmt`, `check`, `test` with the version 2 envelope. Every other command of the [tooling chapter](../../../spec/05-tooling.md#command-surface) is `@command.unavailable` and listed as unsupported in `help`. |
| `vibra-conformance` | rewritten from the kept runner core (step 02) | Corpus loader, `case.toml` manifest with V2 identities and camelCase profile and operation atoms, dispatcher, runner, binary. Handlers arrive with the steps that make them observable. The workspace architecture test lives here from step 02. |
| `vibra-host` | kept, adapted (step 27; M2 rewrites) | Documents move to the data grammar; rows read from the new `vibra-ir`. Not used by the M1 interpreter. |
| `vibra-native` | kept, adapted (step 27) | Compiles against the new `vibra-ir`. The M1 interpreter runs Vibra bodies and intrinsics and uses no native implementation. |
| `vibra-wasm`, `vibra-wasm-run` | kept, adapted fail-closed (step 27) | Lower what maps one-to-one, report the rest unsupported. Removed in v0.0.3. |
| `vibra-exec`, `vibra-lower`, `vibra-nexus`, `vibra-nexus-eval`, `vibra-nexus-bridge`, `vibra-codegen`, `vibra-build`, `vibra-actors` | kept | Moved to the new `vibra-ir` by package X2-01, not by M1. |
| every `-v1` crate | deleted (step 28) | With `stdlib-v1/`, `examples-v1/`, `fuzz-v1/`, `conformance/v1/`. |

### Dependency graph of the new pipeline

```text
vibra-diagnostics
  ^        ^
vibra-syntax   vibra-ir
  ^   ^          ^   ^
vibra-fmt  vibra-resolve   vibra-interp
              ^              ^
            vibra-types -----+        (the checker calls the interpreter at compile time)
              ^
         vibra-workspace  (+ syntax, fmt, resolve, ir, interp, diagnostics)
           ^         ^
    vibra-schema   vibra-cli (+ schema)
           ^
    vibra-conformance (everything)
```

The arrow from `vibra-types` to `vibra-interp` is new and deliberate: the
charter makes the interpreter the compile-time evaluator, so it is part of
language semantics, not a backend. No language crate depends on `vibra-fmt`,
`vibra-schema`, `vibra-cli`, `vibra-workspace`, or `vibra-conformance`. The
architecture test in the new `vibra-conformance` encodes this table.

### Module responsibilities of the new front end

| Crate | Module | Responsibility |
| --- | --- | --- |
| `vibra-syntax` | `lexer` | UTF-8 bytes to tokens: identifiers, reserved words, atoms, literals, doc strings, comments, the four bracket pairs, `:`. Encoding and literal diagnostics. Lossless. |
| | `cst` | The lossless tree with recovery: opener and closer matching for every `… end…` pair and bracket, section boundaries, error nodes. Every byte is in exactly one leaf. |
| | `source` | Typed views over the CST for the source grammar: module, the seven declarations, sections, type expressions, expressions, statements, patterns. Views are projections; they own no bytes. |
| | `data` | VIBON: the generic value tree, validation of the data grammar, canonical text. |
| | `document` | Mode by extension, `parse_source`, `parse_data`, the `Document` that carries bytes, tree, diagnostics. |
| `vibra-fmt` | `layout` | Line breaking, indentation, the one-line rule, comment placement. |
| | `source` | Canonical section order, declaration and body layout. |
| | `data` | Canonical VIBON whitespace. |
| `vibra-resolve` | `graph` | Units, modules, declarations with anchor identities. |
| | `imports` | Derived names, `as`, cycles, private access. |
| | `scope` | The flat module scope, lexical scopes, shadowing, binder classification, constant patterns. |
| | `paths` | Longest entity prefix against field reads; entity kinds. |
| `vibra-types` | `stdlib` | Manifest decode, digests, roles claimed exactly once, externals against the registry. |
| | `declare` | Headers of every declaration; generic parameters and bounds. |
| | `nominal` | Type bodies, finite size, representation visibility, member namespace. |
| | `infer` | Expected types flowing inward; literal typing; generic argument inference. |
| | `expr` | Bodies: application classification, `let`, `if`, `match`, `do`, `return`, `try`, evaluation order. |
| | `pattern` | Irrefutability and coverage. |
| | `interfaces` | Implementations, overlap, dispatch, associated types, interface values, widening. |
| | `contract` | The certified fragment, obligations, the seven discharge rules, the integer facts, consequences. |
| | `ctfe` | Compile-time evaluation through `vibra-interp` under tool limits. |
| | `lower` | Checked AST to `vibra-ir`. |
| `vibra-interp` | `machine` | Activations on the heap, tail transfer, module values, guard consequences, memory accounting. |
| | `value` | Runtime values with representation latitude. |
| | `intrinsics` | The compiler intrinsic registry. |
| | `encode` | `@value.v2` and `@type.v2`. |
| | `evaluate` | The checker's entry: evaluate one certified expression against a module set under limits. |

### Boundary types

These are proposals, not claims that the APIs exist. Two agents build the
two sides of each boundary against these shapes; a change to one is a
change to both PRs.

**`vibra-syntax` to `vibra-fmt`, `vibra-resolve`, `vibra-types`,
`vibra-workspace`:**

```rust
pub enum DocumentMode { Source, Data }
pub struct Document { mode, source_id: Option<String>, text: String, root: CstNode, diagnostics: Vec<Diagnostic> }
pub struct CstNode { kind: SyntaxKind, span: ByteSpan, children: Vec<CstNode> }  // leaves carry text
pub enum SyntaxKind { Root, Declaration, Section, Closer, Keyword, Identifier, Atom, Literal, DocString, Comment, Whitespace, Brace, Bracket, Paren, Colon, Error, … }
pub fn parse_source(source_id, text) -> Document;   pub fn parse_data(source_id, text) -> Document;
pub fn module(&Document) -> Option<Module>;          // typed view, None when recovery hit the top level
pub enum Declaration<'a> { Use(Use<'a>), Type(TypeDecl<'a>), Interface(..), Fn(FnDecl<'a>), Proc(ProcDecl<'a>), Let(ModuleLet<'a>), Test(Test<'a>) }
pub enum TypeExpr<'a> { Path(Path, Vec<TypeExpr>, Vec<(Ident, TypeExpr)>), Atom(Atom), Tuple(Vec<..>), Record(Vec<(Ident, ..)>), Fn(Inputs, Box<..>), Proc(Inputs, Box<..>, Option<Row>), Intent(Box<..>, Option<Row>) }
pub enum Expr<'a> { Literal, Name(Path), Atom, Apply { callee, args: Vec<Argument> }, Field { base, name }, Tuple, Record, List, Dict, If, Match, Do, AnonFn, AnonProc, Return, Try, Await }
pub enum Pattern<'a> { Discard, Name(Ident), Literal, Atom, Path(Path), Apply(Path, Vec<PatternArg>), Tuple, Record, List }
pub struct DataNode { value: DataValue, span: ByteSpan }
pub enum DataValue { String, Char, Bool, Integer, Float, Void, Atom, Tuple(Vec<DataNode>), Record(Vec<(Ident, DataNode)>), List(Vec<DataNode>), Dict(Vec<(DataNode, DataNode)>) }
```

**`vibra-resolve` to `vibra-types`, `vibra-workspace`:**

```rust
pub struct ResolveInput { package: PackageId, units: Vec<SourceUnit>, stdlib: Option<StdlibOverlay> }
pub struct SourceUnit { name: String, kind: UnitKind /* Lib | Bin | Tests */, modules: Vec<SourceModule> }
pub struct SourceModule { unit, segments: Vec<String>, source_id: String, bytes: Vec<u8> }
pub struct ResolvedSnapshot { modules, declarations: Vec<ResolvedDeclaration>, imports, references, diagnostics }
pub struct DeclarationId(String);   // the anchor identity of the type chapter, see Q13
pub enum EntityKind { Module, Type, Interface, Case, Field, Function, Procedure, Value, Test, AssociatedType, Implementation }
pub struct ResolvedDeclaration { id, anchor: Anchor, kind, visibility, representation, source_id, span, owner: Option<DeclarationId> }
```

**`vibra-ir` to `vibra-types`, `vibra-interp`, the kept runtime crates, the
native track:**

```rust
pub enum Type { Void, Never, Char, Atom, AtomSingleton(String), I8, …, U64, F32, F64,
                List(Box<Type>), Tuple(Vec<Type>), Record(Vec<(String, Type)>),
                Fn(Box<FnSignature>), Proc(Box<ProcSignature>), Intent(Box<IntentType>),
                Declared(TypeId, Vec<Type>), Interface(TypeId, Vec<Type>, Vec<(String, Type)>), Param(String), Any }
pub struct TypeDefinition { id: TypeId, parameters: Vec<String>, body: TypeBody, invariant: Option<Invariant>, representation: Visibility }
pub enum TypeBody { Record(Vec<(String, Type)>), Tuple(Vec<Type>), Wrapper(Type), Variant(Vec<Case>) }
pub struct Case { name: String, fields: Vec<(String, Type)> }
pub struct Invariant { predicate: FunctionRef, consequence: Option<Constant> }   // `where P` / `where P else E`
pub struct FnSignature { parameters: Vec<Type>, labelled: Vec<Labelled>, result: Type }
pub struct CheckedFunction { name, signature, body: Expr, entry_guard: Option<Guard>, exit_guard: Option<Guard>, origin, slot_count, external: Option<Intrinsic>, implements: Option<Implements> }
pub struct Guard { predicate: Expr, consequence: Expr }      // only guards with `else`; discharged guards leave nothing
pub enum Expr { Literal, Intrinsic{..}, Sequence, Variable, Global, Function, Closure{..}, Captured, Let{slot, value, body}, LetElse{pattern, value, fallback, body}, Match, If, Construct{type, arguments}, Field{..}, Lookup{..}, Call{target, arguments}, TailCall, Return, Try, WidenToInterface{..}, WidenAtom{..}, Default{..} }
pub struct CheckedModuleSet { types, interfaces, globals: Vec<CheckedGlobal>, functions: Vec<CheckedFunction>, implementations }
pub struct CheckedProgram { set: Arc<CheckedModuleSet>, entry: usize }
pub fn canonical_value(&Value, &Type, &CheckedModuleSet) -> String;   // @value.v2
```

Bool, Str, Bytes, Dict, Option, Result, Ordering, and the error types are
`Declared` types whose identities the checker binds through their roles; the
IR has no scalar case for them. The interpreter may represent them directly.

**`vibra-interp` to `vibra-types`, `vibra-workspace`:**

```rust
pub struct MemoryBudget(usize);
pub struct Limits { reductions: u64, memory: MemoryBudget }
pub fn run(&CheckedProgram, MemoryBudget) -> Result<Execution, RuntimeError>;
pub fn run_test(&CheckedProgram, MemoryBudget) -> Result<TestExecution, RuntimeError>;
pub fn evaluate(&CheckedModuleSet, &Expr, &[Value], Limits) -> Evaluation;   // Value(Value) | LimitReached(Limit) | Defect(Trap)
pub struct Execution { canonical: String, program_failed: bool, max_activation_depth, tail_transfer_count, peak_memory_bytes }
```

**`vibra-types` to `vibra-workspace`, `vibra-conformance`:**

```rust
pub struct Stdlib { package: PackageId, modules: Vec<StdlibModule>, roles: BTreeMap<Role, DeclarationId> }
pub fn load_stdlib() -> Result<Stdlib, StdlibError>;
pub fn check_resolved(&ResolvedSnapshot, scope: &[String], &Stdlib) -> CheckResult;
pub struct CheckResult { module_set: Option<Arc<CheckedModuleSet>>, diagnostics, obligations: Vec<Obligation>, types_document: Option<String> /* @types.v2 */ }
pub fn check_source(source_id, text) -> CheckResult;   // one module of a one-module library workspace
```

**`vibra-workspace` to `vibra-cli`, `vibra-conformance`:**

```rust
pub struct WorkspaceSnapshot { project: Project, source: SourceSnapshot }   // revision per the tooling chapter
pub fn check_all(&WorkspaceSnapshot) -> WorkspaceCheckResult;
pub fn run_tests(&WorkspaceSnapshot, Option<&TestSelector>, MemoryBudget) -> TestRun;   // @testRun.v2
pub fn format(&WorkspaceSnapshot, source_id) -> FormatOutcome;
```

**`vibra-conformance`:** `ProfileHandler { can_run(&Case) -> bool; run(&Case) ->
Result<CaseObservation, HandlerError> }` and `CaseObservation { accepted,
diagnostics, formatted, tree, graph, resolved, types, result, test_run, wasm:
Option<..> }`.

## 3. The conformance corpus

### The 604 existing cases

They are written in the old surface, the old `(record …)` data notation,
kebab-case codes, and `V1-` identities. They move to `conformance/v1/cases` in
step 01, keep running on the v1 interpreter until step 28 and on the Wasm
backend until step 27, and are deleted in step 28 with the v1 crates. None is
mechanically translated: no translator exists, writing one is a compatibility
bridge in disguise, and a translated expectation is not an expectation written
from the specification. None is kept for Wasm after the cutover (decision 5
above). The rules whose meaning did not change (source-graph sorting, project
discovery, module path rules, diagnostic ordering) are re-authored by hand under
their `V1-` identity with new-notation inputs, as the [conformance
chapter](../../../spec/07-diagnostics-and-conformance.md#rule-identities-and-example-coverage)
allows; everything else gets a `V2-` identity. The retired identities are listed
in `conformance/rules.tsv`.

### Identity scheme and layout

- A case identity is `<rule identity>-<descriptive-suffix>`, where the rule
  identity is `V2-<PREFIX>` or a kept `V1-<PREFIX>` and the prefix is one of
  the twenty of the conformance chapter. Suffixes are lower case with
  hyphens, as the chapter requires for rule identities; they are directory
  names, not atoms.
- Layout: `conformance/cases/<PREFIX>/<case identity>/case.toml` plus the
  case's inputs and snapshots. One directory level per prefix keeps a
  prefix's cases listable and keeps `git` renames local.
- `conformance/rules.tsv` is the rule index: `identity`, `status`
  (`active` or `retired`), `chapter anchor`. A removed rule stays as
  `retired`. The runner rejects a case whose rule is not `active`.
- `case.toml` stays the neutral oracle, so the VIBON decoder is never its
  own judge: `id`, `rule`, `profile` (`readerV2`, `staticV2`,
  `interpreterV2`, `wasmTransitionV2`), `operation` (`reader`,
  `projectDecode`, `sourceGraph`, `resolve`, `workspaceCheck`, `format`,
  `interpret`, `workspaceTest`), `description` with the specification
  anchor, `[inputs]`, `[expect]` with `accepted`, `[[expect.diagnostics]]`
  rows (`code`, `level`, `source`, `span`, `related`, `notes`), and
  snapshot paths `tree`, `formatted`, `graph`, `resolved`, `types`,
  `result`, `testRun`.
- Snapshots are canonical VIBON of the formats the chapter lists:
  `@sourceGraph.v1`, `@resolved.v2`, `@types.v2`, `@value.v2`,
  `@testRun.v2`. The lossless tree snapshot of a `reader` case has no
  format in the chapter yet; [Q4](spec-questions.md) proposes one, and
  until it is answered the reader steps assert tree shape through the
  `tree` snapshot in the proposed text form and mark it provisional in the
  case description.

### Expectations are written from the specification

- The new runner has no mode that writes snapshots or diagnostics from a
  handler's output. The v1 `--write-native-parity` option has no successor
  in M1. A mismatch is a failure, always.
- Every case `description` cites the chapter anchor the expectation comes
  from. A reviewer checks the expectation against that anchor, not against
  the implementation's output.
- A case for a static rule comes in a pair: the accepted example and the
  nearest rejected one, named `…-accepted` and `…-rejected` or by what they
  reject. The step guides list the pairs.
- Expected spans are computed from the input text by hand, in the guide's
  matrix, before the handler exists. The guide's matrix row is the review
  record.

## 4. The standard library

`stdlib/` is rewritten in the new surface. The old sources live in
`stdlib-v1/` until the cutover.

| Step | Modules | Why then |
| --- | --- | --- |
| 14 | `manifest.vibon` (`@stdlibManifest.v2`), `@std.core` (`Ordering`, `ArithmeticError`, `ConversionError`, `Equatable`, `Ordered` with its defaults, `From`, `TryFrom`), `@std.bool`, `@std.option`, `@std.result`, `@std.iter` (interface and the four adapter types), `@std.builtin` (the `external @compiler` declarations of the intrinsic registry, `List` members), the loader | `if`, lookups, `try`, guard predicates, and every `match` on `Bool` need the roles before the first checker step. The modules are parsed and loaded in 14 and checked from 15 on, because the checker checks every prelude module in every graph. |
| 16 | `@std.builtin`: `Dict(K V)` with `entries` and the `From(List({K V}))` implementation; `List.length`, `append`, `concat`, `slice` | dict literals and lookups |
| 19 | `Equatable` and `Ordered` for the integer types, `Char`, `Bool`; `From` and `TryFrom` between integer types | dispatch evidence and guard predicates |
| 20 | `@std.str`, `@std.bytes`, `Str` and `Bytes` members and conversions, `Str.trim`, the `Char` classifiers, float operations, `Iter` implementations for `List`, `Dict`, `Str`, `Bytes`, `Option`, `fold`, the lexicographic helper | slice 6 |
| 23 | `@std.assert` | slice 8 |
| M2 | `@std.test`, `@std.process`, `@std.policy`, `@std.io`, `@std.fs`, `@std.env`, `@std.time`, `@std.random` | operations and grants |

The manifest's `native` list is empty in M1: the interpreter runs every
Vibra body, which the runtime chapter permits, and native acceleration is a
v0.0.3 concern. The `host` list is empty until M2.

## 5. Risks, ranked

1. **The cutover converges three tracks.** Step 28 needs every language
   step, the runtime port (27), and X2-01. Mitigation: the IR is closed in
   step 12 with a migration table, so 27 and X2-01 start early; 28 can do
   X2-01's mechanical re-pointing itself and leaves a written remainder; the
   v1 pipeline stays green however long 28 waits.
2. **The checker is one long chain (15 to 22).** Eight steps serialize on
   `vibra-types`. Mitigation: the IR, interpreter, stdlib, CLI, graph, and
   resolve steps run beside the chain; each checker step has a closed
   diagnostic list so two agents can prepare consecutive steps; a step that
   grows past its guide is split before it is started, per the execution
   model.
3. **Specification gaps surface mid-step.** The identity encoding, the
   `@types.v2` shape, the retired-form trigger, and the tree snapshot format
   are open ([spec-questions.md](spec-questions.md)). Mitigation: Q1 to Q6
   are answered before the steps that need them start (the guides name
   which), and an agent that meets a new gap stops and files it rather than
   choosing.
4. **Independently authored expectations are expensive and error-prone.**
   Hand-computed spans drift from the input. Mitigation: the guide matrices
   carry the input and the span together; the runner reports the actual
   span on mismatch so the reviewer compares two numbers against the text,
   never accepts the actual one by default; small inputs.
5. **The stdlib must check under a checker that is being built.** A stdlib
   module that uses a form the checker does not have yet makes every graph
   fail. Mitigation: step 14's modules use only the forms of steps 15 and
   16 and are listed form by form in its guide; later modules land with the
   checker step that admits their forms.
6. **Compile-time evaluation through the interpreter couples two crates.**
   A limit that reports `true` instead of "not established" would be a
   soundness hole. Mitigation: `evaluate` returns a three-way result and the
   contract step has a case per outcome; the interpreter's reduction count
   is the only limit the checker consults.
7. **The Wasm port is scoped by what maps one-to-one.** If less maps than
   expected the backend's column is small. Mitigation: the column is not a
   gate; the count is recorded; v0.0.3 removes the backend.
8. **Parallel agents touch `Cargo.toml`, CI, and the architecture table.**
   Mitigation: only steps 01, 02, 03, 09, 10, 12, 13, 14, 24, 27, and 28 add
   crates or CI lines; every other step touches only its crates and
   `conformance/cases`.
