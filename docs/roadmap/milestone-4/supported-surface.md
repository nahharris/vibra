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
(Step 9 lowered interface values, contract calls, and a dict whose key is dispatched from a run-time type; Step 8b lowered `array`, `dict`, a variadic tail, the array pattern, and a wrapper over `str` or `bytes`).
Step 6
lowers function values, `lambda` and its captures, calls through function
values, a tail call to every kind of callee, omitted labelled operands, and
generic functions and types, so no form of a call is named by `NotLowered` but a
contract call. Step 7 lowers `match` with every pattern kind, which also serves
a destructuring `let`, parameter, and `lambda` parameter and `let-else`, since
typed IR writes each of them as a `match`; `as` narrowing; `try`; and `never`.
Since Step 11 no form of a checked program is named by `NotLowered`; it reports only
a program the checker cannot have produced (`lowers_every_form_m4_step11`).

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
| `PatternKind::Array` | Lowered | Step 8b | Exact length over arrays (landed: `collections_m4_step8b`, `V1-RUNTIME-match-patterns`) |
| `PatternKind::As` | Lowered | Step 7 | Compares the union discriminant in written member order (landed: `V1-RUNTIME-union-narrowing`, `V1-RUNTIME-union-atom-member`) |
| `VariadicBinding::Array` | Lowered | Step 8b | One more slot, filled with the packed array (landed: `V1-RUNTIME-variadics`) |
| `VariadicBinding::Dict` | Lowered | Step 8b | One more slot, filled with the built dict (landed) |
| `Declaration::Import` | Static | — | Resolved before lowering |
| `Declaration::Def` | Lowered | Step 5b | Lazy, once-only module values |
| `Declaration::Defn` | Lowered | Step 5b | Generic signatures: Step 6 (landed); a variadic parameter: Step 8b |
| `Declaration::Test` | Lowered | Step 11 | `workspace-test` observations and assertion outcomes (landed: a test module with `vibra_v1_test`, the three assertions, canonical equality, and the failure exports, `tests_traps_m4_step11`) |
| `Declaration::Deftype` | Lowered | Step 5b | Generic: Step 6 (landed); union discriminants in written order: Step 5b |
| `Declaration::Defint` | Lowered | Step 9 | — |
| `Declaration::Deffect` | Stage 4B | Step 14 | Landed with Step 14, conditional on its pull request merging: a root and its operations with Vibra bodies in both checkers, the typed IR, and both backends, where an operation lowers as an ordinary function and applying it is a call (landed: `effects_m4_step14`, `V1-RUNTIME-effect-operations`, `V1-RUNTIME-workspace-run-effect-operations`). An operation with `external: @host` is checked against the host registry since Step 16b-2 (landed: `@std.io` in 16b-2, `@std.fs` in Step 17) |
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
| `TypeExpr::Array` | Lowered | Step 8b | An arena value of kind `array` (landed) |
| `TypeExpr::Dict` | Lowered | Step 8b | Canonical key order, or a declared key's own `compare` (landed; a key dispatched from a run-time type is Step 9) |
| `VariadicType::Array` | Lowered | Step 8b | (landed) |
| `VariadicType::Dict` | Lowered | Step 8b | (landed) |
| `DeftypeBody::Type` | Lowered | Step 5b | — |
| `DeftypeBody::Intrinsic` | Lowered | Step 8a | Builtin numeric types: Step 8a (landed: the integer rows but `to-str` and `parse`, the integer conversions, and the `char` rows, `primitives_m4_step8a`; Step 8c landed the rest: the float arithmetic, equality, and order rows inline, and `to-str` and `parse` of every integer and float type as imports, `numbers_m4_step8c`); `array` and `dict`: Step 8b |
| `Attribute::Where` | Lowered | Step 6 | Generic parameters take run-time type arguments; bounds select implementations: Step 9 |
| `Attribute::Labelled` | Static | — | Resolved into parameter order before lowering |
| `Attribute::Variadic` | Lowered | Step 8b | (landed) |
| `Attribute::Visibility` | Static | — | — |
| `Attribute::Effects` | Stage 4B | Step 14 | Landed with Step 14, conditional on its pull request merging: a row on a `defn`, an operation, a `lambda`, a test, a function type, and an interface contract member is the record of ledger D25.1, resolved in the effect namespace, sorted by canonical identity, and part of the function type; rows are erased by both backends (landed: `V1-EFFECT-*`, `V1-RUNTIME-effect-function-values`). `effects:` on a `deffect` is `@syntax.invalid-attribute` |
| `Attribute::External` | Lowered | Step 8a | `@compiler` rows: Steps 8a–8c (8c landed: the float rows, inline, and the 20 `to-str` and `parse` rows as imports, 42 in `NATIVE_ROWS` with 8b's 22, so every `@compiler` row is lowered, `array.fold` by running its checked body (Step 8d landed); 8b landed: the text, bytes, array, and dict rows, inline or as the 22 native imports of `NATIVE_ROWS`; 8a landed: the integer arithmetic, shift, comparison, and conversion rows and the `char` rows, lowered inline); `@host`: Stage 4B Steps 16b-2 and 17 (the six `io` rows landed in PR #376; the six `fs` rows are lowered, conditional on the pull request of Step 17 merging; the registry, snapshot, and manifest list arrived in Step 16a) |
| `Attribute::Symbol` | Static | — | — |
| `Attribute::Native` | Lowered | Step 10 | 8b: the text, bytes, array, and dict natives are imports of `vibra_native_v1` backed by `vibra-native`, and `array.fold` (no import) is the one native a module runs as its body, the checked body of the embedded standard library that the typed IR carries beside the wrapper the interpreter runs (Step 8d landed); Step 10 completes and proves the rule (landed: `natives_m4_step10` audits that no native or looping row has an algorithm outside `vibra-native` and that the manifest, the registry, and the import table agree, `natives_m3_step4b` runs 112 samples of the 19 natives on four paths, and `native_faults_m4_step10` shows a faulting native ends as `@runtime.invalid-checked-program`) |
| `Attribute::Role` | Static | — | — |
| `Attribute::Doc` | Static | — | — |

## Inherited rows

The M4 [deferral inventory](README.md#m3-deferral-inventory) lists nine rows
that earlier milestones left to M4 (I1–I9), and Step 13 adds ten Stage 4B
contract rows (I10–I19), so that a closed contract cannot lose its
implementation. Each has one owner here, and the test requires all nineteen.
Rows I3–I6 are implemented by Step 2 in the type checker,
the typed IR, and the reference interpreter, conditional on its pull request
merging, and report `@tool.unavailable` no longer. Step 9 lowers them to WebAssembly.

| ID | Inherited row | Owner |
| --- | --- | --- |
| I1 | `Declaration::Deffect`; nonempty `Attribute::Effects`, including on a function type and an `iter` default callback | Step 14; landed, conditional on its pull request merging |
| I2 | `Attribute::External` with `@host` | Step 16b-2; landed, PR #376 (the registry check and `@std.io`, PR #376; `@std.fs`, Step 17, conditional on its pull request merging) |
| I3 | An abstract contract member with its own generic parameters | Step 2; implemented in Step 2, Wasm in Step 9 (landed) |
| I4 | Labelled operands and written `types:` arguments on a contract member call | Step 2; implemented in Step 2, Wasm in Step 9 (landed) |
| I5 | A dict variadic tail on a contract member | Step 2; implemented in Step 2, Wasm in Step 9 (landed) |
| I6 | A contract member with its own generics or labelled parameters as a function value | Step 2; implemented in Step 2, Wasm in Step 9 (landed) |
| I7 | M3's G21 (G16 here): the command result for an entry that returns `err` | Step 19 |
| I8 | Pre-M4 C16: effect rows of a `let` value, a `let-else` fallback, and a `return` operand | Step 14; landed: `V1-EFFECT-let-else-return-rows` |
| I9 | The conformance chapter's effectful walk over `iter.next` | Step 16b-2; landed, PR #376 (`V1-RUNTIME-io-effectful-iter-walk`, `examples/m4-console`) |
| I10 | The `vibra_v1` host registry of 17 rows, its versioning rule, and its checked-in snapshot (ledger D17, D19) | Step 16a; landed, PR #374 (the table in `vibra-ir`, the snapshot and `host_registry_snapshot_m4_step16`); rows in Steps 16b–18 |
| I11 | The `@host` ABI: borrowed parameters, an owned result, and the seven builders (D30) | Step 16b-1; landed, conditional on its pull request merging (the imports, the builders, and the adoption in `vibra-wasm`; `host_builders_m4_step16b`) |
| I12 | The `@audit-trace.v1` event record and the CLI `AuditEvent` objects (D18) | Step 16a; landed, PR #374 (the event type and its VIBON and JSON codecs, `audit_events_m4_step16`); events: Step 16b-1, landed conditional on its pull request merging (both backends append `AuditEvent::completed`; `host_boundary_m4_step16b`) |
| I13 | The `@host-world.v1` test world, the system provider, and the corpus `host-world` input (D20) | Step 16a; landed, PR #374 (the decoder, `WorldProvider`, `SystemProvider`, and the corpus input); fields in Steps 16b–18 |
| I14 | Program arguments, `vibra run TARGET -- ARG...` (D22) | Step 18 |
| I15 | `@command.program-failed` and exit code 6 (D21) | Step 19 |
| I16 | The effect diagnostics, the witness, the indirect-call rule, and the row record (D23–D25) | Step 14; landed; target array in Step 15, landed |
| I17 | The manifest `host` list, the `0.3.0` package, and the five effect modules (D27) | Step 16a; landed, PR #374 (the list, the load-time check, and `0.3.0`); `std/io.vib` landed in PR #376 and `std/fs.vib` in Step 17 (conditional on its pull request merging), with the six `fs` symbols in the list; modules in Step 18 |
| I18 | The `effects` fact of the position envelope and the effect fields of the index (D28) | Step 20 |
| I19 | The performance baseline and its CI check (D29) | Step 21 |
