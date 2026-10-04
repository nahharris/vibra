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

The rows of Steps 5b, 6, and 7 are lowered to WebAssembly (exercised by the
matched cases of `conformance/parity.tsv`, by `core_lowering_m4_step5b`,
`calls_m4_step6`, and `patterns_m4_step7`): the exceptions inside them are the
forms still named by `NotLowered`, which their notes give to later steps
(`array`, `dict`, a variadic tail, the array pattern, and a wrapper over `str`
or `bytes` to Step 8b, interface values and contract calls to Step 9). Step 6
lowers function values, `lambda` and its captures, calls through function
values, a tail call to every kind of callee, omitted labelled operands, and
generic functions and types, so no form of a call is named by `NotLowered` but a
contract call. Step 7 lowers `match` with every pattern kind, which also serves
a destructuring `let`, parameter, and `lambda` parameter and `let-else`, since
typed IR writes each of them as a `match`; `as` narrowing; `try`; and `never`.
The array pattern is the one form of control flow or failure `NotLowered` still
names.

| AST variant | Disposition | Owner | Notes |
| --- | --- | --- | --- |
| `ExpressionKind::Literal` | Lowered | Step 5a | Scalars, `void`, and the `atom`, `str`, and `bytes` literals, which Step 5a builds as arena objects from passive data segments so the accessors have emitted values to read; their operations follow in Step 8b |
| `ExpressionKind::Name` | Lowered | Step 5b | Locals and module values: Step 5b; function values as names (a module function, a labelled default carried): Step 6 |
| `ExpressionKind::Application` | Lowered | Step 5b | Constructors, projection, and direct calls: Step 5b; calls through function values, `types:`, and tail calls to every kind of callee: Step 6; lookups, `array.of`, and `dict.of`: Step 8b; contract-member calls: Step 9 |
| `ExpressionKind::Lambda` | Lowered | Step 6 | Closures, captures, and generic lambdas (landed: `calls_m4_step6`) |
| `ExpressionKind::Do` | Lowered | Step 5b | Body sequences |
| `ExpressionKind::Let` | Lowered | Step 5b | Binder patterns; destructuring is a one-arm `match`: Step 7 (landed) |
| `ExpressionKind::LetElse` | Lowered | Step 7 | Typed IR writes it as a two-arm `match` with a wildcard fallback of type `never` (landed: `patterns_m4_step7`, `V1-RUNTIME-let-else`) |
| `ExpressionKind::Return` | Lowered | Step 5b | The operand is a tail call: Step 6 (landed) |
| `ExpressionKind::If` | Lowered | Step 5b | — |
| `ExpressionKind::Match` | Lowered | Step 7 | Every pattern kind and union arms, in the order of the arms; the array pattern stays with Step 8b (landed: `patterns_m4_step7`) |
| `ExpressionKind::As` | Lowered | Step 7 | Erased in expression position, as the widening typed IR already carries; the `as` pattern narrows a union (landed) |
| `ExpressionKind::Try` | Lowered | Step 7 | `option` and `result` early exit (landed: `patterns_m4_step7`, `V1-RUNTIME-try-and-never`) |
| `ExpressionKind::TupleOf` | Lowered | Step 5b | — |
| `ExpressionKind::RecordOf` | Lowered | Step 5b | — |
| `ExpressionKind::EnumOf` | Lowered | Step 5b | — |
| `PatternKind::Binding` | Lowered | Step 5b | Parameters and `let`; destructuring: Step 7 (landed) |
| `PatternKind::Literal` | Lowered | Step 7 | A scalar of every width, a `char`, a `str`, and an atom; `bool` and a constant name read as the pattern of their value (landed: `V1-RUNTIME-literal-patterns`, `V1-RUNTIME-constant-patterns`) |
| `PatternKind::Atom` | Lowered | Step 7 | An atom literal (landed: `V1-RUNTIME-literal-patterns`) |
| `PatternKind::Constructor` | Lowered | Step 7 | Record, tuple, enum, and wrapper constructors; a wrapper over `str` or `bytes` is `wrap`, Step 8b (landed: `V1-RUNTIME-constructor-patterns`) |
| `PatternKind::Tuple` | Lowered | Step 7 | (landed: `V1-RUNTIME-constructor-patterns`) |
| `PatternKind::RecordOf` | Lowered | Step 7 | (landed: `V1-RUNTIME-constructor-patterns`) |
| `PatternKind::EnumOf` | Lowered | Step 7 | (landed: `V1-RUNTIME-constructor-patterns`) |
| `PatternKind::Array` | Lowered | Step 8b | Exact length over arrays |
| `PatternKind::As` | Lowered | Step 7 | Compares the union discriminant in written member order (landed: `V1-RUNTIME-union-narrowing`, `V1-RUNTIME-union-atom-member`) |
| `VariadicBinding::Array` | Lowered | Step 8b | — |
| `VariadicBinding::Dict` | Lowered | Step 8b | — |
| `Declaration::Import` | Static | — | Resolved before lowering |
| `Declaration::Def` | Lowered | Step 5b | Lazy, once-only module values |
| `Declaration::Defn` | Lowered | Step 5b | Generic signatures: Step 6 (landed); a variadic parameter: Step 8b |
| `Declaration::Test` | Lowered | Step 11 | `workspace-test` observations and assertion outcomes |
| `Declaration::Deftype` | Lowered | Step 5b | Generic: Step 6 (landed); union discriminants in written order: Step 5b |
| `Declaration::Defint` | Lowered | Step 9 | — |
| `Declaration::Deffect` | Stage 4B | Step 14 | Reports `@tool.unavailable` through Stage 4A |
| `TypeMember::Method` | Lowered | Step 5b | Contract members: Step 9 |
| `TypeMember::Implementation` | Lowered | Step 9 | Static dispatch, interface values, defaults, and destination dispatch |
| `TypeExpr::Name` | Lowered | Step 5b | Interfaces and `any` as types: Step 9 |
| `TypeExpr::Function` | Lowered | Step 6 | A function value is an arena value of kind `function` |
| `TypeExpr::Void` | Lowered | Step 5a | — |
| `TypeExpr::Applied` | Lowered | Step 6 | Type arguments pass at run time, as descriptors in the frame of each generic activation |
| `TypeExpr::Tuple` | Lowered | Step 5b | — |
| `TypeExpr::Record` | Lowered | Step 5b | — |
| `TypeExpr::Enum` | Lowered | Step 5b | — |
| `TypeExpr::Union` | Lowered | Step 5b | — |
| `TypeExpr::Array` | Lowered | Step 8b | — |
| `TypeExpr::Dict` | Lowered | Step 8b | Canonical key order |
| `VariadicType::Array` | Lowered | Step 8b | — |
| `VariadicType::Dict` | Lowered | Step 8b | — |
| `DeftypeBody::Type` | Lowered | Step 5b | — |
| `DeftypeBody::Intrinsic` | Lowered | Step 8a | Builtin numeric types: Step 8a (landed: the integer rows but `to-str` and `parse`, the integer conversions, and the `char` rows, `primitives_m4_step8a`; the integer `to-str` and `parse` and the float rows: Step 8c); `array` and `dict`: Step 8b |
| `Attribute::Where` | Lowered | Step 6 | Generic parameters take run-time type arguments; bounds select implementations: Step 9 |
| `Attribute::Labelled` | Static | — | Resolved into parameter order before lowering |
| `Attribute::Variadic` | Lowered | Step 8b | — |
| `Attribute::Visibility` | Static | — | — |
| `Attribute::Effects` | Stage 4B | Step 14 | Empty rows stay supported |
| `Attribute::External` | Lowered | Step 8a | `@compiler` rows: Steps 8a–8c (8a landed: the integer arithmetic, shift, comparison, and conversion rows and the `char` rows, lowered inline); `@host`: Stage 4B Step 16 |
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
