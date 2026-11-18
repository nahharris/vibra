# Carry-over audit: atoms, imports, patterns and the ledger

[argued] Accept the inventory and carry-over table as the way to proceed.
The K cards were deliberately small experimental languages. Neither their
coverage nor their measured scores establishes that an omitted Vibra feature
should be removed. I contributed to the omission: the previous sheet review
should have separated **existing normative behavior**, **explicit redesign
decisions**, and **experimental omissions**. **Instead:** reconcile those three
columns before treating the sheet as a complete replacement language.

[cited] This review checks the current [source chapter](../../../../spec/01-source-language.md),
[type chapter](../../../../spec/02-type-system.md),
[effects chapter](../../../../spec/03-effects.md), and
[programs chapter](../../../../spec/04-programs-and-packages.md).
They remain the normative target; the debate and its proposed K3 spellings
remain non-normative. This is a focused audit of proposal (3) and affected
decisions, not a competing complete inventory of chapters 01–04.

## 1. Atoms: retain the value, constrain each interpreting position

[cited] Source chapter, Reader: an atom is an ordinary value unless a grammar
or data-schema slot explicitly interprets it as an entity reference. Writing
`@std.io` in an expression does not load or invoke code. Type chapter, Model:
`atom` contains interned atoms and each written atom also has a singleton type.
Source chapter, Labels and applications: atoms are not applicable. These are
already specified behaviors, not a novel feature justified only by imports.

[argued] There is no general objection to atoms. My earlier "K3 has no atoms,
so use a string" premise described the frozen card subset; it was inadequate
as a reason to remove an existing source/data value. Accept retaining atoms
and an `Atom` type under the owner naming rule. **Instead:** preserve literal
equality/identity and non-applicability, specify canonical encoding and limits,
and mark any change to singleton typing or reference resolution explicitly.
Restoring the lexical token alone does not settle those semantic contracts.

[argued] An atom is neither a capability nor an implicit lookup instruction.
`@std.files.read` as a value must not acquire a file handle; a metadata atom
must not resolve or invoke a same-spelled user declaration. Likewise it must
not circumvent private access. **Instead:** keep ordinary atom values separate
from the closed grammar/schema positions that interpret locators or metadata.
Resolve entity references once to canonical identities and check their kinds.

[argued] Do not copy the old atom lexer mechanically. Its components derive
from kebab-case source symbols; the new owner rule permits PascalCase types,
uses camelCase values, and excludes hyphens from identifiers. Module paths
also currently derive from kebab-case filesystem segments. **Instead:** settle
one atom component grammar plus one module-layout/identity policy, preserving
the distinction between a locator's spelling and the pinned entity it denotes.
Do not secretly accept old/new identifier conventions as two compatibility
surfaces. The existing no-search/no-directory-index loader rule is reusable.

## 2. Imports: declaration imports already solve the doubled List name

[cited] Programs chapter, Modules and imports, already specifies **both module
imports and declaration imports**. Its example `(import ordering
@std.core.ordering)` binds a public declaration in every namespace it occupies;
`ordering.less` then reaches its variant. A module import instead provides only
the alias and qualified public members. Files already are modules without
redeclaring their identity. No explicit module declaration is being removed.

[argued] Thus my earlier `list.List.map` example was too restrictive as a
description of existing Vibra. Under a translated import grammar, a direct
declaration import could be:

```text
import @std.list.List as List
```

[argued] That example assumes the final library actually exports that path;
it is not a factual claim about its current spelling. It binds the type and
its namespace directly, so `List(Int)` and `List.map` need neither module
fallback nor first-class record modules. **Instead:** carry over declaration
imports explicitly, irrespective of whether selective brace imports are added.

[argued] Accept atoms in the proposed single import production. Compared with
current alias-only binding, `import @std.list as {List}` adds a new **selective
import binding** form. It does not merely recover a pattern feature the cards
forgot. I accept that convenience with the restricted rules below, but reject
an unrestricted runtime pattern as an import operand. **Instead:** define a
static projection from exported namespace members to local bindings.

### Concrete proposed import contract

[argued] Use one top-level, non-executing import production:

```text
import @std.list as list
import @std.list as {List}
import @std.list as {List: IntList}
```

[argued] These illustrate alternatives, not three simultaneous bindings that
would be legal in the same scope. The first keeps a whole-module namespace;
the last two select exported members. A locator resolving to one public
top-level declaration admits only the name form at this gate. Rules:

- The `as` name binds the resolved module or declaration. A brace form requires
  a module and lists one or more immediate public exports. There is no wildcard,
  recursive export traversal, import matching failure branch or runtime test.
- `{List}` is a pun for `{List: List}`. `List: IntList` means exported name on
  the left and local name on the right. Unknown, private, repeated selected
  members and duplicate local bindings are errors; no source-order winner.
- A projected declaration keeps its kind, generic binders, canonical identity,
  owner namespace and visibility restrictions. Importing a type under a local
  name does not redeclare a nominal type or export its private constructors.
- Casing follows the kind of local binding: a module alias such as `list` is
  camelCase; an imported type alias such as `IntList` is PascalCase. Casing is
  checked after resolution, not used to guess the entity's kind.
- Literal, constructor, array, union-narrowing, discard and arbitrary nested
  value patterns are not permitted here. Namespace selection shares pair/pun
  notation with records; a module does not become an ordinary record value.
- Namespace access is static and side-effect-free. Preserve pinned unit roots,
  exact component resolution, private access checks, cycle rejection, no host
  filesystem fallback, and no package download during semantic checking.
  Pure constant definitions do not authorize effectful import initialization.

[argued] This introduces a brace form whose contents **bind exported entities**;
it is not a declaration of a record shape. `{List}` also introduces pair punning
without a written colon. C1 and the global "braces declare pairs only" sentence
therefore need an explicit qualification. **Instead:** describe the common
notation and the distinct judgments honestly. If the additional judgment is
unwanted at gate 1, keep direct declaration imports and defer selective imports;
do not claim full pattern reuse makes the new form semantically free.

## 3. Metadata atoms are acceptable only with closed schemas

[argued] Accept `visibility @public` / `visibility @private`, with private as
the omitted default, and `delivery @once` / `@repeatable` / `@unknown` as
compile-time metadata spellings. These are atoms interpreted by closed
metadata schemas, not global variables, callable constructors or user-extensible
effect declarations. **Instead:** reject misspellings and unknown values with
an unsupported/invalid-metadata diagnostic; never silently fall back to public,
repeatable or successful behavior.

[argued] Drop the proposed `Delivery.once()` spelling as an unmeasured proposal
being replaced, not an experiment loser. An ordinary unconstrained Atom field
would admit too many values: each metadata slot needs a fixed accepted set.
Specify whether metadata singleton types are represented in the type system
or validated directly by the declaration schema. **Instead:** keep that choice
explicit; it need not restore unrestricted union syntax as a prerequisite.

[argued] Atoms do not settle what `once` promises. Keep F8's actual retry,
outcome and uncertainty semantics and the previous readLine counterexample:
retrying a consuming read can consume a different line. `@public` on a type
also does not expose a sealed host-handle constructor. **Instead:** preserve
the delivery and opaque-construction rules independently of their spelling.

## 4. Restore pattern semantics; do not promote narrowing to a replacement

[cited] Source chapter, Functions and expressions / let / let-else / Collections:
current let and positional parameters require irrefutable patterns; match uses
refutable patterns with reachability/exhaustiveness checking. Tuple, record,
constructor and array patterns, literals, constant patterns, discard and `as`
patterns exist. Record patterns can omit fields; array patterns have exact
length. A let-else mismatch executes a `never` fallback and exposes successful
bindings only afterward. These are substantive guarantees and capabilities.

[argued] Accept destructuring in the redesign and retain its full semantic
inventory as the starting point. The pilot's `case some` followed by `.value`
does not establish that binding payloads in patterns is unnecessary or that
the narrowed-name form is the unique match syntax. **Instead:** write genuine
pattern positions for let, parameters and match, with nested constructor,
record-field, constant, literal and discard cases; port let-else with single
evaluation and a non-returning fallback. Track each unsupported pattern family
against the deferred underlying type rather than silently omit it.

[argued] Do not introduce two independently drifting match languages. Bare
`case some` may be retained as precisely specified constructor shorthand, or
replaced by a qualified constructor pattern; choose before the rewrite. Match
must evaluate the scrutinee once, use arm-local bindings, reject duplicate
binders, and check coverage at the actual matched type. **Instead:** make
binding-versus-comparison and irrefutability shared checker judgments, not
ad hoc behaviors of individual surface forms.

[argued] Parameter patterns need an explicit interaction with `takes` and
labels. `{x: Int}` was a name/type pair in the experiment. A structural pattern
in place of `x` no longer satisfies the simple "name: Type" grammar. Likewise
record punning must state when a token introduces a binder versus compares
against a module constant. **Instead:** settle the parameter-pattern production
and constant-pattern precedence in the carry-over table; do not infer them
from the import form. Casing alone must not choose binds-versus-compares.

[cited] There is already a wording inconsistency to flag rather than port
blindly: Source chapter's Reader discusses resolving module values as constant
patterns before treating names as binders, while its let section says a bare
unqualified local name "always introduces a binding." The type chapter's
Constant patterns section specifies the qualified rule. **Instead:** have the
inventory record this inconsistency and choose one canonical rule before any
implementation depends on it. No normative file is edited by this review.

## 5. Which ledger decisions need correction, and why

[argued] "Existing" is a factual provenance category, not a veto on redesign.
Conversely, a subset experiment is no evidence that deleting an existing
feature improves the full language. These are the specific rows I would amend:

| Row | Diagnosis | Replacement disposition |
| --- | --- | --- |
| C7, C4, J5 | C7 explicitly excluded lambda as a reversible gate default; X4 never ran. Existing lambda semantics were not disproved, and J5 already requires higher-order lowering. Our latest anonymous-Fn agreement supersedes the exclusion. | Retain anonymous functions; record K3's anonymous `fn` as changed spelling of an existing capability. Preserve capture, generic, result and effect/Proc obligations. Keep X4 unrun, with no measured ergonomics claim. |
| X1b-6, D1, D7 | "New semantics" for tuples, anonymous records and dict is wrong relative to current Vibra. It is true only relative to the experiment grammar. Structural and nominal types already coexist in chapter 02. | Correct that provenance now. Mark the full-language fate as carry-over pending inventory reconciliation, not a measured removal. Any continued gate deferral is a deliberate scope cut requiring a row, lost-capability statement and reopening case. Literal notation remains unmeasured. |
| F1, F3, F6 | Removing rows was a deliberate capabilities-first redesign, not something the card silently proved. Current rows supply a static ceiling on may-attempt effects; capabilities supply runtime authority. They are orthogonal. | Retain the recorded trade-off provisionally, but explicitly compare it with existing closed ceilings and resolve the owner's audit requirement before the normative rewrite. Reopen F1/F3 if broad/unknown summaries do not meet it. Do not restore `deffect` automatically merely because atoms return. |
| C1, C4, C8 | Match narrowing and typed name-only let were experimental restrictions, not an agreed abolition of destructuring. C8 concerns annotation inference, not permission to forbid patterns. | Add a pattern/carry-over row, including let-else, binding scope, constant precedence, coverage and parameter syntax. Version the expanded grammar and keep pilot scores scoped to the original cards. |
| B5 | Five declaration kinds leave existing `def` and `test` unaccounted for. Ordinary protected tests are a deliberate replacement for a top-level test form; constants have not been accounted for by merely listing five kinds. | Map `def` explicitly to a kept constant declaration or a stated replacement/cut. Map test discovery, naming, failures and effect consent to protected ordinary functions plus manifest selection, or retain a test declaration. Amend the five-kinds count if needed. |
| C6 | Current labelled/default/variadic parameters are broader than the experiment's positional calls. X3 was deferred; it supplies no evidence for removing those capabilities. | Record each as existing and either retain it with complete binding/evaluation/default rules or identify the deliberate gate cut. Constructor labels alone do not cover function labels/defaults/variadics. |
| C1, B10, B11 | Atoms, selective imports and pattern braces change the proposed grammar; current imports already bind modules or declarations without search. | Keep atom/declaration import provenance; selective brace import is new. Qualify brace rules and pin resolver/interpretation versions. Import renaming preserves declaration identities. |
| D4, E1–E3 | K's executable `where` predicate is not current Vibra's generic `where:` bound list. Primitive/numeric scope and failure policies were simplified in the cards. | Separate type bounds (`given` proposal), invariant predicates (`where` proposal), and existing numeric/partial-operation semantics. No silent assertion that arbitrary pure predicates are statically total. |

[cited] In particular [R5 in r2-codex](r2-codex.md#2-r5-authority-does-not-appear-from-nowhere)
already conceded that unforgeable delegation can enforce confinement without
an effect index, while preserving the counterexample to **exact local auditing**.
Current chapter 03 explicitly says its rows are erased and are not grants or
a path-level sandbox; higher-order Fn types carry exact closed rows. Therefore
this is not evidence that our F1 argument assumed effect checking never existed.

[argued] The strongest comparison case remains: accept a callback that may
log, reject one that may use the network, without inspecting its witness, while
distinguishing actual directory/host scopes at runtime. Existing closed rows
address nominal families but not those value scopes; capability confinement
alone addresses delegation but not the exact local family declaration. Cold
intents also require distinguishing plan creation from execution. **Instead:**
compare row-plus-capability and conservative-summary-plus-capability designs
on that case. If exact family ceilings are a required retained guarantee,
F1/F3 must change; do not relabel conservative unknown as equivalent checking.

## 6. Additional carry-over traps worth putting in the inventory

[argued] Beyond the suspected rows, require explicit dispositions for:

- **Evaluation order:** current calls evaluate fixed/labelled operands in
  resolved declaration order, allowing formatter normalization. The proposed
  record invariant review uses written order. Record that semantic change;
  formatter reordering cannot remain harmless without the corresponding rule.
- **Control flow:** existing bodies use final-expression results and reject
  redundant tail return; the cards use explicit returns. `never` underlies
  return, let-else and coverage. Unit/void naming does not remove those rules.
- **Scope:** current no-shadowing rules are stricter than the pilot's allowed
  parameter shadowing of outer names. Neither can silently become the other.
- **Privacy:** ordinary current wrappers already have restricted construction
  and unwrapping. W's one-field record replacement must preserve any intended
  opaque abstraction explicitly, beyond sealing host handles alone.
- **Associated operations/interfaces:** current methods, default members,
  implementation identity, generic bounds and function-value access need a
  mapping. New static dispatch and explicit Self are changes, not just renames.
- **Values/types:** numeric widths, floats, characters, bytes, Atom singleton
  types, any, never, void, union widening/ascription, enum payloads, tuple
  projection, iterators, ordered dict keys and duplicate-key order each need
  a row. Naming `Int` or `List` does not define their replacement semantics.
- **Docs/data:** existing quoted strings preserve interior CR/CRLF; the new
  raw triple-string rule normalizes LF. Keep it specific to that new literal
  form and version changed persistent value interpretations deliberately.

[argued] Use columns: existing construct and source anchor; existing observable
behavior/guarantee; proposed K3 spelling; semantic change; status and gate;
ledger/owner decision; conformance or experiment needed. Distinguish **kept**,
**changed**, **deliberately deferred**, **dropped with reason**, and **unresolved**.
One spelling column is insufficient where the checker/runtime contract changes.
**Instead:** finish that reconciliation before calling the sheet complete;
do not restore every feature uncritically or invent a deletion by omission.

## Disposition and reopening checks

[argued] Accept atoms, atom locators and closed metadata atoms. Accept selective
brace imports only as static exported-member bindings with the restrictions
above; direct declaration imports already provide the simplest solution for
one imported type. Accept the inventory-driven restoration of real patterns.
No new syntax result is claimed measured and no frozen experiment is changed.

[argued] Reopen the import recommendation on fixtures covering module versus
declaration locators, rename-preserved nominal identity, generic type imports,
private constructors, missing exports, duplicate locals, Windows path case,
cycles and atom-as-value non-resolution. Reopen unrestricted import patterns
only on a concrete needed use case beyond static export selection. For pattern
rules, use the same nested/refutable/irrefutable/constant cases in let,
parameters, match and let-else. For effect ceilings, use the explicit callback
policy above. These are proposed settling checks, not experiments already run.
