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
| `ExpressionKind::Literal` | M2 | — | Atom singleton types: Step 6 |
| `ExpressionKind::Name` | M2 | — | Nominal constructors and methods as names: Step 2 |
| `ExpressionKind::Application` | M2 | — | Constructors and projection: Step 2; lookups, `tupleof`, `array.of`, and `map.of`: Step 4; `recordof` and `enumof`: Step 2; `types:`: Step 3 |
| `ExpressionKind::Lambda` | M2 | — | Destructuring parameters: Step 5 |
| `ExpressionKind::Do` | M2 | — | Unhandled-fallible positions: Step 7 |
| `ExpressionKind::Let` | M2 | — | Destructuring patterns: Step 5 |
| `ExpressionKind::If` | M2 | — | — |
| `ExpressionKind::Match` | Stage 3A | Step 5 | Union arms: Step 6 |
| `ExpressionKind::As` | Stage 3A | Step 6 | Interface targets: Step 12 |
| `ExpressionKind::Try` | Stage 3A | Step 7 | — |
| `ExpressionKind::TupleOf` | Stage 3A | Step 4 | Reader and formatter: Step 2 |
| `ExpressionKind::RecordOf` | Stage 3A | Step 2 | — |
| `ExpressionKind::EnumOf` | Stage 3A | Step 2 | — |
| `PatternKind::Binding` | M2 | — | — |
| `PatternKind::Literal` | Stage 3A | Step 5 | — |
| `PatternKind::Constructor` | Stage 3A | Step 5 | Record, tuple, enum, union, and wrapper constructors |
| `PatternKind::Tuple` | Stage 3A | Step 5 | Spelled `(tupleof …)` from Step 2 |
| `PatternKind::RecordOf` | Stage 3A | Step 5 | Reader and formatter: Step 2 |
| `PatternKind::EnumOf` | Stage 3A | Step 5 | Reader and formatter: Step 2 |
| `PatternKind::Array` | Stage 3A | Step 5 | — |
| `PatternKind::As` | Stage 3A | Step 6 | — |
| `VariadicBinding::Array` | Stage 3A | Step 4 | — |
| `VariadicBinding::Map` | Stage 3A | Step 4 | — |
| `Declaration::Import` | M2 | — | Standard-library input replacement: Step 4 |
| `Declaration::Def` | M2 | — | — |
| `Declaration::Defn` | M2 | — | Generic signatures: Step 3 |
| `Declaration::Test` | M2 | — | Generic assertion: Step 8 |
| `Declaration::Deftype` | Stage 3A | Step 2 | Unions: Step 6; generic: Step 3 |
| `Declaration::Defint` | Stage 3B | Step 11 | — |
| `Declaration::Deffect` | M4 | — | — |
| `TypeMember::Method` | Stage 3A | Step 2 | Non-interface methods; contract members: Step 11 |
| `TypeMember::Implementation` | Stage 3B | Step 11 | — |
| `TypeExpr::Name` | M2 | — | Nominal names: Step 2; `any` and interfaces as types: Step 12 |
| `TypeExpr::Function` | M2 | — | Generic and variadic function types: Steps 3–4 |
| `TypeExpr::Void` | M2 | — | — |
| `TypeExpr::Applied` | Stage 3A | Step 3 | Cases `V1-TYPE-GENERIC-applied-types`, `V1-RUNTIME-generic-types` |
| `TypeExpr::Tuple` | Stage 3A | Step 4 | — |
| `TypeExpr::Record` | Stage 3A | Step 2 | — |
| `TypeExpr::Enum` | Stage 3A | Step 2 | — |
| `TypeExpr::Union` | Stage 3A | Step 6 | Reader and formatter: Step 2 |
| `TypeExpr::Array` | Stage 3A | Step 4 | — |
| `TypeExpr::Map` | Stage 3A | Step 4 | Generic key types: Step 11 |
| `VariadicType::Array` | Stage 3A | Step 4 | — |
| `VariadicType::Map` | Stage 3A | Step 4 | — |
| `DeftypeBody::Type` | Stage 3A | Step 2 | Declared record, enum, and wrapper bodies: Step 2; tuple: Step 4; union: Step 6 |
| `DeftypeBody::Intrinsic` | Stage 3A | Step 4 | Reader: Step 2; standard-library declarations: Steps 4 and 8 |
| `Attribute::Where` | Stage 3A | Step 3 | `any` bounds only (`V1-RUNTIME-generic-functions`, `V1-RUNTIME-generic-lambda`); interface bounds: Step 11 (`V1-TYPE-GENERIC-interface-bound`) |
| `Attribute::Labelled` | M2 | — | — |
| `Attribute::Variadic` | Stage 3A | Step 4 | — |
| `Attribute::Visibility` | M2 | — | — |
| `Attribute::Effects` | M4 | — | Empty rows stay supported |
| `Attribute::External` | M2 | — | `@compiler` only; `@host`: M4 |
| `Attribute::Symbol` | M2 | — | — |
| `Attribute::Doc` | M2 | — | — |

## M2 ledger rows deferred to M3

| M2 row | Owner |
| --- | --- |
| C1.3 — generics, nominal collections, interfaces, conversion | Steps 2–4, 11–13; `any`-bounded generics implemented by Step 3 (`V1-RUNTIME-generic-functions`) |
| C1.5 — `match`, `try`, `option`, `result`, refutable patterns | Steps 4, 5, 7 |
| C1.6 — variadic array and map operands | Step 4 |
| C1.7 — `as`, singleton widening, narrowing | Step 6 |
| C5.2 — constructor and destructuring patterns | Step 5 |
| C6.2 — `types:` generic arguments | Step 3; implemented (`V1-TYPE-GENERIC-type-arguments`, `V1-TOOL-format-types-order`) |
| Unavailable `integer.*` compiler symbols (M2 registry note) | Step 8, as static methods of the numeric primitive types |
