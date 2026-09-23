# M2 availability audit

This matrix audits every `deferred` and `supported-empty-only` row in the
[supported-surface inventory](supported-surface.md) against the active
specification and the real conformance handler. Each case manifest records the
expected code, level, source ID, and exact span. The common source diagnostic
contract is [M2 unavailable-form spans](../../spec/07-diagnostics-and-conformance.md#m2-unavailable-form-spans).

An expected source diagnostic `@tool.unavailable` is a valid negative
observation when the declared handler returns it. The conformance runner's
separate `unavailable` status means there was no handler for the operation; it
is never a passing result and is forbidden by the M2 gate.

## Deferred AST families

| AST family | Normative contract | Handler evidence |
| --- | --- | --- |
| `ExpressionKind::Match` | [Source expressions](../../spec/01-source-language.md#functions-and-expressions); [control flow and failure](../../spec/02-type-system.md#control-flow-and-failure) | `V1-TYPE-CONTROL-availability-expressions` — `static-v1/type-check`, `@tool.unavailable` at the complete match form `[32,66)`. |
| `ExpressionKind::As` | [Source ascription and narrowing](../../spec/01-source-language.md#type-ascription-and-narrowing); [type ascription and widening](../../spec/02-type-system.md#type-ascription-and-widening) | `V1-TYPE-CONTROL-availability-expressions` — `static-v1/type-check`, complete expression `[94,107)`. |
| `ExpressionKind::Try` | [Source expressions](../../spec/01-source-language.md#functions-and-expressions); [control flow and failure](../../spec/02-type-system.md#control-flow-and-failure) | `V1-TYPE-CONTROL-availability-expressions` — `static-v1/type-check`, complete expression `[134,144)`. |
| `PatternKind::Literal` | [Source patterns](../../spec/01-source-language.md#functions-and-expressions); [control flow and failure](../../spec/02-type-system.md#control-flow-and-failure) | `V1-TYPE-CONTROL-availability-patterns` — `static-v1/type-check`, owning parameter `[23,31)`. The form is parsed and reached through parameter checking; M3's irrefutability analysis is not claimed. |
| `PatternKind::Constructor` | [Source patterns](../../spec/01-source-language.md#functions-and-expressions); [nominal declarations](../../spec/02-type-system.md#nominal-declarations) | `V1-TYPE-CONTROL-availability-patterns` — `static-v1/type-check`, owning parameter `[70,85)`. The checker rejects this pattern before resolving a constructor, so this case makes no claim about nominal constructor lookup. |
| `PatternKind::Tuple` | [Source patterns](../../spec/01-source-language.md#functions-and-expressions); [control flow and failure](../../spec/02-type-system.md#control-flow-and-failure) | `V1-TYPE-CONTROL-availability-patterns` — `static-v1/type-check`, owning parameter `[118,140)`. |
| `PatternKind::Array` | [Source patterns](../../spec/01-source-language.md#functions-and-expressions); [control flow and failure](../../spec/02-type-system.md#control-flow-and-failure) | `V1-TYPE-CONTROL-availability-patterns` — `static-v1/type-check`, owning parameter `[173,189)`. |
| `PatternKind::As` | [Source ascription and narrowing](../../spec/01-source-language.md#type-ascription-and-narrowing); [type ascription and widening](../../spec/02-type-system.md#type-ascription-and-widening) | `V1-TYPE-CONTROL-availability-patterns` — `static-v1/type-check`, owning parameter `[219,236)`. |
| `VariadicBinding::Array` | [Labels and applications](../../spec/01-source-language.md#labels-and-applications); [application](../../spec/02-type-system.md#application) | `V1-SRC-CALLS-variadic-array-application` — `static-v1/type-check`: declaration `[0,75)`, no-tail application `[105,125)`, tail application `[126,151)`. `V1-PROJECT-workspace-check-variadic-applications` — `static-v1/workspace-check`: declaration `[0,75)`, applications `[105,125)` and `[126,151)`. `V1-SRC-CALLS-variadic-captured-alias` (`static-v1/type-check`) and `V1-PROJECT-workspace-check-variadic-captured-alias` (`static-v1/workspace-check`): captured alias applications `[154,166)` and `[285,297)`, including a nested capture chain. |
| `VariadicBinding::Map` | [Labels and applications](../../spec/01-source-language.md#labels-and-applications); [application](../../spec/02-type-system.md#application) | `V1-SRC-CALLS-variadic-map-application` — `static-v1/type-check`: declaration `[0,75)`, no-tail application `[103,121)`, tail application `[122,151)`. `V1-PROJECT-workspace-check-variadic-applications` — `static-v1/workspace-check`: declaration `[154,229)`, applications `[257,275)` and `[276,305)`. |
| `Declaration::Deftype` | [Declarations](../../spec/01-source-language.md#declarations); [nominal declarations](../../spec/02-type-system.md#nominal-declarations) | `V1-PROJECT-workspace-check-nominal-availability` — `static-v1/workspace-check`; complete declaration forms `[0,20)`, `[21,64)`, `[65,107)`, `[108,140)`, and `[141,210)`. |
| `Declaration::Defint` | [Types, interfaces, and methods](../../spec/01-source-language.md#types-interfaces-and-methods); [interfaces and methods](../../spec/02-type-system.md#interfaces-and-methods) | `V1-PROJECT-workspace-check-nominal-availability` — `static-v1/workspace-check`; interface declaration `[211,320)`, member `[231,261)`, implementation `[264,319)`, implementation member `[278,318)`. |
| `Declaration::Deffect` | [Nominal effects](../../spec/03-effects.md#nominal-effects); [M2 executable effects](../../spec/03-effects.md#m2-executable-effects) | `V1-EFFECT-availability-host-member` — `static-v1/type-check`, complete valid `deffect` form `[0,97)`. `V1-PROJECT-workspace-check-nominal-availability` — `static-v1/workspace-check`, declaration `[321,418)` and operation `[338,417)`. |
| `TypeMember::Method` | [Types, interfaces, and methods](../../spec/01-source-language.md#types-interfaces-and-methods); [interfaces and methods](../../spec/02-type-system.md#interfaces-and-methods) | `V1-PROJECT-workspace-check-nominal-availability` — `static-v1/workspace-check`; method source forms `[174,209)` and `[231,261)`. |
| `TypeMember::Implementation` | [Types, interfaces, and methods](../../spec/01-source-language.md#types-interfaces-and-methods); [interfaces and methods](../../spec/02-type-system.md#interfaces-and-methods) | `V1-PROJECT-workspace-check-nominal-availability` — `static-v1/workspace-check`; implementation `[264,319)` and member `[278,318)`. |
| `TypeExpr::Applied` | [Types, interfaces, and methods](../../spec/01-source-language.md#types-interfaces-and-methods); [nominal declarations](../../spec/02-type-system.md#nominal-declarations) | `V1-TYPE-NOMINAL-availability-types` — `static-v1/type-check`, owning parameter `[15,30)`. |
| `TypeExpr::Tuple` | [Types, interfaces, and methods](../../spec/01-source-language.md#types-interfaces-and-methods); [nominal declarations](../../spec/02-type-system.md#nominal-declarations) | `V1-TYPE-NOMINAL-availability-types` — `static-v1/type-check`, owning parameter `[56,77)`. |
| `TypeExpr::Array` | [Types, interfaces, and methods](../../spec/01-source-language.md#types-interfaces-and-methods); [nominal declarations](../../spec/02-type-system.md#nominal-declarations) | `V1-TYPE-NOMINAL-availability-types` — `static-v1/type-check`, owning parameter `[104,121)`. |
| `TypeExpr::Map` | [Types, interfaces, and methods](../../spec/01-source-language.md#types-interfaces-and-methods); [nominal declarations](../../spec/02-type-system.md#nominal-declarations) | `V1-TYPE-NOMINAL-availability-types` — `static-v1/type-check`, owning parameter `[147,166)`. |
| `VariadicType::Array` | [Application](../../spec/02-type-system.md#application); [functions as values](../../spec/02-type-system.md#functions-as-values) | `V1-SRC-CALLS-variadic-array-type` — `static-v1/type-check`, owning parameter `[20,65)`; body includes an attempted call that is not lowered after the rejected type. |
| `VariadicType::Map` | [Application](../../spec/02-type-system.md#application); [functions as values](../../spec/02-type-system.md#functions-as-values) | `V1-SRC-CALLS-variadic-map-type` — `static-v1/type-check`, owning parameter `[18,65)`; body includes an attempted call that is not lowered after the rejected type. |
| `DeftypeBody::Type` | [Types, interfaces, and methods](../../spec/01-source-language.md#types-interfaces-and-methods); [nominal declarations](../../spec/02-type-system.md#nominal-declarations) | `V1-PROJECT-workspace-check-nominal-availability` — `static-v1/workspace-check`, owning `deftype` `[0,20)`. |
| `DeftypeBody::Record` | [Types, interfaces, and methods](../../spec/01-source-language.md#types-interfaces-and-methods); [nominal declarations](../../spec/02-type-system.md#nominal-declarations) | `V1-PROJECT-workspace-check-nominal-availability` — `static-v1/workspace-check`, owning `deftype` `[21,64)`; child fields `[43,51)` and `[52,62)`. |
| `DeftypeBody::Enum` | [Types, interfaces, and methods](../../spec/01-source-language.md#types-interfaces-and-methods); [nominal declarations](../../spec/02-type-system.md#nominal-declarations) | `V1-PROJECT-workspace-check-nominal-availability` — `static-v1/workspace-check`, owning `deftype` `[65,107)`; variants `[87,95)` and `[96,105)`. |
| `DeftypeBody::Union` | [Types, interfaces, and methods](../../spec/01-source-language.md#types-interfaces-and-methods); [nominal declarations](../../spec/02-type-system.md#nominal-declarations) | `V1-PROJECT-workspace-check-nominal-availability` — `static-v1/workspace-check`, owning `deftype` `[108,140)`. |
| `DeftypeBody::Newtype` | [Types, interfaces, and methods](../../spec/01-source-language.md#types-interfaces-and-methods); [nominal declarations](../../spec/02-type-system.md#nominal-declarations) | `V1-PROJECT-workspace-check-nominal-availability` — `static-v1/workspace-check`, owning `deftype` `[141,210)` and method `[174,209)`. |
| `Attribute::Where` | [Declarations](../../spec/01-source-language.md#declarations); [generics](../../spec/02-type-system.md#generics) | `V1-TYPE-GENERIC-availability` — `static-v1/type-check`, generic declaration `[0,55)`. The explicit `types:` application is also unavailable at expression `[83,110)`. |
| `Attribute::Variadic` | [Declarations](../../spec/01-source-language.md#declarations); [application](../../spec/02-type-system.md#application) | `V1-SRC-CALLS-variadic-array-application`, `V1-SRC-CALLS-variadic-map-application` — `static-v1/type-check`; array/map declaration spans `[0,75)`. Workspace coverage is in `V1-PROJECT-workspace-check-variadic-applications`; captured target coverage is in `V1-SRC-CALLS-variadic-captured-alias` and `V1-PROJECT-workspace-check-variadic-captured-alias`. |

## Other availability boundaries

These rows are not marked `deferred` in the AST inventory, but they affect the
same M2 availability contract or use a separate operation boundary.

| Surface | Normative contract | Handler evidence |
| --- | --- | --- |
| Nonempty `Attribute::Effects` | [M2 executable effects](../../spec/03-effects.md#m2-executable-effects); [function effect rows](../../spec/03-effects.md#function-effect-rows) | `V1-SRC-CALLS-functions-effects` — `static-v1/type-check`, nonempty declaration ceiling diagnostic at row `[31,42)`. `V1-EFFECT-availability-function-type` — `static-v1/type-check`, nonempty function-type row rejected at owning parameter `[34,78)`. |
| Empty `Attribute::Effects` | [M2 executable effects](../../spec/03-effects.md#m2-executable-effects); [function effect rows](../../spec/03-effects.md#function-effect-rows) | `V1-EFFECT-availability-empty-row` — `static-v1/type-check`, accepted explicit `effects: ()`. |
| `@host` external member | [External definitions](../../spec/01-source-language.md#external-definitions); [nominal effects](../../spec/03-effects.md#nominal-effects); [M2 executable effects](../../spec/03-effects.md#m2-executable-effects) | `V1-EFFECT-availability-host-member` — `static-v1/type-check`, valid `@host` member syntax is unavailable at its owning `deffect` `[0,97)`. `V1-PROJECT-workspace-check-nominal-availability` also checks the containing effect and operation. `@host` is only valid on an effect member, so no standalone function-provider case exists. |
| Compiler external and symbol metadata | [External definitions](../../spec/01-source-language.md#external-definitions); [M2 bootstrap trust input](../../spec/04-programs-and-packages.md#m2-bootstrap-trust-input) | `V1-RUNTIME-external-untrusted` — `interpreter-v1/interpret`, copied `@compiler` declaration unavailable at full declaration `[0,71)`. `V1-RUNTIME-external-std-text-concat` and `V1-RUNTIME-external-std-text-length` exercise the verified closed provider. |
| User nominal type-name references | [Types, interfaces, and methods](../../spec/01-source-language.md#types-interfaces-and-methods); [nominal declarations](../../spec/02-type-system.md#nominal-declarations) | `V1-PROJECT-workspace-check-nominal-availability` — `static-v1/workspace-check`, use of `handle` in a parameter is unavailable at its owning parameter `[437,449)`. Primitive `TypeExpr::Name` remains supported. |
| Dependency availability | [Dependencies and lock](../../spec/04-programs-and-packages.md#dependencies-and-lock); [M2 project discovery and source snapshot](../../spec/04-programs-and-packages.md#m2-project-discovery-and-source-snapshot) | `V1-PROJECT-graph-static-dependency` — `static-v1/source-graph`, an unsupported path dependency is unavailable at `project.vibon` `[158,165)`. |
| Reserved M2 CLI commands | [V1 CLI](../../spec/05-tooling.md#v1-cli); [M2 command contract](../../spec/05-tooling.md#m2-command-contract) | Actual-binary test `reserved_v1_commands_are_unavailable_and_unknown_names_are_invalid` in `crates/vibra-cli/tests/process_step13.rs` covers `lint`, `build src/app`, `query @workspace`, `edit fix`, `mcp`, and `project inspect/add/remove/sync`: each returns exit `4`, `@command.unavailable`, and error `@tool.unavailable`; an unknown spelling returns exit `2` and `@command.invalid-input`. |
| Test-command availability and status | [Tests](../../spec/04-programs-and-packages.md#tests); [M2 assertion contract](../../spec/04-programs-and-packages.md#m2-assertion-contract); [M2 command contract](../../spec/05-tooling.md#m2-command-contract) | `V1-RUNTIME-workspace-test-unavailable-assertion`, `V1-RUNTIME-workspace-test-passing`, `V1-RUNTIME-workspace-test-failure`, `V1-RUNTIME-workspace-test-empty`, and `V1-PROJECT-workspace-test-invalid-missing-assert-import` — `interpreter-v1/workspace-test`. CLI process coverage includes `unavailable_assertion_member_stays_distinct_from_failure`, `unavailability_takes_precedence_over_assertion_failure`, `failed_assertion_is_a_structured_test_failure_and_exit_one`, and empty/selector tests in `crates/vibra-cli/tests/process_step13.rs`. |

The test-command row distinguishes item-level unavailability from invalid
source and assertion failure. The command process tests exercise exit and JSON
contracts; the corpus rows exercise the workspace-test operation. They are not
source-level `@tool.unavailable` cases.

The reserved `project add/remove/sync` command results above are separate from
source-graph dependency availability. `V1-PROJECT-graph-static-dependency`
observes an unsupported dependency edge during `source-graph`; the process test
observes a valid but unimplemented command name. The `sync` command's future
network behavior is not exercised by that graph case.

## Implemented pure subset: clause and case map

The following is the implemented M2 slice supported by positive corpus
observations. It does not claim the remaining v1 forms, effects, host behavior,
generics, or nominal types are implemented.

| Implemented slice | Normative contract | Positive case evidence |
| --- | --- | --- |
| Primitive literals, primitive `TypeExpr::Name`, and `void` | [Source expressions](../../spec/01-source-language.md#functions-and-expressions); [type model](../../spec/02-type-system.md#model) | `V1-TYPE-INFER-primitives` — `static-v1/type-check`; `V1-RUNTIME-literal` and `V1-RUNTIME-void` — `interpreter-v1/interpret`. |
| Resolved names, imports, immutable `def`, and `defn` | [Modules and imports](../../spec/04-programs-and-packages.md#modules-and-imports); [namespaces and resolution](../../spec/02-type-system.md#namespaces-and-resolution) | `V1-TYPE-NAMES-binding-forward` — `static-v1/type-check`; `V1-TYPE-NAMES-resolve-public-forward` — `static-v1/resolve`; `V1-PROJECT-workspace-check-import-closure` — `static-v1/workspace-check`. |
| Fixed positional and labelled applications | [Labels and applications](../../spec/01-source-language.md#labels-and-applications); [application](../../spec/02-type-system.md#application); [functions as values](../../spec/02-type-system.md#functions-as-values) | `V1-SRC-CALLS-functions-labelled`, `V1-SRC-CALLS-functions-value-default` — `static-v1/type-check`. |
| Closures, bindings, sequencing, and boolean branching | [Source expressions](../../spec/01-source-language.md#functions-and-expressions); [functions as values](../../spec/02-type-system.md#functions-as-values); [control flow and failure](../../spec/02-type-system.md#control-flow-and-failure) | `V1-RUNTIME-functions-closures`, `V1-RUNTIME-bindings`, `V1-RUNTIME-sequence`, `V1-RUNTIME-if-selected` — `interpreter-v1/interpret`; `V1-TYPE-CONTROL-empty-sequences` — `static-v1/type-check`. |
| Binding patterns and pure tests | [Source patterns](../../spec/01-source-language.md#functions-and-expressions); [Tests](../../spec/04-programs-and-packages.md#tests); [M2 assertion contract](../../spec/04-programs-and-packages.md#m2-assertion-contract) | `V1-RUNTIME-workspace-test-passing`, `V1-RUNTIME-workspace-test-empty` — `interpreter-v1/workspace-test`; `V1-RUNTIME-bindings` covers local binding/discard patterns. |
| Monomorphic empty-effect function values; explicit empty effect ceilings | [Functions as values](../../spec/02-type-system.md#functions-as-values); [M2 executable effects](../../spec/03-effects.md#m2-executable-effects); [function effect rows](../../spec/03-effects.md#function-effect-rows) | `V1-SRC-CALLS-functions-value-default`, `V1-EFFECT-availability-empty-row` — `static-v1/type-check`. |
| Public visibility and source documentation metadata | [Declarations](../../spec/01-source-language.md#declarations); [modules and imports](../../spec/04-programs-and-packages.md#modules-and-imports) | `V1-TYPE-NAMES-resolve-public-forward` — `static-v1/resolve`; `V1-SRC-DECL-doc-attribute` — `static-v1/type-check`. |
| Verified compiler externals | [External definitions](../../spec/01-source-language.md#external-definitions); [M2 bootstrap trust input](../../spec/04-programs-and-packages.md#m2-bootstrap-trust-input) | `V1-RUNTIME-external-std-text-concat`, `V1-RUNTIME-external-std-text-length` — `interpreter-v1/interpret`. |

`TypeExpr::Name` is implemented for primitive names and the specified resolved
IDs; the nominal-name observation in the deferred matrix does not make
`deftype` support available. Function types are available only when their
signature is monomorphic, nonvariadic, and has an empty effect row. `@compiler`
externals remain tied to the verified bootstrap and closed registry. The M1
reader example inventory remains unchanged at
`docs/roadmap/milestone-1/syntax-examples.tsv`.
