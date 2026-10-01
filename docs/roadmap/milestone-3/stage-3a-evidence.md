# M3 Stage 3A evidence

Captured 2026-09-30 for [Step 9](09-stage-3a-evidence.md). This step claims
no new behavior. It records the demo, the corpus sub-gate, and the map from
the conformance chapter's Stage 3A coverage clauses to case IDs. Its own PR
adds two findings the audit surfaced:

- `any` in type position now reports `@tool.unavailable`, as a Stage 3B form
  must, instead of `@name.unknown-symbol`.
- An unknown member of a declared type is reported once in the workspace
  path. The resolver leaves such a path to the checker, as the single-file
  path already did.

## Integration

| Step | PR and merge | CI run on the merged head |
| --- | --- | --- |
| 1 | [#307](https://github.com/nahharris/vibra/pull/307), `23d5067` | [36354226060](https://github.com/nahharris/vibra/actions/runs/36354226060) |
| 2 | [#308](https://github.com/nahharris/vibra/pull/308), `bd15e36` | [36558418854](https://github.com/nahharris/vibra/actions/runs/36558418854) |
| 3 | [#310](https://github.com/nahharris/vibra/pull/310), `cbafc12` | [36601830394](https://github.com/nahharris/vibra/actions/runs/36601830394) |
| 4 | [#311](https://github.com/nahharris/vibra/pull/311), `15f394d` | [36634355914](https://github.com/nahharris/vibra/actions/runs/36634355914) |
| 4a | [#312](https://github.com/nahharris/vibra/pull/312), `6d90cde` | [36652796321](https://github.com/nahharris/vibra/actions/runs/36652796321) |
| 4b | [#313](https://github.com/nahharris/vibra/pull/313), `32649ca` | [36659697686](https://github.com/nahharris/vibra/actions/runs/36659697686) |
| 5 | [#314](https://github.com/nahharris/vibra/pull/314), `f696089` | [36661462021](https://github.com/nahharris/vibra/actions/runs/36661462021) |
| 6 | [#315](https://github.com/nahharris/vibra/pull/315), `c653f39` | [36662910499](https://github.com/nahharris/vibra/actions/runs/36662910499) |
| 7 | [#316](https://github.com/nahharris/vibra/pull/316), `6f8bfe3` | [36663848099](https://github.com/nahharris/vibra/actions/runs/36663848099) |
| 8 | [#317](https://github.com/nahharris/vibra/pull/317), `b0c3092` | [36665171650](https://github.com/nahharris/vibra/actions/runs/36665171650) |
| 8b | [#318](https://github.com/nahharris/vibra/pull/318), `5407632` | [36684304542](https://github.com/nahharris/vibra/actions/runs/36684304542) |
| 8c | [#319](https://github.com/nahharris/vibra/pull/319), `e5a5c08` | [36686228747](https://github.com/nahharris/vibra/actions/runs/36686228747) |

Every run passed all five jobs: Ubuntu, Windows, macOS, the reader corpus,
and the archive boundary.

## Stage demo

[`examples/stage-3a-config`](../../../examples/stage-3a-config) is a
two-target project:

- The `config` library parses `key = value` lines into a nominal model:
  - an `entry` record;
  - a `value` enum over `i64`, `bool`, and `str`;
  - failures reported through the nominal error union `parse-error`, whose
    members are the records `missing-separator`, `empty-key`, and
    `bad-integer`.

  It propagates failures with `try` and uses only the public standard-library
  modules `@std.text` and `@std.char`, with no interface.
- The `app` binary's entry returns `(result void config.parse-error)`.
- `tests/parse.vib` covers every model shape and every error member with
  `assert.equal`.

From a clean checkout, with no network, in `examples/stage-3a-config`:

```bash
vibra check
```

`check accepted`, exit 0.

```bash
vibra test
```

`test suite passed: 6 test(s)`, exit 0.

```bash
vibra --format json run src/app
```

Exit 0, `@command.ok`, with `programResult`
`(record type: (record type: @std.result.result arguments: (array @void
@config.parse.parse-error)) value: (record kind: @enum type:
@std.result.result variant: @ok))`.

The three demo sources are in canonical format (`vibra fmt` reports no
change). The process test `vibra-cli/tests/process_stage_3a_demo` repeats the
three commands with the actual binary on every CI run.

## Corpus sub-gate

The full corpus reports 278 passed, 0 failed, and 0 unavailable, by profile:

| Profile | Passed | Failed | Unavailable |
| --- | --- | --- | --- |
| reader-v1 | 76 | 0 | 0 |
| static-v1 | 146 | 0 | 0 |
| interpreter-v1 | 51 | 0 | 0 |
| tooling-v1 | 5 | 0 | 0 |

The workspace suite reports 623 passed and 0 failed.

### Stage 3A inventory rows

Each Stage 3A row of the [inventory](supported-surface.md) has a positive and a
negative case.

| AST variant | Positive | Negative |
| --- | --- | --- |
| `ExpressionKind::Match` | `V1-RUNTIME-match-patterns`, `V1-TYPE-CONTROL-match-exhaustive` | `V1-TYPE-CONTROL-match-non-exhaustive`, `V1-TYPE-CONTROL-match-unreachable-arm` |
| `ExpressionKind::As` | `V1-TYPE-CONVERT-ascription-erased`, `V1-RUNTIME-union-values` | `V1-TYPE-CONVERT-invalid-ascription`, `V1-SRC-EXPR-as-arity` |
| `ExpressionKind::Try` | `V1-RUNTIME-try-propagation` | `V1-TYPE-CONTROL-invalid-try`, `V1-TYPE-CONTROL-invalid-try-test-body` |
| `ExpressionKind::TupleOf` | `V1-RUNTIME-tuples` | `V1-SRC-EXPR-anonymous-value-shapes` |
| `ExpressionKind::RecordOf` | `V1-RUNTIME-anonymous-record` | `V1-SRC-EXPR-anonymous-value-shapes` |
| `ExpressionKind::EnumOf` | `V1-RUNTIME-anonymous-enum` | `V1-SRC-EXPR-anonymous-value-shapes`, `V1-TYPE-INFER-anonymous-values` |
| `PatternKind::Literal` | `V1-TYPE-CONTROL-match-exhaustive` | `V1-TYPE-CONTROL-pattern-type-mismatch` |
| `PatternKind::Atom` | `V1-RUNTIME-match-patterns` | `V1-TYPE-CONTROL-match-non-exhaustive` |
| `PatternKind::Constructor` | `V1-RUNTIME-match-patterns`, `V1-RUNTIME-workspace-test-patterns` | `V1-TYPE-CONTROL-pattern-type-mismatch`, `V1-TYPE-CONTROL-pattern-refutable-binding` |
| `PatternKind::Tuple` | `V1-RUNTIME-match-patterns` | `V1-TYPE-CONTROL-pattern-type-mismatch` |
| `PatternKind::RecordOf` | `V1-RUNTIME-match-patterns` | `V1-SRC-EXPR-anonymous-value-shapes` |
| `PatternKind::EnumOf` | `V1-RUNTIME-match-patterns` | `V1-SRC-EXPR-anonymous-value-shapes` |
| `PatternKind::Array` | `V1-RUNTIME-match-patterns` | `V1-TYPE-CONTROL-pattern-refutable-binding` |
| `PatternKind::As` | `V1-TYPE-CONVERT-narrowing-exhaustive`, `V1-RUNTIME-union-values` | `V1-TYPE-CONVERT-narrowing-rejections` |
| `VariadicBinding::Array` | `V1-RUNTIME-variadics`, `V1-SRC-CALLS-variadic-array-application` | `V1-SRC-DECL-malformed-variadic` |
| `VariadicBinding::Dict` | `V1-RUNTIME-dict-order`, `V1-SRC-CALLS-variadic-dict-application` | `V1-TYPE-NOMINAL-collection-construction` (odd `dict.of` tail) |
| `Declaration::Deftype` | `V1-RUNTIME-nominal-record`, `V1-TYPE-NOMINAL-union-declarations` | `V1-TYPE-NOMINAL-infinite-size`, `V1-SRC-DECL-builtin-name-reservation` |
| `TypeMember::Method` | `V1-RUNTIME-nominal-method` | `V1-SRC-DECL-member-collision` |
| `TypeExpr::Applied` | `V1-RUNTIME-generic-types` | `V1-TYPE-GENERIC-applied-types` |
| `TypeExpr::Tuple` | `V1-RUNTIME-tuples` | `V1-TYPE-NOMINAL-distinct-identity` |
| `TypeExpr::Record` | `V1-RUNTIME-anonymous-record` | `V1-TYPE-NOMINAL-distinct-identity` |
| `TypeExpr::Enum` | `V1-RUNTIME-anonymous-enum` | `V1-TYPE-INFER-anonymous-values` |
| `TypeExpr::Union` | `V1-TYPE-NOMINAL-union-declarations` | `V1-TYPE-NOMINAL-union-member-overlap`, `V1-TYPE-NOMINAL-union-member-not-concrete`, `V1-SRC-DECL-union-arity` |
| `TypeExpr::Array` | `V1-RUNTIME-lookups` | `V1-TYPE-NOMINAL-collection-construction` |
| `TypeExpr::Dict` | `V1-RUNTIME-dict-order`, `V1-TYPE-NOMINAL-admissible-keys` | `V1-TYPE-NOMINAL-dict-keys` |
| `VariadicType::Array` | `V1-SRC-CALLS-variadic-array-type` | `V1-SRC-DECL-malformed-variadic` |
| `VariadicType::Dict` | `V1-SRC-CALLS-variadic-dict-type` | `V1-SRC-DECL-malformed-variadic` |
| `DeftypeBody::Type` | `V1-RUNTIME-nominal-wrapper`, `V1-RUNTIME-union-values` | `V1-TYPE-NOMINAL-wrapper-representation` |
| `DeftypeBody::Intrinsic` | `V1-SRC-DECL-intrinsic-type`, `V1-PROJECT-workspace-check-builtin-members` | `V1-SRC-DECL-builtin-name-reservation` |
| `Attribute::Where` | `V1-RUNTIME-generic-functions`, `V1-RUNTIME-generic-lambda` | `V1-TYPE-GENERIC-invariance`, `V1-TYPE-GENERIC-redeclaration` |
| `Attribute::Variadic` | `V1-RUNTIME-variadics` | `V1-SRC-DECL-malformed-variadic` |
| `Attribute::Native` | `natives_m3_step4b` (every native against its body) | `V1-TYPE-NAMES-library-attributes` |
| `Attribute::Role` | `V1-TYPE-NAMES-role-vocabulary`, `V1-RUNTIME-library-roles` | `V1-TYPE-NAMES-library-attributes` |

`PatternKind::RecordOf` has no separate type-level negative: an omitted field
matches anything, so its only rejections are shape errors of the pattern form
itself, which the anonymous-value-shapes case covers.

### Stage 3B forms stay unavailable

Every Stage 3B form reports `@tool.unavailable` at its owning form:

| Form | Case |
| --- | --- |
| `defint`, nested `impl`, and effect declarations | `V1-PROJECT-workspace-check-nominal-availability` |
| An interface bound in `where:` | `V1-TYPE-GENERIC-interface-bound` |
| `any` in type position; a dict keyed by a generic parameter | `V1-TYPE-GENERIC-stage-3b-types` |
| Nonempty effect rows (M4) | `V1-EFFECT-availability-function-type`, `V1-SRC-CALLS-functions-effects` |

### Step 1 diagnostic codes

Step 1 registered eight codes. `@type.anonymous-newtype` was removed with
`newtype` (ledger D6.4). Each of the other seven has a focused case at an exact
span:

| Code | Case | First span |
| --- | --- | --- |
| `@type.ambiguous-inference` | `V1-TYPE-GENERIC-ambiguous` | `[206, 218]` |
| `@type.infinite-size` | `V1-TYPE-NOMINAL-infinite-size` | `[0, 33]` |
| `@type.invalid-dict-key` | `V1-TYPE-NOMINAL-dict-keys` | `[48, 63]` |
| `@type.invalid-try` | `V1-TYPE-CONTROL-invalid-try` | `[281, 305]` |
| `@type.unhandled-fallible` | `V1-TYPE-CONTROL-unhandled-fallible` | `[72, 81]` |
| `@pattern.non-exhaustive` | `V1-TYPE-CONTROL-match-non-exhaustive` | `[83, 135]` |
| `@pattern.unreachable-arm` | `V1-TYPE-CONTROL-match-unreachable-arm` | `[66, 70]` |

## Conformance-chapter coverage

The Stage 3A clauses of `docs/spec/07-diagnostics-and-conformance.md`:

| Clause | Cases |
| --- | --- |
| Collection construction: heterogeneous `tupleof`/`recordof`, `enumof` with and without a written enum, homogeneous and expected-empty `array.of`, even and duplicate-key `dict.of`, declared constructors, type forms in value position | `V1-RUNTIME-tuples`, `V1-RUNTIME-anonymous-record`, `V1-RUNTIME-anonymous-enum`, `V1-TYPE-INFER-anonymous-values`, `V1-RUNTIME-variadics`, `V1-RUNTIME-dict-order`, `V1-TYPE-NOMINAL-collection-construction`, `V1-TYPE-NOMINAL-constructor-fields`, `V1-SRC-EXPR-anonymous-value-shapes` |
| Pattern coverage: binders, nested destructuring in `let`, parameters, lambdas, and `match`; no-shadowing and duplicates; repeated discards; irrefutability; retired `(bind …)` | `V1-RUNTIME-match-patterns`, `V1-TYPE-CONTROL-match-exhaustive`, `V1-TYPE-CONTROL-pattern-redeclaration`, `V1-TYPE-CONTROL-pattern-refutable-binding`, `V1-SRC-EXPR-pattern-retired-form` |
| Match coverage: non-exhaustive enum, union, `bool`, tuple, and `atom`; repeated arm and arm after a binder; `str` and integer literal sets | `V1-TYPE-CONTROL-match-non-exhaustive`, `V1-TYPE-CONVERT-narrowing-rejections`, `V1-TYPE-CONTROL-match-unreachable-arm`, `V1-TYPE-CONTROL-match-exhaustive` |
| Failure coverage: `try` over `option` and `result`; differing error, test body, non-container; ignored `result`, discards, ignored `option` | `V1-RUNTIME-try-propagation`, `V1-TYPE-CONTROL-invalid-try`, `V1-TYPE-CONTROL-invalid-try-test-body`, `V1-TYPE-CONTROL-unhandled-fallible`, `V1-TYPE-CONTROL-fallible-discards` |
| Inference coverage: unsuffixed literal, empty `array.of`, uninferable generic; `def`, `if` condition, and branch mismatches | `V1-TYPE-INFER-context-numeric`, `V1-TYPE-NOMINAL-collection-construction`, `V1-TYPE-GENERIC-ambiguous`, `V1-TYPE-NAMES-binding-initializer-mismatch`, `V1-TYPE-CONTROL-if-condition`, `V1-TYPE-CONTROL-branch-mismatch` |
| Dict coverage: every closed key type including nested tuples; `f64`, `void`, array, and record keys rejected; insertion-order independence | `V1-TYPE-NOMINAL-admissible-keys`, `V1-TYPE-NOMINAL-dict-keys`, `V1-RUNTIME-dict-order` |
| Nominal coverage: direct self-containment rejected; containment through an array accepted | `V1-TYPE-NOMINAL-infinite-size`, `V1-TYPE-NOMINAL-recursive-through-array` |
| Union coverage: declarations, one-member body, non-concrete members, overlap under instantiation, no lifting, union dict key | `V1-TYPE-NOMINAL-union-declarations`, `V1-SRC-DECL-union-arity`, `V1-TYPE-NOMINAL-union-member-not-concrete`, `V1-TYPE-NOMINAL-union-member-overlap`, `V1-TYPE-NOMINAL-union-no-lifting` |
| Structural-type coverage: anonymous types in every position, wrapper and tuple distinctness, order-insensitive identity and formatting, reserved heads | `V1-SRC-DECL-structural-types`, `V1-TYPE-NOMINAL-distinct-identity`, `V1-TYPE-NOMINAL-wrapper-representation`, `V1-TYPE-CONVERT-widening-boundaries`, `V1-SRC-FMT-structural-order` |
| Builtin-type coverage: `intrinsic-type` only in the embedded package | `V1-SRC-DECL-intrinsic-type`, `V1-SRC-DECL-builtin-name-reservation` |
| Reserved heads and names | `V1-SRC-DECL-builtin-name-reservation`, `V1-TYPE-NAMES-resolve-reserved-value`, `V1-TYPE-GENERIC-redeclaration` |
| Unification: overlap with a bound | `V1-TYPE-NOMINAL-union-member-overlap` |
| Widening: every written boundary, member-to-union and singleton-to-`atom`; none without an expected type; `if` branches; invariance; atom arrays | `V1-TYPE-CONVERT-widening-boundaries`, `V1-TYPE-CONVERT-widening-rejections`, `V1-RUNTIME-union-values` |
| Ascription: no-op, empty collection, generic result; `(as i64 3i32)` and `(as i32 some-number)`; erasure | `V1-TYPE-CONVERT-ascription-erased`, `V1-TYPE-CONVERT-invalid-ascription` |
| Narrowing: exhaustive union match, binder remainder, non-exhaustive, unreachable, `as` in `let` and parameters, non-union and non-member | `V1-TYPE-CONVERT-narrowing-exhaustive`, `V1-TYPE-CONVERT-narrowing-rejections` |
| `assert.equal` over compound values; function operands | `V1-RUNTIME-workspace-test-assert-equal`, `V1-TYPE-CONVERT-assert-equal-function` |

The concrete-to-interface widening clauses, the interface no-chaining
rejections, the conversion and destination-dispatch clauses, and the interface
and `iter` coverage belong to Stage 3B (Steps 11–14).
