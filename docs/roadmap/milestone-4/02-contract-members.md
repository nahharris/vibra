# Step 2 — contract-member forms and the binder defect

Prerequisite: Step 1 merged. Stage 4A behavior step, reference interpreter only.
It adds no WebAssembly. Wasm lowers these forms in Step 9, from the IR shape
this step fixes, so the IR must carry what both backends need
([ledger D9.1](decision-ledger.md)).

## Read before editing

- [Types](../../spec/02-type-system.md): **Generics** (the `types:` rules and
  the complete type-argument list), **Interfaces and methods**, **Functions as
  values**, **Conversion**, **Iteration**.
- [Runtime](../../spec/06-runtime.md): **Evaluation**, **Generic
  instantiation**, **Tail calls**.
- [Source](../../spec/01-source-language.md): **Reader** (the reserved-binder
  sentence) and **Labels and applications**.
- [Diagnostics](../../spec/07-diagnostics-and-conformance.md): **Binding,
  `return`, and `never` diagnostics**.
- [M3 exit evidence](../milestone-3/exit-evidence.md#reassigned-to-later-milestones)
  and [ledger D22.2 and D27.1](../milestone-3/decision-ledger.md); the
  [M4 ledger](decision-ledger.md) rows D9.1, D9.2, D14.1.

## Scope

Four forms that M3 reports as `@tool.unavailable`, implemented in the type
checker, the typed IR, and the interpreter together so that no valid form stays
malformed, plus one inherited defect:

1. an abstract contract member with its own generic parameters;
2. labelled operands and a written `types:` list on a contract member call;
3. a dict variadic tail on a contract member (an array tail is supported);
4. a contract member with its own generics or labelled parameters named as a
   function value; and
5. the binder defect: a lexical binder spelled as a keyword, boolean, `void`,
   `any`, `never`, or primitive type name is `@name.reserved-declaration`
   ([ledger D14.1](decision-ledger.md)).

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-types/src/interfaces.rs`: `check_contract_call` and `check_contract_value` (three `unavailable` sites: labelled operands or `types:`, own generics, and the function-value form) | The M3 rejections to replace; keep the default-member path, which already handles a member's own generics |
| `crates/vibra-ir/src/lib.rs`: `CallTarget::Contract`, `Implements`, `Expr::Function` | The call shape and implementation identity. Proposed: `CallTarget::Contract` gains `member_types: Vec<Type>`, the member's own type arguments in `where:` order; an implementation's function gains the member's generics after the receiver's and the interface's, so its complete list is receiver, interface, then member |
| `crates/vibra-ir/src/lib.rs`: `Expr` | Proposed new variant `ContractFunction { interface, member, arguments, member_types, signature, origin }`, a function value that selects its implementation from the operand at the application, so named and anonymous callees need no new `CallTarget` |
| `crates/vibra-interp/src/lib.rs`: `Callable`, contract dispatch, `bind_type` | Evaluate `ContractFunction` to a `Callable::Contract`, and thread `member_types` into the callee's type map |
| `crates/vibra-resolve`, `crates/vibra-types/src/lib.rs` | Binder checks: where a `let`, `let-else`, parameter, `match` arm, or `lambda` parameter introduces a name |

Operands arrive in resolved parameter order (fixed, labelled in declaration
order, then the packed variadic operand), so labelled operands and a dict tail
add no field to a call; they add checking only. These names are proposals.

## Ordered tasks

1. Pin the current behavior: the three availability rejections and the keyword
   binders, each in a host test, so a regression is visible.
2. Binder defect first, as it is independent: reject the reserved spellings at
   every binder site, still bind the name, and add negative cases.
3. Checker: accept a `types:` list on a contract call, defined by the member
   the call addresses (the contract's parameters, then the member's own), and
   check agreement when inference also succeeds. Accept labelled operands under
   the existing binding rules. Accept a dict tail by the same rule as a
   function's.
4. IR: extend `CallTarget::Contract` and add `ContractFunction`; update
   validation, the canonical form, and the typed-IR deserialization.
5. Interpreter: select an implementation from the receiver, the interface
   arguments, and the member's own type arguments; evaluate the new function
   value; keep tail calls through both reusing the activation.
6. Replace the availability cases that these forms satisfy with positive and
   negative cases, and move the I3–I6 rows of the
   [M4 inventory](supported-surface.md#inherited-rows) to implemented.

Invariants preserved: an application evaluates its callee once before operands;
generics stay observable; a contract call selects from instantiated types; a
tail call reuses the activation, including through a contract-member value; no
valid form is reclassified as malformed.

## Test matrix

- Positive: an abstract member with its own generic, called with inference and
  with `types:`; labelled operands on a contract call in a different written
  order; a dict tail, including an empty tail; each as a function value at a
  written `fn` type; a tail call through each; a generic interface; a
  destination-dispatched member among them.
- Negative: wrong `types:` length (`@type.type-argument-mismatch`); a `types:`
  that contradicts inference; an unknown or duplicate label
  (`@type.argument-mismatch`); an odd dict tail; a member value with no
  written `fn` type (`@type.ambiguous-inference`); a member value no
  implementation satisfies.
- Recovery: a malformed contract call followed by a valid one in the same
  module still checks.
- Boundary: zero, one, and many own type arguments; an implementation whose
  owner has a different generic arity; an interface and a member that both
  name `t`, which is `@name.generic-redeclaration`.
- Binder: each reserved spelling at each binder site, each followed by a valid
  binder that still checks.
- Formatter: `types:` and labels written out of canonical order on a contract
  call are rewritten idempotently, and each is `@style.argument-order` when
  accepted.

## Diagnostic and schema changes

No new code. `@name.reserved-declaration` gains the lexical-binder condition
(registry summary updated in Step 1). `@tool.unavailable` is no longer reported
for the four forms. The typed-IR canonical form changes in place with no old
shape kept; no JSON schema changes.

## Validation

The [Step 2 row](validation.md#focused-checks), then the full
[pre-merge list](validation.md#before-merging-each-step). The corpus must stay
at zero failed and zero unavailable, and each replaced availability case is
named in the PR.

## Excluded

Lowering any of these forms to WebAssembly (Step 9); effects on a contract
member (Step 14); a monomorphizing strategy (M7).

## Completion evidence

The three availability messages are gone from `interfaces.rs`; the host tests
and corpus cases above pass; the inventory rows I3–I6 name this step as
implemented; the binder cases pass; the PR lists every case whose expectation
changed and why.
