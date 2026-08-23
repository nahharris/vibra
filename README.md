# Vibra

Vibra is being rebuilt as a vibe-coding-first programming language: a language
whose syntax, type system, effect system, tooling, and runtime are designed as
one product for code primarily authored and maintained by agents.

## Soft reboot

The active repository is specification-first. There is currently no supported
compiler, runtime, CLI, standard library, package, or release. The earlier
implementation was archived intact under [`archive/pre-v1/`](archive/pre-v1/)
because its features had different maturity levels and its implementation had
started to define the language accidentally.

The archive is evidence, not a compatibility target. The v1 implementation
will be new work against the specification and will not preserve pre-v1 source,
CLI, schema, ABI, or package compatibility.

## The v1 target

Vibra v1 is defined by five inseparable surfaces:

- canonical `.vib` source and non-executable `.vibon` data grammars over one
  lexical S-expression reader;
- a nominal static type system with explicit module boundaries;
- nominal effects checked statically against explicit project-target ceilings;
- versioned CLI and MCP contracts for projects, diagnostics, context, and
  transactional code changes; and
- a deterministic reference runtime with a closed host ABI and a WebAssembly
  production backend.

The complete source-of-truth map starts at [`docs/index.md`](docs/index.md).
The delivery sequence and measurable release gates are in
[`docs/roadmap/v1.md`](docs/roadmap/v1.md).

## Repository status

| Surface | Status |
| --- | --- |
| v1 specification | Normative target, open to deliberate revision |
| v1 roadmap | Defined |
| v1 implementation | Not started |
| pre-v1 implementation | Archived; unsupported |

No build or install instructions are published while the active tree has no
implementation. That is intentional: a command belongs here only after its
specified behavior and conformance tests exist.

## License

Vibra is licensed under the terms in [`LICENSE`](LICENSE).
