# Vibra language conventions

## Canonical references

- Language specification: [`../../../../DRAFT.md`](../../../../DRAFT.md)
- Project layout and imports:
  [`../../../../docs/project-layout.md`](../../../../docs/project-layout.md)
- Source schemas: [`../../../../schemas/`](../../../../schemas/)
- Standard library: [`../../../../stdlib/src/`](../../../../stdlib/src/)
- Test conventions: [`../../../../tests/README.md`](../../../../tests/README.md)

## Modules and imports

A `.vibra` file is a module. Import a sibling relative to the importing file:

```yaml
model:
  $import: ./model.vibra
```

Use a manifest target or dependency alias for cross-root imports:

```yaml
io:
  $import: "@std/io.vibra"
core:
  $import: "@core/lib.vibra"
```

Keep import aliases kebab-case and use the alias to qualify imported symbols.
Files under `dep/` are synced inputs; change the dependency source and run
`vibra sync` instead of editing vendored files.

## Functions and documentation

Declare compile-time documentation beside a symbol with `=doc`:

```yaml
greet:
  =doc: Return a greeting for the supplied name.
  $fn:
    name: $str
  $return: $str
  do:
    - $return: {$str.concat: ["Hello, ", $args.name]}
```

Use a top-level `=doc` for module documentation and `package.=doc` in
`project.vibra` for package documentation. Keep docs factual and focused on
the contract. Inspect them with `vibra docs`.

## Tests

Put project tests under `tests/`. A test module imports `@std/test.vibra` and
may contain multiple top-level tests without a `main` function:

```yaml
test:
  $import: "@std/test.vibra"

greeting-is-stable:
  $test: core
  do:
    - $test.assert-eq:
        actual: "Hello, Vibra"
        expected: "Hello, Vibra"
```

A bare `vibra test` selects capability-free `core` tests. Profiles and tags
select tests; they do not grant host capabilities. Capability tests must
declare policy and use matching `--allow-*` flags. Tests using
`workspace: temp` also require `--allow-test-workspace`.

Files named `foo.<flag>.vibra` are conditional parts of `foo.vibra`.
`vibra test` enables the `test` flag, so colocated unit tests may live in
`foo.test.vibra`.
