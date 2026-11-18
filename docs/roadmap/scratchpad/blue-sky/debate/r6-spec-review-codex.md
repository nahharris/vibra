# Round 6: bounded adversarial specification review

Reviewed the on-disk chapters 00, 01, 02, 03, 04, 05 and 07 against each other, effective-design.md (including section 11a), ledger section R, and the relevant carry-over entries. This report contains 16 findings. No runtime or roadmap rewrite is reviewed here. No canonical document was edited. Locations below are in `docs/spec/` unless qualified otherwise. All findings are [cited]; proposed fixes are recommendations, not newly agreed decisions.

## 1. P1 — Metadata values are incorrectly required to resolve to code entities

**Location and quote:** `01-source-language.md:100`: “Only a grammar position that expects an entity reference resolves an atom as one: ... the operands of the `visibility`, `representation`, `role`, and `external` sections.” Line 104 requires these paths to match “actual entity names”.

**Broken rule:** This contradicts the closed metadata values in 01:297 (`@public`, `@private`), the language-role tags in 02:46–60 (`@bool`, `@iter`, etc.), and the provider tags in 01's external declarations. These are discriminants, not declarations in the source graph. Applying the stated resolution rule rejects ordinary public declarations and the embedded role declarations. Effective-design 2.6 and its binding section 11a preserve slot-specific atom meanings.

**Fix:** Resolve import locators as entities, but validate visibility, representation, role and external operands against their respective closed tag sets.

## 2. P1 — The grammar cannot declare the required interface roles

**Location and quote:** `01-source-language.md:167`: an interface contains only “`given | visibility | doc | fn-decl | proc-decl | impl`”.

**Broken rule:** `role` is absent, while 02:58 requires the embedded library to declare each role with that section. The source examples `interface Equatable role @equatable` (02:210 onward), `Ordered`, and `Iter` (02:496–497) consequently fail the grammar. Effective-design 3.10 and ledger A-13 retain these standard-library sections.

**Fix:** Admit `role` in the interface production, with the same embedded-library-only restriction as on types.

## 3. P1 — Types cannot contain the exported procedures the agreed brand design requires

**Location and quote:** `01-source-language.md:163`: “`type-item = ... | fn-decl | impl`”; 01:353 likewise permits only nested `fn` declarations and implementations.

**Broken rule:** The binding effect-ceiling position, `debate/r4-effect-ceiling-form-codex.md:291–305`, explicitly uses an exported nested procedure to operate through a privately represented grant brand. The canonical-format rule itself lists nested “`fn` and `proc` members” (01:719). The grammar rejects the `proc` half, leaving a public/private brand without that agreed static procedure surface.

**Fix:** Admit nested `proc` declarations on types, and include them in the type-member namespace and visibility rules.

## 4. P1 — Section-order freedom contradicts the declaration grammar

**Location and quote:** `01-source-language.md:277`: “Sections of one declaration are parsed in any order.” But 01:179 fixes `takes`, then `labelled`, then `returns`; 01:171 requires that entire signature before any other section.

**Broken rule:** Effective-design 3.9 accepts any unambiguous section order. For example, `fn f returns r: I32 takes {x: I32} do return x endfn` satisfies that rule but not the grammar. The same conflict affects type `of`/case sections, test sections, and anonymous callable signatures.

**Fix:** Express declaration sections as an order-independent collection with required-section and uniqueness constraints, retaining canonical order only for formatting.

## 5. P1 — Canonical dictionary sorting changes grant acquisition order

**Location and quote:** `04-programs-and-packages.md:46`: canonical VIBON sorts “dict keys ... by their canonical encoded bytes”; `03-effects.md:300`: acquisition breaks dependency ties “in written order”.

**Broken rule:** Grant recipes are stored in a dict (04:76, 220), so canonicalizing the project can change startup order and its observable effects. In addition, 05:174–176 explicitly forbids the formatter from reordering binding recipes. The canonical-data and order-preservation requirements cannot both hold for this representation.

**Fix:** Store binding recipes in an ordered array of named records, with unique names, so canonical formatting preserves the agreed acquisition tie-break.

## 6. P1 — Composite grant bindings cannot satisfy the stated key rule

**Location and quote:** `04-programs-and-packages.md:221`: “The dict's keys MUST be exactly the entry's parameter names”; 04:262: “A parameter of a record or tuple type binds each of its components under a dotted key, such as `@logs.reader`.”

**Broken rule:** A parameter named `logs` cannot simultaneously require the exact key `@logs` and component keys `@logs.reader`/`@logs.writer`. Tuple components have no field names, and no positional key spelling is supplied. This contradicts the admitted composite entry signatures in 04:189–191 and the bundled-grant design in effective-design 7.6.

**Fix:** Define a recursive leaf-binding path scheme, including tuple positions and optional bundles, and validate keys against those leaf paths rather than top-level parameter names.

## 7. P1 — Branded grant entry parameters have no valid issuer

**Location and quote:** `04-programs-and-packages.md:189`: an entry may take “a declared type whose representation is a grant type”; 04:225 requires that type to be “one of the issuer's grant types”.

**Broken rule:** The closed issuer table (04:244–254) issues only leaf operation interfaces, not user brands such as `LogDir`. Merely unwrapping and automatically rewrapping such a brand would instead violate 03:277 (“Only the module that declares `LogDir` can construct one”) and 02:143–149. Thus a stated valid entry shape cannot be bound under the stated rules, and bypassing the restriction would destroy the issuer certificate of effective-design 7.8.

**Fix:** Specify typed acquisition through a defining-module issuer for branded parameters, preserving constructor privacy; do not let an arbitrary manifest mint a brand by wrapping a leaf.

## 8. P1 — Erased `Self` results lost the recursive substitution rule

**Location and quote:** `02-type-system.md:469`: “Where a member's result is `Self`, the result is the same interface value type.”

**Broken rule:** This covers a direct `Self` result only. `Iter.next` returns `Option({Item Self})` (02:501), and procedure results are inside `Intent`. The accepted interface-value rules explicitly include the iterator remainder and substitution inside the cold result (`debate/r4-effect-ceiling-form-codex.md:262–264, 304–305`; effective-design section 11a). Without that rule a call on `Iter(Item)` cannot expose its remainder with a usable public type, or risks exposing the hidden concrete type.

**Fix:** Restore the supported recursive result substitution, including the iterator product and cold `Intent`, while preserving the prohibition on returned callables accepting hidden `Self`.

## 9. P2 — The same invalid invariant consequence has two diagnostic codes

**Location and quote:** `02-type-system.md:667`: a “`where ... else` consequence that is not constant” is `@contract.invalid-consequence`; `07-diagnostics-and-conformance.md:247`: a “`where ... else` error that is not constant” is `@type.not-constant`.

**Broken rule:** These are the same rejection. Even 07:269 assigns a consequence “not constant where required” to `@contract.invalid-consequence`. This violates 07's single diagnostic authority and one-fault-once rule (07:16–17, 183–184).

**Fix:** Use `@contract.invalid-consequence` for a nonconstant invariant consequence in every chapter and reserve `@type.not-constant` here for labelled defaults.

## 10. P2 — Prelude import collisions have three diagnostic codes

**Location and quote:** `01-source-language.md:323`: an import “whose name is a prelude name” is `@name.redeclaration`; `02-type-system.md:85–86` assigns `@name.reserved-declaration`; `07-diagnostics-and-conformance.md:245` assigns `@name.reserved-value-spelling` to a prelude spelling used as an import.

**Broken rule:** One import introduction has three incompatible canonical outcomes, contrary to 07:16–17 and charter 00:17–19.

**Fix:** Apply the diagnostic authority's `@name.reserved-value-spelling` consistently to prelude import introductions and distinguish this case from ordinary duplicate aliases.

## 11. P2 — Procedure application has two machine-kind spellings

**Location and quote:** `02-type-system.md:275`: a `Proc` application has kind `@plan`; `05-tooling.md:466`: the application fact's closed kind set instead contains `@proc-plan`.

**Broken rule:** These both describe the same resolved application, while 05:393 requires query facts to be projections of the one checker and the charter requires one canonical machine contract.

**Fix:** Choose one tag and use it in the checked application classification, query schema, index and conformance expectations.

## 12. P2 — Test discovery order disagrees between chapters

**Location and quote:** `04-programs-and-packages.md:403`: discovery is module identity followed by “declaration order”; `05-tooling.md:191`: “canonical module order, then name order”.

**Broken rule:** Two tests named `z` then `a` in one module run and are reported in different orders under these rules. The carried-over TST-03 rule is module order followed by source order, with no recorded disposition changing it.

**Fix:** Use declaration/source order within each module in chapter 05 as well.

## 13. P2 — Project initialization can generate an invalid package name

**Location and quote:** `05-tooling.md:141–145`: init upper-cases the first letter of later destination-name runs, and prefixes digit-leading results with `project`; `04-programs-and-packages.md:109`: a package name is “lower-case words joined by hyphens”.

**Broken rule:** For destination `my-app`, init produces `myApp`, which violates the lower-case project schema it is required to generate.

**Fix:** Generate a lower-case hyphenated package name satisfying 04's rule, and separately generate any source-identifier target name.

## 14. P2 — The closed standard-library module inventory omits the policy module

**Location and quote:** `03-effects.md:260`: `ScopeError` is in `@std.policy`; 03:317–318 places the policy types there too. `04-programs-and-packages.md:354` says “Its modules are”, but its table has no `@std.policy`.

**Broken rule:** Under 04:368, adding a module is a specification and registry change, and non-prelude types need imports. The policy/narrowing API required by chapter 03 is outside chapter 04's module inventory.

**Fix:** Add `@std.policy` and its policy, selector, limits and error types to the canonical standard-library inventory.

## 15. P2 — Accepted examples introduce reserved prelude names

**Location and quote:** `01-source-language.md:301` and `04-programs-and-packages.md:288`: `import @std.str.Str`; `01-source-language.md:342`: `type Option(T)`.

**Broken rule:** 02:79–86 makes `Str` and `Option` prelude names that ordinary modules cannot introduce. These unqualified examples therefore fail their own name rules. The `Option` declaration could be an embedded-library fragment, but it is not identified as one.

**Fix:** Remove redundant `Str` imports from ordinary examples and explicitly identify the `Option` definition as an embedded-library fragment, or use a non-prelude example type.

## 16. P2 — Source examples contain unmarked non-grammar placeholders

**Location and quote:** `03-effects.md:124`, 03:214 and `04-programs-and-packages.md:210` contain a bare `…` inside `vibra` source fences.

**Broken rule:** `…` is neither an identifier nor any expression admitted by 01:209–220. These examples are not marked schematic, contrary to 07:462–465 and the agreed rewrite convention. This is a grammar failure, not an objection to abbreviated explanatory examples.

**Fix:** Mark these fences explicitly schematic, or replace each placeholder with grammar-valid source that satisfies the declared return paths.
