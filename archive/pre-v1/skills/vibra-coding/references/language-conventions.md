# Vibra language conventions

## Canonical references

- Language contract: [`../../../docs/decisions/s-expression-language.md`](../../../docs/decisions/s-expression-language.md)
- Effects contract: [`../../../docs/decisions/effect-system.md`](../../../docs/decisions/effect-system.md)
- Project layout and imports:
  [`../../../docs/reference/project-layout.md`](../../../docs/reference/project-layout.md)
- Source schemas: [`../../../schemas/`](../../../schemas/)
- Standard library: [`../../../stdlib/src/`](../../../stdlib/src/)
- Test conventions: [`../../../tests/README.md`](../../../tests/README.md)

## Modules and imports

A `.vib` file is a sequence of UTF-8 S-expression forms. Import a sibling
relative to the importing file:

```vibra
(import model "./model.vib")
```

Use a manifest target or dependency alias for cross-root imports:

```vibra
(import io "@std/io.vib")
(import core "@core/lib.vib")
```

Keep import aliases kebab-case and use the alias to qualify imported symbols.
Files under `dep/` are synced inputs; change the dependency source and run
`vibra sync` instead of editing vendored files.

Reader comments use `;`. Persisted documentation belongs in a trailing `doc:`
attribute, not in comments.

## Functions and documentation

Declare functions with explicit parameter and return types:

```vibra
(defn
  greet
  (name str)
  void
  (do (io.stdout.println name))
  effects: (io.stdout stream.write)
  doc: "Write a name followed by a newline."
)
```

Use a top-level `doc:` attribute for module and symbol documentation. Keep docs
factual and focused on the contract. Inspect them with `vibra docs`.

Functions that reach host imports must declare an `effects:` row; an absent row
means pure. See the effect-system contract for the complete rule.

## Tests

Put project tests under `tests/`. A test module imports `@std/test.vib` and
may contain multiple scenarios without a `main` function:

```vibra
(import test "@std/test.vib")

(test.scenario "greeting"
  (test.case "is stable"
    (test.assert true)
    tags: (@language)))
```

A bare `vibra test` selects capability-free `core` tests. Profiles and tags
select tests; they do not grant host access. Tests using `workspace: @temp` or
real host state belong in an explicitly non-core profile with the required
runner options.

Files named `foo.<flag>.vib` are conditional parts of `foo.vib` when the
base file exists. `vibra test` enables the `test` flag, so colocated tests may
live in `foo.test.vib`.
