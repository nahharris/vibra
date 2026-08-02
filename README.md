# Vibra

Vibra is a vibe-coding-first, statically typed functional language with an
S-expression surface that compiles to WebAssembly.

- **Language contract:** [S-expression language design](docs/decisions/s-expression-language.md)
- **Historical record:** [retired YAML draft](docs/archive/yaml-surface-draft.md)
- **Documentation index:** [docs/index.md](docs/index.md)
- **Project layout:** [docs/reference/project-layout.md](docs/reference/project-layout.md)
- **Schemas and tooling contracts:** [schemas/](schemas/)
- **Examples:** [examples/](examples/)
- **Agent skills:** [skills/](skills/) and [usage notes](docs/reference/agent-skills.md)
- [Container images](docs/reference/containers.md)

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
(import stream "../stdlib/src/stream.vibra")

(defn main () void (do (io.stdout.println "Hello, World!")) effects: (io.stdout stream.write))
```

Use `;` for reader comments. Persisted documentation belongs in a trailing
`doc:` attribute, not in comments.

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

## Effects

Native effects are nominal roots declared with `deffect`. An operation owns its
root implicitly; `effects:` lists only additive roots. Calls use the exact
declared union—there is no compound-root or dependency-closure mechanism.

```vibra
(deffect read
  (defn open (path path) (result reader fs-error)
    (intrinsic @fs-open-read path)
    effects: ()))
```

`read.open` is the operation name; a module-level `file` remains `file`, not
`read.file`. Host-backed endpoints are nominal `(newtype (handle @read))`
values minted only by validated intrinsics. They may widen to a weaker generic
stream capability, never be forged or cast, and retain their provider-specific
identity.

Ordinary functions and interface methods still declare a complete effect
ceiling. The compiler checks the body against it and reports both declared and
performed rows. `intrinsic` names are closed compiler-known operations: pure
value operations may be used directly, while effectful transitions are owned
by a `deffect`. Raw `wasm` is reserved for custom/dependency Wasm owned by an
effect operation.

```sh
vibra effects examples/fs-roundtrip.vibra
```

reports declared/performed surfaces, per-function and per-operation rows, call
edges, and primitive capability witnesses — see
[schemas/effects.schema.json](schemas/effects.schema.json).

Effects are static and fully erased: there is no runtime representation and no
enforcement. Every host operation remains unconditionally available at runtime,
so embedders must still sandbox untrusted Vibra source themselves.

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
