# v0.0.2 M1: specification questions

Three questions the M1 plan cannot answer from the specification. Each
names the place, the question, why the specification does not answer it,
the answer the plan recommends, and the task it blocks. An answer changes
the specification, not a step guide; the guide then follows the chapter.
No step chooses an answer.

| Item | Blocks | Area |
| --- | --- | --- |
| S1 | the `tree` snapshots of step 06 whose input has a `_`, and those of any step whose input has a byte that starts no token | `@tree.v2` leaf kinds |
| S2 | the `types` snapshots of step 22 that list `rules` | obligation records |
| S3 | `stdlib/src/std/assert.vib` in step 23 | `@std.assert` |

## S1. The leaf kind of `_` and of a byte that starts no token

Where: [diagnostics
chapter](../../../spec/07-diagnostics-and-conformance.md#persistent-observations),
`@tree.v2`; [source chapter](../../../spec/01-source-language.md#grammar), the
lexical production `discard`.

Question: which `kind` does the leaf of the discard `_` have, and which
does the leaf of a character that begins no token, such as `?` or `#`?

Why the specification does not answer it: the chapter says a lexical
production is a leaf and closes the list of leaf kinds: `@identifier`,
`@reservedWord`, `@integer`, `@float`, `@atom`, `@string`, `@docString`,
`@character`, `@punctuation`, `@whitespace`, `@comment`, and `@missing`.
`discard` is a lexical production, `_` is not an identifier character and
not a reserved word, and no sentence calls it punctuation. A character
that starts no token is kept by recovery inside an `@error` node "as its
ordinary token leaf", but it has no token class and so no kind in the
list. Every byte must be in a leaf, so both need one.

Recommended: `_` is a `@punctuation` leaf, and so is each character that
starts no token, inside its `@error` node. One sentence in the leaf-kind
item of `@tree.v2` settles both.

## S2. The identifiers of the seven discharge rules

Where: [diagnostics
chapter](../../../spec/07-diagnostics-and-conformance.md#persistent-observations),
the obligation record of `@types.v2`; [type
chapter](../../../spec/02-type-system.md#bare-guards).

Question: which strings does the `rules` field of an obligation record
hold for the seven discharge rules?

Why the specification does not answer it: `rules` is "the discharge rule
identifiers used, in order". The integer facts have the identifiers
`INT-1` to `INT-8` in their table. The seven rules are a numbered list
with names in prose and no identifier, and the tooling chapter's
`ruleIds` field of the index does not spell them either. A `types`
snapshot compares the strings, so two implementations must agree on them.

Recommended: `GUARD-1` to `GUARD-7`, in the order of the type chapter's
list, stated in that list or beside the obligation record. When rule 6
discharges an obligation, `rules` holds `GUARD-6` followed by the facts
used.

## S3. The source form of the `@std.assert` declarations

Where: [programs chapter](../../../spec/04-programs-and-packages.md#assertions);
[runtime chapter](../../../spec/06-runtime.md#test-assertions); [type
chapter](../../../spec/02-type-system.md#language-core-and-standard-library).

Question: how does the embedded module `@std.assert` declare `true`,
`false`, and `equal(T)`?

Why the specification does not answer it: the runtime chapter says an
assertion "is an ordinary typed call" that "has no registry symbol", so
the three are not `external @compiler` declarations. The type chapter
closes the language roles at nine, and none is an assertion. Yet a false
assertion records a failure and stops its test, which no `fn` body can
express: the pure language has no such outcome. The module must contain
three declarations that the checker and the interpreter treat specially,
and no rule says what their source is or by what the toolchain knows
them.

Recommended: the three are ordinary `fn` declarations in `@std.assert`
with the bodies that hold for a passing assertion, which is `return void`,
and the toolchain knows them by their declaration identities, as it knows
`@std.assert` itself for `@module.assertionOutsideTests`. A sentence in
"Assertions" says so. This adds no keyword, no role, and no registry row.
