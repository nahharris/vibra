# Vibra

Vibra is a vibe-coding-first, statically typed functional language with an
S-expression surface that compiles to WebAssembly.

- **Language contract:** [S-expression language design](docs/superpowers/specs/2026-07-25-s-expression-language-design.md)
- **Historical record:** [DRAFT.md](DRAFT.md) documents the retired YAML surface.
- **Project layout:** [docs/project-layout.md](docs/project-layout.md)
- **Schemas and tooling contracts:** [schemas/](schemas/)
- **Examples:** [examples/](examples/)
- [Container images](docs/containers.md)

## Quick start

```sh
cargo run -- run examples/hello.vibra
cargo run -- check examples/hello.vibra
cargo run -- fmt
cargo run -- lint --deny-warnings
cargo run -- test
```

Vibra source uses UTF-8 S-expressions. A module contains imports and
definitions; calls are positional and labels configure enclosing forms.

```vibra
(import io "../stdlib/src/io.vibra")

(defn main () void (do (io.println "Hello, World!")))
```

Use `;` for reader comments. Persisted documentation belongs in a trailing
`doc:` attribute, not in comments.

### Vibra Machine emulator

`cargo run -- emu program.vmi` executes a newline-delimited 32-bit instruction
image. Blank lines and lines beginning with `#` are ignored; words may be
decimal or `0x` hexadecimal. Use `--trace --format json` for the v0.1 trace
contract. A `HALT` exits successfully; an architectural trap or step limit
prints its report and exits nonzero.

```vibra
(defn
  greet
  (name str)
  void
  (do (io.println name))
  doc: "Write a name followed by a newline."
)
```

## Atoms

An atom is a self-naming constant written `@name`, with lowercase kebab-case
dot segments (`@ok`, `@http.not-found`). Atoms compare by identity, match in
patterns, and key maps. Each atom also names a singleton type that widens to
`atom`.

```vibra
(defn classify (value atom) bool
  (do (match value @ok (do (return true)) _ (do (return false)))))

(defn always-ok () @ok (do (return @ok)))
```

Contextual keywords are atoms too: `visibility: @public`, `kind: @bin`,
`format: @json`, `profile: @core`, `tags: (@language)`, `workspace: @temp`,
`expect-error: (@compile E-OP-002 "overflow")`, and macro syntax categories
such as `@expr-syntax`. A bare symbol in those positions is rejected with
`E-ATOM-003`.

`convert.format-atom` renders an atom back to its written form, sigil
included.

## Projects

`project.vibra` is an S-expression manifest and source files use the `.vibra`
extension. Lockfiles and compiler-owned package metadata are canonical JSON.

```vibra
(project
  (package "hello" "0.1.0")
  (target hello kind: @bin root: "src" entry: "main.vibra")
  (dependency std path: "../stdlib"))
```

## Tests

Tests are first-class declarations. A bare `vibra test` selects the capability
free `core` profile.

```vibra
(import test "@std/test.vibra")

(test.scenario "truth"
  (test.case "is true"
    (test.assert true)
    tags: (@language)))
```

```sh
cargo run -- test
cargo run -- test --filter truth
cargo run -- test --profile core --tag language
cargo run -- test --deny-skips --deny-warnings
```

Profiles and tags select tests; they do not grant host permissions. See
[tests/README.md](tests/README.md) for capability-gated test requirements.

## Data interoperability

YAML is not Vibra syntax, a manifest format, compiler output, or an `embed`
format. Use `text`, `binary`, `json`, `toml`, or `xml` when embedding external
data. YAML paths and `format: yaml` are rejected with `E-SYN-008`.

## Build and validate

Run both suites before a commit:

```sh
cargo test
cargo run -- test
```

## License

MIT OR Apache-2.0 (see [Cargo.toml](Cargo.toml)).
