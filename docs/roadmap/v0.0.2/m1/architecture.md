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
holds its lowering, differential, and linking tests in old syntax).
Rejected: fails every requirement above.

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

1. **Step 01 renames the old world.** Thirteen crates become `<name>-v1`
   (directory, package name, workspace dependency, every `use` path):
   `vibra-diagnostics`, `vibra-syntax`, `vibra-fmt`, `vibra-resolve`,
   `vibra-types`, `vibra-ir`, `vibra-interp`, `vibra-host`, `vibra-native`,
   `vibra-workspace`, `vibra-schema`, `vibra-cli`, and `vibra-conformance`.
   `vibra-host` and `vibra-native` are among them because they share types
   with the IR they are compiled against, and both IRs are live until the
   cutover. The `vibra` binary becomes `vibra-v1`, `conformance/cases`
   becomes `conformance/v1/cases`, `conformance/native-parity.tsv` becomes
   `conformance/v1/native-parity.tsv`, `stdlib/` becomes `stdlib-v1/` (the
   old `vibra-types` embeds it by path with `include_bytes!`,
   `crates/vibra-types/src/stdlib.rs:64`), `examples/` becomes
   `examples-v1/`, and `fuzz/` becomes `fuzz-v1/`. The nine other crates,
   `vibra-fs-scope` among them, keep their names. CI runs the renamed
   binaries. Nothing else changes; the diff is mechanical and the tests
   prove it.
2. **New crates carry the final names.** Each is created by the step that
   first needs it (see the crate map) and depends only on new crates plus
   the crates the map marks kept. The new `vibra-conformance` owns the
   architecture test for the whole workspace, old and new rows alike, from
   step 02.
3. **Two corpora, two runners, one CI job.** The v1 runner keeps running
   `conformance/v1/cases` with the v1 handlers. The
   new runner runs `conformance/cases` with the `readerV2`, `staticV2`, and
   `interpreterV2` handlers as they land. Both lines are in the conformance
   job until the cutover.
4. **The cutover is step 28.** It deletes every `-v1` crate, `stdlib-v1/`,
   `examples-v1/`, `fuzz-v1/`, and `conformance/v1/`, removes the v1 lines
   from CI and the v1 rows from the architecture test, and updates the
   repository README and roadmap status lines. Its prerequisites are every
   language step of M1 (01 to 27) and the first PR of package X2-01, which
   moves the native track to the new `vibra-ir`, `vibra-native`, and
   `vibra-host`. Until then the v1 pipeline
   is live and green, and the `vibra` binary, which is new from step 24, is
   the only command surface a user sees.
5. **What the native track needs.** From step 01 the native crates depend
   on `vibra-ir-v1`, `vibra-host-v1`, and `vibra-native-v1`. Step 12
   publishes the new `vibra-ir` with the complete M1 node set and a
   migration table from the old nodes, step 13 the new interpreter, and
   step 27 the new `vibra-native` and `vibra-host`. Package X2-01 then
   moves the native track to them and re-homes the native host tests out
   of `vibra-conformance-v1`; in the same PR the v1 runner loses the
   executor columns of the crates that moved. If X2-01 has not landed
   when every other prerequisite of step 28 has, step 28 does the
   mechanical re-pointing itself and records the tests it had to drop as X2-01's remaining list; it never deletes
   the native crates.
6. **`vibra-native` and `vibra-host` exist twice until the cutover.** The
   `-v1` crates serve the v1 pipeline and, until X2-01, the native track.
   Step 27 creates the crates of the final names on the new `vibra-ir`
   from the `-v1` sources, with behaviour unchanged: the natives are the
   same routines, and the host keeps the major-1 registry rows, with its
   world and audit documents in the data grammar of the [programs
   chapter](../../../spec/04-programs-and-packages.md#vibon-data-documents)
   under the format atoms `@hostWorld.v1` and `@auditTrace.v1`. The M2
   packages N7 to N12 change the new `vibra-host` to registry major 2.
   The new interpreter has no host dependency in M1, because the pure
   language performs no operation.

### The three ignored tests

Nothing is red at any point. Three host tests of the v0.0.1 conformance
crate are `#[ignore]`d, because they compare the specification with code
that implements v0.0.1:

| Ignored test | What happens to it | Step |
| --- | --- | --- |
| `diagnostic_registry::the_registry_and_the_specification_table_have_the_same_codes` | restored: the test moves to the new `vibra-conformance`, runs unignored against the new `vibra-diagnostics`, and the v1 copy is deleted | 02 |
| `evidence::spec_examples_are_inventoried_and_losslessly_exercised` | deleted with its inventory file; replaced by `spec_examples::every_fenced_block_of_the_specification_is_read`, which runs every fenced block of the specification through the new reader | 08 |
| `native_parity_inventory::a_pending_cell_names_a_package_of_the_plan` | deleted with `vibra-conformance-v1` and `conformance/v1/native-parity.tsv`; the inventory of the new corpus is package X2-04's | 28 |

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
| 27 | yes | the new `vibra-native` and `vibra-host` are additive |
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
| `vibra-ir` | rewritten (step 12; runtime modules in step 27) | The checked IR of the new language: variant types with named fields, no unions, no variadics, guard consequences, `List` instead of `Array`, library roles bound to declared types, two defect traps, the `@type.v2` and `@value.v2` encodings with its own canonical writer. Compiler intrinsic registry major 2, with dotted camelCase wire symbols. Step 27 adds `arena` and the host and native registries. |
| `vibra-types` | rewritten (steps 14 to 22) | Stdlib loader, declaration collection, nominal checks, local inference, bodies, coverage, interfaces, dispatch, associated types, widening, the certified fragment, the seven discharge rules, compile-time evaluation through the interpreter, lowering to checked IR. |
| `vibra-interp` | rewritten (step 13) | Heap activations, tail calls, lazy module values, memory budget, `@value.v2` observation, the compiler intrinsics, guard consequences, an evaluation entry for the checker. The design of v1 (`crates/vibra-interp/src/lib.rs:40`) is carried over; the code is not, because every value kind changes. |
| `vibra-workspace` | rewritten from kept modules (steps 09, 15, 23) | `confined_fs`, `discovery`, `snapshot` are copied and adapted; `project` decodes `@project.v2`; `source_graph`, `semantic`, `test_runner` are rewritten. `query` and `index` are M3's. |
| `vibra-schema` | rewritten (step 24) | Version 2 schemas: command result, diagnostic, span, test item, capability. Query, index, and edit-plan schemas are M3's. |
| `vibra-cli` | rewritten (steps 24 to 26) | `help`, `fmt`, `check`, `test` with the version 2 envelope. Every other command of the [tooling chapter](../../../spec/05-tooling.md#command-surface) is `@command.unavailable` and listed as unsupported in `help`. |
| `vibra-conformance` | rewritten from the kept runner core (step 02) | Corpus loader, `case.toml` manifest with V2 identities and camelCase profile and operation atoms, dispatcher, runner, binary. Handlers arrive with the steps that make them observable. The workspace architecture test lives here from step 02. |
| `vibra-host` | recreated on the new IR (step 27; old → `-v1`, deleted in 28) | The same operations and providers, compiled against the new `vibra-ir`; documents in the data grammar. Not used by the M1 interpreter. The M2 packages N7 to N12 change it. |
| `vibra-native` | recreated on the new IR (step 27; old → `-v1`, deleted in 28) | The same routines against the new registry rows. The M1 interpreter runs Vibra bodies and intrinsics and uses no native implementation; the native track calls these routines. |
| `vibra-fs-scope` | kept, unrenamed, untouched by M1 | The Windows file-scope adapter. It depends on no workspace crate and none depends on it until package N9 puts it behind the filesystem provider in M2. |
| `vibra-exec`, `vibra-lower`, `vibra-nexus`, `vibra-nexus-eval`, `vibra-nexus-bridge`, `vibra-codegen`, `vibra-build`, `vibra-actors` | kept, unrenamed | Moved to the new `vibra-ir`, `vibra-native`, and `vibra-host` by package X2-01, not by M1. |
| every `-v1` crate | deleted (step 28) | With `stdlib-v1/`, `examples-v1/`, `fuzz-v1/`, `conformance/v1/`. |

### Native contract packages

The [native contract](../m0/native-contract.md#7-suggested-package-cut)
cuts the adaptation of the IR, the interpreter, and the host into the
packages N1 to N12 and describes them as edits of the existing crates.
This plan builds `vibra-ir` and `vibra-interp` as new crates instead, so
each package is either absorbed by the M1 steps that write the same
content into the new crates, or stays a package that M2 runs on the new
crates. The table assigns every package once. No N package edits a `-v1`
crate: the old IR and interpreter are never adapted, they are deleted.

| Package | Where the work is done | What is left outside M1 |
| --- | --- | --- |
| N1 IR removals and renames | absorbed: step 12 (the new IR never has the removed nodes and names `List`), step 13 (interpreter), step 27 (`vibra-native`). The current checker is not adapted | the native-track consumers (`vibra-lower`, `vibra-exec`, `vibra-nexus`, `vibra-nexus-bridge`), which X2-01 moves |
| N2 Two defect traps | absorbed: step 02 (registry rows of the new `vibra-diagnostics`), step 12 (the trap type), step 13 (interpreter) | `vibra-lower` and `vibra-exec`, in X2-01 |
| N3 Encodings and identities v2 | absorbed: step 12 (`@type.v2`, `@value.v2`, `seed`, signature records, `ObservedValue`), step 13 (observation), step 15 (the cross-check with the reader) | the host's use of the encodings, in N10; `vibra-nexus-bridge`, in X2-04 |
| N4 Callable kinds, plans, and `await` | split: step 12 defines the `Fn`, `Proc`, and `Intent` types and their encodings, because type expressions and signature records need them in M1 | stays N4 in M2, on the new crates: plan and `await` nodes in the IR, the three value kinds and invocation in the interpreter |
| N5 Guards with consequences and `where … else` | absorbed: step 12 (IR fields, construction results, the tail flag), step 13 (continuations and the `try` path), step 21 (the checker that produces them) | guards on `proc` and their run at `await`, with N4 in M2 |
| N6 Interface values and associated types | absorbed: step 12 (labelled interface value types, the stored substitution), step 13 (unboxing and reboxing), step 19 (the checker's elaboration) | nothing |
| N7 Host registry major 2 | not in M1 | stays N7 in M2, on the new `vibra-ir` and `vibra-host`; it replaces the major-1 rows that step 27 copies |
| N8 Broker, grants, policies, narrowing, dispatch boundary | not in M1 | stays N8 in M2 |
| N9 Windows filesystem adapter | not in M1 | stays N9 in M2; it puts `vibra-fs-scope` behind the provider of the new `vibra-host` |
| N10 Host world v2 and audit trace v2 | not in M1 | stays N10 in M2 |
| N11 Startup and the `ExitCode` entry | not in M1 | stays N11 in M2, on the new `vibra-cli` |
| N12 `TestWorld` | not in M1 | stays N12 in M2, on the test runner of step 23 |

The native track's order follows: X2-01 starts after steps 12, 13, and 27
(its N1 and N2); X2-02 after N4 in M2 and step 21; X2-03 after N7 to N11;
X2-04 after step 15 and N10.

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
pub struct DeclarationId(String);   // the declaration identity of the type chapter: "sha256:…"
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

### What the checked IR validates and what it trusts the checker for

The checked IR is a trust boundary: `CheckedModuleSet::try_new` rejects every
module set an execution could go wrong on, except where the table says the IR
trusts the checker. A trusted property that fails reaches execution as the
defect `@runtime.invalidCheckedProgram`, never as a program outcome. The step
that owns a trusted property tests it at the checker.

| Property | Checked IR | Owner |
| --- | --- | --- |
| Every type is well formed, names a declared type or an interface at its arity, and nests at most `Type::MAX_DEPTH` (64) levels; a type the check of a body works out is held to the same bound | validates | step 12 |
| Every node's slots, arity, and types agree; a `return`, a `try`, and a tail call stand only where they may | validates | step 12 |
| A widening, a call through an interface value, a generic instantiation, a contract call on an open receiver, and a dict key have conformance evidence: a bound, `Self` in a default body, a matching implementation with its bounds, or the structural rule for atoms, tuples, and records | validates | step 12 |
| An implementation supplies every abstract member, supplies no default member, and overlaps no other implementation | validates | step 12 |
| A declared type holds itself only through `List`, `Dict`, a callable, or an interface value | validates | step 12 |
| Every labelled parameter of a function or a closure has one constant default of its type, and a closure value carries its defaults | validates | step 12 |
| A constant fits its type | validates | step 12 |
| A constant or an evaluation's input satisfies every invariant its type carries, and every dict in it holds its keys ascending and distinct | the interpreter checks it where the value comes in: `@runtime.invalidCheckedProgram` for a constant, `@runtime.invalidHostValue` for an input | step 13 |
| Every value of a `match` subject matches an arm, and every arm is reachable | trusts the checker; a value no arm matches is `@runtime.invalidCheckedProgram` | steps 15 and 17 |
| An implementation member's labelled default values and guards are the contract member's under the implementation's substitution | trusts the checker; the IR holds neither for an abstract member | step 19 |
| A bare guard the checker discharges holds | trusts the checker; the guard leaves no node | step 22 |

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
diagnostics, formatted, tree, graph, resolved, types, result, test_run }`.

## 3. The conformance corpus

### The 597 existing cases

They are written in the old surface, the old `(record …)` data notation,
kebab-case codes, and `V1-` identities. They move to `conformance/v1/cases` in
step 01, keep running on the v1 interpreter until step 28, and are deleted
in step 28 with the v1 crates. None is
mechanically translated: no translator exists, writing one is a compatibility
bridge in disguise, and a translated expectation is not an expectation written
from the specification. The rules whose meaning did not change (source-graph
sorting, project discovery, module path rules, diagnostic ordering) are
re-authored by hand under
their `V1-` identity with new-notation inputs, as the [conformance
chapter](../../../spec/07-diagnostics-and-conformance.md#rule-identities-and-example-coverage)
allows; everything else gets a `V2-` identity. The retired identities are listed
in `conformance/rules.tsv`.

### Identity scheme and layout

- A rule identity is `V2-<PREFIX>-<suffix>`, or a kept `V1-<PREFIX>-<suffix>`,
  where the prefix is one of the twenty of the conformance chapter and the
  suffix is lower case with hyphens, as the chapter requires. A case
  identity is its rule identity, or the rule identity with one further
  suffix when a rule has several cases. Identities are directory names,
  not atoms.
- Layout: `conformance/cases/<PREFIX>/<case identity>/case.toml` plus the
  case's inputs and snapshots. One directory level per prefix keeps a
  prefix's cases listable and keeps `git` renames local.
- `conformance/rules.tsv` is the rule index, one row per rule identity:
  `identity`, `status` (`active` or `retired`), `anchor` (the chapter
  anchor). Step 02 writes its header, and each step appends the rows of
  the cases it adds. A removed rule stays as `retired`. The runner rejects
  a case whose rule is not `active`.
- `case.toml` stays the neutral oracle, so the VIBON decoder is never its
  own judge: `id`, `rule`, `profile` (one of the six of the chapter:
  `readerV2`, `staticV2`, `interpreterV2`, `toolingV2`, `nativeV1`,
  `fullV2`), `operation` (one of the ten of the
  chapter: `reader`, `projectDecode`, `sourceGraph`, `resolve`,
  `workspaceCheck`, `format`, `query`, `interpret`, `workspaceRun`,
  `workspaceTest`), `description` with the specification anchor,
  `[inputs]` (`source`, `project`, `data`, `tree`, `hostWorld`), `[expect]`
  with `verdict` (`accepted`, `refuted`, `unknown`, or `unsupported`),
  `[[expect.diagnostics]]` rows (`code`, `level`, `source`, `span`,
  `related`, `notes`, `fixes`), and snapshot paths `tree`, `formatted`,
  `graph`, `resolved`, `types`, `result`, `testRun`. A case passes when its
  observed verdict equals the expected one and every other expectation is
  met.
- Snapshots are canonical VIBON of the formats the chapter lists:
  `@tree.v2`, `@sourceGraph.v1`, `@resolved.v2`, `@types.v2`, `@value.v2`,
  `@testRun.v2`. The runner compares them as decoded values.

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
| 14 | `manifest.vibon` (`@stdlibManifest.v2`), `@std.core` (`Ordering`, `ArithmeticError`, `ConversionError`, `Equatable`, `Ordered` with its defaults, `From`, `TryFrom`), `@std.bool`, `@std.option`, `@std.result`, `@std.iter` (interface and the four adapter types `MappedIter(In Out)`, `FilteredIter(T)`, `SkippedIter(T)`, `TakenIter(T)`), `@std.builtin` (the `external @compiler` declarations of the intrinsic registry, `List` members), the loader | `if`, lookups, `try`, guard predicates, and every `match` on `Bool` need the roles before the first checker step. The modules are parsed and loaded in 14 and checked from 15 on, because the checker checks every prelude module in every graph. |
| 16 | `@std.builtin`: `Dict(K V)` with `entries` and the `From(List({K V}))` implementation; `List.length`, `append`, `concat`, `slice` | dict literals and lookups |
| 19 | `Equatable` and `Ordered` for the integer types, `Char`, `Bool`; `From` and `TryFrom` between integer types | dispatch evidence and guard predicates |
| 20 | `@std.str`, `@std.bytes`, `Str` and `Bytes` members and conversions, `Str.trim`, the `Char` classifiers, float operations, `Iter` implementations for `List`, `Dict`, `Str`, `Bytes`, `Option`, `fold`, the helper `compareLexicographic(T I)` | slice 6 |
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
3. **Specification gaps surface mid-step.** None is known when the plan
   is written. Mitigation: an agent that meets one stops and files it
   rather than choosing; the answer changes the specification, and the
   guide then follows the chapter.
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
7. **Parallel agents touch `Cargo.toml`, CI, and the architecture table.**
   Mitigation: only steps 01, 02, 03, 09, 10, 12, 13, 14, 24, 27, and 28 add
   crates or CI lines; every other step touches only its crates and
   `conformance/cases`.
