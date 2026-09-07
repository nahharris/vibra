# M2 supported and deferred surface inventory

This inventory is derived from the public enum variants in
`crates/vibra-syntax/src/ast.rs` at the M1 base. The Step 1 conformance test
requires every `Enum::Variant` token below to remain present in this document;
adding an AST variant without a disposition fails the test. `supported` means
the M2 checker/interpreter will implement it by the owning step. `deferred`
means M1 may parse it, but M2 reports `@tool.unavailable` when semantic support
is required. `rejected` means the active v1 grammar itself rejects it.

| AST variant | Disposition | Owner / reason |
| --- | --- | --- |
| `ExpressionKind::Literal` | supported | Steps 5–8 primitive values |
| `ExpressionKind::Name` | supported | Steps 4–7 resolved names and values |
| `ExpressionKind::Application` | supported | Step 6 fixed positional calls; Step 7 labels and callable values |
| `ExpressionKind::Lambda` | supported | Step 7 closures |
| `ExpressionKind::Do` | supported | Step 6 sequencing |
| `ExpressionKind::Let` | supported | Step 6 direct local bindings |
| `ExpressionKind::If` | supported | Step 6 boolean branching |
| `ExpressionKind::Match` | deferred | M3 exhaustive patterns |
| `ExpressionKind::As` | deferred | M3 explicit widening/narrowing |
| `ExpressionKind::Try` | deferred | M3 typed failure |
| `PatternKind::Binding` | supported | Step 6 direct local names/discards |
| `PatternKind::Literal` | deferred | M3 pattern matching |
| `PatternKind::Constructor` | deferred | M3 nominal constructors |
| `PatternKind::Tuple` | deferred | M3 destructuring |
| `PatternKind::Array` | deferred | M3 collections |
| `PatternKind::As` | deferred | M3 union narrowing |
| `VariadicBinding::Array` | deferred | M3 collection variadics |
| `VariadicBinding::Map` | deferred | M3 collection variadics |
| `Declaration::Import` | supported | Steps 3–4 module graph |
| `Declaration::Def` | supported | Step 6 immutable module values |
| `Declaration::Defn` | supported | Steps 5–9 functions |
| `Declaration::Test` | supported | Step 13 pure tests |
| `Declaration::Deftype` | deferred | M3 nominal types |
| `Declaration::Defint` | deferred | M3 interfaces |
| `Declaration::Deffect` | deferred | M4 effects |
| `TypeMember::Method` | deferred | M3 methods |
| `TypeMember::Implementation` | deferred | M3 implementations |
| `TypeExpr::Name` | supported | Step 5 primitive names and Step 4 IDs |
| `TypeExpr::Function` | supported | Step 7 monomorphic `fn` values |
| `TypeExpr::Void` | supported | Step 5 |
| `TypeExpr::Applied` | deferred | M3 nominal/generic applications |
| `TypeExpr::Tuple` | deferred | M3 products |
| `TypeExpr::Array` | deferred | M3 collections |
| `TypeExpr::Map` | deferred | M3 collections |
| `VariadicType::Array` | deferred | M3 |
| `VariadicType::Map` | deferred | M3 |
| `DeftypeBody::Type` | deferred | M3 nominal declarations |
| `DeftypeBody::Record` | deferred | M3 |
| `DeftypeBody::Enum` | deferred | M3 |
| `DeftypeBody::Union` | deferred | M3 |
| `DeftypeBody::Newtype` | deferred | M3 |
| `Attribute::Where` | deferred | M3 generics |
| `Attribute::Labelled` | supported | Step 7 literal-default labels |
| `Attribute::Variadic` | deferred | M3 |
| `Attribute::Visibility` | supported | Step 4/7 |
| `Attribute::Effects` | supported-empty-only | Step 1 admission; nonempty M4 |
| `Attribute::External` | supported-compiler-only | Step 8; `@host` M4 |
| `Attribute::Symbol` | supported-compiler-only | Step 8 |
| `Attribute::Doc` | supported-preserved | Steps 2/11 formatting |

The inventory deliberately classifies syntax that M1 already parses. It does
not add a source dialect or reinterpret a deferred form as a generic call.
Retired heads (`while`, `for`, `break`, `continue`, `return`, `bind`, `case`)
remain `@syntax.retired-form` as specified.
