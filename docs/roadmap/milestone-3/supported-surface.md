# M3 surface inventory

Every public AST variant in `crates/vibra-syntax/src/ast.rs` has one M3
disposition and one owning step. The test
`crates/vibra-conformance/tests/m3_contract_inventory.rs` fails when a variant
is missing, when a row names no step, or when an M3-owned row is left without
one. `M2` means the form is already supported and M3 only widens the types it
accepts. `M4` rows stay `@tool.unavailable` through M3. Normative behavior is
defined by the linked chapters in `docs/spec/`, not by this table.

| AST variant | Disposition | Owner | Notes |
| --- | --- | --- | --- |
| `ExpressionKind::Literal` | M2 | — | Atom singleton types: Step 6 (`V1-TYPE-INFER-primitives`, `V1-TYPE-CONVERT-widening-rejections`) |
| `ExpressionKind::Name` | M2 | — | Nominal constructors and methods as names: Step 2 |
| `ExpressionKind::Application` | M2 | — | Constructors and projection: Step 2; lookups, `tupleof`, `array.of`, and `map.of`: Step 4; `recordof` and `enumof`: Step 2; `types:`: Step 3 |
| `ExpressionKind::Lambda` | M2 | — | Destructuring parameters: Step 5 (`V1-RUNTIME-match-patterns`, `V1-TYPE-CONTROL-pattern-refutable-binding`) |
| `ExpressionKind::Do` | M2 | — | Unhandled-fallible positions: Step 7 (`V1-TYPE-CONTROL-unhandled-fallible`, `V1-TYPE-CONTROL-fallible-discards`) |
| `ExpressionKind::Let` | M2 | — | Destructuring patterns: Step 5 (`V1-RUNTIME-match-patterns`, `V1-RUNTIME-workspace-test-patterns`) |
| `ExpressionKind::If` | M2 | — | — |
| `ExpressionKind::Match` | Stage 3A | Step 5 | Cases `V1-RUNTIME-match-patterns`, `V1-TYPE-CONTROL-match-exhaustive`, `V1-TYPE-CONTROL-match-non-exhaustive`, `V1-TYPE-CONTROL-match-unreachable-arm`; union arms: Step 6 (`V1-TYPE-CONVERT-narrowing-exhaustive`, `V1-TYPE-CONVERT-narrowing-rejections`) |
| `ExpressionKind::As` | Stage 3A | Step 6 | Cases `V1-TYPE-CONVERT-ascription-erased`, `V1-TYPE-CONVERT-invalid-ascription`, `V1-RUNTIME-union-values`; interface targets: Step 12 |
| `ExpressionKind::Try` | Stage 3A | Step 7 | Cases `V1-RUNTIME-try-propagation`, `V1-TYPE-CONTROL-invalid-try`, `V1-TYPE-CONTROL-invalid-try-test-body`; host test `try_m3_step7` |
| `ExpressionKind::TupleOf` | Stage 3A | Step 4 | Reader and formatter: Step 2; case `V1-RUNTIME-tuples` |
| `ExpressionKind::RecordOf` | Stage 3A | Step 2 | — |
| `ExpressionKind::EnumOf` | Stage 3A | Step 2 | — |
| `PatternKind::Binding` | M2 | — | Binder rules in patterns: Step 5 (`V1-TYPE-CONTROL-pattern-redeclaration`) |
| `PatternKind::Literal` | Stage 3A | Step 5 | Not floats or `void`; cases `V1-TYPE-CONTROL-match-exhaustive`, `V1-TYPE-CONTROL-pattern-type-mismatch` |
| `PatternKind::Atom` | Stage 3A | Step 5 | Atom scrutinees are never closed; cases `V1-RUNTIME-match-patterns`, `V1-TYPE-CONTROL-match-non-exhaustive` |
| `PatternKind::Constructor` | Stage 3A | Step 5 | Record, tuple, enum, and wrapper constructors (`V1-RUNTIME-match-patterns`, `V1-RUNTIME-workspace-test-patterns`); a union narrows only through `as` |
| `PatternKind::Tuple` | Stage 3A | Step 5 | Spelled `(tupleof …)` from Step 2; case `V1-RUNTIME-match-patterns` |
| `PatternKind::RecordOf` | Stage 3A | Step 5 | Reader and formatter: Step 2; case `V1-RUNTIME-match-patterns` |
| `PatternKind::EnumOf` | Stage 3A | Step 5 | Reader and formatter: Step 2; case `V1-RUNTIME-match-patterns` |
| `PatternKind::Array` | Stage 3A | Step 5 | Exact length; case `V1-RUNTIME-match-patterns` |
| `PatternKind::As` | Stage 3A | Step 6 | Cases `V1-TYPE-CONVERT-narrowing-exhaustive`, `V1-TYPE-CONVERT-narrowing-rejections`, `V1-RUNTIME-union-values` |
| `VariadicBinding::Array` | Stage 3A | Step 4 | Cases `V1-RUNTIME-variadics`, `V1-SRC-CALLS-variadic-array-application` |
| `VariadicBinding::Map` | Stage 3A | Step 4 | Cases `V1-RUNTIME-map-order`, `V1-SRC-CALLS-variadic-map-application` |
| `Declaration::Import` | M2 | — | Standard-library input replacement: Step 4 (`stdlib::tests`, `V1-PROJECT-workspace-check-bootstrap-source-id-collision`); declaration imports: Step 4b (`V1-PROJECT-workspace-check-declaration-imports`) |
| `Declaration::Def` | M2 | — | — |
| `Declaration::Defn` | M2 | — | Generic signatures: Step 3 |
| `Declaration::Test` | M2 | — | Generic assertion: Step 8 |
| `Declaration::Deftype` | Stage 3A | Step 2 | Unions: Step 6 (`V1-TYPE-NOMINAL-union-declarations`); generic: Step 3 |
| `Declaration::Defint` | Stage 3B | Step 11 | — |
| `Declaration::Deffect` | M4 | — | — |
| `TypeMember::Method` | Stage 3A | Step 2 | Non-interface methods; contract members: Step 11 |
| `TypeMember::Implementation` | Stage 3B | Step 11 | — |
| `TypeExpr::Name` | M2 | — | Nominal names: Step 2; `any` and interfaces as types: Step 12 |
| `TypeExpr::Function` | M2 | — | Generic and variadic function types: Steps 3–4 |
| `TypeExpr::Void` | M2 | — | — |
| `TypeExpr::Applied` | Stage 3A | Step 3 | Cases `V1-TYPE-GENERIC-applied-types`, `V1-RUNTIME-generic-types` |
| `TypeExpr::Tuple` | Stage 3A | Step 4 | Case `V1-RUNTIME-tuples` |
| `TypeExpr::Record` | Stage 3A | Step 2 | — |
| `TypeExpr::Enum` | Stage 3A | Step 2 | — |
| `TypeExpr::Union` | Stage 3A | Step 6 | Reader and formatter: Step 2; cases `V1-TYPE-NOMINAL-union-declarations`, `V1-TYPE-NOMINAL-union-member-overlap`, `V1-TYPE-NOMINAL-union-member-not-concrete` |
| `TypeExpr::Array` | Stage 3A | Step 4 | Cases `V1-RUNTIME-lookups`, `V1-TYPE-NOMINAL-collection-construction` |
| `TypeExpr::Map` | Stage 3A | Step 4 | Cases `V1-RUNTIME-map-order`, `V1-TYPE-NOMINAL-map-keys`; generic key types: Step 11 |
| `VariadicType::Array` | Stage 3A | Step 4 | Case `V1-SRC-CALLS-variadic-array-type` |
| `VariadicType::Map` | Stage 3A | Step 4 | Case `V1-SRC-CALLS-variadic-map-type` |
| `DeftypeBody::Type` | Stage 3A | Step 2 | Declared record, enum, and wrapper bodies: Step 2; tuple: Step 4; union: Step 6 (`V1-TYPE-NOMINAL-union-declarations`, `V1-RUNTIME-union-values`) |
| `DeftypeBody::Intrinsic` | Stage 3A | Step 4 | Reader: Step 2; standard-library declarations: Steps 4 and 8 |
| `Attribute::Where` | Stage 3A | Step 3 | `any` bounds only (`V1-RUNTIME-generic-functions`, `V1-RUNTIME-generic-lambda`); interface bounds: Step 11 (`V1-TYPE-GENERIC-interface-bound`) |
| `Attribute::Labelled` | M2 | — | — |
| `Attribute::Variadic` | Stage 3A | Step 4 | Cases `V1-RUNTIME-variadics`, `V1-PROJECT-workspace-check-variadic-applications` |
| `Attribute::Visibility` | M2 | — | — |
| `Attribute::Effects` | M4 | — | Empty rows stay supported |
| `Attribute::External` | M2 | — | `@compiler` only; `@host`: M4 |
| `Attribute::Symbol` | M2 | — | — |
| `Attribute::Native` | Stage 3A | Step 4b | Embedded standard library only; `natives_m3_step4b`, `V1-TYPE-NAMES-library-attributes` |
| `Attribute::Role` | Stage 3A | Step 4b | Embedded standard library only; `V1-TYPE-NAMES-role-vocabulary`, `V1-TYPE-NAMES-library-attributes` |
| `Attribute::Doc` | M2 | — | — |

## M2 ledger rows deferred to M3

| M2 row | Owner |
| --- | --- |
| C1.3 — generics, nominal collections, interfaces, conversion | Steps 2–4, 11–13; `any`-bounded generics implemented by Step 3 (`V1-RUNTIME-generic-functions`) |
| C1.5 — `match`, `try`, `option`, `result`, refutable patterns | Steps 4, 5, 7; `match` and refutable patterns implemented by Step 5 (`V1-TYPE-CONTROL-match-non-exhaustive`, `V1-TYPE-CONTROL-pattern-refutable-binding`); `try` and `result` implemented by Step 7 (`V1-RUNTIME-try-propagation`, `V1-TYPE-CONTROL-unhandled-fallible`) |
| C1.6 — variadic array and map operands | Step 4; implemented (`V1-RUNTIME-variadics`, `V1-SRC-CALLS-variadic-map-application`) |
| C1.7 — `as`, singleton widening, narrowing | Step 6; implemented (`V1-TYPE-CONVERT-widening-boundaries`, `V1-TYPE-CONVERT-narrowing-rejections`) |
| C5.2 — constructor and destructuring patterns | Step 5; implemented (`V1-RUNTIME-match-patterns`, `V1-RUNTIME-workspace-test-patterns`) |
| C6.2 — `types:` generic arguments | Step 3; implemented (`V1-TYPE-GENERIC-type-arguments`, `V1-TOOL-format-types-order`) |
| Unavailable `integer.*` compiler symbols (M2 registry note) | Step 8, as static methods of the numeric primitive types |
