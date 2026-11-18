# Effective design

Status: draft by Claude for Codex's review, 2026-11-16. Working notes,
not normative. One list of what is in force after every amendment, so
the rewrite has a single source.

How to read it:

- This file states decisions. Reasons, rejected options, and reopening
  triggers stay in `ledger.md`.
- Where a rule needs exact wording, the named file is authoritative
  and this file only points to it.
- Per-construct mapping from current Vibra is `k3-carryover.md`
  (741 inventory ids, all mapped).
- Basis marks: **O** owner ruling, **A** agreed between Claude and
  Codex by argument, **M** measured in a pilot. Nothing beyond the X1b
  card subset is measured.

Pointer files, abbreviated below:

| Short | File |
| --- | --- |
| OT | `r4-open-topics-codex.md` |
| EC | `r4-effect-ceiling-form-codex.md` |
| RO | `r4-required-optional-effects-codex.md` |
| SL | `r4-stdlib-pure-codex.md` |
| NH | `r5-native-how-codex.md` |
| LR | `r5-carryover-ledger-review-codex.md` |
| RM | `r5-roadmap-codex.md` |
| SP | `r5-scope-policy-codex.md` |
| GS | `r5-guards-static-codex.md` |

## 1. Standing rules

| # | Rule | Basis |
| --- | --- | --- |
| 1.1 | Vibra is built; product order is command-line programs, libraries, services | O (A1, A2) |
| 1.2 | Every construct of current Vibra is kept, changed, deferred, or dropped explicitly. Nothing disappears by omission | O (A-10) |
| 1.3 | Small surface: collapse close concepts, reuse shapes, familiar terms; minimize distinct rules and illegal combinations, and state every necessary exception explicitly | O |
| 1.4 | No import cycles, Go style (owner's focus), extended to every prerequisite graph: packages, module-value initializers, startup recipes, layouts, build inputs. Symbolic references are not prerequisites: recursion among functions of one module and recursive nominal types with the finite-size check stay (owner confirmed). Run-time wait-for cycles are a separate question | O (A-25); GS section 7 |
| 1.5 | Prefer a compile-time check or an error as a value over a trap. Target gate contract, not yet a property of the tree: no trap triggered by user data in supported checked code | O (A-26) |
| 1.6 | A library algorithm that does not depend on the concrete type is written once, on an interface or as a generic function | O (A-11) |
| 1.7 | The interpreter stays permanently. A native compiler and runtime are built. Wasm is removed at native cutover | O (A-16) |
| 1.8 | Unshipped v0.0.2+ plans are rewritten in place, with no archive and no parallel authoritative plan. Shipped v0.0.1 evidence and reusable implementation (native scaffolding, tests) are kept | O |
| 1.9 | Claims about what models find easy are dated hypotheses per model population | A (A4) |

## 2. Surface

| # | Decision | Basis |
| --- | --- | --- |
| 2.1 | K3: opening keyword, word-led sections with no colon, typed closer (`fn … endfn`). Layout insignificant. No commas. `//` comments | O, M for the subset (C1, X1b-8) |
| 2.2 | A colon only joins a pair: `name: Type`, `name: value`, `Pattern: Type` in `takes`, `keyExpr: value` in a dict | O, A; M only for the X1b subset |
| 2.3 | Parentheses only apply: `f(a b)`, `Cents(5i64)`, `List(T)`. No grouping parentheses; any expression may be a callee | A (A-1) |
| 2.4 | Braces: tuple `{a b}` and record `{name: n}` as types, values, patterns; `takes {…}`; effect rows. Brackets: list `[a b]`, dict `[k: v]`, empty dict `[:]` | O (A-1) |
| 2.5 | Identifiers `[A-Za-z][A-Za-z0-9]*`; types PascalCase, values camelCase, a convention `check` reports | O (A-2) |
| 2.6 | Atoms kept (`@public`, `@std.io`); components may keep hyphens; wire tags are not renamed | O; OT E1 |
| 2.7 | A dotted name is a path or a record field read, by resolution | A (A-2) |
| 2.8 | Literals: strings as today plus a triple-quoted form; `'c'` characters; numbers as today with sized suffixes; `true`, `false` prelude values; `void`; discard `_` | A (OT item 9) |
| 2.9 | No infix operators; no `=` anywhere | A, O |

## 3. Declarations

| # | Decision | Basis |
| --- | --- | --- |
| 3.1 | Top level: `import`, `type`, `interface`, `fn`, `proc`, `let`, `test`. `impl` only nested. No module declaration | A (A-3) |
| 3.2 | `import @std.list.List`, optional `as alias`; default name is the last segment; collision is an error | O |
| 3.3 | `type Name of TypeExpr … endtype`: wrapper, tuple, record; variants by `case name {fields}` sections. All nominal. Type parameters in the header: `type Pair(A B)` | O (A-4) |
| 3.4 | Identity is a hash of the logical anchor (package, module path, owner, kind, name, scheme version); rename or move changes it | A (A-15) |
| 3.5 | `visibility @public`, private by default; `representation @private` hides constructor, pattern, raw projection | A (A-4) |
| 3.6 | `doc` holds one triple-quoted Markdown string. No example form | O (A-6) |
| 3.7 | Module value: `let name: Type be expr endlet`, pure, typed, lazily initialized once per run | O; OT E5 |
| 3.8 | `test "name" … endtest` kept with its current contract | O |
| 3.9 | Sections parse in any unambiguous order and format canonically | A |
| 3.10 | Standard-library-only: `role`, `native`, `external`, `symbol`, `Intrinsic(@i32)` | A (A-13) |

## 4. Types

| # | Decision | Basis |
| --- | --- | --- |
| 4.1 | `Bool Void Never Char Str Bytes Atom`, `I8…I64`, `U8…U64`, `F32`, `F64`. No plain `Int` | O |
| 4.2 | Structural tuples and records; declared wrappers, tuples, records, variants | O |
| 4.3 | `Fn({A B} R)`, `Proc({A} R {Row})`, `Intent(R {Row})`. Input vector always braced; labeled entries allowed in it; rows exact | O; EC section 1 |
| 4.4 | Interfaces, interface values by explicit widening, `Any`, `Self`, one nominal bound per parameter as `given {T: Bound}` | O, A |
| 4.5 | No call-site type arguments; an undetermined call is rejected | A |
| 4.6 | Deferred: untagged unions, `as` narrowing, anonymous enums, tuple index, variadics, call-site type arguments, selective brace imports | A (A-5, A-14) |
| 4.7 | Widening: concrete to interface, atom singleton to `Atom`, once, at a written expected type (parameter, result, typed `let`, field, payload) | A |

## 5. Expressions, patterns, control

| # | Decision | Basis |
| --- | --- | --- |
| 5.1 | `let pattern [: Type] be expr [else statements] endlet`; one pattern; bindings live after `endlet`; `else` only for a refutable pattern and must leave | O (A-5); OT section B |
| 5.2 | Patterns in `let`, parameters, `match`: names, `_`, literals, constants, constructors, tuples, records, exact-length lists. Bare name resolving to a constant module value is a constant pattern, otherwise a binder | O; OT B1 to B8 |
| 5.3 | Constructor labels exactly when the shape has field names | A |
| 5.4 | `if c then … else … endif`, `match v case P … endmatch`: statements, and expressions when every arm ends in a final expression | M for statements; A (A-12) |
| 5.5 | `do statements finalExpression enddo` expression block | A |
| 5.6 | `return` on every normally returning path; `try` on `Option` and `Result`, passing its expected type inward | A; SL S5 |
| 5.7 | Callee once, then operands in written order | A |
| 5.8 | Lookup by application `items(0u64)`, `table(key)`, returning `Option` | A |
| 5.9 | No loops, no assignment; recursion and `Iter` | A (C9) |
| 5.10 | An ignored fallible value is an error | A |

## 6. Callables and contracts

| # | Decision | Basis |
| --- | --- | --- |
| 6.1 | `fn name takes {Pattern: Type …} labelled {name: Type be constant} returns r: Type … do … endfn`; anonymous `fn` and `proc` | O, A |
| 6.2 | The result name exists only in an `ensures` predicate and its `else` consequence | O |
| 6.3 | Bare `requires`, `ensures`, `where`: static obligations, discharged by the fixed rule set or rejected; never moved to run time. A bare `where` keeps the constructor returning T | O direction; GS sections 3, 5 |
| 6.4 | `requires C else V`: false returns V and skips the body and its `ensures`. `ensures C else V`: C is checked on the body's candidate result; false returns V, evaluated once with that candidate, not revalidated, so C is not promised of the final value. `where P else E`: constructor returns `Result(T ErrorType)`, E a constant author value, no implicit standard error. No trap | O idea; GS section 5a |
| 6.5 | `where` only on wrapper, tuple, record representations; predicate is a certified pure, fault-free `Fn({Representation} Bool)`; unwrapping a valid T is infallible. A proven or folded check never changes a type | A (A-14); GS section 2 |
| 6.6 | A callable converts to a plain function type only if no bare `requires` remains unestablished by its input types. The converted value keeps its `else` entry and exit checks | A; GS section 4 |
| 6.7 | Tail calls reuse the activation when no work remains after the call | A; NH |

## 7. Effects and authority

| # | Decision | Basis |
| --- | --- | --- |
| 7.1 | Two mechanisms together: closed ceilings and capability values | O (A-7) |
| 7.2 | `performs {DirWrite}` on `proc`, anonymous `proc`, `test`, interface `proc` members; the ceiling is never inferred, the body is checked against it; omitted is empty; entries are operation interfaces. An interface `proc` member carries its owning operation plus its written additive row | O; EC |
| 7.3 | An operation is a small nominal interface with `proc` members; the 17 current host operations seed them | A; OT E3 |
| 7.4 | Calling a `proc` builds a reusable cold `Intent`; `await` runs it, only inside a `proc` or a `test` (tests run in a runner-owned proc context under fakes); each `await` is a fresh invocation | A (A-20); NH |
| 7.5 | A grant is a sealed value. Required: plain parameter. Optional: `Option(Cap)`. Refusal of a request: typed `Result` | O direction; RO |
| 7.6 | Narrow operations by taking a leaf from a record of grants; narrow the resource by a pure `restrict` or `sub` that keeps the parent as a conjunction | A; SP |
| 7.7 | Scope policy is typed library data kept in target binding recipes: file rules (root object, prefix, depth, actions, basename test), HTTP rules (origin, peers, methods, target), shared limits | A (A-24); SP |
| 7.8 | A brand such as `type LogDir of DirWrite` with private representation certifies the issuer | A; EC section 6 |
| 7.9 | Entry: `proc main takes {grants} returns code: ExitCode`; recipes acquired in dependency order before `main`; build fails on a missing required binding | A (A-8); NH |
| 7.10 | Delivery atoms `@once`, `@repeatable`, `@unknown`; "not observed" is not "did not happen" | A (F8) |
| 7.11 | Not provided: static identity of a resource instance, row polymorphism, subset subtyping of capabilities, revocation, information-flow guarantees | A |

## 8. Standard library (pure part)

| # | Decision | Basis |
| --- | --- | --- |
| 8.1 | `Iter`: `next`, `map filter skip take collect fold`, four adapter types; `List`, `Dict`, `Str`, `Option`, `Bytes` declare it in nested impls | O rule; SL |
| 8.2 | `Equatable.equal`; `Ordered.compare` plus `less lessOrEqual greater greaterOrEqual min max inRange` | A; SL |
| 8.3 | `From`, `TryFrom`, selected by the expected type | A |
| 8.4 | Integer arithmetic saturates; checked forms return `Result`; division and remainder exist only in checked form | A (A-26); OT E4 |
| 8.5 | `Bool.not and or xor` as type members, eager | A |
| 8.6 | `length`, `concat`, `slice` stay per type | A |
| 8.7 | `Option`, `Result`: no new container-preserving `map` members at this gate. Constructors stay; `Option` implements `Iter`; `Result` does not, and errors are never silently dropped | A; SL S11 |

## 9. Runtime, compiler, tooling

| # | Decision | Basis |
| --- | --- | --- |
| 9.1 | Pipeline: K3 source, resolve, checked IR; interpreter as oracle; `vibra-lower` to verified execution IR; stepper and Cranelift; static runtime archive; platform linker | O; NH section 1 |
| 9.2 | Frames in runtime memory, five checkpoints, context arena with precise reference counting, 4 GiB per context | A (A-17) |
| 9.3 | No language fuel. Scheduler quota is unobservable. Tools set documented limits; exhaustion is an incomplete outcome | A (A-18) |
| 9.4 | Grants live in one sealed authority envelope with host-resource and actor-endpoint kinds, not interchangeable; a host resource is checked at host dispatch, an actor endpoint at kernel dispatch; accounting in a broker | A (A-20) |
| 9.5 | Completed-call audit trace with grant data projections; deterministic test worlds; canonical value encoding; an unobservable value is an explicit error | A (A-9, A-26) |
| 9.6 | Compile-time evaluation: pure functions, constant inputs, tool limits; failure leaves the expression for run time unless a constant is required | A; NH |
| 9.7 | Files are the source; a content store is at most a cache; claim and witness hashed separately; approval against a protected manifest | A (B1 to B3, A-21) |
| 9.8 | Tooling starts from the existing command hierarchy; minimum query, bounded eval, revision-checked file edit in the language release; adapters later | A (A-21) |
| 9.9 | Diagnostics, schemas, exit codes 0 to 6 carried over; changed schemas get new versions | A; OT E6 |

## 10. Roadmap

| Release | Content | Basis |
| --- | --- | --- |
| v0.0.2 | M0 specification and roadmap rewrite (with `endlet`), Windows scope-adapter feasibility; M1 pure language on the interpreter; M2 operations and grants, then pilot X-K3c on the real checker; M3 identity and agent repair loop; M4 first native slice | A (A-22); RM section 3 |
| v0.0.3 | Full native lowering; Windows, Linux, macOS; `vibra build`; Wasm removed; full gate 1 | A |
| v0.0.4 | Libraries and adapters; concurrency semantics; native workers | A |
| v0.0.5 | Services and selected recovery profiles | A |

## 11. Open

| # | Item | Whose |
| --- | --- | --- |
| 11.1 | Closed 2026-11-16: the owner delegated the concurrency policy. In force: actor kernel as substrate; scoped child and join plus bounded parallel map as the first user-facing library; long-lived actors with explicit lifetime rules (A-23) | Settled |
| 11.2 | Pilot X-K3c preregistration; its card must use sections 5 and 6 as written here | Claude, Codex |
| 11.3 | Go-ahead for the rewrite | Owner |
| 11.4 | Whether the static rule set of 6.3 is large enough for gate-1 programs: unknown until programs are written | Evidence |

## 11a. Codex's review

`r5-effective-design-review-codex.md` is accepted in full, with no
design dissent. Its corrections to rows 1.3 to 1.8, 2.2, 6.2 to 6.6,
7.2, 7.4, 8.7, 9.4 are merged above. For the rewrite, its remaining
precisions are binding and read together with these rows: 2.6 and 2.9
(atoms keep uppercase; token bans do not apply inside text), 3.5,
4.4 and 4.7 (interface-value restrictions), 5.1, 5.4, 5.6, 7.6, 7.9
and 9.9 (startup, the two kinds of exit code), 8.3, 9.2, 9.6, and the
five additions of its section 2 (protected evidence boundary,
scope-policy boundary, initial concurrency implementation, diagnostic
status, full-cycle delivery).

## 12. What is not established

- No part of this design beyond the X1b card subset has been tested on
  a model.
- No native binary, conformance suite, or driver was run for this
  design. The Windows file-confinement approach is unverified.
- The static guard checker does not exist; its rule set is a proposal.
- Inventory coverage shows every listed construct has a disposition,
  not that the inventories are complete.
