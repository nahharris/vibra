# 08 Tooling

Status: draft specification; not normative

The toolchain is how an agent meets the language. The evidence behind
this chapter's choices is in the [tooling note](../tooling.md).

## Rules for every tool

1. **Facts are pushed.** The result of an action carries the facts needed
   for the next one. A tool MUST NOT require a second query to learn what
   a failed action broke or what would fix it.
2. **Few verbs.** The commands below are the whole surface.
3. **Text in, text out.** Input is line-surface text. Output is text an
   agent can read, with a `--vibon` form for programs. Source is never
   passed inside a JSON string.
4. **Bounded output.** Every listing has a limit and states what was
   omitted.
5. **Command line first.** Every capability exists as a command. Other
   interfaces are adapters over the same engine and MUST NOT add
   capabilities.
6. **Deterministic.** The same command at the same revision gives the
   same output.

## Workspace

```text
vibra.vibon          package manifest: name, dependencies by revision
src/                 text projection, one .vib file per module
.vibra/store/        objects, revisions, evidence
.vibra/stage/        the staged overlay of this workspace
journals/            journals of local runs
```

`src/` is written by the tool and edited by the author. It can be
searched with ordinary text tools. It is never the authority: a file that
differs from the store is a pending change until staged.

## Commands

| Command | Does |
| --- | --- |
| `vibra show <name>` | Prints a cell's claim and effective grades. Options add the witness, evidence, debts, dependents, or tree |
| `vibra find <query>` | Finds cells by result or parameter type, by meaning, or by text |
| `vibra eval <leaf>` | Evaluates a leaf on the reference evaluator |
| `vibra stage [paths]` | Reads changed declarations into the staged overlay and checks them |
| `vibra status` | Prints the worklist |
| `vibra hole <id>` | Prints a hole's context and moves |
| `vibra check [names]` | Runs checkers to obtain or raise evidence |
| `vibra publish` | Admits the staged overlay as a new revision |
| `vibra discard [names]` | Drops staged changes |
| `vibra decline <name>` | Records that a claim could not be met |
| `vibra accept <debt>` | Accepts a debt. Intent authority only |
| `vibra run <target>` | Runs a target on the reference runtime |
| `vibra replay <journal>` | Replays, in a mode of [06](06-runtime.md#replay) |
| `vibra simulate <target>` | Runs seeded simulations |
| `vibra explore <target>` | Explores reachable states |
| `vibra ask <journal> <question>` | Answers a question of a journal |
| `vibra build <target>` | Checks policy and produces an artifact and manifest |
| `vibra sync` | Rewrites `src/` from the current revision and overlay |

## Staging

All change goes through a staged overlay on a pinned revision.

- `stage` parses each changed declaration, replaces the staged cell, and
  checks it. A declaration is the unit of change. There is no patch
  language and no edit below a declaration.
- A staged overlay MAY be invalid: cells with holes, failed obligations,
  and dependents broken by a claim change are all allowed while staged.
- `publish` is atomic. It applies the admission rules of
  [01](01-programs.md#admission) to the whole overlay and either creates
  one revision or changes nothing.
- `publish` MUST fail if the pinned revision is no longer current and a
  staged cell or one of its dependencies changed underneath it. The
  overlay is kept and can be re-pinned.
- A transaction that needs a debt, or that changes a claim the principal
  may not change, is held as pending for an intent authority.

### The worklist

After every `stage`, and on `status`, the tool prints what remains before
`publish` can succeed.

```text
$ vibra stage src/bank/money.vib
staged   split-cents        claim unchanged, witness replaced
check    6 of 7 obligations proved
  ensures-4   (list.descending shares)   counterexample
    input   total: (cents.of 100), parts: (part-count.of 3)
    result  (33 33 34)
    wanted  first element is the largest
worklist 1 item
  split-cents   1 obligation failed
dependents 2, all unaffected: claim unchanged
```

- The worklist MUST list every staged cell that is not admissible and
  every dependent invalidated by a staged claim change, each with its
  reason.
- It is computed, never remembered. An agent does not track its own
  progress across a multi-cell change. It reads the worklist.

## Holes and moves

A hole `?` or `?name` may stand for a leaf, a type, or a block.

`vibra hole` prints:

- the expected type;
- the facts in scope: bindings with their types, known `where` clauses,
  the current `case`, the `requires` in force;
- the claim the enclosing cell must meet;
- the **moves** available.

| Move | Effect |
| --- | --- |
| Fill | Replace the hole by a leaf of the expected type |
| Call | Apply a named cell, leaving a hole for each argument |
| Construct | Build the expected type, leaving a hole for each field |
| Split | Replace a block hole by a `match` on a name, with a hole per case |
| Branch | Replace a block hole by `if`, with holes for the condition and blocks |
| Bind | Introduce a `let` before the hole |

- Moves are written as line-surface text for the enclosing declaration
  and staged like any change. A move is not a separate edit format.
- The list of fills offered MUST contain only well-typed candidates. With
  a decoder constrained to that list, an ill-typed program cannot be
  produced. Without one, it is produced and rejected.
- Cells with holes evaluate. A result that needed a hole is reported as
  waiting on it.

## Queries

Queries exist for facts that a text search cannot compute.

| Question | Command | Answer |
| --- | --- | --- |
| What is this? | `show` | Claim, one example, effective grades, debts |
| Does something like this exist? | `find` | Cells whose type or meaning matches |
| What fits here? | `hole` | Expected type and candidates |
| What breaks if this claim changes? | `show --dependents` | Direct dependents and which of their obligations use the claim |
| Why does this use a port, carry a label, or have this grade? | `show --why` | The chain of cells responsible |
| What is unproven below this? | `show --debts` | Debts and obligations below a grade, with their cells |

`find` MUST be used by `stage` on every new claim: a new cell whose
meaning matches an existing one is reported with that cell
([01](01-programs.md#admission)).

## Evaluation

`eval` runs on the reference evaluator against the pinned revision and
the staged overlay.

- Pure leaves need no permission and no sandbox: they cannot have
  effects, they terminate, and their cost is bounded.
- A procedure or machine is evaluated under port models, never under
  drivers, unless run through a target.

| Capability | Form |
| --- | --- |
| Evaluate | `vibra eval "(split-cents (cents.of 100) (part-count.of 3))"` |
| Promote a result to an example | `--keep`, which stages an `example` clause for an intent authority |
| Evaluate with holes | Automatic. Holes reached are listed |
| Compare candidates | `--try` with several leaves. Each result is printed |
| Find an input | `--find "(not (list.descending (split-cents total parts)))"` |
| Tell two cells apart | `--differ a b`, which searches for an input where they differ |

Results are cached by hash. An evaluation is evidence at grade `tested`
for the facts it shows.

## Diagnostics

A diagnostic is a record with a stable code, and is printed as text.

```text
error obligation.failed
  cell      bank.money.split-cents
  at        body: return (list.build parts.value share-at)
  goal      ensures-4 (list.is-descending (list.map shares cents.value))
  input     total: (cents.of 100), parts: (part-count.of 3)
  result    [(cents.of 33), (cents.of 33), (cents.of 34)]
  facts     base is 33; extra is 1; share-at gives 34 only at index 2
  fix       the larger shares must come first
```

Every diagnostic MUST carry:

- a code from the published catalogue, the cell, and a position in the
  line surface;
- what was expected and what was found;
- for a type error, the expected type and the in-scope values that have
  it;
- for a failed obligation, the goal and a confirmed counterexample, or
  "unknown" with the reason;
- for a rejected claim, the strength check that failed and its witness
  values;
- for an unknown name, the nearest existing names and their claims;
- where the fix is mechanical, the corrected line.

A diagnostic MUST be complete in itself. It MUST NOT refer the reader to
documentation for information the tool has.

## Adapters

- An MCP server MAY expose the commands above, one tool per command, with
  the same text results. It MUST NOT add tools.
- An editor integration MAY present claims, grades, debts, worklists, and
  journals to people. People review claims and manifests. They are not
  expected to read witnesses.
- The agent skill for the language is generated from the published
  grammar, the diagnostic catalogue, and the standard library's claims,
  and is versioned with the toolchain.

## Conformance

Two suites decide whether a toolchain conforms.

| Suite | Written in | Tests |
| --- | --- | --- |
| Implementation tests | The host language | The toolchain's components |
| Conformance corpus | Vibra | The language: cells that must be admitted, cells that must be rejected with a given code, evaluations, journals that must replay |

- Every MUST in this specification has a corpus case or is listed as a
  review-only invariant.
- The corpus runs on the reference evaluator and on every backend. A
  backend that differs from the evaluator on any case does not conform.
- The corpus is itself a store. Its cells carry evidence like any other.
