# v0.0.2 M1: pure language

**Planned.** The pure part of the language, checked and run on the reference
interpreter.

Design record: [architecture.md](architecture.md). Owner table for M0:
[../m0/owners.md](../m0/owners.md). Step guides: [steps/](steps/).
Integration branch: `v0.0.2/m1`, created from `main` after M0 exits.

## Scope, in slices

1. **Reader and formatter.** Lexical structure, the full grammar, recovery,
   the lossless tree, and the canonical format, with golden trees for every
   production and an idempotence corpus.
2. **Names and modules.** Imports with derived names, the flat module scope,
   paths against field reads, acyclic imports, case rules for module paths.
3. **Types and construction.** Scalars, structural tuples and records,
   declared wrappers, tuples, records, and variant types, representation
   visibility, generics with `given`, recursive types with the finite-size
   check.
4. **Bodies.** `let` with patterns, optional type, and `else`; `if`,
   `match`, and `do` as statements and expressions; explicit `return`;
   `try` with its expected type; written evaluation order; coverage.
5. **Callables and interfaces.** Named and anonymous `fn`, labelled
   defaults, interfaces, nested `impl`, dispatch, interface values, the two
   widenings.
6. **Standard library.** The pure modules: `Iter` with `fold` and the
   adapters, `Equatable`, `Ordered` with its defaults, `From` and `TryFrom`,
   saturating and checked arithmetic with no trapping operation, nested
   `Iter` implementations for the builtin collections.
7. **Guards.** The certified fragment; bare `requires`, `ensures`, and
   `where` discharged by the seven rules or rejected; the `else` forms;
   fallible construction; conversion of guarded callables.
8. **Module values and tests.** Lazy, acyclic module values; `test`
   declarations without grants; assertions.
9. **Compile-time evaluation.** The interpreter evaluates constant
   predicates and constant expressions for the checker, under tool limits
   that report "not established", never "true".

Each slice lands with its diagnostics, schemas, conformance cases, and
formatter support.

## Fixed design decisions

The decisions and their reasons are in [architecture.md](architecture.md).
In one line each:

- New crates under the final names beside the old crates, which step 01
  renames with a `-v1` suffix and step 28 deletes.
- The checked IR is closed in step 12 for the whole of M1, with a migration
  table for the native track; package X2-01 moves the native track to it.
- The packages N1 to N13 of the native contract are each assigned once:
  N1, N2, N3, N5, N6, and N13 are absorbed by steps 02, 12, 13, 15, 19,
  21, and 27; N4 and N7 to N12 are M2 packages on the new crates. The
  [architecture](architecture.md#native-contract-packages) has the table.
- The checker calls the interpreter for compile-time evaluation; that arrow
  is part of language semantics.
- The Wasm backend is ported fail-closed to the new IR in step 27 and runs
  the new corpus's pure executable cases it supports; its column is not an
  M1 gate.
- The three host tests ignored when the specification merged are settled
  by steps 02 (the registry test, restored), 08 (the example inventory,
  deleted and replaced by a test that reads every fenced block of the
  specification), and 28 (the native parity inventory, deleted).
- The corpus is re-authored under `V2-` identities at
  `conformance/cases/<PREFIX>/<id>/`, from the specification, with no
  snapshot-writing mode in the runner.
- `stdlib/` is rewritten; the roles land in step 14 before the first
  checker step.

## Step table

Status values: `planned`, `in progress`, `landed`. Kind: `vertical` unless
marked `infrastructure`, `specification prerequisite`, or `evidence`, which
the [execution model](../../execution.md#decomposition-rule) exempts from
the vertical-slice rule.

| Id | Name | Kind | Depends on | Can run in parallel with | Crates touched | Status |
| --- | --- | --- | --- | --- | --- | --- |
| 01 | [rename-v1](steps/01-rename-v1.md) | infrastructure | — | — | every existing crate, `Cargo.toml`, CI, `stdlib-v1/`, `examples-v1/`, `fuzz-v1/`, `conformance/v1/` | planned |
| 02 | [diagnostics-and-harness](steps/02-diagnostics-and-harness.md) | infrastructure, specification prerequisite | 01 | — | `vibra-diagnostics`, `vibra-conformance` (new) | landed |
| 03 | [lexer-and-vibon](steps/03-lexer-and-vibon.md) | vertical | 02 | 12, 13, 24 | `vibra-syntax`, `vibra-fmt` (new), `vibra-conformance`, `conformance/cases`, CI | landed |
| 04 | [source-skeleton](steps/04-source-skeleton.md) | vertical | 03 | 09, 12, 13, 24, 27 | `vibra-syntax`, `vibra-fmt`, `conformance/cases` | planned |
| 05 | [type-declarations](steps/05-type-declarations.md) | vertical | 04 | 09, 12, 13, 24, 27 | `vibra-syntax`, `vibra-fmt`, `conformance/cases` | planned |
| 06 | [expressions-and-patterns](steps/06-expressions-and-patterns.md) | vertical | 05 | 09, 12, 13, 24, 27 | `vibra-syntax`, `vibra-fmt`, `conformance/cases` | planned |
| 07 | [callables-and-tests-syntax](steps/07-callables-and-tests-syntax.md) | vertical | 06 | 09, 12, 13, 24, 27 | `vibra-syntax`, `vibra-fmt`, `conformance/cases` | planned |
| 08 | [reader-exit](steps/08-reader-exit.md) | evidence | 07 | 09, 10, 11, 12, 13, 14, 24, 27 | `vibra-conformance`, `vibra-conformance-v1` (the ignored inventory test deleted), `fuzz/`, `conformance/cases` | planned |
| 09 | [project-and-source-graph](steps/09-project-and-source-graph.md) | vertical | 03 | 04, 05, 06, 07, 08, 12, 13, 14, 24, 27 | `vibra-workspace` (new), `vibra-conformance`, `conformance/cases` | planned |
| 10 | [resolve-declarations](steps/10-resolve-declarations.md) | vertical | 07, 09 | 08, 12, 13, 14, 24, 27 | `vibra-resolve` (new), `vibra-workspace`, `vibra-conformance`, `conformance/cases` | planned |
| 11 | [resolve-bodies](steps/11-resolve-bodies.md) | vertical | 10 | 08, 12, 13, 14, 24, 27 | `vibra-resolve`, `conformance/cases` | planned |
| 12 | [checked-ir](steps/12-checked-ir.md) | infrastructure | 02 | 03 to 11, 24 | `vibra-ir` (new) | planned |
| 13 | [interpreter](steps/13-interpreter.md) | infrastructure | 12 | 03 to 11, 14, 24 | `vibra-interp` (new) | planned |
| 14 | [stdlib-bootstrap](steps/14-stdlib-bootstrap.md) | infrastructure | 07, 12 | 08, 09, 10, 11, 13, 24, 27 | `stdlib/` (new), `vibra-types` (new) | planned |
| 15 | [check-core](steps/15-check-core.md) | vertical | 11, 13, 14 | 08, 24, 27 | `vibra-types`, `vibra-interp`, `vibra-workspace`, `vibra-conformance`, `stdlib/`, `conformance/cases` | planned |
| 16 | [check-data](steps/16-check-data.md) | vertical | 15 | 08, 24, 27 | `vibra-types`, `vibra-interp`, `stdlib/`, `conformance/cases` | planned |
| 17 | [check-bodies](steps/17-check-bodies.md) | vertical | 16 | 08, 24, 27 | `vibra-types`, `vibra-interp`, `conformance/cases` | planned |
| 18 | [check-callables](steps/18-check-callables.md) | vertical | 17 | 08, 23, 24, 25, 27 | `vibra-types`, `vibra-interp`, `conformance/cases` | planned |
| 19 | [check-interfaces](steps/19-check-interfaces.md) | vertical | 18 | 08, 23, 24, 25, 26, 27 | `vibra-types`, `vibra-interp`, `stdlib/`, `conformance/cases` | planned |
| 20 | [stdlib-pure](steps/20-stdlib-pure.md) | vertical | 19 | 08, 23, 24, 25, 26, 27 | `stdlib/`, `vibra-types`, `vibra-interp`, `conformance/cases` | planned |
| 21 | [guards-with-else](steps/21-guards-with-else.md) | vertical | 20 | 08, 23, 24, 25, 26, 27 | `vibra-types`, `vibra-interp`, `conformance/cases` | planned |
| 22 | [discharge](steps/22-discharge.md) | vertical | 21 | 08, 23, 24, 25, 26, 27 | `vibra-types`, `conformance/cases` | planned |
| 23 | [tests-and-assertions](steps/23-tests-and-assertions.md) | vertical | 17 | 18 to 22, 24, 25, 27 | `vibra-workspace`, `vibra-types`, `vibra-interp`, `vibra-conformance`, `stdlib/`, `conformance/cases` | planned |
| 24 | [cli-skeleton](steps/24-cli-skeleton.md) | vertical | 02 | 03 to 23, 27 | `vibra-schema`, `vibra-cli` (new) | landed |
| 25 | [cli-fmt-check](steps/25-cli-fmt-check.md) | vertical | 08, 15, 24 | 16 to 23, 27 | `vibra-cli`, `vibra-schema`, `vibra-workspace`, `vibra-conformance`, `conformance/cases` | planned |
| 26 | [cli-test](steps/26-cli-test.md) | vertical | 23, 25 | 18 to 22, 27 | `vibra-cli`, `vibra-schema` | planned |
| 27 | [runtime-crate-port](steps/27-runtime-crate-port.md) | infrastructure | 03, 13 | 04 to 11, 14 to 26 | `vibra-host`, `vibra-native` (new), `vibra-wasm`, `vibra-wasm-run`, `vibra-ir`, `vibra-conformance`, `vibra-conformance-v1` (its Wasm column removed) | planned |
| 28 | [cutover](steps/28-cutover.md) | infrastructure | 08, 20, 22, 23, 26, 27; X2-01 (Nexus plan) | — | every `-v1` crate (deleted), `Cargo.toml`, CI, `README.md`, `examples/`, `docs/roadmap` | planned |
| 29 | [m1-exit](steps/29-m1-exit.md) | evidence | 28 | — | `examples/`, `conformance/cases`, this directory | planned |

After 01 and 02 land, steps 03, 12, and 24 are in flight at once; after 03,
steps 04, 09, 12, 13, and 24 are. The widest front is during the checker
chain, when 08, 23, 24 to 27 run beside it.

## Slices and steps

| Slice | Steps that implement it | Step | Slices it serves |
| --- | --- | --- | --- |
| 1 Reader and formatter | 03, 04, 05, 06, 07, 08 | 01, 02 | all (infrastructure) |
| 2 Names and modules | 09, 10, 11 | 03 to 08 | 1 |
| 3 Types and construction | 15, 16 | 09, 10, 11 | 2 |
| 4 Bodies | 15, 17 | 12, 13 | 3 to 9 (infrastructure) |
| 5 Callables and interfaces | 18, 19 | 14 | 6 (and the roles 3, 4, 7 need) |
| 6 Standard library | 14, 16, 19, 20 | 15 | 3, 4, 8 |
| 7 Guards | 21, 22 | 16 | 3, 6 |
| 8 Module values and tests | 15, 23 | 17 | 4 |
| 9 Compile-time evaluation | 21, 22 | 18, 19 | 5, 6 |
| | | 20 | 6 |
| | | 21, 22 | 7, 9 |
| | | 23 | 8 |
| | | 24, 25, 26 | tool surface of 1, 3 to 8 |
| | | 27, 28 | transition (infrastructure) |
| | | 29 | exit evidence |

Every slice maps to at least one step and every step maps to a slice or to
the transition.

## Exit evidence

- Positive, negative, recovery, and format cases for every form of the
  slices above, in both suites.
- Contract cases: discharged, refuted, and unproved obligations with their
  diagnostics; guard consequence values; a handled constant
  construction error; no run-time check emitted for a bare guard.
- Deep tail and non-tail recursion; invariant checks on every ingress path.
- A list of every gate-program construction the contract rules rejected,
  with the repair used.
- The reference solutions and controls of the model-writing pilot's pure tasks
  pass.

| Exit clause | Steps that produce it | Recorded by |
| --- | --- | --- |
| cases for every form, both suites | 03 to 07, 09 to 11, 15 to 23 | 29 |
| contract cases | 21, 22 | 29 |
| deep recursion, ingress invariants | 13, 15, 21 | 29 |
| gate-program rejections and repairs | 29 | 29 |
| pilot pure tasks | 29 | 29 |

## Plan validation

Recorded when this plan was written, by the scripts named in each line:

- Every EBNF production of the source chapter appears in
  [../m0/owners.md](../m0/owners.md): `check-owners.ps1`, empty diff.
- Every diagnostic code cited in this directory and in the owner table is a
  row of the registry table of the diagnostics chapter: `check-codes.ps1`,
  no unknown code.
- The step graph is acyclic and every `-p` package in a validation command is
  known to `cargo metadata` or marked proposed: `check-graph.ps1`.

The scripts and their output are in the planning report, not in the
repository.

## Not in this milestone

`proc`, plans, `await`, grants, host operations, and the entry contract.
