# Pre-M4 step 1: bindings, `return`, and `never`

Status: implemented in the pre-M4 bindings PR
Applies to: the pre-M4 language revision in [`../v1.md`](../v1.md)

This is the behavior checklist that [`../execution.md`](../execution.md) step 4
asks for, plus the implementation order, the migration plan, and the contract
questions that Stage A could not close. It is process guidance and has no
normative weight over `docs/spec/`. The rules are in
[`01-source-language.md`](../../spec/01-source-language.md) ("Body sequences",
"`let`", "`let-else`", "`return`", "Canonical format"),
[`02-type-system.md`](../../spec/02-type-system.md) ("Control flow and
failure", including "The `never` type"), [`05-tooling.md`](../../spec/05-tooling.md)
(contexts), [`06-runtime.md`](../../spec/06-runtime.md) ("Evaluation", "Tail
calls"), and [`07-diagnostics-and-conformance.md`](../../spec/07-diagnostics-and-conformance.md)
("Binding, `return`, and `never` diagnostics").

## Rules at a glance

1. `(let pattern expr { pattern expr })` binds in the enclosing body sequence.
2. `(let-else pattern expr fallback)` binds a refutable pattern; the fallback has
   type `never`.
3. `(return expr)` exits the innermost `defn`, nested method, or `lambda`, and a
   `return` in tail position is redundant.
4. `never` is a predeclared uninhabited type: accepted at any expected type,
   skipped in branch joins, never inferred, satisfying only `any`, and allowed
   only in the positions the type chapter lists.
5. `let`, `let-else`, and `return` have the tail-position rules of the runtime
   chapter, and the tooling `context` vocabulary changes.

## Conventions

- **Span** names the primary span as a complete half-open UTF-8 range of the
  named form or token. Corpus expectations record exact offsets, which this
  checklist does not, and Stage B computes them from the case input, never from
  an implementation's output.
- **Format** is the `vibra fmt` result. Rows whose input is written canonically
  say `unchanged`. A rejected input has no formatter row of its own: the
  formatter result of a document that carries an AST-level error is an
  existing, separate contract (question 16), so those rows say `n/a (rejected)`.
- **Host test** names a proposed test. The crate prefixes are `syn`
  (`crates/vibra-syntax/tests/pre_m4_bindings.rs`), `res`
  (`crates/vibra-resolve/tests/pre_m4_bindings.rs`), `typ`
  (`crates/vibra-types/tests/pre_m4_bindings.rs`), `fmt`
  (`crates/vibra-fmt/tests/pre_m4_bindings.rs`), `int`
  (`crates/vibra-conformance/tests/pre_m4_runtime.rs`, through the real
  `interpret` handler), `wks` (`crates/vibra-workspace/tests/pre_m4_queries.rs`),
  and `dia` (`crates/vibra-diagnostics` unit tests). Names are proposals, not
  claims that a test exists.
- **Corpus** is a proposed case ID under `conformance/cases/`. A case holds many
  defns, one per row, in source order, exactly as the existing negative cases do.
  The operation is `type-check` unless the row says otherwise.
- Every rejected row is followed in its case by a valid declaration that must
  still check, so the case proves recovery as well as rejection (rows A20-A22
  state it separately).
- Names such as `spin`, `attempt`, and `use` stand for helper declarations the
  case supplies: `(defn spin () never (spin))`,
  `(defn attempt () (result void str) (result.ok))`, and
  `(defn use (value i32) void (do))`. A bare body fragment in a row stands for
  the body of a small `defn` the case wraps around it.

## A. Grammar, position, and recovery (`V1-SRC-EXPR`)

| ID | Rule | Input | Expected | Diagnostic | Format | Host test | Corpus |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A01 | `let` single pair is a body element | `(defn f () i32 (let a 1i32) a)` | accepted; the `let` has type `void` | none | unchanged | `syn::let_single_pair` | `V1-SRC-EXPR-let-pairs` |
| A02 | `let` multi pair, later value sees earlier pair | `(defn f () i32 (let a 1i32 b a) b)` | accepted | none | unchanged | `syn::let_multi_pair` | `V1-SRC-EXPR-let-pairs` |
| A03 | `let` with a discard pair and a destructuring pair | `(let - (attempt) (tupleof x y) pair)` | accepted | none | unchanged | `syn::let_discard_and_destructure` | `V1-SRC-EXPR-let-pairs` |
| A04 | Body sequences that admit a binding | `let` as a direct element of a `defn`, nested method, `impl` member, interface default member, `lambda`, `do`, and `test` body | each accepted | none | unchanged | `syn::let_in_every_body_sequence` | `V1-SRC-EXPR-body-sequences` |
| A05 | `let-else` is a body element | `(defn f (o (option i32)) i32 (let-else (option.some v) o (return 0i32)) v)` | accepted | none | unchanged | `syn::let_else_element` | `V1-SRC-EXPR-let-else` |
| A06 | `let` misplaced as an application operand | `(defn f () i32 (g (let a 1i32)))` | rejected | `@syntax.misplaced-binding`, `@error`, the `let` form | n/a (rejected) | `syn::misplaced_application_operand` | `V1-SRC-EXPR-misplaced-binding` |
| A07 | Misplaced as an `if` condition, `then` branch, and `else` branch | `(if (let a true) 1i32 2i32)`, `(if c (let a 1i32) 2i32)`, `(if c 1i32 (let a 2i32))` | three diagnostics, in source order | `@syntax.misplaced-binding`, `@error`, each `let` form | n/a (rejected) | `syn::misplaced_if_positions` | `V1-SRC-EXPR-misplaced-binding` |
| A08 | Misplaced as a `match` subject and an arm result | `(match (let a 1i32) - 0i32)`, `(match x - (let a 1i32))` | two diagnostics | `@syntax.misplaced-binding`, `@error`, each `let` form | n/a (rejected) | `syn::misplaced_match_positions` | `V1-SRC-EXPR-misplaced-binding` |
| A09 | Misplaced in a `def` initializer and in a `let` or `let-else` value | `(def x i32 (let a 1i32))`, `(let b (let a 1i32))`, `(let-else p (let a 1i32) f)` | three diagnostics | `@syntax.misplaced-binding`, `@error`, each inner `let` form | n/a (rejected) | `syn::misplaced_def_and_binding_values` | `V1-SRC-EXPR-misplaced-binding` |
| A10 | Misplaced as a `return`, `try`, `as`, and anonymous-value operand, and as a `let-else` fallback | `(return (let a 1i32))`, `(try (let a x))`, `(as i32 (let a 1i32))`, `(tupleof (let a 1i32))`, `(let-else p v (let a 1i32))` | five diagnostics | `@syntax.misplaced-binding`, `@error`, each inner form | n/a (rejected) | `syn::misplaced_remaining_positions` | `V1-SRC-EXPR-misplaced-binding` |
| A11 | `let-else` misplaced the same way as `let` | `(g (let-else p v f))` | rejected | `@syntax.misplaced-binding`, `@error`, the `let-else` form | n/a (rejected) | `syn::misplaced_let_else` | `V1-SRC-EXPR-misplaced-binding` |
| A12 | A misplaced form is accepted once wrapped in `do` | `(if c (do (let a 1i32) a) 2i32)`; `(match x - (do (let a 1i32) a))` | accepted | none | unchanged | `syn::do_wrapped_bindings` | `V1-SRC-EXPR-misplaced-binding-do` |
| A13 | `let` with no operands | `(let)` as a body element | rejected | `@syntax.invalid-form`, `@error`, the `let` form | n/a (rejected) | `syn::let_arity_none` | `V1-SRC-EXPR-binding-arity` |
| A14 | `let` with one operand | `(let a)` | rejected | `@syntax.invalid-form`, `@error`, the `let` form | n/a (rejected) | `syn::let_arity_one` | `V1-SRC-EXPR-binding-arity` |
| A15 | `let` with an odd operand count | `(let a 1i32 b)` | rejected; no pair is bound | `@syntax.invalid-form`, `@error`, the `let` form | n/a (rejected) | `syn::let_arity_odd` | `V1-SRC-EXPR-binding-arity` |
| A16 | `let-else` with two operands | `(let-else p v)` | rejected | `@syntax.invalid-form`, `@error`, the form | n/a (rejected) | `syn::let_else_arity_two` | `V1-SRC-EXPR-binding-arity` |
| A17 | `let-else` with four operands | `(let-else p v f g)` | rejected | `@syntax.invalid-form`, `@error`, the form | n/a (rejected) | `syn::let_else_arity_four` | `V1-SRC-EXPR-binding-arity` |
| A18 | `return` with no operand and with two | `(return)`, `(return a b)` | rejected | `@syntax.invalid-form`, `@error`, each form | n/a (rejected) | `syn::return_arity` | `V1-SRC-EXPR-binding-arity` |
| A19 | The old `let` shape has no bridge | `(let x 1i32 (f x))` and `(let x 1i32 (f x) (g x))` | read under the pair grammar: the first has an odd operand count; the second has a second pair whose pattern `(f x)` is a constructor pattern that does not resolve | first: `@syntax.invalid-form`, `@error`, the form; second: `@name.unknown-symbol`, `@error`, the symbol `f`, and `@name.redeclaration` over the inner `x` (pin the exact set from the resolver's existing pattern behaviour) | n/a (rejected) | `syn::old_let_shape_has_no_bridge` | `V1-SRC-EXPR-old-let-shape` |
| A20 | Recovery after a misplaced form | a misplaced `let` in one defn, followed by a valid defn using `let` | the first rejected, the second still checks and is accepted | one `@syntax.misplaced-binding` | n/a (rejected) | `syn::recovery_after_misplaced` | `V1-SRC-EXPR-misplaced-binding` |
| A21 | Recovery after a malformed form | `(let a 1i32 b)` followed by a valid defn | the second checks | one `@syntax.invalid-form` | n/a (rejected) | `syn::recovery_after_malformed` | `V1-SRC-EXPR-binding-arity` |
| A22 | A misplaced form's value operands are still read | `(g (let a (h (let b 1i32))))` | both misplacements are reported; the declaration is checked no further, so no `@name.unknown-symbol` follows | `@syntax.misplaced-binding` over each `let` | n/a (rejected) | `syn::misplaced_operands_checked` | `V1-SRC-EXPR-misplaced-binding` |
| A23 | `return` is no longer retired; the other four are | `(defn f () void (return void))` accepted; `(while true)`, `(for x)`, `(break)`, `(continue)` in a case | `return` accepted; the others rejected | `@syntax.retired-form`, `@error`, the head atom, for each of the four | n/a | `syn::retired_forms_exclude_return` | `V1-SRC-EXPR-retired-form` (existing, migrated) |
| A24 | `let-else` and `return` are reserved heads, not applications | a value named `return` called as `(return 1i32)` in a position the grammar reads as the form | the list is the form, never an application of a value | none beyond the form's own rules | unchanged | `syn::reserved_heads_win_over_application` | `V1-SRC-EXPR-reserved-heads` |
| A25 | Old retired-pattern case keeps rejecting `(bind x)` | `(defn bad (value i32) i32 (let (bind x) value) value)` | `bind` rejected as before | `@syntax.retired-form`, `@error`, the `bind` head | n/a (rejected) | `syn::bind_still_retired` | `V1-SRC-EXPR-pattern-retired-form` (existing, migrated) |

## B. Scope and names (`V1-TYPE-NAMES`)

| ID | Rule | Input | Expected | Diagnostic | Format | Host test | Corpus |
| --- | --- | --- | --- | --- | --- | --- | --- |
| B01 | A binding is not visible to its own value | `(let a a)` | rejected | `@name.unknown-symbol`, `@error`, the value `a` | n/a (rejected) | `res::own_value_does_not_see_binding` | `V1-TYPE-NAMES-let-scope` |
| B02 | Not visible to an earlier element | `(use a) (let a 1i32)` | rejected | `@name.unknown-symbol`, `@error`, the earlier `a` | n/a (rejected) | `res::earlier_element_does_not_see_binding` | `V1-TYPE-NAMES-let-scope` |
| B03 | Visible to a later pair and to every later element | `(let a 1i32 b a) (use a b)` | accepted | none | unchanged | `res::later_pairs_and_elements_see_binding` | `V1-TYPE-NAMES-let-scope` |
| B04 | A `do` ends its bindings | `(do (let a 1i32)) a` | rejected | `@name.unknown-symbol`, `@error`, the later `a` | n/a (rejected) | `res::do_ends_binding_scope` | `V1-TYPE-NAMES-let-scope` |
| B05 | Consecutive `let` forms are not siblings | `(let a 1i32) (let a 2i32)` | rejected | `@name.redeclaration`, `@error`, the later binder `a`; related: the earlier binder | n/a (rejected) | `res::consecutive_lets_redeclare` | `V1-TYPE-NAMES-let-redeclaration` |
| B06 | A name repeated inside one `let` | `(let a 1i32 a 2i32)` | rejected | `@name.redeclaration`, `@error`, the second `a`; related: the first | n/a (rejected) | `res::repeat_within_one_let` | `V1-TYPE-NAMES-let-redeclaration` |
| B07 | Sibling `do` forms may reuse a name | `(do (let a 1i32) a) (do (let a 2i32) a)` | accepted | none | unchanged | `res::sibling_dos_reuse_name` | `V1-TYPE-NAMES-let-redeclaration` |
| B08 | A nested form may not shadow a visible binding | `(let a 1i32) (do (let a 2i32) a)` | rejected | `@name.redeclaration`, `@error`, the inner `a`; related: the outer | n/a (rejected) | `res::nested_do_shadows_visible_binding` | `V1-TYPE-NAMES-let-redeclaration` |
| B09 | The `let-else` pattern is not visible to its value | `(let-else (option.some x) x (return 0i32))` | rejected | `@name.unknown-symbol`, `@error`, the value `x` | n/a (rejected) | `res::let_else_value_does_not_see_pattern` | `V1-TYPE-NAMES-let-else-scope` |
| B10 | Nor to its fallback | `(let-else (option.some x) o (return x))` | rejected | `@name.unknown-symbol`, `@error`, the `x` in the fallback | n/a (rejected) | `res::let_else_fallback_does_not_see_pattern` | `V1-TYPE-NAMES-let-else-scope` |
| B11 | A fallback may bind a name the pattern binds, because neither is visible to the other | `(let-else (option.some x) o (do (let x 1i32) (return x))) x` | accepted | none | unchanged | `res::fallback_binder_is_sibling_of_pattern` | `V1-TYPE-NAMES-let-else-scope` |
| B12 | Bound by `let-else`, visible to later elements | `(let-else (option.some x) o (return 0i32)) x` | accepted | none | unchanged | `res::let_else_bindings_visible_after` | `V1-TYPE-NAMES-let-else-scope` |
| B13 | A `lambda` before a `let` cannot capture it; one after can | `(let f (lambda () i32 g)) (let g 1i32)`, and the reverse order | first rejected, second accepted | `@name.unknown-symbol`, `@error`, the `g` in the first lambda | n/a (rejected) | `res::capture_follows_visibility` | `V1-TYPE-NAMES-let-scope` |
| B14 | Discards repeat in one `let` | `(let - 1i32 @- 2i32 -: 3i32)` | accepted | none | unchanged | `res::discard_pairs_repeat` | `V1-TYPE-NAMES-binding-discards` (existing, migrated) |
| B15 | Single-source and workspace checks agree on every redeclaration span | B05 through B08 in a workspace | identical primary and related spans | as the rows | n/a | `wks::redeclaration_parity` | `V1-PROJECT-workspace-check-binding-shadow` (existing, migrated) |
| B16 | A declaration named `never` | `(deftype never i32)`, `(defint never)`, a `where:` name `never` | rejected | `@name.reserved-declaration`, `@error`, the name (reused code) | n/a (rejected) | `syn::never_is_reserved_declaration` | `V1-SRC-DECL-builtin-name-reservation` (extended) |
| B17 | A module value or import alias named `never` | `(def never i32 1i32)` | rejected | `@name.reserved-value-spelling`, `@error`, the name (reused code) | n/a (rejected) | `syn::never_is_reserved_value_spelling` | `V1-SRC-DECL-builtin-name-reservation` (extended) |
| B18 | A local binder named `let-else`, `return`, or `never` | `(let never 1i32)` | behaves as a local binder named `if` or `i32` does today, which the spec forbids and the implementation accepts (question 19) | to be pinned from current behaviour | n/a | `res::keyword_binders_follow_existing_behaviour` | none until question 19 closes |

## C. Typing of `let` and `let-else` (`V1-TYPE-CONTROL`)

| ID | Rule | Input | Expected | Diagnostic | Format | Host test | Corpus |
| --- | --- | --- | --- | --- | --- | --- | --- |
| C01 | A body ending in `let` has type `void` | `(defn f () void (let a 1i32))` | accepted | none | unchanged | `typ::final_let_is_void` | `V1-TYPE-CONTROL-empty-sequences` (existing, migrated) |
| C02 | A final `let` against a non-`void` result | `(defn f () i32 (let a 1i32))` | rejected | `@type.mismatch`, `@error`, the `let` form; related: the written result `i32` | n/a (rejected) | `typ::final_let_mismatches_result` | `V1-TYPE-CONTROL-binding-forms` |
| C03 | Empty `do` and an empty body stay `void` | `(defn e () void (do))` | accepted | none | unchanged | `typ::empty_sequences_are_void` | `V1-TYPE-CONTROL-empty-sequences` |
| C04 | A `let` pattern must be irrefutable | `(let (shape.circle r) v)` against a two-variant enum | rejected | `@pattern.refutable-binding`, `@error`, the pattern; note names the uncovered shape | n/a (rejected) | `typ::let_pattern_refutable` | `V1-TYPE-CONTROL-pattern-refutable-binding` (existing, migrated) |
| C05 | A `let` value has no written expected type | `(let a 1)` | rejected | `@type.ambiguous-inference`, `@error`, the literal | n/a (rejected) | `typ::let_value_has_no_expected_type` | `V1-TYPE-INFER-binding-ambiguity` (existing, migrated) |
| C06 | Fallible values: ignored element rejected, bound or discarded accepted | `(attempt)` non-final; `(let r (attempt))`; `(let - (attempt))` | first rejected, others accepted | `@type.unhandled-fallible`, `@error`, the `(attempt)` expression | n/a (rejected) | `typ::let_consumes_fallible` | `V1-TYPE-CONTROL-fallible-discards` (existing, migrated) |
| C07 | `let-else` pattern must be refutable | `(let-else (option.some v) o (return 0i32))` | accepted | none | unchanged | `typ::let_else_refutable_accepted` | `V1-TYPE-CONTROL-let-else-patterns` |
| C08 | An irrefutable `let-else` pattern | `(let-else x o (return 0i32))`; `(let-else - o f)`; `(let-else (tupleof a b) pair f)`; a single-variant enum constructor; a wrapper constructor | each rejected | `@pattern.irrefutable-let-else`, `@error`, the pattern; note names `let` | n/a (rejected) | `typ::let_else_irrefutable` | `V1-TYPE-CONTROL-let-else-patterns` |
| C09 | An `as` pattern is a valid `let-else` pattern | `(let-else (as i32 n) number (return 0i32)) n` over a two-member union | accepted | none | unchanged | `typ::let_else_as_pattern` | `V1-TYPE-CONTROL-let-else-patterns` |
| C10 | Fallback is a `return` | `(let-else p v (return 0i32))` | accepted | none | unchanged | `typ::fallback_return` | `V1-TYPE-CONTROL-let-else-fallbacks` |
| C11 | Fallback is a call of a `never` function | `(let-else p v (spin))` | accepted | none | unchanged | `typ::fallback_never_call` | `V1-TYPE-CONTROL-let-else-fallbacks` |
| C12 | Fallback is an `if` or `match` whose branches are all `never`, or a `do` ending in `return` | `(let-else p v (if c (spin) (spin)))`, `(let-else p v (do (use 1i32) (return 0i32)))` | accepted | none | unchanged | `typ::fallback_compound_never` | `V1-TYPE-CONTROL-let-else-fallbacks` |
| C13 | Fallback of another type | `(let-else p v 0i32)`, `(let-else p v (try (attempt)))` | rejected | `@type.mismatch`, `@error`, the fallback; no related span | n/a (rejected) | `typ::fallback_not_never` | `V1-TYPE-CONTROL-let-else-fallbacks` |
| C14 | `let-else` as the final element | `(defn f (o (option i32)) void (let-else (option.some v) o (return void)))` | accepted; sequence `void` | none | unchanged | `typ::final_let_else_is_void` | `V1-TYPE-CONTROL-let-else-fallbacks` |
| C15 | `let-else` with a value of `(option never)` | `(let-else (option.some v) (no-values) (return 0i32))` where the `some` variant is uninhabited | accepted: the pattern is refutable (it matches no value), so the fallback is the only path | none | unchanged | `typ::let_else_over_uninhabited_payload` | `V1-TYPE-CONTROL-let-else-patterns` |
| C16 | Effect rows of a `let` value and a `let-else` fallback count toward the ceiling | a value performing an effect outside its ceiling | deferred to the M4 effect step; no case before M4 | none now | n/a | none | none (deferred, listed under Deferred) |

## D. `return` (`V1-TYPE-CONTROL`)

| ID | Rule | Input | Expected | Diagnostic | Format | Host test | Corpus |
| --- | --- | --- | --- | --- | --- | --- | --- |
| D01 | `return` in a `defn`, not in tail position | `(defn f (c bool) i32 (if c (return 1i32) void) 2i32)` | accepted | none | unchanged | `typ::return_in_defn` | `V1-TYPE-CONTROL-return` |
| D02 | In a nested method, an `impl` member, an interface default member, and a `lambda` | the same shape in each | each accepted | none | unchanged | `typ::return_in_every_function_body` | `V1-TYPE-CONTROL-return` |
| D03 | A `return` in a `lambda` exits the lambda only | a `lambda` returning `i32` inside a `defn` returning `str`, with `(return 1i32)` in the lambda | accepted; the `defn` result is unaffected | none | unchanged | `typ::lambda_return_exits_lambda` | `V1-TYPE-CONTROL-return` |
| D04 | The operand is checked at the written result type, so widening fires | `(defn f (c bool) number (if c (return 1i32) void) 2i32)` where `number` is `(union i32 f32)` | the `return` operand widens at the written result; accepted | none | unchanged | `typ::return_operand_widens` | `V1-TYPE-CONVERT-return-widening` |
| D05 | Operand of the wrong type | `(defn f () i32 (if c (return "x") void) 1i32)` | rejected | `@type.mismatch`, `@error`, the operand; related: the written result `i32` | n/a (rejected) | `typ::return_operand_mismatch` | `V1-TYPE-CONTROL-return` |
| D06 | A `void` function returns `void` | `(defn f (c bool) void (if c (return void) void) void)` | accepted | none | unchanged | `typ::return_void` | `V1-TYPE-CONTROL-return` |
| D07 | `return` in a `def` initializer | `(def x i32 (return 1i32))` | rejected; the form has no type, so no position error follows | `@type.invalid-return`, `@error`, the `return` form | n/a (rejected) | `typ::return_in_def_initializer` | `V1-TYPE-CONTROL-return-outside-function` |
| D08 | `return` directly in a `test` body | `(test "t" (return void))` | rejected | `@type.invalid-return`, `@error`, the `return` form | n/a (rejected) | `typ::return_in_test_body` | `V1-TYPE-CONTROL-return-outside-function` (a `workspace-test` tree) |
| D09 | `return` inside a `lambda` in a `test` body | `(test "t" (let f (lambda () i32 (if c (return 1i32) void) 2i32)) (assert.equal-i32 (f) 2i32))` | accepted | none | unchanged | `typ::return_in_test_lambda` | `V1-TYPE-CONTROL-return-outside-function` |
| D10 | Redundant: the final expression | `(defn f () i32 (return 1i32))` | rejected | `@type.redundant-return`, `@error`, the `return` form | n/a (rejected) | `typ::redundant_final` | `V1-TYPE-CONTROL-redundant-return` |
| D11 | Redundant: a branch of a final `if`, each branch counted | `(defn f (c bool) i32 (if c (return 1i32) 2i32))`, `(if c (return 1i32) (return 2i32))` | first: one diagnostic; second: two | `@type.redundant-return`, `@error`, each `return` form | n/a (rejected) | `typ::redundant_final_if` | `V1-TYPE-CONTROL-redundant-return` |
| D12 | Redundant: an arm of a final `match`, and the last element of a final `do` | `(match x - (return 1i32))`, `(do (use 1i32) (return 2i32))` | rejected | `@type.redundant-return`, `@error`, each `return` form | n/a (rejected) | `typ::redundant_final_match_and_do` | `V1-TYPE-CONTROL-redundant-return` |
| D13 | Redundant in a `lambda` body | `(lambda () i32 (return 1i32))` | rejected | `@type.redundant-return`, `@error`, the `return` form | n/a (rejected) | `typ::redundant_final_lambda` | `V1-TYPE-CONTROL-redundant-return` |
| D14 | Not redundant: a `let-else` fallback in a non-final position | `(let-else p v (return 0i32)) x` | accepted | none | unchanged | `typ::fallback_return_not_redundant` | `V1-TYPE-CONTROL-redundant-return` |
| D15 | Not redundant: a `let-else` fallback that is the final element | `(defn f () void (let-else p v (return void)))` | accepted: the fallback is not in tail position | none | unchanged | `typ::final_let_else_return_not_redundant` | `V1-TYPE-CONTROL-redundant-return` |
| D16 | A `never` operand | `(defn f (c bool) i32 (if c (return (spin)) void) 1i32)` | rejected at the operand; the outer `return` is not in tail position and is judged on its own | `@type.unreachable-code`, `@error`, the operand `(spin)` | n/a (rejected) | `typ::return_operand_never` | `V1-TYPE-CONTROL-return` |
| D17 | A `return` nested in a `return` operand | `(defn f () i32 (return (return 1i32)))` | the outer form is redundant; the inner form has a position error and is not also called redundant | in span order, `@type.redundant-return` over the outer form, then `@type.unreachable-code` over the inner form, both `@error` | n/a (rejected) | `typ::nested_return_precedence` | `V1-TYPE-CONTROL-redundant-return` |
| D18 | `return` in a function whose result is `never` | `(defn f () never (return 1i32))` | rejected: the operand does not have type `never`, and the `return` is also in tail position | in span order, `@type.redundant-return` over the form, then `@type.mismatch` over the operand `1i32`, relating `never` | n/a (rejected) | `typ::return_in_never_function` | `V1-TYPE-CONTROL-return` |
| D19 | `try` is unchanged beside `return` | a body using `try` and `return` | `try` still leaves with the container; `return` with the value | none | unchanged | `typ::try_and_return_coexist` | `V1-TYPE-CONTROL-try-and-return` |
| D20 | `return` is the operand-of-`try` position | `(try (return 1i32))` | rejected | `@type.unreachable-code`, `@error`, the `return` form | n/a (rejected) | `typ::return_as_try_operand` | `V1-TYPE-CONTROL-never-positions` |

## E. `never` (`V1-TYPE-CONTROL`, `V1-TYPE-INFER`, `V1-TYPE-INTERFACE`, `V1-TYPE-NOMINAL`)

| ID | Rule | Input | Expected | Diagnostic | Format | Host test | Corpus |
| --- | --- | --- | --- | --- | --- | --- | --- |
| E01 | A `never` function whose final expression is `never` | `(defn spin () never (spin))` | accepted | none | unchanged | `typ::never_function_accepts_never_final` | `V1-TYPE-CONTROL-never-functions` |
| E02 | A `never` function with another final expression | `(defn f () never 1i32)` | rejected | `@type.mismatch`, `@error`, the final expression; related: the written `never` | n/a (rejected) | `typ::never_function_rejects_other_final` | `V1-TYPE-CONTROL-never-functions` |
| E03 | A `never` function with an empty body | `(defn f () never)` | rejected | `@type.mismatch`, `@error`, at the span the existing empty-body mismatch uses (pin from the current case before coding) | n/a (rejected) | `typ::never_function_empty_body` | `V1-TYPE-CONTROL-never-functions` |
| E04 | Acceptance at expected types | `(defn a () i32 (spin))`, `(defn b () str (spin))`, `(defn c () (option i32) (spin))`, `(defn d () void (spin))` | each accepted | none | unchanged | `typ::never_accepted_at_expected_types` | `V1-TYPE-CONTROL-never-acceptance` |
| E05 | A `never` call in a `def` initializer | `(def x i32 (spin))` | rejected | `@type.unreachable-code`, `@error`, the call | n/a (rejected) | `typ::never_in_def_initializer` | `V1-TYPE-CONTROL-never-positions` |
| E06 | Branch join: one `never` branch | `(defn f (c bool) i32 (if c 1i32 (spin)))` | accepted; the form has type `i32` | none | unchanged | `typ::join_skips_never_branch` | `V1-TYPE-CONTROL-never-join` |
| E07 | Branch join: all `never` | `(defn f (c bool) never (if c (spin) (spin)))`; a `match` with all arms `never` | accepted; type `never` | none | unchanged | `typ::join_all_never` | `V1-TYPE-CONTROL-never-join` |
| E08 | Branch join checks the other branch at the written expected type | `(defn f (c bool) number (if c 1i32 (spin)))` | accepted; `1i32` widens to the union at the written result | none | unchanged | `typ::join_remaining_branch_widens` | `V1-TYPE-CONTROL-never-join` |
| E09 | The join is not a least upper bound | `(if c 1i32 2.0f32)` with no written union | still rejected | `@type.mismatch`, `@error`, the `if` form (existing behaviour) | n/a (rejected) | `typ::join_is_not_lub` | `V1-TYPE-CONVERT-widening-rejections` (existing, migrated) |
| E10 | Admitted positions | a `never` call as a sequence element, an `if` branch, a `match` arm result, a `let-else` fallback, and a final expression | each accepted | none | unchanged | `typ::never_admitted_positions` | `V1-TYPE-CONTROL-never-positions` |
| E11 | Rejected positions | a `never` call as an application operand, a `let` value, an `if` condition, a `match` subject, a `try`, `as`, `return`, and `tupleof` operand, and an `array.of` operand | nine diagnostics, in source order | `@type.unreachable-code`, `@error`, each `never` expression | n/a (rejected) | `typ::never_rejected_positions` | `V1-TYPE-CONTROL-never-positions` |
| E12 | An element after a `never` element | `(spin) 1i32 (use 2i32)` | the first following element reported once; a type error in a later element is still reported | `@type.unreachable-code`, `@error`, the `1i32` element; related: the `(spin)` element | n/a (rejected) | `typ::element_after_never` | `V1-TYPE-CONTROL-never-positions` |
| E13 | `never` writable wherever a type is | parameter `(x never)`, result `(result i32 never)`, `(array never)`, `(option never)`, `(fn () never)`, `types: (never)`, an `as` type | each accepted | none | unchanged | `typ::never_in_every_type_position` | `V1-TYPE-NOMINAL-never-type-positions` |
| E14 | A parameter that only a diverging operand could fix | `(pick (spin))` for `(defn pick (a t) t where: (t any) a)` | the operand is rejected and the parameter stays ambiguous | `@type.ambiguous-inference`, `@error`, the application; then `@type.unreachable-code`, `@error`, the operand `(spin)` (the application starts first, so it sorts first) | n/a (rejected) | `typ::diverging_operand_fixes_nothing` | `V1-TYPE-INFER-never-inference` |
| E15 | `never` written explicitly is accepted | `(none-of types: (never))`; `(as (option never) (option.none))`; a written result `(result i32 never)` for `(result.ok 1i32)` | each accepted | none | unchanged | `typ::never_fixed_by_writing` | `V1-TYPE-INFER-never-inference` |
| E16 | An uninferable `never` is not defaulted | `(none-of)` with no expected type | rejected | `@type.ambiguous-inference`, `@error`, the application | n/a (rejected) | `typ::no_never_fallback` | `V1-TYPE-INFER-never-inference` |
| E17 | `never` against bound `any` | a `deftype box` bounded by `any` applied with `types: (never)` | accepted | none | unchanged | `typ::never_satisfies_any` | `V1-TYPE-INTERFACE-never-bounds` |
| E18 | `never` against any other interface | the same with a bound of `ordered` | rejected | `@type.unsatisfied-bound`, `@error`, the type argument; related: the bound | n/a (rejected) | `typ::never_fails_other_bounds` | `V1-TYPE-INTERFACE-never-bounds` |
| E19 | `never` as a dict key | `(dict never i32)` | rejected | `@type.invalid-dict-key`, `@error`, the key type expression | n/a (rejected) | `typ::never_is_not_a_dict_key` | `V1-TYPE-INTERFACE-never-bounds` |
| E20 | No implementation can be written for `never` | an `impl` block targeting `never` inside a `defint` | rejected, although `never` is a builtin type that the `defint` placement would otherwise admit | `@name.wrong-entity-kind`, `@error`, the target | n/a (rejected) | `typ::no_impl_for_never` | `V1-TYPE-INTERFACE-never-bounds` |
| E21 | Inhabitedness, structural | `never`; `(tuple i32 never)`; a record with a `never` field; a wrapper over `never`; an enum all of whose payloads are `never`; a union of such members | each uninhabited | none | n/a | `typ::uninhabited_types` (unit test over the predicate) | `V1-TYPE-CONTROL-inhabitedness` (through the exhaustiveness rows below) |
| E22 | Inhabited types | `(array never)`; `(option never)`; an enum with a `void` payload variant; `(fn () never)`; an interface value; a generic name | each inhabited | none | n/a | `typ::inhabited_types` | `V1-TYPE-CONTROL-inhabitedness` |
| E23 | A recursive nominal type is assumed inhabited | a recursive declared type examined while being examined | the predicate terminates and answers inhabited | the declaration's own `@type.infinite-size` when it applies | n/a | `typ::inhabitedness_terminates_on_recursion` | `V1-TYPE-CONTROL-inhabitedness` |
| E24 | Exhaustiveness: only the inhabited arm is needed | `(match r (result.ok v) v)` for `r` of type `(result i32 never)` | accepted | none | unchanged | `typ::match_skips_uninhabited_variant` | `V1-TYPE-CONTROL-never-exhaustiveness` |
| E25 | An arm for the uninhabited variant is unreachable | `(match r (result.ok v) v (result.err e) 0i32)` | rejected | `@pattern.unreachable-arm`, `@error`, the arm's pattern `(result.err e)`; no related span | n/a (rejected) | `typ::uninhabited_arm_unreachable` | `V1-TYPE-CONTROL-never-exhaustiveness` |
| E26 | The uncovered shape skips uninhabited variants | an enum `(enum a never b i32 c i32)` matched with only `b` | rejected | `@pattern.non-exhaustive`, `@error`, the `match` form; the note names `(c -)`, not `(a -)` | n/a (rejected) | `typ::uncovered_shape_skips_uninhabited` | `V1-TYPE-CONTROL-never-exhaustiveness` |
| E27 | `let` is irrefutable over an uninhabited variant | `(let (result.ok value) r)` for `r` of type `(result i32 never)` | accepted | none | unchanged | `typ::let_irrefutable_over_never_error` | `V1-TYPE-CONTROL-never-exhaustiveness` |
| E28 | A scrutinee that is itself uninhabited | a `match` whose scrutinee type is `(tuple i32 never)` with a `-` arm | the arm is unreachable; the same value bound by `let` with `-` is irrefutable | `@pattern.unreachable-arm`, `@error`, the arm pattern | n/a (rejected) | `typ::uninhabited_scrutinee` | `V1-TYPE-CONTROL-never-exhaustiveness` |
| E29 | A `result` that cannot hold an error is not fallible | `(attempt-never) 1i32` where `attempt-never` returns `(result i32 never)` | accepted | none | unchanged | `typ::result_never_error_not_fallible` | `V1-TYPE-CONTROL-never-fallibility` |
| E30 | Other `result` values stay fallible | the same with `(result i32 str)`, and with `(result never str)` | rejected | `@type.unhandled-fallible`, `@error`, the expression | n/a (rejected) | `typ::result_other_still_fallible` | `V1-TYPE-CONTROL-never-fallibility` |
| E31 | `try` is unchanged for such a result | `(try r)` for `(result i32 never)` in a function returning `(result i32 never)`, and in one returning `i32` | first accepted, second rejected | `@type.invalid-try`, `@error`, the `try` form (existing behaviour) | n/a | `typ::try_over_never_error_unchanged` | `V1-TYPE-CONTROL-never-fallibility` |
| E32 | `never` is an `@never` primitive for queries and encodings | a function with a `never` result, queried for its type; an observation of a `(result i32 never)` value | the query type is `primitive` named `never`; the observation encodes `@never` only inside the applied type | none | unchanged | `wks::never_type_fact`, `int::never_inside_type_encoding` | `V1-TOOL-never-type-fact`, `V1-RUNTIME-never-type-encoding` |
| E33 | An entry may not return `never` | an entry `(defn main () never (spin))`; an entry returning `(result void never)` | rejected | `@project.invalid-entry-signature`, `@error`, the entry's declaration (existing behaviour, new input) | n/a (rejected) | `wks::entry_never_is_invalid` | `V1-PROJECT-entry-never` |

## F. Evaluation and tail position (`V1-RUNTIME`)

| ID | Rule | Input | Expected | Diagnostic | Format | Host test | Corpus |
| --- | --- | --- | --- | --- | --- | --- | --- |
| F01 | `let` evaluates in order and binds for the rest of the sequence | a body that uses an earlier pair and then a later element | the result follows the order | none | unchanged | `int::let_order_and_scope` | `V1-RUNTIME-bindings` (existing, migrated) |
| F02 | `let-else` match path | a value that matches | the bindings are used; the fallback does not run | none | unchanged | `int::let_else_match_path` | `V1-RUNTIME-let-else` |
| F03 | `let-else` mismatch path | a value that does not match | the fallback `return` leaves with its value | none | unchanged | `int::let_else_mismatch_path` | `V1-RUNTIME-let-else` |
| F04 | `return` skips the rest of the activation | `(if c (return 1i32) void) 2i32` with `c` true and false | `1i32` and `2i32` | none | unchanged | `int::return_skips_rest` | `V1-RUNTIME-return` |
| F05 | A `return` in a `lambda` leaves only the lambda | an outer function continuing after calling a lambda that returns early | the outer function's later elements run | none | unchanged | `int::lambda_return_leaves_lambda_only` | `V1-RUNTIME-return` |
| F06 | `return` attaches the written result type's widening | `(return 1i32)` where the result is a union | the observed value is the union with the member discriminant | none | unchanged | `int::return_widens_at_runtime` | `V1-RUNTIME-return` |
| F07 | The operand of `return` is in tail position | `(let-else p v (return (loop next)))` recursing one million times | completes without `@runtime.host-stack-exhausted` | none | unchanged | `int::return_operand_is_tail_call` | `V1-RUNTIME-return-tail-call` |
| F08 | `let` and `let-else` values and the fallback are not tail | `(let r (loop next))` recursing deeply | `@runtime.host-stack-exhausted` as the existing non-tail case does | `@runtime.host-stack-exhausted`, `@error`, unlocated `0..0` (existing code) | n/a | `int::let_value_is_not_tail` | `V1-RUNTIME-tail-negative` (existing, migrated) |
| F09 | A `return` in a tail position of a branch does not change tail calls | the final `if` with a call in each branch | the calls remain tail calls | none | unchanged | `int::final_branch_calls_remain_tail` | `V1-RUNTIME-widened-tail-call` (existing, migrated) |
| F10 | A `never` function is never run to completion | a program whose diverging call is not reached | evaluates normally; the call is never made | none | unchanged | `int::never_call_on_unreached_branch` | `V1-RUNTIME-never-unreached` |

## G. Tooling, queries, and schemas (`V1-TOOL`, `V1-DIAG`)

| ID | Rule | Input | Expected | Diagnostic | Format | Host test | Corpus |
| --- | --- | --- | --- | --- | --- | --- | --- |
| G01 | Context of a `let` pattern and value | a position inside a pair | `context` is `let-value`; the pattern's `role` is `@local-binding` | none | unchanged | `wks::let_value_context` | `V1-TOOL-workspace-position-query` (existing, migrated) |
| G02 | Context of a `let-else` pattern and value | a position inside either | `let-value` | none | unchanged | `wks::let_else_value_context` | `V1-TOOL-let-else-contexts` |
| G03 | Context of a `let-else` fallback | a position inside the fallback | `let-else-fallback` | none | unchanged | `wks::let_else_fallback_context` | `V1-TOOL-let-else-contexts` |
| G04 | Context of a `return` operand | a position inside the operand | `return-operand`; the expected type is the written result type | none | unchanged | `wks::return_operand_context` | `V1-TOOL-let-else-contexts` |
| G05 | An element after a `let` keeps the sequence's context | a position in an element after a `let` in a `defn` body and in a `do` | the context of the `defn` body, or of the enclosing `do` | none | unchanged | `wks::element_after_let_keeps_context` | `V1-TOOL-let-else-contexts` |
| G06 | The vocabulary has no `let-body` | the schema enum and any result | `let-body` never appears and the schema rejects it | none | n/a | `wks::context_vocabulary_is_closed` | `V1-TOOL-let-else-contexts` |
| G07 | `visibleLocals` after a `let` | a position after a two-pair `let` | both bindings, in order; a position inside the second value sees the first only | none | unchanged | `wks::visible_locals_follow_sequence` | `V1-TOOL-let-else-contexts` |
| G08 | Registry entries | the five new codes and the reused ones | each in the registry with the table's level, domain, and fix capability `@none`; the retired-form summary no longer mentions `return` | none | n/a | `dia::registry_matches_spec_table` (existing, extended) | `V1-DIAG-registry-coverage` (existing, extended) |
| G09 | `vibra query` of a new code | `vibra query @type.unreachable-code --include diagnostic` | the registry entry | none | n/a | `wks::query_new_diagnostic_codes` | `V1-DIAG-registry-query` |
| G10 | Schema validation | a diagnostic carrying each new code and a position result carrying each new context | validates against the versioned schemas | none | n/a | `wks::schemas_accept_new_codes_and_contexts` | `V1-DIAG-schemas` |

## H. Formatter (`V1-SRC-FMT`)

All rows run the real formatter handler (`format` operation) with a
`formatted.vib` snapshot and assert idempotence by formatting the output again.
Widths are 88 columns.

| ID | Rule | Input | Expected | Format | Host test | Corpus |
| --- | --- | --- | --- | --- | --- | --- |
| H01 | A `let` that fits is one line | `(let   a   1i32)` in a body | `(let a 1i32)` | `(let a 1i32)` | `fmt::let_fits_on_one_line` | `V1-SRC-FMT-let-let-else` |
| H02 | A two-pair `let` is always multiline, even when it would fit | `(let a 1i32   b 2i32)` | head alone, one pair per line | `(let` / `  a 1i32` / `  b 2i32)` | `fmt::two_pair_let_is_multiline` | `V1-SRC-FMT-let-let-else` |
| H03 | A three-pair `let`: head alone, one pair per line | three long pairs | head on its line, each pair on its own line at two spaces | the spec's `(let` / pair / pair / `count (entry-count entry))` example | `fmt::multi_pair_let_one_pair_per_line` | `V1-SRC-FMT-let-let-else` |
| H04 | A single pair whose pair does not fit: pattern line, value line | the spec's `summary` example | `(let` / `summary` / the value and the closing delimiter | as the spec example | `fmt::single_pair_let_splits_pattern_and_value` | `V1-SRC-FMT-let-let-else` |
| H05 | A `let-else` that fits is one line | `(let-else   (option.some second) (items 1u64) (return (result.ok entry)))` | one line | one line | `fmt::let_else_fits_on_one_line` | `V1-SRC-FMT-let-let-else` |
| H06 | A `let-else` whose pattern and value fit together | the spec's `picked` example | head alone; pattern and value on one line; fallback on the next | as the spec example | `fmt::let_else_pattern_and_value_share_a_line` | `V1-SRC-FMT-let-let-else` |
| H07 | A `let-else` whose pattern and value do not fit together | the spec's `chosen` example | head alone; pattern, value, and fallback each on a line | as the spec example | `fmt::let_else_three_lines` | `V1-SRC-FMT-let-let-else` |
| H08 | A multiline `do` | a `do` of three long elements | head alone; each element on its own line at two spaces | `(do` / three lines / closing delimiter on the last | `fmt::multiline_do_one_element_per_line` | `V1-SRC-FMT-let-let-else` |
| H09 | A line comment between pairs | a comment before the second pair | the form is multiline; the comment keeps its own line at the pair indentation | as written, normalized | `fmt::comment_between_pairs` | `V1-SRC-FMT-let-comments` |
| H10 | A comment written on a pair's line | `(let a 1i32 ; note` then `b 2i32)` | the comment moves to the line after the form it was on | as the existing comment rule | `fmt::comment_on_pair_line` | `V1-SRC-FMT-let-comments` |
| H11 | A comment inside a `let-else` | a comment before the fallback | the comment keeps its own line before the fallback | as written, normalized | `fmt::comment_before_fallback` | `V1-SRC-FMT-let-comments` |
| H12 | A `let` nested in a `do` in a `match` arm | an arm result `(do (let a 1i32) a)` too long for the line | the `do` breaks as H08 and the arm stays one arm per line | as written, normalized | `fmt::let_inside_do_inside_match_arm` | `V1-SRC-FMT-let-let-else` |
| H13 | Every spec example round-trips | each `vibra` fence of the source and type chapters that holds the new forms | the formatter leaves the canonical ones unchanged | unchanged | `fmt::spec_examples_are_canonical` | `V1-SRC-FMT-spec-examples` |
| H14 | A `return` is an ordinary list | `(return   (some-call   a   b))` | `(return (some-call a b))`; breaks like any one-operand list when long | one line, or head alone and the operand below | `fmt::return_is_a_plain_list` | `V1-SRC-FMT-let-let-else` |

## Evidence

Every row of sections A through H maps to a passing host test, a passing corpus
case, or an entry below under "Rows not done". Host tests live in the files named
under Conventions; corpus cases are under `conformance/cases/`. Diagnostics are
listed in emission order, which is the order the checker produces them: the
single-source path does not sort, so a position error found while an operand is
checked precedes the application error that follows it.

| Rows | Host tests | Corpus cases |
| --- | --- | --- |
| A01-A05 | `syn::let_forms_are_body_elements_with_one_or_more_pairs`, `syn::a_let_has_pairs_in_the_ast` | `V1-SRC-EXPR-let-pairs`, `V1-SRC-EXPR-body-sequences`, `V1-SRC-EXPR-let-else` |
| A06-A12 | `syn::a_binding_form_anywhere_else_is_misplaced`, `syn::a_misplaced_form_is_accepted_once_wrapped_in_do` | `V1-SRC-EXPR-misplaced-binding`, `V1-SRC-EXPR-misplaced-binding-do` |
| A13-A18, A21 | `syn::binding_and_return_arity_is_checked`, `syn::a_rejected_form_leaves_the_next_declaration_readable` | `V1-SRC-EXPR-binding-arity` |
| A19-A20 | `syn::the_old_let_shape_has_no_bridge`, `syn::a_rejected_form_leaves_the_next_declaration_readable` | `V1-SRC-EXPR-old-let-shape`, `V1-SRC-EXPR-misplaced-binding` |
| A23, A25 | `syn::return_is_no_longer_retired_but_the_loop_forms_are` | `V1-SRC-EXPR-retired-form`, `V1-SRC-EXPR-pattern-retired-form` (existing, migrated) |
| B01-B04, B13 | `res::a_binding_reaches_later_pairs_and_later_elements_only` | `V1-TYPE-NAMES-let-scope` |
| B05-B08 | `res::a_binding_that_outlasts_its_form_makes_a_later_repeat_a_redeclaration` | `V1-TYPE-NAMES-let-redeclaration` |
| B09-B12 | `res::a_let_else_pattern_binds_after_the_form_and_not_in_its_value_or_fallback` | `V1-TYPE-NAMES-let-else-scope` |
| B14-B15 | `res::discard_pairs_repeat_freely` | `V1-TYPE-NAMES-binding-discards`, `V1-PROJECT-workspace-check-binding-shadow` (existing, migrated) |
| B16-B17 | `syn::never_is_reserved_as_a_declaration_and_a_value_spelling` | `V1-SRC-DECL-never-reserved` |
| C01-C06 | `typ::a_final_let_is_void`, `typ::let_patterns_must_be_irrefutable_and_values_have_no_expected_type`, `typ::a_bound_or_discarded_result_is_handled` | `V1-TYPE-CONTROL-binding-forms` |
| C07-C15 | `typ::let_else_needs_a_refutable_pattern_and_a_never_fallback` | `V1-TYPE-CONTROL-let-else-patterns`, `V1-TYPE-CONTROL-let-else-fallbacks` |
| D01-D06, D16 | `typ::return_exits_the_innermost_function_at_its_written_result_type` | `V1-TYPE-CONTROL-return`, `V1-TYPE-CONVERT-return-widening` |
| D07-D09 | `typ::return_outside_a_function_is_invalid` | `V1-TYPE-CONTROL-return-outside-function`, `V1-TYPE-CONTROL-return-in-test-body`, `V1-TYPE-CONTROL-return-in-test-lambda` |
| D10-D15, D17-D18 | `typ::a_return_in_tail_position_is_redundant`, `typ::a_return_nested_in_a_return_reports_the_position_error_once`, `typ::a_never_function_needs_a_never_final_expression` | `V1-TYPE-CONTROL-redundant-return` |
| D19-D20 | `typ::never_is_unreachable_code_outside_its_admitted_positions` | `V1-TYPE-CONTROL-try-and-return`, `V1-TYPE-CONTROL-never-positions` |
| E01-E04 | `typ::a_never_function_needs_a_never_final_expression`, `typ::never_is_admitted_at_expected_types_and_skipped_in_joins` | `V1-TYPE-CONTROL-never-functions`, `V1-TYPE-CONTROL-never-acceptance` |
| E05-E12 | `typ::never_is_unreachable_code_outside_its_admitted_positions`, `typ::never_is_admitted_at_expected_types_and_skipped_in_joins` | `V1-TYPE-CONTROL-never-join`, `V1-TYPE-CONTROL-never-positions` |
| E13-E16 | `typ::never_is_written_wherever_a_type_is_and_is_never_inferred` | `V1-TYPE-NOMINAL-never-type-positions`, `V1-TYPE-INFER-never-inference` |
| E17-E20 | `typ::never_satisfies_any_and_nothing_else` | `V1-TYPE-INTERFACE-never-bounds` |
| E21-E28 | `typ::inhabitedness_is_structural`, `typ::inhabitedness_terminates_on_recursive_types`, `typ::uninhabited_types_need_no_arm` | `V1-TYPE-CONTROL-inhabitedness`, `V1-TYPE-CONTROL-never-exhaustiveness`, `V1-TYPE-CONTROL-never-spec-example` |
| E29-E31 | `typ::a_result_that_cannot_fail_is_not_fallible` | `V1-TYPE-CONTROL-never-fallibility` |
| E32 | `schema::never_is_a_primitive_type_name` | `V1-RUNTIME-never-type-encoding` |
| E33 | none beyond the case | `V1-PROJECT-entry-never` |
| F01-F07, F10 | `int::let_binds_in_order_for_the_rest_of_the_sequence`, `int::let_else_takes_the_match_path_or_the_fallback`, `int::return_skips_the_rest_and_a_lambda_return_leaves_only_the_lambda`, `int::the_operand_of_return_widens_at_the_written_result_type`, `int::the_operand_of_return_is_a_tail_call`, `int::a_diverging_call_on_an_untaken_branch_is_never_made` | `V1-RUNTIME-let-else`, `V1-RUNTIME-return`, `V1-RUNTIME-return-tail-call`, `V1-RUNTIME-never-unreached` |
| F08-F09 | existing tail-call host tests | `V1-RUNTIME-tail-negative`, `V1-RUNTIME-widened-tail-call` (existing, migrated) |
| G01-G05, G07 | none beyond the case | `V1-TOOL-let-else-contexts` |
| G06 | `schema::the_context_vocabulary_has_the_new_contexts_and_no_let_body` | `V1-TOOL-let-else-contexts` |
| G08 | `crates/vibra-conformance/tests/diagnostic_registry.rs` (existing, unedited) and the registry unit test | `V1-DIAG` registry coverage (existing) |
| H01-H14 | `fmt::*` in `crates/vibra-fmt/tests/pre_m4_bindings.rs` | `V1-SRC-FMT-let-let-else`, `V1-SRC-FMT-let-comments` |

Crate prefixes `syn`, `res`, `typ`, `fmt`, `int`, and `schema` are the test files
`crates/vibra-syntax/tests/pre_m4_bindings.rs`,
`crates/vibra-resolve/tests/pre_m4_bindings.rs`,
`crates/vibra-types/tests/pre_m4_bindings.rs`,
`crates/vibra-fmt/tests/pre_m4_bindings.rs`,
`crates/vibra-conformance/tests/pre_m4_runtime.rs`, and
`crates/vibra-schema/tests/pre_m4_vocabulary.rs`.

### Rows not done

- **A22** changed: the declaration that holds a misplaced form is checked no
  further, so a misplaced operand does not also yield a name error (the spec
  states this).
- **A24** has no dedicated case; the reserved heads are exercised by the arity
  and misplacement cases.
- **B18** is a known gap, pinned by no case (below).
- **C16** is deferred to the M4 effect step.
- **G09** is not applicable: the CLI has no `query` command until M6.
- **G10** has no dedicated test: the query snapshots are produced by the real
  handler and the schema enum change is checked by `schema::*`.
- **E20** reports at the `impl` block rather than the target, as an anonymous
  target does today.

## Stage B tasks, in order

1. **Baseline.** On a clean branch, run the workspace tests, the conformance
   runner, and `cargo fmt`/`clippy` as CI does, and record the counts. Decide the
   branch model (question 20).
2. **`vibra-diagnostics`.** Add the five codes in the spec table's order, with
   domain, level, fix `@none`, and summaries; drop `return` from the
   retired-form summary. Update the `diagnostic-registry-entry` expectations and
   the registry-agreement test.
3. **`vibra-syntax` (reader and AST).** Add `never` to the builtin type names;
   make `let-else` and `return` reserved heads and remove `return` from the
   retired heads; introduce a body-sequence node
   (`body-element = expr | let | let-else`) for `defn`, nested-method, `impl`
   member, default member, `lambda`, `do`, and `test` bodies; give `let` its
   pair list and add `let-else` and `return` nodes; emit
   `@syntax.misplaced-binding` where a binding form is read as an expression and
   `@syntax.invalid-form` for the arity rules; keep recovery per the execution
   model (every malformed path consumes input or returns). This is the widest
   API change, so land it with the callers compiling (steps 4-8 follow it).
4. **`vibra-fmt`.** Add the `do`, `let`, and `let-else` layouts of the spec,
   with the comment placements; keep the existing declaration and `match` layout
   tests green.
5. **`vibra-resolve`.** Scope bindings to the body sequence; add earlier-pair
   visibility; make the `let-else` pattern invisible to its value and fallback;
   keep the redeclaration spans and the single-source/workspace parity.
6. **`vibra-types`.** Add `Never`: type expressions, the type model, reserved
   spelling; inhabitedness predicate; the exhaustiveness engine's skip of
   uninhabited variants, members, and positions and the unreachable-arm rule;
   body-sequence typing; `let`, `let-else`, and `return` checking with a stack of
   enclosing function result types; the `never` positions and the after-`never`
   rule; branch join and acceptance; the inference rule; fallibility; the
   interface, bound, and dict-key exclusions; redundant-`return` detection from
   the shared tail-position function.
7. **`vibra-ir`.** Replace the `let`-with-body node by a sequence of bindings
   and a result, add `let-else` and `return`, add a `never` type, keep origin
   spans, and update the tail-position and recursive-group analysis to the
   runtime chapter's bullets (the `return` operand is tail; `let` and `let-else`
   parts are not).
8. **`vibra-interp`.** Execute sequences with an extending environment, `return`
   as an exit of the innermost activation, and `let-else`; keep the tail-call
   path working for a `return` operand; keep the host-stack bound for non-tail
   `let` values.
9. **`vibra-schema` and `vibra-workspace`.** Replace `let-body` in the context
   enum, add `let-else-fallback` and `return-operand`, add `never` to the
   primitive names, update scopes for `visibleLocals`, the index text, and the
   canonical type encoding; update every JSON schema that enumerates them.
10. **Embedded standard library, examples, and corpus migration** under the
    migration plan below, formatter pass first, then rewrite, then check.
11. **Host tests** for rows A through H.
12. **Corpus cases** for every row that names one, independently authored from
    the spec text and reviewed against it before commit.
13. **Inventory and docs.** Regenerate `docs/roadmap/milestone-1/syntax-examples.tsv`
    with the evidence test's expected rows, keep or update the test's fence
    count of 50 (Stage A kept it at 50), update the implementation-status
    banners in the charter and chapters, and record the evidence for this step.
14. **Gates.** Run the host suite, the corpus runner, formatter idempotence
    across stdlib, examples, and conformance source, and the registry and schema
    checks; record commands and counts in the handoff.

## Migration plan for existing `(let` uses

The measured baseline on `origin/main`, counting the regular expression
`\(let[\s)]` over tracked files outside `archive/`, is 230 uses. This differs from
the brief's 217 because the brief's count method is not recorded; Stage B must
recount with its own method before and after.

| Area | Files | Uses | Notes |
| --- | --- | --- | --- |
| `stdlib/src/std` | 3 (`text.vib` 12, `bytes.vib` 7, `char.vib` 1) | 20 | the wrapper-unwrapping chains are the main shape |
| `examples/` | 3 (`m3-catalog`, `stage-3a-config`) | 7 | |
| `conformance/cases/` | 66 files in 64 cases | 108 | spans in `case.toml` shift when text changes |
| `crates/**/tests` | 18 | 72 | Rust string fixtures with byte-offset expectations |
| `crates/**/src` | 4 (`vibra-types`, `vibra-interp`, `vibra-fmt`, `m1-fuzz`) | 12 | embedded source strings, doc comments, and fuzz seeds |
| `docs/spec` | 3 | 8 | rewritten in Stage A |
| `docs/roadmap` | 2 | 3 (the examples inventory TSV 2, `milestone-3/07-failure.md` 1) | the TSV is regenerated; historical step documents stay as written |

Rewrite rules:

- **R1 flatten.** `(let p v e1 … en)` as the tail of a body sequence becomes
  `(let p v)` followed by `e1 … en` as elements of that sequence.
- **R2 merge chains.** `(let p1 v1 (let p2 v2 body))` becomes
  `(let p1 v1 p2 v2)` then `body`, because a later value sees earlier pairs.
- **R3 wrap.** A `let` whose old position is not a body element (an application
  operand, an `if` branch, a `match` arm, a `def` initializer, a `let` value)
  becomes `(do (let p v) body…)`.
- **R4 empty body.** `(let p v)` with no body was `void`; it stays valid as a
  final element, and `(let - 1i32 (do))` becomes `(let - 1i32)` then `(do)` only
  if the case needs its `void` explicitly.
- **R5 retired shapes.** The `(let (bind x) …)` case keeps `(bind x)` as the
  rejected input and moves its `let` to the new shape.

Hazards, each of which needs an explicit check rather than trust:

1. **Wider scope.** A binding now lasts to the end of its sequence, so two
   `let` forms that reused a name as siblings become redeclarations. Fix by
   renaming, or by wrapping one in `do`.
2. **Silent reinterpretation.** An old `let` whose body has an even number of
   expressions parses under the new grammar as more pairs. Every migrated site
   is checked after rewriting, and a site that checks without a rewrite is read
   by a person.
3. **Corpus spans.** Every `case.toml` span in a migrated case is recomputed from
   the new input by reading it. Do not accept regenerated snapshots wholesale
   (`execution.md`).
4. **Rust fixtures.** Byte offsets in host tests that embed source are
   recomputed the same way, and a fixture that tests the old shape on purpose
   (retired forms, odd arity) is rewritten as the new rejected shape from
   section A.
5. **Hand-formatted sources.** `vibra fmt` today lays out the standard library
   differently from its checked-in text (the checked-in `let` chains hug their
   operands; the formatter hangs them). Run `vibra fmt` over `stdlib/` and
   `examples/` in one mechanical commit before the rewrite so the rewrite diff
   stays readable, or record that formatting the whole tree is out of scope
   (question 16).
6. **Inventory.** The evidence test compares the TSV with the spec by line
   number, so every spec edit regenerates it.

## Contract questions Stage A could not close

The full numbered list, with recommendations, is the Stage A report. These are
the ones the Stage B rows depend on:

- Question 2 (fallback mismatch reuses `@type.mismatch`) fixes rows C13 and E02.
- Question 5 (written expected types can fix an argument to `never`) fixes E14
  through E16.
- Question 6 (both diagnostics for a diverging operand) fixes E14.
- Question 7 (an arm for an uninhabited variant is unreachable) fixes E25 and
  E28.
- Question 8 (`never` has no terminating source before M4's host operations)
  limits F10 and the `never` corpus to type-level cases.
- Question 11 (the old `let` shape has no bridge and can parse as more pairs)
  fixes A19 and the migration hazards.
- Question 12 (context names) fixes G01 through G06.
- Question 13 (`never` as a `primitive` query kind) fixes E32.
- Question 16 (formatter behaviour on a document with an AST-level error)
  fixes the `n/a (rejected)` format column.

## Deferred

- Effect-ceiling checks of a `let` value, a `let-else` fallback, and a `return`
  operand land with the M4 effect step (row C16).
- A terminating source of `never` (a host `exit`, or a pure intrinsic) is an M4
  decision (question 8).
- Wasm lowering of `return`, `let-else`, and sequences is M4 work; the
  interpreter is the oracle until then.

## Known gaps

- The spec says keywords and primitive type names cannot be rebound, but the
  implementation accepts a local binder named `if` or `i32`, and so also one
  named `let-else`, `return`, or `never`. This revision pins that behaviour
  (row B18) and does not fix it.
- `vibra fmt` reports `@project.io-error: formatted document did not reparse
  cleanly` for a document that carries an AST-level error. This revision does
  not change that contract.
- `never` has no terminating source in v1; a host `exit` or pure intrinsic is
  left to M4.
