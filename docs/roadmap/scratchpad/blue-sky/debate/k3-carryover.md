# K3 carry-over table

Status: second pass, 2026-11-16, for Codex to confirm. Working notes,
not normative. Supersedes the first pass and `k3-syntax-sheet.md`.

Every construct of current Vibra gets a disposition here. The two
inventories carry stable ids: `vibra-carryover-inventory.md` (spec
01–04, 301 rows) and `vibra-carryover-inventory-05-07.md` (charter,
tooling, runtime, diagnostics, 440 rows). A range `LEX-01..03` means
every id in it. A script checks that no id is unmapped (section 13).

Dispositions:

- **kept**: same meaning; at most a new spelling.
- **changed**: meaning or observable behavior differs.
- **added**: not in current Vibra.
- **deferred**: left out of this gate; the row says what reopens it.
- **dropped**: removed, with the reason.

Basis: **owner** (Hannah's ruling), **agreed** (Claude and Codex,
argued), **measured** (in an X1/X1b card). Nothing beyond the X1b card
subset is measured. "Ledger" names the row that records the decision;
`A-n` is an amendment in the ledger's 2026-11-16 section.

Sources of the agreed rules: `r4-open-topics-codex.md` (sections A–E),
`r4-effect-ceiling-form-codex.md`, `r4-required-optional-effects-codex.md`,
`r4-k3-carryover-review-codex.md`, `r4-stdlib-pure-codex.md`.

## 1. Lexical

| # | Existing | K3 | Disposition | Basis, ledger | Check needed |
| --- | --- | --- | --- | --- | --- |
| C1 | LEX-01, LEX-02, LEX-23, LEX-24 | Opening keyword, word-led sections, typed closer. Braces and brackets are used; no commas; layout insignificant. Application is `f(a b)`; any expression may be the callee, `choose(c)(x)`; `f()` is a nullary call. There are no grouping parentheses, so `name(` is always application | changed | owner, measured for the subset; C1, A-1 | Golden parse trees for every production |
| C2 | LEX-03, TST-09 | `// text` to end of line; no block comment | kept, new spelling | measured | |
| C3 | LEX-04, LEX-05 | Same string escapes. Added: triple-quoted string for `doc`, no interpolation, common indentation removed | kept; added | owner | Triple-quote edge cases |
| C4 | LEX-06, LEX-07 | `'c'`: exactly one Unicode scalar, string escapes plus `\'` | changed | agreed (A item 9) | Empty, multi-scalar, surrogate rejected |
| C5 | LEX-08..10 | Unchanged: `42`, `-1`, `1u8`, `2.5f64`; unsuffixed takes its type from context | kept | agreed | |
| C6 | LEX-11, LEX-17, PAT-02 | One discard, `_` | changed | agreed | Diagnostics print `_` |
| C7 | LEX-12, TYP-22, FUN-22 | Type `Void`, value `void`; `return void` | kept | agreed | |
| C8 | LEX-13, LEX-21, LEX-22 | Code identifier `[A-Za-z][A-Za-z0-9]*`. Types PascalCase, values camelCase; casing is a convention `check` reports, never a parse rule | changed | owner; A-2 | |
| C9 | LEX-14, CON-17 | A dotted name is a path or a record field read, decided by resolution: `user.name`. Replaces `(person @name)` | changed | measured for field reads; A-2 | Path-versus-field resolution cases |
| C10 | LEX-15, TYP-05, FUN-17 | Atoms kept: `@public`, `@std.io`. Atom components may contain hyphens and upper case, so existing wire tags (`@runtime.division-by-zero`) keep their spelling. Singleton atom types kept | kept | owner; E1 | Entity slot versus protocol tag |
| C11 | LEX-16, ETC-01 | `name: value` pair, one form for fields, labels, record patterns | kept | measured | |
| C12 | LEX-18, CON-08, CST-05, ETC-05 | `true`, `false` are prelude values; `Bool.true()` | kept | agreed | |
| C13 | LEX-19, LEX-20, TYP-33 | Reserved words become the K3 keywords and closers; prelude names stay non-rebindable | changed | agreed | Reserved-word list in the grammar |

## 2. Top-level declarations

| # | Existing | K3 | Disposition | Basis, ledger | Check needed |
| --- | --- | --- | --- | --- | --- |
| T1 | TOP-01, TOP-09 | `import`, `type`, `interface`, `fn`, `proc`, `let`, `test`; `impl` only nested. `deffect` is gone (T5) | changed | agreed; B5, A-3 | |
| T2 | TOP-02, MOD-03..05, MOD-08 | `import @std.list.List`, `import @std.io`, `import @billing.cents as money`. Default name is the last segment; a collision is an error | changed (alias was mandatory) | owner 7 | Derived-name collision with prelude |
| T3 | TOP-03, TYP-14, TYP-15 | `type Celsius of F64 endtype`; `type User of {name: Str id: U64} endtype`; variants by `case` sections. Every declared type is nominal; no alias | kept, new spelling | owner 1; D1, A-4 | Same-shape types stay distinct |
| T4 | TOP-04 | `interface Printable … endinterface` | kept, new spelling | agreed | |
| T5 | TOP-05, EFF-01..06 | An effect root is an `interface` with `proc` members. The interface is the nominal operation family. A host-bound member is declared only in the standard library | changed | owner 8; F4, A-7 | Impl cannot launder another family |
| T6 | TOP-06, CST-01..03, CST-06, CST-07, EFF-11 | Top-level `let name: Type be expr endlet`; pure, single name, lazily initialized once per run, cycles rejected. `visibility` and `doc` sit before `be`. No `const` | kept, new spelling | owner; E5 | Lazy-once and cycle cases |
| T7 | TOP-07, FUN-01 | `fn name takes {…} returns r: T requires … ensures … do … endfn` | changed (named result, guards) | measured; owner; E1 | |
| T8 | TOP-08, TST-01..08, EFF-21 | `test "name" performs {…} do … endtest`. Void body, no direct `return` or `try`, under `tests/`, `@std.assert`, isolated deterministic world | kept, new spelling | owner 6; A-3 | Discovery order, isolation |
| T9 | TOP-10, TOP-11, MOD-12 | One flat spelling space, forward references, no shadowing | kept | agreed | |
| T10 | TOP-12, MOD-09..11 | Section `visibility @public`; private by default. Added: `representation @private` or `@public`, defaulting to the visibility, hiding constructor, unwrapping pattern, and raw projection | kept; added | agreed (C); A-7 | Public type with private constructor |
| T11 | TOP-13 | `doc` section with one triple-quoted Markdown string; no `enddoc`, no example form | changed | owner; E1, A-6 | |
| T12 | TOP-14..16, TOP-21 | Sections are parsed in any unambiguous order, at most once each, and formatted in canonical order | kept | agreed | Order-tolerance cases |
| T13 | TOP-17..20, FUN-23, ETC-06 | Standard-library-only sections: `role @bool`, `native "str.concat"`, `external @compiler`, `symbol "i32.add"`, body `of Intrinsic(@i32)`. Same rules as today | kept, new spelling (proposed here) | **new in this pass** | |
| T14 | TOP-22 | `fmt` stays the one canonical form; the K3 layout rules are rewritten. `fmt` never reorders runtime operands | kept; layout changed | agreed; L4 | Idempotence corpus |

## 3. Types

| # | Existing | K3 | Disposition | Basis, ledger | Check needed |
| --- | --- | --- | --- | --- | --- |
| Y1 | TYP-01, TYP-02, TYP-04, TYP-36 | `Bool Void Never Char Str Bytes Atom I8…U64 F32 F64`. No plain `Int` | kept, renamed | owner 2 | |
| Y2 | TYP-03, TYP-39 | No null, truthiness, implicit conversion; immutable values | kept | agreed | |
| Y3 | TYP-38 | Host grants are sealed, unforgeable values. Still no close protocol, no linear types | changed | agreed; F1, D3 | Counterfeit-handle case |
| Y4 | TYP-06, TYP-07, TYP-16, TYP-17, TYP-19 | Tuple `{I32 Str}`, record `{name: Str id: U64}`, empty tuple `{}`; structural identity and canonical order as today; allowed in every type position | kept, new spelling | owner 3; A-5 | Zero, one, many arities |
| Y5 | TYP-08 | Declared variants by `case name {fields}` and `case name`. Anonymous enum types | kept (declared); deferred (anonymous) | agreed; **anonymous cut is new in this pass** | |
| Y6 | TYP-09, TYP-10, CON-04, CON-12, PAT-09, PAT-16, CTL-20 | Untagged unions, injection, `as` narrowing pattern | deferred | agreed (A item 4) | Reopen: a gate-1 program tagged variants cannot serve |
| Y7 | TYP-11, TYP-27 | `List(T)`, `Dict(K V)`, `Option(T)`; invariant; no partial application | kept | measured | |
| Y8 | TYP-12, TYP-13, FUN-16 | `Fn({I32 Str verbose: Bool} Bool)`; `Proc({Str} Void {DirWrite})`; `Intent(Void {DirWrite})`. Input vector always braced; row is an unlabeled last operand; omitted row is empty; rows compare exactly | changed | owner 4, 8; A-7 | `Proc({Str} {DirWrite})` reads as tuple result |
| Y9 | TYP-18, TYP-37 | Recursive and uninhabited type rules | kept | agreed | |
| Y10 | TYP-20, TYP-23, INT-22, INT-23 | `Any`; interface values by explicit widening, with all current restrictions (an interface value implements no interface; no `Self` in other parameters) | kept | owner 9; A-7 | |
| Y11 | TYP-21 | `Never` | kept | agreed | |
| Y12 | TYP-24, INT-18 | `Self` | kept | agreed | |
| Y13 | TYP-25, TYP-26, TYP-28 | Parameters in the header, `fn firstOr(T)`, `type Pair(A B)`; bounds `given {T: Printable}`; one nominal bound each | kept, new spelling | agreed | |
| Y14 | TYP-29, FUN-06, ETC-03, INT-15 | No call-site type arguments. A call whose type arguments context cannot fix is rejected. A static member with no receiver is selected by the expected type | dropped (gate cut; loses some expressible calls) | agreed | Reopen: first necessary call no written boundary can fix |
| Y15 | TYP-30, TYP-31 | Dict keys need `Ordered`; overlap rule for impl targets and conversion sources | kept | agreed | |
| Y16 | TYP-32 | Namespaces: type, value, interface. The effect namespace merges into interfaces | changed | agreed | |
| Y17 | TYP-34, TYP-35, MOD-15 | Roles and prelude, PascalCase; `array` is named `List` | kept, renamed | agreed (S10) | |
| Y18 | (none) | Type invariant: `where predicate`, a pure `Fn({Representation} Bool)`, run once on every ingress before the value exists | added | owner direction; agreed; D1, A-4 | False, fault, exhaustion all reject |

## 4. Construction and projection

| # | Existing | K3 | Disposition | Basis, ledger | Check needed |
| --- | --- | --- | --- | --- | --- |
| N1 | CON-01, CON-03, CTL-13, CTL-14 | Labels exactly when the shape has field names: `User(name: "Ada")`, `Option.some(value: x)`, `Option.none()`, `Result.ok(value: x)`, `Result.err(error: e)` | changed (case payloads become labeled) | agreed; C6, A-5 | |
| N2 | CON-02, CON-05..07, MOD-10 | Positional for wrapper and declared tuple: `Celsius(21.5f64)`, `Pair(1 "x")`; unwrap by pattern `Celsius(c)` | kept | owner 1 | Visibility of constructor and pattern |
| N3 | CON-09, CON-10 | Anonymous tuple `{a b}`, record `{name: n}` as values | kept, new spelling | owner 3; X1b-5, A-5 | |
| N4 | CON-11, PAT-07 | Anonymous enum value and pattern | deferred with Y5 | **new in this pass** | |
| N5 | CON-13, COL-02, FUN-08 | List literal `[1i32 2i32]`; `[]` needs an expected type. `List.of` removed | changed | agreed (S) | |
| N6 | CON-14, COL-04, FUN-09 | Dict literal `["k": v]`, empty `[:]`; later duplicate key wins. From a run-time list: `From.convert(entries)` at a written `Dict(K V)` type. `Dict.of` removed | changed | agreed (S) | Uses the selected `Ordered` |
| N7 | CON-15, CON-24 | Constructors, projections, lookups are not function values; wrap in an anonymous `fn` | kept | agreed | |
| N8 | CON-16 | No tuple index; destructure | dropped | agreed | Reopen: generic code needing projection |
| N9 | CON-18..23 | Lookup by application: `items(0u64)`, `table(key)`, `text(0u64)`, returning `Option`. Application kinds: function, collection lookup, constructor | kept (tuple and record projection kinds leave) | agreed (S) | |
| N10 | CON-25..29 | `From.convert(x)`, `TryFrom.convert(x)`, selected by the written expected type; `try` passes its expected type inward | kept; contextual `try` added | agreed (S5) | Error type must match exactly |
| N11 | CON-30 | No `as` expression. A typed `let`, inside `do … enddo` when needed, supplies the boundary | dropped | agreed | |
| N12 | CON-31, CON-32 | Widenings: concrete to interface, atom singleton to `Atom`. Written expected types: parameter, result, typed `let`, field, payload. Once, never inside a container | kept (typed `let` joins; union, `types:`, `as` leave) | agreed; C8, A-5 | |
| N13 | CON-33, INT-17 | Calls name the owner: `User.nameLength(u)`, `Printable.render(v)`. No method-call sugar | kept | agreed | |

## 5. Patterns

| # | Existing | K3 | Disposition | Basis, ledger | Check needed |
| --- | --- | --- | --- | --- | --- |
| P1 | PAT-01, PAT-10, PAT-11, PAT-21, CST-04 | A bare name that resolves to a constant module value is a constant pattern; otherwise it binds. A dotted name never binds. Casing never decides. This settles the contradiction between PAT-01 and PAT-10 | kept (the detailed rule wins) | agreed (B1, B2) | Adding a constant changes an old binder |
| P2 | PAT-03 | Literal patterns as today | kept | agreed | |
| P3 | PAT-04..06 | `Option.some(value: h)`, `User(name: n)`, `Pair(a b)`, `{l r}`, `{name: n}`; omitted record fields ignored; tuple arity exact | kept, new spelling | owner 5 | |
| P4 | PAT-08 | `[a b]`, exact length, refutable | kept, new spelling | agreed | |
| P5 | PAT-12..15, FUN-02, FUN-03 | Patterns in `let`, `let … else`, `match`, and parameters: `takes {Pair(left: a right: b): Pair(I32 I32)}`. Irrefutable where required | kept, new spelling | owner 5; agreed (B5, B10) | |
| P6 | PAT-17..20 | Exhaustiveness, unreachable arm, atom scrutinee, binder collisions | kept | agreed (B6–B8) | Invariants never prove coverage |

## 6. Functions

| # | Existing | K3 | Disposition | Basis, ledger | Check needed |
| --- | --- | --- | --- | --- | --- |
| F1 | (none) | `returns name: Type`; the name exists only in `ensures`. `requires` and `ensures` are run-time guards, never proofs | added | owner; E1 | Guard on every return path |
| F2 | FUN-04, FUN-05, FUN-10 | `labelled {verbose: Bool be false}`; call `f(x verbose: true)`; defaults are constants and belong to the value, not the type | kept, new spelling | agreed; C6 | |
| F3 | FUN-07, ETC-11 | Variadic parameters | dropped (literals replace them) | agreed | |
| F4 | FUN-11..13 | Anonymous `fn takes … do … endfn` and anonymous `proc … performs … endproc`; no self-reference; may be generic | kept, new spelling | owner; C7, A-5 | |
| F5 | FUN-14, FUN-18, FUN-19 | Functions are values; no call operator | kept | agreed; J5 | |
| F6 | FUN-15, ETC-02, ETC-04 | Callee once, then every operand in written order, labeled ones included | changed | agreed (A item 13); A-5 | Order-observing cases |
| F7 | FUN-20, FUN-21 | Recursion kept. A tail call reuses the activation only when no `ensures` check is pending after it | changed (adapted to guards) | agreed (E4); A-9 | Guarded and unguarded recursion |
| F8 | FUN-24, FUN-25 | Public signatures complete. Local `let` type optional; inference is local and never guesses | changed (annotation was absent, then mandatory in the pilots) | agreed; C8, A-5 | |
| F9 | FUN-26, MOD-19 | `proc main takes {…grants…} returns code: ExitCode`. `ExitCode` wraps `U8`. `vibra run` maps non-zero to exit 6 and keeps the code in `programResult`; a standalone build returns the code | changed | agreed (E6); F2, A-8 | Zero, non-zero, startup failure, trap |
| F10 | CTL-09, CTL-10 | `return` on every normally returning path; the redundant-return diagnostic goes | changed | measured as syntax; agreed | |

## 7. Interfaces and methods

| # | Existing | K3 | Disposition | Basis, ledger | Check needed |
| --- | --- | --- | --- | --- | --- |
| I1 | INT-01..04 | Member signature without `do` is abstract; with `do` it is a default, never overridden | kept | agreed | |
| I2 | INT-05..09, ETC-08 | `impl Printable … endimpl` nested in a `type`; `impl I32 … endimpl` nested in an `interface`. No top-level `impl`; orphan rule | kept | agreed | |
| I3 | INT-10..13, INT-16, EFF-17 | Completeness, identity, overlap, member namespace; an impl writes its contract's row | kept | agreed | |
| I4 | INT-14 | Nested `fn` in a `type`, reached as `Type.name` | kept | agreed | |
| I5 | INT-19..21 | Receiver dispatch, destination dispatch, undispatchable-member error | kept | agreed | |
| I6 | INT-24, ETC-07, ETC-09 | No inheritance, overloading, associated types | kept | agreed | |
| I7 | INT-25 | `Equatable.equal`, `Ordered.compare`. Added defaults on `Ordered`: `less lessOrEqual greater greaterOrEqual min max inRange` | kept; added | agreed (S3, S4) | Truth table, left-biased ties |
| I8 | INT-26, INT-27, COL-07..10 | `Iter` with `next`, `map filter skip take collect`, the four adapter types. Added: `Iter.fold` (moved from array) | kept; changed (fold) | owner rule; agreed (S1, S2) | Pull order, laziness |
| I9 | INT-28, INT-29, COL-12..14 | `List`, `Dict`, `Str`, `Option` declare `Iter` in nested impls; the closed registry goes. `Bytes` gains `Iter(U8)`. Shared lexicographic helper | changed | agreed (S9, S2b) | `Dict` remainder keeps `Self` |

## 8. Control flow

| # | Existing | K3 | Disposition | Basis, ledger | Check needed |
| --- | --- | --- | --- | --- | --- |
| L1 | CTL-01 | `if c then … else … endif` | kept as a statement (measured). **Open in this pass:** whether `if` is also an expression | measured; open O1 | |
| L2 | CTL-02 | `match v case Pattern … endmatch`; ordered arms; scrutinee evaluated once | changed (`case` word returns; arms hold sequences) | measured; owner 5; **expression use open, O1** | |
| L3 | CTL-03 | `do statements finalExpression enddo`, an expression with its own scope | kept, new spelling | agreed | |
| L4 | CTL-04..06, CTL-08 | `let pattern [: Type] be expr [else statements] endlet`. One pattern per `let`. Bindings live after `endlet` to the end of the enclosing sequence. `else` only for a refutable pattern, and it must leave | changed (typed form widens; one pair per `let`) | owner; C4, C8, A-5 | Closer pilot (X-K3c) |
| L5 | CTL-07 | An ignored fallible value is an error; discard with `let _ be risky() endlet` | kept | agreed | |
| L6 | CTL-11, CTL-12 | `try` on `Option` and `Result`; not in tests | kept | agreed | |
| L7 | CTL-15..17 | `Never` rules | kept | agreed | |
| L8 | CTL-18 | No loops, no assignment | kept | agreed; C9 | |
| L9 | CTL-19 | Integer arithmetic saturates; division by zero traps; `addChecked` and the other checked forms return `Result`. The inventory row said "no wrap or trap", which is wrong against the runtime chapter | kept (row corrected) | agreed (E4); D4 | Exact overflow, zero, shift cases |
| L10 | COL-01, COL-03, COL-05, COL-06, COL-11 | `List`, `Dict`, immutability, `Dict.entries`, effectful walks by recursion over `Iter.next` in a `proc` | kept | agreed | |

**O1, for Codex.** `let x be if c then a else b endif endlet` is the
common need. My default: `if` and `match` are expressions when every
arm ends in a final expression, exactly as `do … enddo` does, and
statements otherwise; no second form. Alternative: statement only,
value choice through `do`. Say which.

## 9. Effects and capabilities

| # | Existing | K3 | Disposition | Basis, ledger | Check needed |
| --- | --- | --- | --- | --- | --- |
| E1 | EFF-07..10, EFF-12..16, EFF-18, EFF-19 | Section `performs {DirWrite NetSend}` on `proc`, anonymous `proc`, `test`, and interface `proc` members. Closed, never inferred, omitted means empty; entries are operation interfaces; body row computed and checked; no row subtyping or variables; unused root warns | kept, new spelling and root kind | owner 8; F1, F3, A-7 | Laundering, duplicate alias, empty row |
| E2 | (none) | Authority is an unforgeable value implementing operation interfaces. Required grant: plain parameter. Optional: `Option(Cap)`. Refusal of one request: typed `Result`. Narrow operations by destructuring a record of leaf grants; narrow the resource by a pure library function returning `Result`. A brand such as `type LogDir of DirWrite` certifies the issuer, not the path | added | owner direction; agreed; F1–F4, A-7 | The ten acceptance cases in `r4-required-optional-effects-codex.md` section 6 |
| E3 | (none) | Calling a `proc` builds a cold `Intent`; `await` runs it, only inside a `proc`. Delivery atoms `@once`, `@repeatable`, `@unknown` | added | agreed; F6, F8 | Plan stored, returned, awaited |
| E4 | EFF-20, MOD-18 | Target data binds each `main` parameter to a provider and grant policy. A missing required binding fails the build; failed acquisition fails startup; optional refusal gives `None` | changed | agreed; F2, B10, A-8 | |
| E5 | EFF-22, EFF-23 | The 17 host registry rows become the first operation interfaces, one identity per row; no operation returns `Never` | kept as seed | agreed (E3) | Row-by-row mapping before naming |
| E6 | EFF-24 | Tests run in a fresh deterministic in-memory world; model targets get no real grants | kept | agreed; F5, F7 | |
| E7 | EFF-25 | Effects stay static and erased, no handlers or prompts. "Not capabilities" no longer holds | changed | owner | |

## 10. Modules, packages, data

| # | Existing | K3 | Disposition | Basis, ledger | Check needed |
| --- | --- | --- | --- | --- | --- |
| M1 | MOD-01, MOD-02 | One file, one module; the path spells the atom path, exact case; segments follow the identifier grammar; paths that collide case-insensitively are rejected | changed (segment grammar) | agreed (A item 11) | Windows case mismatch |
| M2 | MOD-06, MOD-07 | No string, relative, glob imports, re-exports, cycles. Brace (selective) imports deferred | kept | agreed | |
| M3 | MOD-13, MOD-14, MOD-16 | Units, entity-reference atoms, embedded standard library modules. Module list changes with the operation interfaces | kept | agreed | |
| M4 | MOD-17, MOD-20, MOD-21 | `project.vibon`, dependencies, VIBON data. Entry and grant-binding fields change (E4), with a new interpretation version | kept; fields changed | agreed; B11, A-8 | |
| M5 | ETC-10 | Unavailable forms report `@tool.unavailable` | kept | agreed | |

## 11. Charter, tooling, runtime, diagnostics (second inventory)

| # | Existing | Disposition | Basis, ledger |
| --- | --- | --- | --- |
| R1 | CHA-01..10, CHA-13..17, CHA-19..26, CHA-28, CHA-30..33, CHA-36..41 | kept | agreed |
| R1b | CHA-11 | changed: source is no longer the S-expression form shared with `.vibon` data; data files keep theirs | owner (K3); found by Codex |
| R1c | CHA-34 | changed: executable type invariants (`where predicate`) are now in, where refinements were excluded | Y18; found by Codex |
| R2 | CHA-12 | changed: naming is camelCase and PascalCase | owner |
| R3 | CHA-18, CHA-27 | changed: ceilings plus capability values | owner 8 |
| R4 | CHA-29 | changed at gate 2: concurrency as a library | H1, H2 |
| R5 | CHA-35, LIM-18 | changed: evaluation is fuel-metered with size limits | I4. **Conflict to confirm: current Vibra has no run-time budgets** |
| R6 | CLI-01..46 | kept as the starting surface. Ledger L1 lists a different command set (`show`, `find`, `eval`, `build`, no `query`, no `mcp`) and L7 defers MCP | **Conflict to confirm: L1 was written without the inventory** |
| R7 | FMT-01..06, FMT-08..10, FMT-12..14, FMT-16..19 | kept | L4 |
| R8 | FMT-07, FMT-11, FMT-15 | changed: no operand reordering; K3 layouts | agreed |
| R9 | CHK-01..04, CHK-13..29 | kept | agreed |
| R10 | CHK-05..12 | edit plans, rename, apply protocol: kept as existing tooling; L7 defers new adapters only | **to confirm** |
| R11 | OUT-01..53 | kept. Schemas that carry rows, entry results, or source spellings get new versions; no silent reuse of a version | L9, B11; E1 |
| R12 | RUN-01..06, RUN-08, RUN-10, RUN-13, RUN-14, RUN-16..20, RUN-23..28, RUN-36..52, RUN-59..72 | kept, including saturating arithmetic, lazy module values, canonical encoding, unobservable functions | E4, E5 |
| R13 | RUN-07 | changed: written operand order | F6 above |
| R14 | RUN-09, RUN-21, RUN-22 | dropped or deferred with variadics, unions, `as` | Y6, F3, N11 |
| R15 | RUN-11, RUN-12, RUN-15 | kept, new spelling | L4, N3 |
| R16 | RUN-29..35 | changed: tail position excludes calls with a pending `ensures` | E4; A-9 |
| R17 | RUN-53..58 | value arena and reclamation: kept as the existing mechanism | **Conflict to confirm: ledger I1 calls the memory mechanism not chosen** |
| R18 | RUN-73, HST-03, HST-47 | changed: consent becomes grant binding | E4 above |
| R19 | LIM-01..17, LIM-19 | kept: traps, exit mapping, not catchable | agreed |
| R20 | HST-01, HST-02 | changed: entry signature and result | F9 above |
| R21 | HST-04..28 | kept: registry of 17 operations, error enums, versioning | E3 |
| R22 | HST-29..43 | kept: providers, test and conformance worlds, `@host-world.v1` | E2 |
| R23 | HST-44..46 | kept: audit event per completed call. Grant arguments are recorded through a data projection under a new schema version | E2; A-9 |
| R24 | HST-48..57 | kept: Wasm backend contract | J4; native output is open (section 12 item 7) |
| R25 | DGF-01..29, DGC-01..14 | kept: codes, families, ordering, recovery. Tables for binding, return, and effects are rewritten; new codes for grants | E1 |
| R26 | CNF-01..35 | kept: corpus structure and profiles. Every case is re-authored in K3 | N2 |
| R27 | ET2-01..25 | restatements of 01–04 rules; follow the rows above (unions and `as`: Y6, N11; iterators: I8, I9; prelude: Y17) | |

## 12. Items raised in this pass for Codex

1. **O1**: `if` and `match` as expressions.
2. **T13**: spelling of the standard-library-only sections.
3. **Y5, N4**: anonymous enum types and values deferred with unions.
4. **R5**: fuel metering (I4) against "no run-time budgets" today.
5. **R6, R10**: ledger L1 and L7 against the existing CLI, query,
   edit, and MCP surface.
6. **R17**: ledger I1 against the existing value arena.
7. **Native output** (owner question, 2026-11-16): current Vibra has
   an interpreter and a Wasm backend; ledger J4 requires no new
   backend at gate 1. Whether and when Vibra produces native
   executables is not decided.

Items 4 to 6 are places where the round-2 ledger was written before
the inventory existed. My default for each: the existing behavior is
the starting point, and the ledger row is a proposal to change it that
needs its own reason.

## 13. Coverage check

A script expands every id and range in this file and compares with the
ids in both inventories. Result: see ledger amendment A-10.
