# M1 reader follow-ups

Places where the code differs from the specification. Each entry names the
rule, the section that states it, the code that must change, and the case
to add. [Step 08](steps/08-reader-exit.md) clears every entry and deletes
this file.

## 1. A malformed name is reported once

- Rule: a malformed identifier is `@syntax.invalidName` over the token and
  nothing else; the token gets no `@syntax.invalidForm`.
- Section: [source chapter](../../../spec/01-source-language.md#identifiers),
  "Identifiers"; [programs
  chapter](../../../spec/04-programs-and-packages.md#vibon-data-documents),
  "VIBON data documents"; the syntax table of the [diagnostics
  chapter](../../../spec/07-diagnostics-and-conformance.md#syntax-and-data-diagnostics).
- Code: `crates/vibra-syntax/src/data.rs`, `DataShaper::classify`. An
  `Identifier` token that fails the identifier production is classified
  `Class::Invalid(Some(span))`, so `{a: my_field}` reports
  `@syntax.invalidName` and `@syntax.invalidForm` at `4..12`. It must be
  `Class::Invalid(None)`. The source reader of steps 05 to 07 follows the
  same rule wherever it rejects an identifier.
- Case: `V2-SRC-READER-name-invalid-value`, input `{a: my_field}`, one
  `@syntax.invalidName` at `4..12`; row in `conformance/rules.tsv` with the
  anchor `01-source-language.md#identifiers`. The host test
  `data::identifiers_doc_strings_and_applications_are_not_data` changes
  with it if it asserts both codes.

## 2. An ordering fault is reported at the misplaced section

- Rule: a section written before the declaration's name, or after `do`, is
  `@syntax.invalidForm` at that section, one diagnostic for each such
  section.
- Section: [source chapter](../../../spec/01-source-language.md#grammar),
  the paragraph on section order under "Grammar"; the syntax table of the
  diagnostics chapter.
- Code: `crates/vibra-syntax/src/source/declaration.rs`. `check_sections`
  reports "a declaration's name comes before its sections" over
  `node.span()`, the whole declaration; `check_construct` reports "`do` and
  its body come last" once over `construct.span()`. Each must report at
  `section.span()` of the misplaced section.
- Cases: `V2-SRC-DECL-section-before-name`, input
  `fn returns r: I32 f do return 1i32 endfn`, `@syntax.invalidForm` at the
  `returns` section; `V2-SRC-DECL-section-after-do`, a `fn` with `takes`
  written after its `do` body, `@syntax.invalidForm` at that `takes`
  section. Both get rows in `conformance/rules.tsv` with the anchor
  `01-source-language.md#grammar`. The host test
  `source::do_comes_last_and_the_name_comes_first` changes its spans.

## 3. Two units with one name

- Rule: two targets with one name, or a target and a dependency with one
  name, are `@project.duplicateName` at the later name in the project
  file, a target's `name` value or a dependency's key, related to the
  earlier one.
- Section: [programs
  chapter](../../../spec/04-programs-and-packages.md#packages-units-and-targets),
  "Packages, units, and targets"; the registry and the discovery table of
  the diagnostics chapter.
- Code: `crates/vibra-workspace/src/snapshot.rs`, `validate_roots`, beside
  the `@project.reservedUnitName` check. Neither collision is reported
  today. The registry row `DiagnosticCode::ProjectDuplicateName` exists in
  `crates/vibra-diagnostics/src/registry.rs`.
- Cases: `V2-PROJECT-graph-duplicate-target`, two targets named `@app`,
  the diagnostic at the second `name` value with the first related;
  `V2-PROJECT-graph-duplicate-unit`, a target `@money` and a dependency
  `"money"`, the diagnostic at the dependency's key with the target's
  `name` value related. Both get rows in `conformance/rules.tsv` with the
  anchor `04-programs-and-packages.md#packages-units-and-targets`.

## 4. The summary of `@project.reservedUnitName`

- Rule: a target or dependency named `tests` or `std` is
  `@project.reservedUnitName`.
- Section: [programs
  chapter](../../../spec/04-programs-and-packages.md#discovery-and-the-source-snapshot),
  "Discovery and the source snapshot".
- Code: `crates/vibra-diagnostics/src/registry.rs`, the summary of
  `ProjectReservedUnitName` names `tests` only. The behaviour is right; the
  summary text must name both.
- Case: none; `V2-PROJECT-graph-reserved-std` covers the behaviour.
