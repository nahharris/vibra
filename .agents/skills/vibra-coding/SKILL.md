---
name: vibra-coding
description: Write, review, and refactor statically typed Vibra source, modules, tests, and documentation. Use for `.vibra` files, Vibra language features, imports, `$test` cases, `=doc` annotations, or idiomatic Vibra API design.
---

# Vibra coding

Write Vibra as typed YAML, not as free-form YAML or a translation of another
language.

## Workflow

1. Read `project.vibra` and identify the target root before editing.
2. Read [references/language-conventions.md](references/language-conventions.md)
   for source, module, test, and documentation conventions.
3. Prefer existing standard-library modules and nearby source patterns.
4. Keep symbols and test names kebab-case.
5. Use relative imports within a target and `@name/path` imports across target
   or dependency namespaces.
6. Add focused `$test` cases for behavior changes.
7. Run `vibra fmt <paths> --write`, `vibra lint <paths> --deny-warnings`, and
   the narrowest useful `vibra test` invocation. Finish with the full project
   validation described by `$vibra-cli`.

## Guardrails

- Treat `project.vibra` as metadata, not a source module.
- Use structural `=comment`, `=doc`, and `=lint` annotations. Never add YAML
  `#` comments.
- Do not edit `dep/`; it is vendored, read-only dependency source.
- Do not add host access implicitly. Capability-gated code and tests must
  declare narrow policy and use matching explicit CLI approvals.
- Do not invent syntax. Confirm uncertain forms in `DRAFT.md`, `schemas/`, the
  standard library, or compiler tests.
