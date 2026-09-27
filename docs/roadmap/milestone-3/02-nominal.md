# Step 2 — nominal declarations

Prerequisite: Step 1 merged. Stage 3A behavior step.

## Read before editing

- [Types](../../spec/02-type-system.md): **Model**, **Nominal declarations**
  (all but union rules), **Application** (constructor and record projection
  rows), **Namespaces and resolution**, **Inference and checking**.
- [Source](../../spec/01-source-language.md): **Declarations**, **Types,
  interfaces, and methods**.
- [Runtime](../../spec/06-runtime.md): **Evaluation**, **Canonical value
  encoding**.
- [Decision ledger](decision-ledger.md) rows D3.3, D6.1, D6.2, D7.1.

## Scope

`deftype` with a type, record, enum, or newtype body; nominal type names in
every type position; record, enum-variant, and newtype constructors as
`@constructor` applications; record projection with an atom selector; newtype
unwrap where visibility permits; nested non-interface `defn` methods and their
path references; the flat member namespace; the finite-size check; and
`@type.anonymous-type-body` for the four body forms outside a `deftype`. Union
bodies stay `@tool.unavailable` (Step 6); generic `deftype`s stay unavailable
(Step 3).

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-ir/src/lib.rs`: `PrimitiveType`, `Value`, `FunctionSignature`, `Expr` | The semantic model is primitive-only. Introduce a general `Type` (primitive, nominal identity with arguments, and the later builtin constructors) and compound `Value` variants; keep `PrimitiveType` as one case rather than widening every call site ad hoc |
| `crates/vibra-types/src/lib.rs` (≈3.5k lines) | Type lowering at `TypeExpr::` match near line 1896; expression checking from `check_expression`. Split into modules (for example `types`, `declarations`, `expressions`) in a first behavior-neutral commit so later steps do not grow one file |
| `crates/vibra-resolve/src/lib.rs`: `EntityKind`, `DeclarationId` | Type, field, variant, and method identities already exist as kinds; verify owner paths for nested members |
| `crates/vibra-interp/src/lib.rs` | Construct and project compound values |
| `crates/vibra-ir/src/lib.rs`: `canonical_vibon`, `canonical_observation` | Implement the canonical value and type encoding |

## Ordered tasks

1. Behavior-neutral split of `vibra-types`; all suites stay green.
2. Generalize the IR type and value model; M2 observations stay byte-identical.
3. Migrate M2 emissions per D6.1/D6.2: unsuffixed-literal ambiguity to
   `@type.ambiguous-inference`, non-application expected-type failures to
   `@type.mismatch`. Update the affected M2 cases' expected codes in the same
   commit and list them in the PR.
4. Declaration collection and type-name resolution for the four admitted body
   forms, member-collision checks, and the finite-size check.
5. Constructor and projection checking, lowering, and evaluation.
6. Nested methods: signature checking, path references, first-class use.
7. Replace `V1-PROJECT-workspace-check-nominal-availability` coverage of the
   admitted forms with positive and negative cases; keep union, `defint`,
   `impl`, and `deffect` availability rows.

## Test matrix

- Positive: each body form declared, constructed, projected, and rendered in an
  `interpret` result; a newtype round trip; a method called by path and passed
  as a value; a record that recurses through an array.
- Negative: missing, duplicate, and unknown constructor fields
  (`@type.argument-mismatch`); unknown selector (`@type.unknown-record-field`);
  enum value applied (`@type.not-applicable`); field/method collision
  (`@name.member-collision`); direct recursion (`@type.infinite-size`); each
  body form in all seven non-body positions (`@type.anonymous-type-body`);
  reserved-head `deftype` names (`@name.reserved-declaration`).
- Recovery: a malformed `deftype` followed by a valid one still checks the
  second.
- Formatter: nested methods and multi-line record bodies round-trip.

## Done

Scope implemented through syntax, checker, IR, interpreter, and workspace
handlers; inventory rows `Declaration::Deftype`, `TypeMember::Method`, and the
three admitted `DeftypeBody` rows reference their cases; validation passes.
