# v0.0.2 M0: owner table

For every supported form of the specification this table names the step
that implements it, the conformance cases planned for it, its diagnostics,
and its schema. A form without all four is not supported. Steps are those of
the [M1 plan](../m1/README.md); a form owned by a later milestone carries
that milestone's name and no detail.

How to read a row:

- **Step** lists the M1 step that reads the form first and, after a slash,
  the steps that give it meaning.
- **Cases** are identities under `conformance/cases/<PREFIX>/<id>/`, each
  defined in the step guide named in the row. A trailing `*` stands for
  every case of that guide whose identity begins with the text before it.
- **Diagnostics** are rows of the registry table of the
  [diagnostics chapter](../../../spec/07-diagnostics-and-conformance.md).
- **Schema** is the persistent observation or machine document a case
  compares. `tree` is the `@tree.v2` document of the `reader` operation.

## Lexical productions

Source chapter, "Lexical structure".

| Production | Step | Cases | Diagnostics | Schema |
| --- | --- | --- | --- | --- |
| `identifier` | 03 / 10 | `V2-SRC-READER-name-invalid`, `V2-SRC-READER-vibon-identifier`, `V2-DIAG-style-casing` | `@syntax.invalidName`, `@style.casing` | tree |
| `letter` | 03 | `V2-SRC-READER-name-invalid` | `@syntax.invalidName` | tree |
| `digit` | 03 | `V2-SRC-READER-numeric-suffix`, `V2-SRC-READER-name-invalid` | `@syntax.invalidNumericLiteral`, `@syntax.invalidName` | tree |
| `atom` | 03 / 17 | `V2-SRC-READER-vibon-literals`, `V2-TYPE-CONTROL-match-atom-singleton`, `V2-TYPE-CONTROL-match-atom-open`, `V2-TYPE-INTERFACE-widen-atom` | `@syntax.invalidName`, `@pattern.nonExhaustive` | tree, `@value.v2` |
| `atom-part` | 03 | `V2-SRC-READER-name-invalid` | `@syntax.invalidName` | tree |
| `discard` | 03 / 11 | `V2-SRC-EXPR-patterns`, `V2-TYPE-NAMES-discard-repeats`, `V2-TYPE-CONTROL-discard-fallible` | `@type.unhandledFallible` | tree, `@resolved.v2` |
| `void` | 03 / 15 | `V2-SRC-READER-vibon-literals`, `V2-SRC-EXPR-pattern-void`, `V2-TYPE-CONTROL-if-statement-void` | `@type.mismatch` | tree, `@value.v2` |
| `integer` | 03 / 15 | `V2-SRC-READER-numeric-suffix`, `V2-TYPE-INFER-literal-suffix`, `V2-TYPE-INFER-literal-expected`, `V2-TYPE-INFER-literal-ambiguous`, `V2-TYPE-INFER-out-of-range` | `@syntax.invalidNumericLiteral`, `@type.numericOutOfRange`, `@type.ambiguousInference` | tree, `@value.v2` |
| `float` | 03 / 15 | `V2-SRC-READER-numeric-suffix`, `V2-TYPE-INFER-float-rounds-to-infinity`, `V2-SRC-EXPR-pattern-float` | `@syntax.invalidNumericLiteral`, `@type.numericOutOfRange`, `@syntax.invalidForm` | tree, `@value.v2` |
| `integer-suffix` | 03 / 15 | `V2-SRC-READER-numeric-suffix`, `V2-TYPE-INFER-literal-suffix`, `V2-TYPE-INFER-no-promotion` | `@syntax.invalidNumericLiteral`, `@type.mismatch` | tree |
| `float-suffix` | 03 / 15 | `V2-SRC-READER-numeric-suffix`, `V2-TYPE-INFER-literal-suffix` | `@syntax.invalidNumericLiteral` | tree |
| `exponent` | 03 | `V2-SRC-READER-numeric-suffix`, `V2-TYPE-CONVERT-float-text` | `@syntax.invalidNumericLiteral` | tree |
| `digits` | 03 | `V2-SRC-READER-numeric-suffix` | `@syntax.invalidNumericLiteral` | tree |
| `string` | 03 / 15 | `V2-SRC-READER-string-escape`, `V2-SRC-READER-string-unterminated`, `V2-TYPE-CONTROL-match-str-literal` | `@syntax.invalidStringLiteral` | tree, `@value.v2` |
| `doc-string` | 03 / 06 | `V2-SRC-READER-vibon-doc-string`, `V2-SRC-READER-doc-string-misplaced` | `@syntax.invalidStringLiteral`, `@syntax.invalidForm` | tree |
| `character` | 03 | `V2-SRC-READER-character`, `V2-TYPE-INTERFACE-char` | `@syntax.invalidCharacterLiteral` | tree, `@value.v2` |
| `escape` | 03 | `V2-SRC-READER-string-escape`, `V2-SRC-READER-character` | `@syntax.invalidStringLiteral`, `@syntax.invalidCharacterLiteral` | tree |

Lexical rules that are not productions:

| Rule | Step | Cases | Diagnostics | Schema |
| --- | --- | --- | --- | --- |
| encoding (UTF-8, no BOM, no NUL) | 03 | `V2-SRC-READER-encoding-bom`, `V2-SRC-READER-encoding-nul` | `@syntax.invalidEncoding` | tree |
| line endings and comments | 04 | `V2-SRC-READER-crlf`, `V2-SRC-READER-comments-kept`, `V2-SRC-READER-vibon-comment` | none | tree |
| token separation | 03 | `V2-SRC-READER-separator` | `@syntax.missingSeparator` | tree |
| reserved words | 04 / 10 | `V2-SRC-READER-name-invalid`, `V2-SRC-DECL-type-reserved-field`, `V2-SRC-EXPR-reserved-label`, `V2-SRC-DECL-fn-reserved-label`, `V2-TYPE-NAMES-reserved-declaration`, `V2-TYPE-NAMES-reserved-value`, `V2-TYPE-NAMES-reserved-binder`, `V2-TYPE-NAMES-reserved-import` | `@syntax.invalidName`, `@name.reservedLabel`, `@name.reservedDeclaration`, `@name.reservedValueSpelling` | tree, `@resolved.v2` |
| typed closers and recovery | 03 / 04 | `V2-SRC-READER-closer-crossed`, `V2-SRC-READER-closer-missing`, `V2-SRC-READER-closer-wrong`, `V2-SRC-READER-closer-eof`, `V2-SRC-READER-recovery-then-valid`, `V2-SRC-READER-type-recovery`, `V2-SRC-READER-expr-recovery`, `V2-SRC-READER-fn-recovery` | `@syntax.unmatchedDelimiter` | tree |
| excluded spellings | 04 / 28 | `V2-SRC-READER-top-paren`, `V2-SRC-EXPR-paren-group`, `V2-TOOL-format-excluded-form` | `@syntax.invalidForm` | tree |
| canonical format | 03 to 07 / 25 | `V2-SRC-FMT-wrap-arguments`, `V2-SRC-EXPR-if-multiline`, `V2-SRC-DECL-fn-one-line`, `V2-SRC-DECL-fn-section-order`, `V2-SRC-DECL-type-section-order`, `V2-SRC-READER-vibon-wraps`, `V2-TOOL-format-*` | none | formatted text |

## Grammar productions

Source chapter, "Grammar".

| Production | Step | Cases | Diagnostics | Schema |
| --- | --- | --- | --- | --- |
| `module` | 04 | `V2-SRC-READER-top-literal`, `V2-SRC-READER-recovery-then-valid`; the host test of step 03 for a source document before step 04 | `@syntax.invalidForm`, `@tool.unavailable` | tree |
| `top` | 04 | `V2-SRC-READER-top-literal`, `V2-SRC-READER-top-paren` | `@syntax.invalidForm` | tree |
| `use` | 04 / 10 | `V2-SRC-DECL-use-*`, `V2-PROJECT-import-*`, `V2-TYPE-NAMES-import-name-collision` | `@syntax.invalidForm`, `@module.unknownPath`, `@module.importCycle`, `@name.privateAccess`, `@name.redeclaration` | tree, `@resolved.v2` |
| `type-decl` | 05 / 16 | `V2-SRC-DECL-type-wrapper`, `V2-SRC-DECL-type-record`, `V2-SRC-DECL-type-tuple`, `V2-SRC-DECL-type-variant`, `V2-SRC-DECL-type-missing-body`, `V2-SRC-DECL-type-both-of-and-case`, `V2-TYPE-NOMINAL-*` | `@syntax.invalidForm`, `@syntax.unmatchedDelimiter`, `@name.redeclaration`, `@type.infiniteSize` | tree, `@types.v2` |
| `type-body` | 05 / 14, 16 | `V2-SRC-DECL-type-wrapper`, `V2-SRC-DECL-type-paren-type`, `V2-SRC-DECL-type-intrinsic-outside-stdlib`, `V2-TYPE-NOMINAL-wrapper` | `@syntax.invalidForm`, `@syntax.invalidSection` | tree |
| `case` | 05 / 16 | `V2-SRC-DECL-type-variant`, `V2-TYPE-NOMINAL-variant-fields`, `V2-TYPE-NOMINAL-variant-fieldless-args`, `V2-TYPE-NAMES-member-collision` | `@name.memberCollision`, `@type.argumentMismatch` | tree, `@types.v2` |
| `type-item` | 05 | `V2-SRC-DECL-type-section-order`, `V2-SRC-DECL-type-invalid-section`, `V2-SRC-READER-duplicate-section` | `@syntax.invalidSection`, `@syntax.duplicateSection` | tree |
| `interface-decl` | 05 / 19 | `V2-SRC-DECL-interface-basic`, `V2-SRC-DECL-interface-default-empty`, `V2-TYPE-INTERFACE-default-member`, `V2-TYPE-INTERFACE-receiver-dispatch` | `@syntax.invalidSection`, `@type.undispatchableContractMember` | tree, `@types.v2` |
| `assoc-type` | 05 / 10, 19 | `V2-SRC-DECL-interface-assoc`, `V2-SRC-DECL-interface-assoc-with-body`, `V2-TYPE-NAMES-assoc-shadows-parameter`, `V2-TYPE-NAMES-no-projection-path`, `V2-TYPE-INTERFACE-assoc-bound-parameter`, `V2-TYPE-INTERFACE-assoc-abstract` | `@syntax.invalidForm`, `@name.memberCollision`, `@name.genericRedeclaration`, `@name.unknownSymbol`, `@type.mismatch` | tree, `@types.v2` |
| `impl` | 05 / 19 | `V2-SRC-DECL-impl-in-type`, `V2-SRC-DECL-impl-top-level`, `V2-SRC-DECL-impl-with-section`, `V2-SRC-DECL-impl-member-guard`, `V2-TYPE-INTERFACE-impl-*`, `V2-TYPE-INTERFACE-missing-member`, `V2-TYPE-INTERFACE-unknown-member`, `V2-TYPE-INTERFACE-member-twice`, `V2-TYPE-INTERFACE-default-override`, `V2-TYPE-INTERFACE-overlap` | `@syntax.invalidForm`, `@syntax.invalidSection`, `@type.redundantImplementation`, `@type.missingAbstractMember`, `@type.defaultOverride`, `@type.overlappingImplementation`, `@type.typeArgumentMismatch`, `@name.wrongEntityKind`, `@name.unknownSymbol`, `@name.memberCollision` | tree, `@types.v2` |
| `assoc-choice` | 05 / 19 | `V2-SRC-DECL-impl-in-type`, `V2-TYPE-INTERFACE-assoc-once`, `V2-TYPE-INTERFACE-assoc-missing`, `V2-TYPE-INTERFACE-assoc-twice`, `V2-TYPE-INTERFACE-assoc-self-reference` | `@name.redeclaration`, `@name.unknownSymbol`, `@type.missingAbstractMember` | tree, `@types.v2` |
| `fn-decl` | 07 / 15, 18 | `V2-SRC-DECL-fn-greet`, `V2-SRC-DECL-fn-one-line`, `V2-SRC-DECL-fn-section-order`, `V2-SRC-DECL-fn-no-body`, `V2-SRC-DECL-fn-local-declaration`, `V2-SRC-DECL-member-and-impl`, `V2-TYPE-CONTROL-missing-return` | `@syntax.invalidForm`, `@syntax.duplicateSection`, `@syntax.invalidSection`, `@type.missingReturn` | tree, `@types.v2` |
| `proc-decl` | 07 (syntax) / M2 | `V2-SRC-DECL-proc-basic`, `V2-SRC-DECL-proc-native`, `V2-TOOL-unavailable-proc` | `@tool.unavailable` in M1 | tree |
| `signature` | 07 / 11, 15 | `V2-SRC-DECL-fn-missing-returns`, `V2-SRC-DECL-fn-empty-takes`, `V2-TYPE-NAMES-result-name-in-body`, `V2-TYPE-NAMES-result-name-in-ensures`, `V2-TYPE-CONTROL-return-type` | `@syntax.invalidForm`, `@name.unknownSymbol`, `@type.mismatch` | tree, `@types.v2` |
| `params` | 07 / 17 | `V2-SRC-DECL-fn-pattern-param`, `V2-TYPE-CONTROL-param-refutable`, `V2-TYPE-CONTROL-param-tuple`, `V2-TYPE-NAMES-shadow-parameter` | `@pattern.refutableBinding`, `@name.redeclaration` | tree, `@types.v2` |
| `labelled` | 07 / 18 | `V2-SRC-DECL-fn-labelled`, `V2-SRC-DECL-fn-reserved-label`, `V2-SRC-CALLS-labelled-*`, `V2-SRC-CALLS-default-not-constant`, `V2-SRC-CALLS-default-constant-construction` | `@name.reservedLabel`, `@type.notConstant`, `@type.argumentMismatch` | tree, `@types.v2` |
| `type-params` | 05 / 10, 16 | `V2-SRC-DECL-type-generic-given`, `V2-SRC-DECL-fn-generic`, `V2-TYPE-NAMES-generic-redeclaration`, `V2-TYPE-GENERIC-pair`, `V2-TYPE-GENERIC-argument-count`, `V2-TYPE-GENERIC-uninferable` | `@name.genericRedeclaration`, `@type.typeArgumentMismatch`, `@type.ambiguousInference` | tree, `@types.v2` |
| `module-let` | 06 / 15 | `V2-SRC-DECL-module-let*`, `V2-TYPE-CONTROL-module-value`, `V2-TYPE-CONTROL-module-value-forward`, `V2-TYPE-CONTROL-initializer-cycle`, `V2-TYPE-CONTROL-initializer-return`, `V2-PROJECT-test-fresh-state` | `@syntax.invalidForm`, `@type.initializerCycle`, `@type.invalidReturn` | tree, `@types.v2` |
| `test` | 07 / 23 | `V2-SRC-DECL-test-basic`, `V2-SRC-DECL-test-world`, `V2-SRC-DECL-test-two-params`, `V2-PROJECT-test-*` | `@module.testOutsideTests`, `@module.missingRequiredImport`, `@name.redeclaration`, `@type.invalidReturn`, `@type.invalidTry`; `@tool.unavailable` for `takes` and `performs` in M1 | tree, `@testRun.v2` |
| `given` | 05 / 10, 19 | `V2-SRC-DECL-type-generic-given`, `V2-TYPE-GENERIC-given-unknown`, `V2-TYPE-NAMES-wrong-kind-bound`, `V2-TYPE-GENERIC-bound-satisfied`, `V2-TYPE-GENERIC-bound-unsatisfied` | `@name.unknownSymbol`, `@name.wrongEntityKind`, `@type.unsatisfiedBound` | tree, `@types.v2` |
| `requires` | 07 / 21, 22 | `V2-SRC-DECL-fn-duplicate-requires`, `V2-CONTRACT-requires-*`, `V2-CONTRACT-bare-requires-unproved`, `V2-CONTRACT-rule*` | `@syntax.duplicateSection`, `@contract.invalidPredicate`, `@contract.invalidConsequence`, `@contract.unsupportedPredicate`, `@contract.unproved`, `@contract.refuted` | tree, `@types.v2` |
| `ensures` | 07 / 21, 22 | `V2-CONTRACT-ensures-*`, `V2-CONTRACT-rule7-certified-result`, `V2-CONTRACT-member-ensures-established` | `@contract.invalidPredicate`, `@contract.invalidConsequence`, `@contract.unproved`, `@contract.refuted` | tree, `@types.v2` |
| `where` | 05 / 21, 22 | `V2-SRC-DECL-type-where`, `V2-CONTRACT-where-*`, `V2-CONTRACT-constant-construction-*`, `V2-CONTRACT-nonconstant-construction`, `V2-TYPE-CONTROL-match-where-no-coverage` | `@type.invalidInvariant`, `@contract.invalidPredicate`, `@contract.invalidConsequence`, `@contract.unproved`, `@contract.refuted`, `@pattern.refutableBinding` | tree, `@types.v2` |
| `performs` | 07 (syntax) / M2 | `V2-SRC-DECL-fn-performs` | `@syntax.invalidSection` on a `fn`; rows are M2 | tree |
| `row` | 07 (syntax) / M2 | `V2-SRC-DECL-proc-basic`, `V2-SRC-DECL-type-callable-types` | M2 | tree |
| `visibility` | 05 / 10 | `V2-SRC-DECL-type-bad-tag`, `V2-SRC-DECL-module-let-visibility`, `V2-PROJECT-import-private` | `@syntax.invalidForm`, `@name.privateAccess` | tree, `@resolved.v2` |
| `representation` | 05 / 16 | `V2-SRC-DECL-type-representation`, `V2-SRC-DECL-type-bad-tag`, `V2-TYPE-NOMINAL-representation-private`, `V2-TYPE-NOMINAL-representation-default`, `V2-TYPE-NOMINAL-representation-public`, `V2-TYPE-INTERFACE-str-representation-private` | `@name.inaccessibleRepresentation` | tree, `@types.v2` |
| `role` | 05 / 14 | `V2-SRC-DECL-type-role-outside-stdlib`; host tests of step 14 (each role claimed once) | `@syntax.invalidSection` | tree, `@stdlibManifest.v2` |
| `external` | 07 / 14 | `V2-SRC-DECL-fn-external-outside-stdlib`, `V2-SRC-DECL-fn-external-with-body`, `V2-SRC-DECL-fn-external-bad-tag`; host tests of step 14 | `@syntax.invalidSection`, `@syntax.invalidForm`, `@external.unknownSymbol`, `@external.signatureMismatch`, `@external.ownerMismatch` | tree, `@stdlibManifest.v2` |
| `symbol` | 07 / 14 | `V2-SRC-DECL-fn-external-outside-stdlib`; host tests of step 14 | `@external.unknownSymbol` | tree, `@stdlibManifest.v2` |
| `native` | 07 / 14 | `V2-SRC-DECL-proc-native`; host tests of step 14 (the `native` list is empty in M1) | `@syntax.invalidSection` | tree, `@stdlibManifest.v2` |
| `doc` | 05 / 06 | `V2-SRC-DECL-type-doc`, `V2-SRC-DECL-type-section-order`, `V2-SRC-READER-doc-string-misplaced` | `@syntax.invalidForm`, `@syntax.duplicateSection` | tree |
| `body` | 06 / 15 | `V2-SRC-EXPR-do-block`, `V2-TYPE-CONTROL-do-value`, `V2-TYPE-CONTROL-do-ends-in-let` | `@type.mismatch` | tree, `@types.v2` |
| `statement` | 06 / 15, 17 | `V2-TYPE-CONTROL-if-statement-void`, `V2-TYPE-CONTROL-unhandled-fallible`, `V2-TYPE-CONTROL-unreachable-after-return` | `@type.unhandledFallible`, `@type.unreachableCode` | tree |
| `let` | 06 / 11, 17 | `V2-SRC-EXPR-let-else`, `V2-SRC-EXPR-let-misplaced`, `V2-SRC-EXPR-let-missing-be`, `V2-TYPE-NAMES-let-*`, `V2-TYPE-CONTROL-let-*` | `@syntax.misplacedBinding`, `@syntax.invalidForm`, `@pattern.refutableBinding`, `@pattern.irrefutableLetElse`, `@type.mismatch` | tree, `@resolved.v2`, `@types.v2` |
| `expr` | 06 / 15 | `V2-SRC-EXPR-return-try-await`, `V2-TYPE-CONTROL-return-type`, `V2-TYPE-CONTROL-never-operand`, `V2-PROJECT-test-return` | `@type.invalidReturn`, `@type.mismatch`, `@type.unreachableCode` | tree, `@types.v2` |
| `unary` | 06 / 17 | `V2-SRC-EXPR-return-try-await`, `V2-TYPE-CONTROL-try-*`, `V2-EFFECT-await-in-fn` | `@type.invalidTry`, `@effect.invalidAwait`; `await` in a `proc` is M2 | tree, `@types.v2` |
| `postfix` | 06 / 15, 16 | `V2-SRC-EXPR-application`, `V2-SRC-EXPR-nullary-and-value`, `V2-SRC-EXPR-field-path`, `V2-SRC-CALLS-function-value`, `V2-SRC-CALLS-callee-expression`, `V2-SRC-CALLS-arity`, `V2-SRC-CALLS-order`, `V2-SRC-CALLS-lookup-*`, `V2-TYPE-NOMINAL-not-applicable`, `V2-TYPE-NOMINAL-field-read` | `@type.notApplicable`, `@type.argumentMismatch`, `@type.unknownRecordField` | tree, `@types.v2`, `@value.v2` |
| `argument` | 06 / 15, 18 | `V2-SRC-EXPR-labelled-arguments`, `V2-SRC-CALLS-label-on-positional`, `V2-SRC-CALLS-labelled-any-position`, `V2-SRC-CALLS-labelled-duplicate`, `V2-SRC-CALLS-labelled-unknown` | `@type.argumentMismatch` | tree |
| `primary` | 06 / 16 | `V2-SRC-EXPR-products`, `V2-SRC-EXPR-collections`, `V2-SRC-EXPR-mixed-record`, `V2-SRC-EXPR-paren-group`, `V2-TYPE-NOMINAL-list`, `V2-TYPE-NOMINAL-list-mixed`, `V2-TYPE-NOMINAL-empty-tuple`, `V2-TYPE-NOMINAL-dict-order-and-duplicate`, `V2-TYPE-INFER-empty-list-ambiguous`, `V2-TYPE-INFER-empty-dict-expected` | `@syntax.invalidForm`, `@type.mismatch`, `@type.ambiguousInference`, `@type.invalidDictKey` | tree, `@value.v2` |
| `literal` | 06 / 15 | `V2-SRC-READER-top-literal`, `V2-TYPE-INFER-literal-*`, `V2-TYPE-CONTROL-match-literal-open` | `@type.numericOutOfRange`, `@type.ambiguousInference` | tree, `@value.v2` |
| `if` | 06 / 15 | `V2-SRC-EXPR-if-expression`, `V2-SRC-EXPR-if-multiline`, `V2-TYPE-CONTROL-if-*`, `V2-TYPE-CONTROL-never-branch-skipped` | `@type.mismatch` | tree, `@types.v2` |
| `match` | 06 / 15, 17 | `V2-SRC-EXPR-match`, `V2-SRC-EXPR-match-no-arm`, `V2-TYPE-CONTROL-match-*` | `@syntax.invalidForm`, `@pattern.nonExhaustive`, `@pattern.unsupportedCoverage` | tree, `@types.v2` |
| `arm` | 06 / 17 | `V2-SRC-EXPR-match`, `V2-TYPE-CONTROL-match-unreachable`, `V2-TYPE-CONTROL-match-unreachable-nested`, `V2-TYPE-NAMES-sibling-scopes` | `@pattern.unreachableArm` | tree |
| `do-block` | 06 / 15 | `V2-SRC-EXPR-do-block`, `V2-TYPE-CONTROL-do-value`, `V2-TYPE-NAMES-shadow-nested` | `@type.mismatch` | tree |
| `anon-fn` | 07 / 11, 18 | `V2-SRC-DECL-anon-fn`, `V2-SRC-DECL-anon-fn-named`, `V2-TYPE-NAMES-anon-fn-scope`, `V2-SRC-EXPR-anon-fn-*`, `V2-RUNTIME-closure-captures-value`, `V2-PROJECT-test-return-in-anon-fn` | `@syntax.invalidForm`, `@name.unknownSymbol`, `@type.ambiguousInference` | tree, `@types.v2` |
| `anon-proc` | 07 (syntax) / M2 | `V2-SRC-DECL-anon-proc` | `@tool.unavailable` in M1 | tree |
| `pattern` | 06 / 11, 17 | `V2-SRC-EXPR-patterns`, `V2-TYPE-NAMES-pattern-twice`, `V2-TYPE-NAMES-constant-pattern`, `V2-TYPE-NAMES-bool-constant-pattern`, `V2-TYPE-NAMES-function-as-pattern`, `V2-TYPE-NAMES-nonconstant-as-pattern`, `V2-TYPE-CONTROL-pattern-type-mismatch`, `V2-TYPE-CONTROL-pattern-shape-*`, `V2-TYPE-CONTROL-match-nested` | `@syntax.invalidForm`, `@name.redeclaration`, `@name.wrongEntityKind`, `@type.mismatch`, `@type.argumentMismatch`, `@pattern.nonExhaustive` | tree, `@resolved.v2` |
| `pattern-literal` | 06 / 17 | `V2-SRC-EXPR-patterns`, `V2-SRC-EXPR-pattern-float`, `V2-SRC-EXPR-pattern-void`, `V2-TYPE-CONTROL-match-literal-open`, `V2-TYPE-CONTROL-match-str-literal`, `V2-TYPE-CONTROL-match-atom-singleton` | `@syntax.invalidForm`, `@type.mismatch`, `@pattern.nonExhaustive` | tree, `@types.v2` |
| `type-expr` | 05 / 10, 16 | `V2-SRC-DECL-type-callable-types`, `V2-SRC-DECL-type-paren-type`, `V2-SRC-DECL-type-assoc-label`, `V2-TYPE-NAMES-unknown-type`, `V2-TYPE-GENERIC-argument-count`, `V2-TYPE-INTERFACE-assoc-label`, `V2-TYPE-INTERFACE-assoc-label-unknown`; the `Proc` and `Intent` alternatives are M2 | `@syntax.invalidForm`, `@name.unknownSymbol`, `@name.wrongEntityKind`, `@type.typeArgumentMismatch` | tree, `@types.v2` |
| `record-type` | 05 / 16 | `V2-SRC-DECL-type-record`, `V2-SRC-DECL-type-mixed-product`, `V2-TYPE-NOMINAL-structural-record-order`, `V2-TYPE-NOMINAL-record-duplicate-field` | `@syntax.invalidForm`, `@name.memberCollision` | tree, `@types.v2` |
| `inputs` | 05 / 18 | `V2-SRC-DECL-type-callable-types`, `V2-TYPE-NOMINAL-fn-type-with-labelled`, `V2-TYPE-NOMINAL-fn-type-labelled-mismatch` | `@type.mismatch` | tree, `@types.v2` |
| `path` | 06 / 11 | `V2-SRC-DECL-type-path`, `V2-SRC-EXPR-field-path`, `V2-TYPE-NAMES-path-vs-field`, `V2-TYPE-NAMES-longest-prefix`, `V2-TYPE-NAMES-unknown-first-part`, `V2-TYPE-NAMES-self-outside` | `@name.unknownSymbol`, `@type.unknownRecordField` | tree, `@resolved.v2` |

## Type system rule families

Type chapter, by section.

| Rule family | Step | Cases | Diagnostics | Schema |
| --- | --- | --- | --- | --- |
| Language core and standard library; the prelude | 14 / 10 | host tests of step 14 (prelude table, roles); `V2-TYPE-NAMES-reserved-declaration`, `V2-TYPE-NAMES-reserved-value` | `@name.reservedDeclaration`, `@name.reservedValueSpelling` | `@stdlibManifest.v2` |
| Scalars | 15 | `V2-TYPE-INFER-literal-*`, `V2-TYPE-INFER-out-of-range`, `V2-TYPE-INFER-no-promotion`, `V2-TYPE-INFER-float-rounds-to-infinity` | `@type.numericOutOfRange`, `@type.ambiguousInference`, `@type.mismatch` | `@types.v2`, `@value.v2` |
| Declared types | 16 | `V2-TYPE-NOMINAL-record-*`, `V2-TYPE-NOMINAL-wrapper`, `V2-TYPE-NOMINAL-wrapper-no-delegation`, `V2-TYPE-NOMINAL-declared-tuple`, `V2-TYPE-NOMINAL-variant-*`, `V2-TYPE-NOMINAL-same-shape-different-type` | `@type.argumentMismatch`, `@type.unknownRecordField`, `@type.mismatch`, `@name.memberCollision` | `@types.v2`, `@value.v2` |
| Representation visibility | 16 | `V2-TYPE-NOMINAL-representation-*` | `@name.inaccessibleRepresentation` | `@types.v2` |
| Type invariants | 21, 22 | `V2-CONTRACT-where-*`, `V2-CONTRACT-ingress-*` | `@type.invalidInvariant`, `@contract.invalidPredicate`, `@contract.unproved`, `@contract.refuted` | `@types.v2` |
| Structural types | 16 | `V2-TYPE-NOMINAL-structural-record-order`, `V2-TYPE-NOMINAL-tuple-arity`, `V2-TYPE-NOMINAL-empty-tuple`, `V2-TYPE-NOMINAL-list`, `V2-TYPE-NOMINAL-list-mixed` | `@type.mismatch` | `@types.v2`, `@value.v2` |
| Recursive types | 16 | `V2-TYPE-NOMINAL-recursive-list`, `V2-TYPE-NOMINAL-infinite-size`, `V2-TYPE-NOMINAL-infinite-size-mutual` | `@type.infiniteSize` | `@types.v2` |
| Callable types (`Fn`) | 18 | `V2-TYPE-NOMINAL-fn-type-with-labelled`, `V2-TYPE-NOMINAL-fn-type-labelled-mismatch`, `V2-SRC-CALLS-function-value` | `@type.mismatch` | `@types.v2` |
| Callable types (`Proc`, `Intent`) | M2 | M2 | M2 | M2 |
| Dict keys, equality, and order | 16 / 19 | `V2-TYPE-NOMINAL-dict-*`, `V2-TYPE-INTERFACE-equatable-tuple`, `V2-PROJECT-test-function-not-equatable` | `@type.invalidDictKey`, `@type.functionNotEquatable` | `@types.v2`, `@value.v2` |
| Application | 15 / 16 | `V2-SRC-CALLS-arity`, `V2-SRC-CALLS-order`, `V2-SRC-CALLS-callee-expression`, `V2-SRC-CALLS-lookup-*`, `V2-SRC-CALLS-evaluation-order`, `V2-TYPE-NOMINAL-not-applicable` | `@type.argumentMismatch`, `@type.notApplicable` | `@types.v2`, `@value.v2` |
| Namespaces and resolution | 10 / 11 | `V2-TYPE-NAMES-*` | `@name.unknownSymbol`, `@name.wrongEntityKind`, `@name.privateAccess`, `@name.redeclaration`, `@name.memberCollision`, `@name.genericRedeclaration`, `@name.reservedDeclaration`, `@name.reservedValueSpelling`, `@style.casing` | `@resolved.v2` |
| Functions as values | 18 | `V2-TYPE-INFER-constructor-not-value`, `V2-TYPE-INFER-constructor-wrapped`, `V2-TYPE-INFER-member-value`, `V2-TYPE-INTERFACE-member-as-value`, `V2-TYPE-INTERFACE-member-as-value-ambiguous` | `@type.mismatch`, `@type.ambiguousInference` | `@types.v2` |
| Inference and checking | 15 / 16, 18 | `V2-TYPE-INFER-*`, `V2-TYPE-CONTROL-let-written-type`, `V2-TYPE-CONTROL-let-written-type-mismatch` | `@type.ambiguousInference`, `@type.mismatch` | `@types.v2` |
| Generics | 16 / 18, 19 | `V2-TYPE-GENERIC-*`, `V2-SRC-EXPR-anon-fn-generic-*` | `@type.typeArgumentMismatch`, `@type.ambiguousInference`, `@type.unsatisfiedBound`, `@name.genericRedeclaration` | `@types.v2` |
| Interfaces; the `Self` map; implementations | 19 | `V2-TYPE-INTERFACE-impl-*`, `V2-TYPE-INTERFACE-missing-member`, `V2-TYPE-INTERFACE-unknown-member`, `V2-TYPE-INTERFACE-member-twice`, `V2-TYPE-INTERFACE-default-override`, `V2-TYPE-INTERFACE-signature-mismatch`, `V2-TYPE-INTERFACE-default-member`, `V2-TYPE-INTERFACE-default-empty-body`, `V2-TYPE-INTERFACE-self-position`, `V2-TYPE-INTERFACE-self-map` | `@type.redundantImplementation`, `@type.missingAbstractMember`, `@type.defaultOverride`, `@type.mismatch`, `@type.missingReturn`, `@type.unsupportedSelfPosition`, `@type.typeArgumentMismatch`, `@name.unknownSymbol`, `@name.memberCollision` | `@types.v2` |
| Associated types | 10 / 19 | `V2-TYPE-INTERFACE-assoc-*`, `V2-TYPE-NAMES-no-projection-path`, `V2-TYPE-NAMES-assoc-shadows-parameter`, `V2-TYPE-INTERFACE-iter-remainder-type` | `@name.redeclaration`, `@name.unknownSymbol`, `@name.genericRedeclaration`, `@type.missingAbstractMember`, `@type.typeArgumentMismatch`, `@type.mismatch` | `@types.v2` |
| Bounds | 10 / 19 | `V2-TYPE-GENERIC-bound-*`, `V2-TYPE-GENERIC-given-unknown`, `V2-TYPE-GENERIC-given-repeated`, `V2-TYPE-INTERFACE-assoc-label` | `@name.wrongEntityKind`, `@name.unknownSymbol`, `@name.redeclaration`, `@type.unsatisfiedBound`, `@type.ambiguousInference`, `@type.mismatch` | `@types.v2` |
| Substitution and dispatch | 19 | `V2-TYPE-INTERFACE-receiver-dispatch`, `V2-TYPE-INTERFACE-destination-dispatch`, `V2-TYPE-INTERFACE-ambiguous-destination`, `V2-TYPE-INTERFACE-destination-shape`, `V2-TYPE-INTERFACE-no-implementation`, `V2-TYPE-INTERFACE-ambiguous-implementation`, `V2-TYPE-INTERFACE-self-parameter-mismatch`, `V2-TYPE-INTERFACE-undispatchable`, `V2-TYPE-INTERFACE-generic-twice` | `@type.ambiguousDestination`, `@type.undispatchableContractMember`, `@type.ambiguousImplementation`, `@type.unsatisfiedBound`, `@type.argumentMismatch`, `@type.mismatch` | `@types.v2`, `@value.v2` |
| Interface values | 19 | `V2-TYPE-INTERFACE-value-*`, `V2-TYPE-INTERFACE-any` | `@type.typeArgumentMismatch`, `@type.argumentMismatch`, `@type.mismatch` | `@types.v2`, `@value.v2` |
| Overlap | 19 | `V2-TYPE-INTERFACE-overlap` | `@type.overlappingImplementation` | `@types.v2` |
| Iteration | 19 / 20 | `V2-TYPE-INTERFACE-iter-*` | `@type.unsatisfiedBound` | `@value.v2` |
| Joins | 15 | `V2-TYPE-CONTROL-if-branch-types`, `V2-TYPE-CONTROL-if-value-no-else`, `V2-TYPE-CONTROL-never-branch-skipped` | `@type.mismatch` | `@types.v2` |
| Coverage | 15 / 17 | `V2-TYPE-CONTROL-match-*`, `V2-TYPE-CONTROL-unsupported-coverage` | `@pattern.nonExhaustive`, `@pattern.unreachableArm`, `@pattern.unsupportedCoverage`, `@pattern.refutableBinding`, `@pattern.irrefutableLetElse` | `@types.v2` |
| Constant expressions and constant patterns | 11 / 17, 18, 21 | `V2-TYPE-NAMES-constant-pattern`, `V2-TYPE-NAMES-nonconstant-as-pattern`, `V2-TYPE-CONTROL-match-constant-pattern`, `V2-SRC-CALLS-default-not-constant`, `V2-CONTRACT-constant-construction-*`, `V2-CONTRACT-constant-limit` | `@type.notConstant`, `@name.wrongEntityKind`, `@pattern.refutableBinding`, `@tool.limitExceeded` | `@types.v2` |
| The `try` expression | 17 | `V2-TYPE-CONTROL-try-*`, `V2-PROJECT-test-try` | `@type.invalidTry`, `@type.mismatch` | `@types.v2`, `@value.v2` |
| Fallible values | 17 | `V2-TYPE-CONTROL-unhandled-fallible`, `V2-TYPE-CONTROL-discard-fallible`, `V2-TYPE-CONTROL-option-not-fallible`, `V2-TYPE-CONTROL-fallible-never-error` | `@type.unhandledFallible` | `@types.v2` |
| `return` and `Never` | 15 | `V2-TYPE-CONTROL-missing-return`, `V2-TYPE-CONTROL-return-type`, `V2-TYPE-CONTROL-unreachable-after-return`, `V2-TYPE-CONTROL-never-operand`, `V2-TYPE-INTERFACE-impl-never` | `@type.missingReturn`, `@type.invalidReturn`, `@type.unreachableCode`, `@type.mismatch` | `@types.v2` |
| Widening | 19 | `V2-TYPE-INTERFACE-value-widen`, `V2-TYPE-INTERFACE-value-no-widen-again`, `V2-TYPE-INTERFACE-widen-container`, `V2-TYPE-INTERFACE-widen-inferred-not-written`, `V2-TYPE-INTERFACE-widen-atom` | `@type.mismatch` | `@types.v2` |
| Conversion | 19 / 20 | `V2-TYPE-CONVERT-*` | `@type.redundantConversion`, `@type.ambiguousDestination`, `@type.unsatisfiedBound` | `@types.v2`, `@value.v2` |
| Value semantics | 13 / 18 | `V2-RUNTIME-closure-captures-value`, `V2-SRC-CALLS-evaluation-order`, `V2-TYPE-INTERFACE-iter-next-pure` | none | `@value.v2` |

## Guards and discharge rules

Type chapter, "Guards"; diagnostics chapter, "Integer facts". Rule
identities are those of `conformance/rules.tsv`.

| Guard form or rule | Step | Cases | Diagnostics | Schema |
| --- | --- | --- | --- | --- |
| Certified fragment (predicates) | 21 | `V2-CONTRACT-requires-not-bool`, `V2-CONTRACT-requires-unsupported`, `V2-CONTRACT-requires-uncertified-fn`, `V2-CONTRACT-requires-certified-fn`, `V2-CONTRACT-requires-recursive-fn`, `V2-CONTRACT-where-not-predicate`, `V2-CONTRACT-where-named-fn` | `@contract.invalidPredicate`, `@contract.unsupportedPredicate` | `@types.v2` |
| Bare `requires` | 22 | `V2-CONTRACT-bare-requires-unproved`, `V2-CONTRACT-rule1-constant-true`, `V2-CONTRACT-rule1-constant-refuted`, `V2-CONTRACT-unproved-notes` | `@contract.unproved`, `@contract.refuted` | `@types.v2` |
| Bare `ensures` | 22 | `V2-CONTRACT-ensures-refuted`, `V2-CONTRACT-ensures-every-return`, `V2-CONTRACT-ensures-through-try` | `@contract.unproved`, `@contract.refuted` | `@types.v2` |
| Bare `where` | 22 | `V2-CONTRACT-where-bare-construction`, `V2-CONTRACT-where-bare-unproved`, `V2-CONTRACT-where-bare-branch` | `@contract.unproved`, `@contract.refuted` | `@types.v2` |
| `requires … else` | 21 | `V2-CONTRACT-requires-else-true`, `V2-CONTRACT-requires-else-false`, `V2-CONTRACT-requires-else-skips-ensures`, `V2-CONTRACT-requires-else-wrong-type`, `V2-CONTRACT-folded-guard-evaluates-operands` | `@contract.invalidConsequence` | `@types.v2`, `@value.v2` |
| `ensures … else` | 21 | `V2-CONTRACT-ensures-else`, `V2-CONTRACT-ensures-else-candidate-bound`, `V2-CONTRACT-ensures-else-once`, `V2-CONTRACT-ensures-result-name` | `@contract.invalidConsequence` | `@types.v2`, `@value.v2` |
| `where … else` and fallible construction | 21 | `V2-CONTRACT-where-else`, `V2-CONTRACT-where-else-type`, `V2-CONTRACT-where-else-plain-type`, `V2-CONTRACT-where-else-nonconstant`, `V2-CONTRACT-where-on-variant`, `V2-CONTRACT-where-pattern-infallible` | `@contract.invalidConsequence`, `@type.invalidInvariant`, `@type.mismatch` | `@types.v2`, `@value.v2` |
| Compile-time evaluation of constant constructions | 21 | `V2-CONTRACT-constant-construction-ok`, `V2-CONTRACT-constant-construction-err`, `V2-CONTRACT-constant-construction-err-matched`, `V2-CONTRACT-nonconstant-construction`, `V2-CONTRACT-constant-limit` | `@pattern.refutableBinding`, `@tool.limitExceeded` | `@types.v2` |
| Guards and callable types | 18 / 22 | `V2-CONTRACT-obligation-lost`, `V2-CONTRACT-obligation-kept-by-type`, `V2-CONTRACT-else-converts-freely` | `@contract.obligationLost` | `@types.v2` |
| Guards of contract members | 07 / 21, 22 | `V2-SRC-DECL-impl-member-guard`, `V2-CONTRACT-member-guard-contract`, `V2-CONTRACT-member-else-copied`, `V2-CONTRACT-member-ensures-established` | `@syntax.invalidSection`, `@contract.unproved` | `@types.v2`, `@value.v2` |
| No run-time check for a bare guard | 22 / 29 | `V2-CONTRACT-no-runtime-check` | none | lowered IR (host test) |
| Obligation records | 21 | `V2-CONTRACT-obligations-snapshot` | none | `@types.v2` |
| `GUARD-1` constants | 22 | `V2-CONTRACT-rule1-constant-true`, `V2-CONTRACT-rule1-constant-refuted` | `@contract.refuted` | `@types.v2` |
| `GUARD-2` type invariant | 22 | `V2-CONTRACT-rule2-invariant`, `V2-CONTRACT-rule2-wrong-invariant` | `@contract.unproved` | `@types.v2` |
| `GUARD-3` dominating branch | 22 | `V2-CONTRACT-rule3-if-true-branch`, `V2-CONTRACT-rule3-if-false-branch`, `V2-CONTRACT-rule3-negation`, `V2-CONTRACT-rule3-outside-branch`, `V2-CONTRACT-rule3-match-fact` | `@contract.unproved` | `@types.v2` |
| `GUARD-4` substitution | 22 | `V2-CONTRACT-rule4-alias`, `V2-CONTRACT-rule4-text-not-enough`, `V2-CONTRACT-rule4-substitution` | `@contract.unproved` | `@types.v2` |
| `GUARD-5` Boolean structure | 22 | `V2-CONTRACT-rule5-and-projection`, `V2-CONTRACT-rule5-and-introduction`, `V2-CONTRACT-rule5-or-insufficient` | `@contract.unproved` | `@types.v2` |
| `GUARD-6` listed integer facts | 22 | the `INT` rows below; `V2-CONTRACT-in-range-relation`, `V2-CONTRACT-min-no-fact`, `V2-CONTRACT-no-arithmetic-fact`, `V2-CONTRACT-no-float-fact`, `V2-CONTRACT-no-user-ordered-fact` | `@contract.unproved` | `@types.v2` |
| `GUARD-7` certified results | 22 | `V2-CONTRACT-rule7-certified-result`, `V2-CONTRACT-rule7-requires-not-discharged` | `@contract.unproved` | `@types.v2` |
| `INT-1` | 22 | `V2-CONTRACT-int1`, `V2-CONTRACT-int1-nearest-failing` | `@contract.unproved` | `@types.v2` |
| `INT-2` | 22 | `V2-CONTRACT-int2` | `@contract.unproved` | `@types.v2` |
| `INT-3` | 22 | `V2-CONTRACT-int3` | `@contract.unproved` | `@types.v2` |
| `INT-4` | 22 | `V2-CONTRACT-int4`, `V2-CONTRACT-int4-nearest-failing` | `@contract.unproved` | `@types.v2` |
| `INT-5` | 22 | `V2-CONTRACT-int5` | `@contract.unproved` | `@types.v2` |
| `INT-6` | 22 | `V2-CONTRACT-int6` | `@contract.unproved` | `@types.v2` |
| `INT-7` | 22 | `V2-CONTRACT-int7`, `V2-CONTRACT-int7-nearest-failing` | `@contract.unproved` | `@types.v2` |
| `INT-8` | 22 | `V2-CONTRACT-int8` | `@contract.refuted` | `@types.v2` |
| Guards on a `proc`; plans | M2 | M2 | M2 | M2 |

## Programs, packages, and data

Programs chapter.

| Form | Step | Cases | Diagnostics | Schema |
| --- | --- | --- | --- | --- |
| VIBON data documents | 03 | `V2-SRC-READER-vibon-*`, `V2-SRC-READER-data-duplicate-field`, `V2-SRC-READER-data-duplicate-key`, `V2-SRC-READER-data-extension` | `@syntax.invalidForm`, `@data.duplicateField`, `@data.duplicateKey`, `@data.invalidExtension` | tree |
| Project file (`@project.v2`), library targets | 09 | `V2-PROJECT-decode-*` | `@data.invalidShape`, `@data.invalidValue`, `@data.duplicateField`, `@data.duplicateKey`, `@project.reservedUnitName` | `@project.v2` |
| Binary targets, entry, grant bindings | M2 | `V2-PROJECT-decode-binary-shape`, `V2-PROJECT-workspace-check-bin-unavailable` (the M1 boundary) | `@tool.unavailable` in M1 | M2 |
| Discovery and the source snapshot | 09 | `V1-PROJECT-graph-static-sorted`, `V1-PROJECT-graph-static-dot-root`, `V2-PROJECT-graph-not-found`, `V2-PROJECT-graph-root-missing`, `V2-PROJECT-graph-empty-tests` | `@project.notFound`, `@project.invalidTargetRoot`, `@project.ioError`, `@module.ioError`, `@module.pathEscape` | `@sourceGraph.v1` |
| Units and targets | 09 | `V1-PROJECT-graph-static-overlap`, `V2-PROJECT-graph-reserved-tests`, `V2-PROJECT-graph-reserved-std` | `@project.overlappingTargetRoots`, `@project.reservedUnitName` | `@sourceGraph.v1` |
| Module paths | 09 | `V1-PROJECT-graph-static-invalid-segment`, `V1-PROJECT-graph-static-collision`, `V2-PROJECT-graph-case-collision` | `@module.invalidSegment`, `@module.fileDirectoryCollision`, `@module.sourceIdCollision`, `@module.caseCollision` | `@sourceGraph.v1` |
| Imports | 10 | `V2-PROJECT-import-*` | `@module.unknownPath`, `@module.importCycle`, `@name.privateAccess`, `@name.unknownSymbol` | `@resolved.v2` |
| Local dependencies | M2 | M2 | M2 | M2 |
| Git dependencies and the lock | not in this release | | | |
| The standard library (embedding, manifest, roles) | 14 | host tests of step 14 | `@external.unknownSymbol`, `@external.signatureMismatch`, `@external.ownerMismatch`, `@runtime.invalidCheckedProgram`, `@module.sourceIdCollision` | `@stdlibManifest.v2` |
| Tests | 23 / 25 | `V2-PROJECT-test-*`; the host test `check_covers_tests` of step 25 | `@module.testOutsideTests`, `@module.missingRequiredImport`, `@name.redeclaration`, `@type.invalidReturn`, `@type.invalidTry` | `@testRun.v2` |
| Test worlds and grants | M2 | `V2-PROJECT-test-unavailable-item` (the M1 boundary) | `@tool.unavailable` in M1 | M2 |
| Assertions | 23 | `V2-PROJECT-test-fails`, `V2-PROJECT-test-stops-at-failure`, `V2-PROJECT-test-assert-renamed`, `V2-PROJECT-test-equal-typed-from-actual`, `V2-PROJECT-assert-outside-tests`, `V2-PROJECT-assert-value-outside-tests`, `V2-PROJECT-test-function-not-equatable`, `V2-PROJECT-test-unobservable` | `@module.assertionOutsideTests`, `@type.functionNotEquatable`, `@tool.unsupportedValue` | `@testRun.v2`, `@stdlibManifest.v2` |
| Build products | not in this release | | | |

## Standard library modules

Programs chapter, "The standard library"; runtime chapter, "Compiler
intrinsic registry".

| Module | Step | Cases | Diagnostics | Schema |
| --- | --- | --- | --- | --- |
| `@std.core` (`Equatable`, `Ordered`, `Ordering`, `From`, `TryFrom`, `ConversionError`, `ArithmeticError`) | 14 / 19, 20 | `V2-TYPE-INTERFACE-ordered-defaults`, `V2-TYPE-INTERFACE-equatable-tuple`, `V2-TYPE-CONVERT-*`, `V2-RUNTIME-checked` | `@type.redundantConversion`, `@type.ambiguousDestination`, `@type.unsatisfiedBound` | `@stdlibManifest.v2`, `@value.v2` |
| `@std.bool` | 14 / 20 | `V2-TYPE-CONTROL-match-bool`, `V2-TYPE-NAMES-bool-constant-pattern`, `V2-TYPE-INTERFACE-bool-eager` | `@type.mismatch` | `@value.v2` |
| `@std.option` | 14 / 17, 20 | `V2-TYPE-CONTROL-try-option`, `V2-TYPE-CONTROL-option-not-fallible`, `V2-TYPE-INTERFACE-iter-str-option` | `@type.invalidTry` | `@value.v2` |
| `@std.result` | 14 / 17 | `V2-TYPE-CONTROL-try-result`, `V2-TYPE-CONTROL-unhandled-fallible`, `V2-TYPE-INTERFACE-iter-result-none` | `@type.unhandledFallible`, `@type.invalidTry` | `@value.v2` |
| `@std.iter` (`Iter`, the four adapters) | 14 / 19, 20 | `V2-TYPE-INTERFACE-iter-*` | `@type.unsatisfiedBound` | `@value.v2` |
| `@std.builtin` (numbers, `Char`, `List`, `Dict`) | 14 / 16, 20 | `V2-RUNTIME-saturating`, `V2-RUNTIME-checked`, `V2-RUNTIME-float-ops`, `V2-RUNTIME-float-nan-observed`, `V2-TYPE-INTERFACE-char`, `V2-TYPE-INTERFACE-char-classifiers`, `V2-TYPE-INTERFACE-float-not-ordered`, `V2-TYPE-INTERFACE-dict-entries`, `V2-SRC-CALLS-lookup-list`, `V2-SRC-CALLS-lookup-dict` | `@type.unsatisfiedBound`, `@type.invalidDictKey` | `@value.v2` |
| `@std.str` | 14 (role) / 20 | `V2-TYPE-INTERFACE-str-*`, `V2-TYPE-CONVERT-parse-integer`, `V2-TYPE-CONVERT-render-integer`, `V2-TYPE-CONVERT-float-text`, `V2-TYPE-CONVERT-float-parse` | `@name.inaccessibleRepresentation` | `@value.v2` |
| `@std.bytes` | 14 (role) / 20 | `V2-TYPE-INTERFACE-bytes-members`, `V2-TYPE-INTERFACE-str-utf8` | none | `@value.v2` |
| `@std.assert` | 23 | `V2-PROJECT-test-passes`, `V2-PROJECT-test-fails`, `V2-PROJECT-assert-outside-tests` | `@module.assertionOutsideTests`, `@module.missingRequiredImport` | `@testRun.v2` |
| `@std.test` | M2 | | | |
| `@std.process` | M2 | | | |
| `@std.policy` | M2 | | | |
| `@std.io`, `@std.fs`, `@std.env`, `@std.time`, `@std.random` | M2 | | | |

## Runtime and tooling forms

| Form | Step | Cases | Diagnostics | Schema |
| --- | --- | --- | --- | --- |
| Evaluation order | 13 / 15, 16 | `V2-SRC-CALLS-order`, `V2-SRC-CALLS-evaluation-order` | none | `@value.v2` |
| Module-value initialization | 13 / 15, 23 | `V2-TYPE-CONTROL-module-value`, `V2-TYPE-CONTROL-initializer-cycle`, `V2-PROJECT-test-fresh-state` | `@type.initializerCycle` | `@value.v2` |
| Callable contract execution | 13 / 21 | `V2-CONTRACT-requires-else-*`, `V2-CONTRACT-ensures-else*` | none | `@value.v2` |
| Generic instantiation | 13 / 16, 19 | `V2-TYPE-GENERIC-pair`, `V2-TYPE-INTERFACE-generic-twice` | none | `@value.v2` |
| Tail calls | 13 / 15, 18 | `V2-RUNTIME-tail-loop`, `V2-RUNTIME-tail-through-value`, `V2-RUNTIME-tail-through-closure` | none | `@value.v2` |
| Activations and memory | 13 / 15 | `V2-RUNTIME-non-tail-depth`, `V2-RUNTIME-memory-exhausted`, `V2-PROJECT-test-exhaustion-isolated` | `@runtime.memoryExhausted` | `@testRun.v2` |
| Compiler intrinsic registry; integer, float, character, sequence, and dictionary operations | 12, 13 / 20 | `V2-RUNTIME-saturating`, `V2-RUNTIME-checked`, `V2-RUNTIME-float-ops`, `V2-TYPE-INTERFACE-str-members`, `V2-TYPE-INTERFACE-dict-entries` | `@runtime.invalidCheckedProgram` | `@value.v2` |
| Canonical value encoding | 12 / 13 | every `interpret` case; `V2-PROJECT-test-unobservable` | `@tool.unsupportedValue` | `@value.v2`, `@type.v2` |
| Wasm transition | 27 | host tests of step 27; `conformance/wasm-support.tsv` | `@tool.unavailable`, `@runtime.invalidCheckedProgram` | none |
| Diagnostic documents, ordering, and the registry | 02 | host tests of step 02 | every row of the registry | `urn:vibra:schema:v2:diagnostic` |
| Conformance manifest, rules, and profiles | 02 | host tests of step 02 | none | `case.toml`, `conformance/rules.tsv` |
| Command line: grammar, envelope, `help` | 24 | host tests of step 24 | `@tool.unavailable` | `urn:vibra:schema:v2:command-result` |
| Command `fmt` | 25 | `V2-TOOL-format-*` | `@module.pathEscape`, `@module.ioError` | `urn:vibra:schema:v2:command-result` |
| Command `check` | 25 | `V2-PROJECT-workspace-check-library`, `V2-PROJECT-workspace-check-bin-unavailable`; host tests of step 25 | `@project.notFound`, `@tool.unavailable` | `urn:vibra:schema:v2:command-result`, `@types.v2` |
| Command `test` | 26 | host tests of step 26 over the `V2-PROJECT-test-*` projects | `@tool.unavailable`, `@tool.unsupportedValue`, `@runtime.memoryExhausted` | `urn:vibra:schema:v2:command-result`, `@testRun.v2` |
| `proc`, plans, `await`, effect rows, ceilings | M2 | | | |
| Grants, policies, bindings, startup, the entry outcome | M2 | | | |
| Host registry, host world, audit trace, `run`, `eval`, `project` commands | M2 | | | |
| `query`, `edit`, `lint`, the index, snapshot and claim identity | M3 | | | |
| MCP server | not in this release | | | |
| Native execution slice | M4 | | | |

## Diagnostic codes

Every row of the registry, with the M1 steps whose guides cite it. A code
with no M1 step belongs to the milestone named.

| Code | Owner |
| --- | --- |
| `@syntax.unmatchedDelimiter` | steps 03, 04, 05, 06, 07, 25, 26 |
| `@syntax.missingSeparator` | steps 03 |
| `@syntax.invalidCharacterLiteral` | steps 03 |
| `@syntax.invalidStringLiteral` | steps 03 |
| `@syntax.invalidNumericLiteral` | steps 03 |
| `@syntax.invalidName` | steps 03 |
| `@syntax.invalidForm` | steps 03, 04, 05, 06, 07, 28 |
| `@syntax.misplacedBinding` | steps 06 |
| `@syntax.duplicateSection` | steps 04, 05, 07 |
| `@syntax.invalidEncoding` | steps 03 |
| `@syntax.invalidSection` | steps 04, 05, 07 |
| `@name.unknownSymbol` | steps 10, 11, 18, 19, 25 |
| `@name.wrongEntityKind` | steps 10, 19 |
| `@name.privateAccess` | steps 10, 16 |
| `@name.inaccessibleRepresentation` | steps 16, 20 |
| `@name.redeclaration` | steps 10, 11, 19, 23 |
| `@name.memberCollision` | steps 10, 16, 19 |
| `@name.genericRedeclaration` | steps 10, 19 |
| `@name.reservedLabel` | steps 04, 05, 06, 07 |
| `@name.reservedDeclaration` | steps 04, 05, 06, 07, 10, 11 |
| `@name.reservedValueSpelling` | steps 10 |
| `@module.fileDirectoryCollision` | steps 09 |
| `@module.sourceIdCollision` | steps 09 |
| `@module.caseCollision` | steps 09 |
| `@module.testOutsideTests` | steps 23 |
| `@module.assertionOutsideTests` | steps 23 |
| `@module.unknownPath` | steps 10 |
| `@module.missingRequiredImport` | steps 23 |
| `@module.importCycle` | steps 10 |
| `@module.invalidSegment` | steps 09 |
| `@module.pathEscape` | steps 09, 25 |
| `@module.ioError` | steps 09, 25 |
| `@type.argumentMismatch` | steps 06, 15, 16, 17, 18, 19 |
| `@type.typeArgumentMismatch` | steps 16, 19 |
| `@type.redundantImplementation` | steps 19 |
| `@type.defaultOverride` | steps 19 |
| `@type.missingAbstractMember` | steps 19 |
| `@type.functionNotEquatable` | steps 16, 23 |
| `@type.notApplicable` | steps 15, 16 |
| `@type.unknownRecordField` | steps 11, 16 |
| `@type.numericOutOfRange` | steps 15 |
| `@type.notConstant` | steps 18 |
| `@type.initializerCycle` | steps 15 |
| `@type.undispatchableContractMember` | steps 19 |
| `@type.unsupportedSelfPosition` | steps 19 |
| `@type.overlappingImplementation` | steps 19 |
| `@type.ambiguousImplementation` | steps 19 |
| `@type.unsatisfiedBound` | steps 16, 19, 20 |
| `@type.ambiguousDestination` | steps 19 |
| `@type.redundantConversion` | steps 19 |
| `@type.mismatch` | steps 15, 16, 17, 18, 19, 21, 25, 26 |
| `@type.ambiguousInference` | steps 15, 16, 18, 19 |
| `@type.infiniteSize` | steps 16 |
| `@type.invalidDictKey` | steps 16, 19 |
| `@type.invalidTry` | steps 17, 23 |
| `@type.unhandledFallible` | steps 17 |
| `@type.invalidReturn` | steps 15, 23 |
| `@type.missingReturn` | steps 07, 15, 17, 19 |
| `@type.invalidInvariant` | steps 21 |
| `@type.suspiciousRow` | M2 |
| `@type.unreachableCode` | steps 15 |
| `@pattern.refutableBinding` | steps 17, 21 |
| `@pattern.irrefutableLetElse` | steps 17 |
| `@pattern.nonExhaustive` | steps 15, 17 |
| `@pattern.unreachableArm` | steps 15, 17 |
| `@pattern.unsupportedCoverage` | steps 17 |
| `@effect.outsideCeiling` | M2 |
| `@effect.invalidReference` | M2 |
| `@effect.duplicateRoot` | M2 |
| `@effect.invalidAwait` | steps 17 |
| `@external.unknownSymbol` | steps 14 |
| `@external.ownerMismatch` | steps 14 |
| `@external.signatureMismatch` | steps 14 |
| `@data.invalidExtension` | steps 03 |
| `@data.invalidShape` | steps 09 |
| `@data.invalidValue` | steps 09, 27 |
| `@data.duplicateField` | steps 03, 09 |
| `@data.duplicateKey` | steps 03, 09 |
| `@project.staleLock` | not in this release |
| `@project.dependencyCycle` | M2 |
| `@project.entryOutsideTarget` | M2 |
| `@project.entryOnLibrary` | M2 |
| `@project.invalidEntrySignature` | M2 |
| `@project.ambiguousDependencyTarget` | M2 |
| `@project.reservedUnitName` | steps 09 |
| `@project.overlappingTargetRoots` | steps 09 |
| `@project.notFound` | steps 09, 25 |
| `@project.invalidTargetRoot` | steps 09 |
| `@project.ioError` | steps 09 |
| `@runtime.invalidHostValue` | M2 |
| `@runtime.invalidCheckedProgram` | steps 12, 13, 14, 27 |
| `@runtime.memoryExhausted` | steps 13, 15, 26, 29 |
| `@style.casing` | steps 02, 10 |
| `@contract.unusedEffect` | M2 |
| `@contract.unproved` | steps 21, 22, 29 |
| `@contract.refuted` | steps 22, 29 |
| `@contract.unsupportedPredicate` | steps 21, 22 |
| `@contract.obligationLost` | steps 18, 21, 22 |
| `@contract.invalidPredicate` | steps 21 |
| `@contract.invalidConsequence` | steps 21 |
| `@grant.invalidBinding` | M2 |
| `@grant.bindingCycle` | M2 |
| `@grant.requiredUnavailable` | M2 |
| `@grant.acquisitionFailed` | M2 |
| `@grant.invalidPolicy` | M2 |
| `@grant.policyTooComplex` | M2 |
| `@tool.unavailable` | steps 03, 04, 05, 06, 07, 15, 23, 24, 25, 26, 27 |
| `@tool.stalePlan` | M3 |
| `@tool.actionDenied` | M3 |
| `@tool.limitExceeded` | steps 21, 22 |
| `@tool.unsupportedValue` | steps 13, 23, 26 |
| `@tool.unsupportedTarget` | M4 |
| `@tool.missingLinkTool` | M4 |
| `@tool.linkFailed` | M4 |
| `@tool.incompatibleArtifact` | M4 |
