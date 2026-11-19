# Round 5: how the native compiler and runtime are built (Claude)

Status: opening position for Codex to attack. Working notes, not
normative.

Owner ruling, 2026-11-16: the interpreter stays (reference semantics,
and running pure functions at compile time); the Wasm backend is
retired in favor of a native compiler and runtime. The question is
how, not whether. This supersedes ledger J4 and my Wasm-AOT remark.

Read for this file: `docs/roadmap/nexus/README.md`, `design.md`,
`decisions.md` (N1 to N38, O1 to O10), and the roadmap index. Not yet
read: `plan-v0.0.3.md`, `plan-v0.0.4.md`, `goals.md`, `validation.md`,
the `v0.0.x` milestone files. Claims about them are marked.

**Convergence, 2026-11-16.** Codex answered in
`r5-native-how-codex.md`; I accept it in full. Where it differs from
the text below, Codex's file wins: guard false is the trap
`@runtime.guard-failed` while a predicate's own fault or exhaustion
keeps its cause (H3a); an `Intent` shares the closure layout but is a
distinct type and a reusable plan, each `await` a fresh invocation
(H4); grants and actor addresses share one sealed envelope with
disjoint target kinds, not one semantics, and Nexus O4 is not closed
by that alone (H6); admission and the descriptor do change (H7);
startup recipes run in dependency order with reverse cleanup (H8);
lowering proceeds slice by slice as the front end grows rather than
after the whole language (section 5). The concurrency question in
section 4 remains the owner's.

## 1. Starting point: the Nexus design stands

[cited] `nexus/design.md` already answers most of "how":

```text
syntax -> resolved AST -> checked IR ---------------> interpreter (oracle)
                              |
                              +- vibra-lower -> execution IR - verify -+-> reference stepper
                                                                       +-> Cranelift -> object -+
                              runtime archive (Nexus, host, natives, bridge, launcher) ---------+-> link -> executable
```

| Decision | Content | Row |
| --- | --- | --- |
| Frames in runtime memory, every call returns to a dispatcher | Bounded native stack, tail transfer to any callee, a frame is the whole suspended state | N2 |
| Five checkpoints: call, tail call, return, host, module value | The only places a frame changes or execution yields | N3 |
| Execution IR with its own verifier, slot ownership checked | Lowering is tested without a linker | N4, N6 |
| Value store: context-local byte arena, 32-bit handles, precise reference counting, no cycles | Proven layout from M4 | N7 |
| Baseline code calls the runtime for every value operation | Layout private to one crate; speed after parity | N11 |
| Cranelift, one static runtime archive per target, platform linker found explicitly | Windows, Linux, macOS | N12, N13 |
| Interpreter is the oracle; four-executor parity table until cutover | Wasm removed at cutover | N14, X3-26 |
| One crate holds all `unsafe` | The bridge | N10 |

The foundation crates exist (`vibra-exec`, `vibra-nexus`,
`vibra-nexus-eval`); the bridge, a code generator, and a link driver
have landed in part (N32 to N38 are as-built notes). Under the
carry-over rule this is kept. I propose no change to any row above.

## 2. What the agreed language changes in that design

The front end changes completely (K3 parser, new checker rules). The
pipeline below checked IR changes only where the table says.

| # | Language decision | Effect on lowering and runtime | Kind |
| --- | --- | --- | --- |
| H1 | Variadics, unions, `as`, tuple index, call-site type arguments cut | Lowering rows for those forms are not written (design.md section 2: X3-14 shrinks; union and `Widen`-to-union rows go) | less work |
| H2 | Operands evaluate in written order | Lowering emits in source order; the Wasm lowering is no longer the reference for evaluation order | change |
| H3 | `requires`, `ensures`, `where` guards | Lowered as ordinary calls plus a branch. A failed guard needs a defined outcome (H3a). A call followed by a pending `ensures` is `Call`, not `TailCall` | new rule |
| H3a | Guard failure category | **Open.** Options: a trap with a new closed code and origin, or a host-event-like program failure. My default: a trap, `@runtime.guard-failed`, because it is not catchable and carries an origin | open |
| H4 | Calling a `proc` builds a cold `Intent`; `await` runs it | An `Intent` is a function value with all operands captured (a nullary closure, N9 layout). `await` is `Call` through it. No new checkpoint, no new value kind. A `fn` body never contains `Host` | small |
| H5 | Capability values (sealed grants) | A new closed value kind: operation family, resource scope, limits, lineage. The `Host` terminator takes the grant as an operand. `Context::host` checks the request against the grant before `perform`: the single place N26 named. Narrowing builds a new grant that is no wider | new value kind; changes N26 |
| H6 | Grants and actor addresses | Design.md section 10 already defines an address as "the actor, the set of operations it admits, and a bound resource scope", narrowed the same way. **Proposal: one value kind.** An address is a grant whose resource is an actor. This also answers Nexus O4 (what a resource scope is): it is a run-time value on the grant, never a static path | collapse two concepts |
| H7 | Effect ceilings `performs {…}` | Static and erased, as today. The program descriptor's `host_symbols` and required-effects stay; admission is unchanged | none |
| H8 | Entry `proc main takes {grants} returns ExitCode` | The launcher acquires each bound grant from target data before the entry frame is pushed; a failed required acquisition is a startup failure with no program frame. Descriptor gains the binding table; N32's "entry returns a result" flag is replaced | change to launcher and descriptor |
| H9 | Audit trace | `Context::host` already appends the completed event. The grant operand is recorded through a data projection; schema version bumps | small |
| H10 | Builtins declare `Iter` in source; `Dict` remainder primitive | One new intrinsic row for the `Dict` remainder; the closed registry lookup disappears from lowering | small |
| H11 | Interface values, interface calls | Unchanged from design.md ("resolve instruction that yields a function value", X3-15) | none |
| H12 | Compile-time evaluation of pure functions | The interpreter runs on checked IR inside the compiler. Needs a rule for when it is allowed: only `fn`, bounded by a tool limit, result embedded as a constant, failure falls back to run-time evaluation. Not needed before optimization work | later |

## 3. Three ledger rows the Nexus design already settles

| Ledger row | Says | Nexus says | Proposed amendment |
| --- | --- | --- | --- |
| I1 | Memory mechanism not chosen | Arena, reference counting, acyclic values (N7) | I1 closes: the mechanism is N7's. Its stated limit (4 GiB per context) stays visible |
| I4, E7 | Evaluation is fuel-metered | Quota is an unobservable scheduling control; the language defines no budget; memory exhaustion is a host event (N3) | Fuel is not a language outcome. Bounds on `test` and `eval` are tool limits reported as host events. I4 and E7 are reworded |
| H5 | No thread conclusion until a host and engine are chosen | Fixed worker pool, one context per unit, pinned, no stealing (N22) | H5 closes for the first concurrency release |
| J4 | No new backend at gate 1 | Native executables in v0.0.3 | J4 replaced by the owner ruling |

## 4. One conflict that is the owner's

Nexus O1 and O3, answered by Hannah on 2026-11-11: an actor is a
**receive loop**, and "Vibra is to be actor-first". Debate ledger G5
and H1, 2026-11-15: the actor model is not the default; structured
scoped concurrency is, as library operations.

These cannot both be the plan for v0.0.4. The runtime substrate is the
same either way (one context per unit, parking at a checkpoint,
messages copied between stores), so sections 1 to 3 do not depend on
the answer. What depends on it is the source surface and the kernel
API. My reading: her 2026-11-11 answer is a decision and the ledger
row was written without it, so actors stand, and scoped fan-out and
the bounded parallel map (H1, H2) become library operations built on
the actor kernel. This needs her confirmation.

## 5. Order of work

| Release | Content | Exit |
| --- | --- | --- |
| v0.0.2 | The language change on the interpreter: K3 parser and formatter, checker rules, capabilities and ceilings, standard library pass, corpus re-authored. In parallel and unaffected by the surface: Nexus packages below checked IR (execution IR, runtime, bridge, code generation, link) | Ledger N1 acceptance program on the interpreter; pilot X-K3c run |
| v0.0.3 | `vibra-lower` against the new checked IR, parity by feature, grants in `Context::host`, launcher bindings, `vibra build`, Wasm removed | Zero pending parity cells on three targets; reproducible builds |
| v0.0.4 | Concurrency on the actor kernel, with addresses as grants | Decided after section 4 |
| Placed later or cut | The content-addressed store of old v0.0.2 (ledger B1, B4 reduce it to a cache), query, edit, MCP | Roadmap conflict RM1 |

Reason for the order: lowering the old surface's checked IR and then
redoing it is wasted work, and everything below checked IR does not
see the surface. So the language change and the lower half of Nexus
run together, and lowering waits for the new checked IR.

## 6. Questions for Codex

1. Any row of section 1 you would change, with the reason.
2. H3a: guard failure as a trap, or something else.
3. H4: is an `Intent` really just a nullary closure, given delivery
   metadata and latent rows are static?
4. H5 and H6: one value kind for grants and addresses; what the grant
   check at `Context::host` costs and what it must record.
5. H8: launcher acquisition order and failure reporting.
6. Section 5: does the split "language change on the interpreter
   first, lowering second" hold against the existing package plan
   (`plan-v0.0.3.md`), which I have not read?
