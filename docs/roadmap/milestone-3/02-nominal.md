# Step 2 — structural and nominal data

Prerequisite: Step 1 merged. Stage 3A behavior step.

## Read before editing

- [Charter](../../spec/00-charter.md): commitments and exclusions.
- [Types](../../spec/02-type-system.md): **Model**, **Nominal declarations**
  (all but union semantics), **Application** (constructors, anonymous value
  forms, record projection), **Namespaces and resolution**, **Inference and
  checking**.
- [Source](../../spec/01-source-language.md): **Declarations**, **Types,
  interfaces, and methods**, **Functions and expressions**.
- [Runtime](../../spec/06-runtime.md): **Evaluation**, **Canonical value
  encoding**.
- [Decision ledger](decision-ledger.md) rows D3.3, D6.1, D6.2, D6.4, D7.1,
  D12.1–D12.4.

## Scope

Two parts, landing together so no valid form is ever malformed:

1. **Reader and formatter.** Parse `(tuple …)`, `(record …)`, `(enum …)`, and
   `(union …)` as type expressions in every type position; `(newtype …)`
   outside a `deftype` body as `@type.anonymous-newtype`; `(intrinsic-type @a)`
   as a `deftype` body; and `tupleof`, `recordof`, and `enumof` as reserved
   expression and pattern forms recognized before application. The pattern
   `(tuple …)` is replaced by `(tupleof …)`. The formatter rewrites anonymous
   record fields, enum variants, and union members into canonical order. Retire
   `@type.anonymous-type-body` from the registry, the specification table, and
   the reader in this change, and replace its reader case.
2. **Semantics.** Declared record, enum, newtype, and plain type-expression
   `deftype`s with constructors, record projection, and nested non-interface
   methods; anonymous records and enums with `recordof` and `enumof`,
   order-insensitive identity, and projection; the flat member namespace; and
   the finite-size check. Anonymous and declared tuples (Step 4), unions
   (Step 6), and generic `deftype`s (Step 3) report `@tool.unavailable` until
   their steps; `intrinsic-type` outside the toolchain package is rejected, and
   inside it is exercised from Step 4.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-syntax/src/ast.rs`: `parse_type_expr_inner` (≈ line 2518), `TypeExpr`, `PatternKind`, `ExpressionKind`, `DeftypeBody` | Currently rejects the four body forms with `@type.anonymous-type-body`; add structural `TypeExpr` variants, anonymous value and pattern variants, and an `intrinsic-type` body |
| `crates/vibra-fmt/src/lib.rs` | Canonical ordering of anonymous record, enum, and union members |
| `crates/vibra-ir/src/lib.rs`: `PrimitiveType`, `Value`, `FunctionSignature`, `Expr` | The semantic model is primitive-only. Introduce a general `Type` with declared, applied, and structural cases and compound `Value` variants, keeping `PrimitiveType` as one case |
| `crates/vibra-types/src/lib.rs` (≈3.5k lines) | Split into modules in a first behavior-neutral commit so later steps do not grow one file |
| `crates/vibra-resolve/src/lib.rs`: `EntityKind`, `DeclarationId` | Type, field, variant, and method identities; verify owner paths for nested members |
| `crates/vibra-interp/src/lib.rs`; `vibra-ir` `canonical_vibon` | Construct and project compound values; implement the canonical value and type encoding |

## Ordered tasks

1. Behavior-neutral split of `vibra-types`; all suites stay green.
2. Reader, AST, and formatter changes with reader-v1 cases; update the M3
   surface inventory for every new AST variant (its test enforces this).
3. Generalize the IR type and value model; M2 observations stay byte-identical.
4. Migrate M2 emissions per D6.1/D6.2 (`@type.ambiguous-inference`,
   `@type.mismatch`) and the affected M2 cases' expected codes, listed in the PR.
5. Declarations, identities, member collisions, and the finite-size check.
6. Constructors, `recordof`, `enumof`, projection, and evaluation.
7. Nested methods: signature checking, path references, first-class use.
8. Replace the admitted rows of `V1-PROJECT-workspace-check-nominal-availability`
   with positive and negative cases; keep union, `defint`, `impl`, and
   `deffect` availability rows.

## Test matrix

- Reader: every structural type in each type position; `newtype` rejected
  outside a body; `tupleof`/`recordof`/`enumof` never parsed as applications;
  recovery after a malformed structural type.
- Formatter: anonymous record, enum, and union members reordered canonically
  and idempotently; declared bodies keep declaration order.
- Positive: each admitted body form declared, constructed, projected, and
  rendered in an `interpret` result; a newtype round trip; `recordof` passed to
  a parameter typed with the fields in another order; `enumof` checked against
  a written anonymous enum; a method called by path and passed as a value; a
  record that recurses through an array.
- Negative: missing, duplicate, and unknown constructor fields
  (`@type.argument-mismatch`); unknown selector (`@type.unknown-record-field`);
  `enumof` with no written enum (`@type.ambiguous-inference`); a declared record
  where its anonymous body is expected (`@type.mismatch`); enum value applied
  (`@type.not-applicable`); field/method collision (`@name.member-collision`);
  direct recursion (`@type.infinite-size`); reserved-head and builtin-named
  `deftype`s (`@name.reserved-declaration`); `intrinsic-type` outside the
  toolchain package.

## Delivery notes

- The behavior-neutral split of `vibra-types` was not done: new semantics landed
  in new modules (`nominal.rs`, `construct.rs`) so `lib.rs` did not grow with
  them, and a wholesale split was judged riskier than it was worth mid-step.
- Record fields and enum variants take their owning type's visibility; the
  resolver previously created them private.
- Calling a function stored in a record field is `@tool.unavailable` until the
  call-flow gap G12 is closed.
- Declared and structural type facts are unavailable in position queries until
  Step 15.

## Done

Scope implemented through syntax, formatter, checker, IR, interpreter, and
workspace handlers; `@type.anonymous-type-body` no longer exists; inventory
rows reference their cases; validation passes.
