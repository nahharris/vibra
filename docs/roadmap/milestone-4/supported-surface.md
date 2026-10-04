# M4 surface inventory

Every public AST variant in `crates/vibra-syntax/src/ast.rs` has one M4
disposition and one owning step. The test
`crates/vibra-conformance/tests/m4_contract_inventory.rs` fails when a variant
is missing, when a row names no step, when a step is outside its stage, and
when an inherited row has no owner. Normative behavior is defined by the linked
chapters in `docs/spec/`, not by this table.

| Disposition | Meaning |
| --- | --- |
| `Lowered` | Supported through M3 in the interpreter. The owning Stage 4A step lowers it to WebAssembly, and its executable cases move from `not lowered` to `matched` in the parity inventory in that step. |
| `Static` | Checked only. It has no run-time form of its own, so no step lowers it; every step's cases cover it where it appears. |
| `Stage 4B` | Reports `@tool.unavailable` through Stage 4A. The owning Stage 4B step implements it in both backends. |

M3's inventory fixed that a valid form is never reclassified as malformed.
Availability shrinks monotonically in M4: each step moves forms from
`@tool.unavailable` to supported with positive and negative cases.

Step 5b's rows are lowered to WebAssembly (exercised by the matched cases of
`conformance/parity.tsv` and by `core_lowering_m4_step5b`): the exceptions inside
them are the forms still named by `NotLowered`, which their notes give to later
steps (`call:tail-direct` and generics to Step 6, `match` to Step 7, `array` and
`dict` to Step 8b, interface values to Step 9).

| AST variant | Disposition | Owner | Notes |
| --- | --- | --- | --- |
| `ExpressionKind::Literal` | Lowered | Step 5a | Scalars, `void`, and the `atom`, `str`, and `bytes` literals, which Step 5a builds as arena objects from passive data segments so the accessors have emitted values to read; their operations follow in Step 8b |
| `ExpressionKind::Name` | Lowered | Step 5b | Locals, module values, and function values as names: Step 6 |
| `ExpressionKind::Application` | Lowered | Step 5b | Constructors, projection, and direct calls: Step 5b; calls through function values, `types:`, and tail calls: Step 6; lookups, `array.of`, and `dict.of`: Step 8b; contract-member calls: Step 9 |
| `ExpressionKind::Lambda` | Lowered | Step 6 | Closures, captures, and generic lambdas |
| `ExpressionKind::Do` | Lowered | Step 5b | Body sequences |
| `ExpressionKind::Let` | Lowered | Step 5b | Binder patterns; destructuring: Step 7 |
| `ExpressionKind::LetElse` | Lowered | Step 7 | Needs the pattern matcher |
| `ExpressionKind::Return` | Lowered | Step 5b | The operand is a tail call: Step 6 |
| `ExpressionKind::If` | Lowered | Step 5b | — |
| `ExpressionKind::Match` | Lowered | Step 7 | Every pattern kind and union arms |
| `ExpressionKind::As` | Lowered | Step 7 | Erased in expression position; the `as` pattern narrows a union |
| `ExpressionKind::Try` | Lowered | Step 7 | `option` and `result` early exit |
| `ExpressionKind::TupleOf` | Lowered | Step 5b | — |
| `ExpressionKind::RecordOf` | Lowered | Step 5b | — |
| `ExpressionKind::EnumOf` | Lowered | Step 5b | — |
| `PatternKind::Binding` | Lowered | Step 5b | Parameters and `let`; destructuring: Step 7 |
| `PatternKind::Literal` | Lowered | Step 7 | — |
| `PatternKind::Atom` | Lowered | Step 7 | — |
| `PatternKind::Constructor` | Lowered | Step 7 | Record, tuple, enum, and wrapper constructors |
| `PatternKind::Tuple` | Lowered | Step 7 | — |
| `PatternKind::RecordOf` | Lowered | Step 7 | — |
| `PatternKind::EnumOf` | Lowered | Step 7 | — |
| `PatternKind::Array` | Lowered | Step 8b | Exact length over arrays |
| `PatternKind::As` | Lowered | Step 7 | Compares the union discriminant |
| `VariadicBinding::Array` | Lowered | Step 8b | — |
| `VariadicBinding::Dict` | Lowered | Step 8b | — |
| `Declaration::Import` | Static | — | Resolved before lowering |
| `Declaration::Def` | Lowered | Step 5b | Lazy, once-only module values |
| `Declaration::Defn` | Lowered | Step 5b | Generic signatures: Step 6 |
| `Declaration::Test` | Lowered | Step 11 | `workspace-test` observations and assertion outcomes |
| `Declaration::Deftype` | Lowered | Step 5b | Generic: Step 6; union discriminants in written order: Step 5b |
| `Declaration::Defint` | Lowered | Step 9 | — |
| `Declaration::Deffect` | Stage 4B | Step 14 | Reports `@tool.unavailable` through Stage 4A |
| `TypeMember::Method` | Lowered | Step 5b | Contract members: Step 9 |
| `TypeMember::Implementation` | Lowered | Step 9 | Static dispatch, interface values, defaults, and destination dispatch |
| `TypeExpr::Name` | Lowered | Step 5b | Interfaces and `any` as types: Step 9 |
| `TypeExpr::Function` | Lowered | Step 6 | — |
| `TypeExpr::Void` | Lowered | Step 5a | — |
| `TypeExpr::Applied` | Lowered | Step 6 | Type arguments pass at run time |
| `TypeExpr::Tuple` | Lowered | Step 5b | — |
| `TypeExpr::Record` | Lowered | Step 5b | — |
| `TypeExpr::Enum` | Lowered | Step 5b | — |
| `TypeExpr::Union` | Lowered | Step 5b | — |
| `TypeExpr::Array` | Lowered | Step 8b | — |
| `TypeExpr::Dict` | Lowered | Step 8b | Canonical key order |
| `VariadicType::Array` | Lowered | Step 8b | — |
| `VariadicType::Dict` | Lowered | Step 8b | — |
| `DeftypeBody::Type` | Lowered | Step 5b | — |
| `DeftypeBody::Intrinsic` | Lowered | Step 8a | Builtin numeric types: Step 8a; `array` and `dict`: Step 8b |
| `Attribute::Where` | Lowered | Step 6 | Bounds select implementations: Step 9 |
| `Attribute::Labelled` | Static | — | Resolved into parameter order before lowering |
| `Attribute::Variadic` | Lowered | Step 8b | — |
| `Attribute::Visibility` | Static | — | — |
| `Attribute::Effects` | Stage 4B | Step 14 | Empty rows stay supported |
| `Attribute::External` | Lowered | Step 8a | `@compiler` rows: Steps 8a–8c; `@host`: Stage 4B Step 16 |
| `Attribute::Symbol` | Static | — | — |
| `Attribute::Native` | Lowered | Step 10 | The body is lowered; the Wasm backend has no native |
| `Attribute::Role` | Static | — | — |
| `Attribute::Doc` | Static | — | — |

## Inherited rows

The M4 [deferral inventory](README.md#m3-deferral-inventory) lists nine rows
that earlier milestones left to M4. Each has one owner here, and the test
requires all nine. Rows I3–I6 are implemented by Step 2 in the type checker,
the typed IR, and the reference interpreter, conditional on its pull request
merging, and report `@tool.unavailable` no longer. Their Wasm lowering stays
Step 9's.

| ID | Inherited row | Owner |
| --- | --- | --- |
| I1 | `Declaration::Deffect`; nonempty `Attribute::Effects`, including on a function type and an `iter` default callback | Step 14 |
| I2 | `Attribute::External` with `@host` | Step 16 |
| I3 | An abstract contract member with its own generic parameters | Step 2; implemented in Step 2, Wasm in Step 9 |
| I4 | Labelled operands and written `types:` arguments on a contract member call | Step 2; implemented in Step 2, Wasm in Step 9 |
| I5 | A dict variadic tail on a contract member | Step 2; implemented in Step 2, Wasm in Step 9 |
| I6 | A contract member with its own generics or labelled parameters as a function value | Step 2; implemented in Step 2, Wasm in Step 9 |
| I7 | M3's G21 (G16 here): the command result for an entry that returns `err` | Step 19 |
| I8 | Pre-M4 C16: effect rows of a `let` value, a `let-else` fallback, and a `return` operand | Step 14 |
| I9 | The conformance chapter's effectful walk over `iter.next` | Step 16 |
