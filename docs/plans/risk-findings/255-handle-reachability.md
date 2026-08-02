## Pre-implementation findings

The spike's phase 1 question was answered empirically against the current
compiler (`target/debug/vibra.exe`, `vibra 0.1.4`, branch
`agent/vib-source-call-order`) rather than by reading the type system alone.

**Verdict: NO-GO.** Every position the plan asks about is reachable — 7 of 7,
not "most". The plan's own stop condition is met at 100%, and the favourable
hypothesis it rests on ("value semantics mean far less aliasing") is not just
unproven, it is **backwards**. Detail below, plus a cheaper alternative that
captures most of the value.

### Method

Each row is a program that was compiled and run. `E-*` means the compiler
rejected it; no output and exit 0 means it was accepted and executed. Handles
were obtained through `io.stdout.open` (an `io.output`, i.e.
`(newtype (handle @write))`, `stdlib/src/io.vib:6`) and `fs.write.open`
(`stdlib/src/fs.vib:123`).

### Results — 7 of 7 YES

| # | Position | Result | Evidence |
| --- | --- | --- | --- |
| 1 | `record` field | **YES** | `(let b (record (out h)))`, then destructured with `(record (out (bind o)))` and written through — the write reached stdout |
| 2 | `array` element | **YES** | `(let xs (array h h))` accepted |
| 3 | `map` key | **YES** | `(let m (map (h "v")))` accepted |
| 3b | `map` value | **YES** | `(let m (map ("k" h)))` accepted |
| 4 | `tuple` | **YES** | `(let t (tuple h 1))` accepted |
| 5 | `task`/`spawn` `captures` | **YES** | `(task (stream.write.string h "…") captures: (h))` accepted **and the task wrote through the captured handle** |
| 6 | function return / `mut` / `ref` | **YES** | `(defn passthru (h io.output) io.output (return h))` chained twice; `(mut h)` and `(ref h)` both accepted |
| 7 | `option`/`result` over `any` | **YES** | `(defn wrap (h io.output) (option.option io.output) (return (option.option.some h)))` and the `result` equivalent both accepted |

Item 7 is the load-bearing one. `A::Any => true` (`src/lower.rs:3649`) places
**no constraint whatsoever** on the ABI's generic slot, and the stdlib's
containers are declared `where: (t any)` throughout
(`stdlib/src/collections.vib:10-47`). Handle types satisfy `any` unrestricted.
A handle also crosses the host ABI's `Any` slot successfully:
`(coll.array-append io.output xs h)` compiles and runs, so
`@array-append`'s `ValueKind::Any` parameter (`src/host_abi.rs:278-283`)
accepts a handle inside an array.

There are no closures in the language to check — the grammar has only top-level
`fn`/`defn` (`docs/decisions/s-expression-language.md:148`), so item 5 reduces
to `task`/`spawn` captures, which is a YES.

### The compiler already enumerates the sharing edges, and there are 15

`contains_host_backed_newtype` (`src/lower.rs:3683-3718`) is the compiler's own
answer to "where can a host-backed newtype hide". It recurses through:

`Instantiated` (every generic argument) · `Tuple` · `Union` · `Intersect` ·
`Array` · `Enum` (every variant payload) · `Record` (every field) ·
`Interface` (every member) · `Map` (**key and value**) · `Mutable` ·
`Reference` · `JoinHandle` · `Newtype` · `FnType` (every parameter **and** the
return type)

That is 15 type constructors, i.e. **every aggregate constructor in the
language**. There is no position from which a handle is excluded. An in-danger
propagation would have to track all 15, interprocedurally, and — because of
`where: (t any)` — sensitively to generic instantiation.

Nesting is unrestricted and compiles today:

```lisp
(let boxed (record (h w)))
(let nested (tuple (array w w) boxed))
```

### The plan's favourable hypothesis is inverted, not merely unproven

The plan hedges: "Vibra's position is favourable — value semantics and
unforgeable nominal endpoints mean far less aliasing … but that is a
hypothesis, not a fact."

Value semantics make this **worse**, not better. `HostHandle` is
`#[derive(Clone, Copy)]` (`src/lower.rs:122-126`) — a bare `{ id: u64, access }`.
Every copy is an independent value that denotes the same host resource. So
`(let boxed (record (h w)))` does not create a *reference* the analysis could
follow; it creates a second, structurally unrelated value with equal authority.
A sharing analysis in a reference-semantics language at least has a reference to
chase. Here there is nothing to chase but the flow of copies — which is
whole-program value-flow analysis, the exact cost the plan set out to avoid.

The unforgeability half of the hypothesis does hold: casts to/from host-backed
newtypes are rejected (`src/type_semantics.rs:478-491`,
`src/typed_body.rs:1349-1354`) and only a validated `deffect` operation may mint
one (`docs/decisions/effect-system.md:55-58`). Constructor sites are closed and
few. But unforgeability constrains where handles are *created*, not where they
*travel*, and it is travel that costs.

The contract also says so directly: "Host handles remain copyable for this
migration" (`docs/decisions/effect-system.md:112`).

### The safety payoff is smaller than the issue assumes

The plan's motivation is "use-after-close and double-close are runtime errors at
best." They are runtime errors — but *precise, deterministic, non-exploitable*
ones, which materially lowers the value of catching them statically.

`FileTable` IDs are monotonic and never recycled (`src/execute.rs:200-270`),
and `classify_absent` (`src/execute.rs:300-309`) exploits that:

```rust
/// Monotonic IDs make tombstones unnecessary: every dynamic ID below
/// `next` was minted by this instance, so an absent one is closed. IDs
/// outside that half-open range were never minted and are invalid.
```

A closed handle can therefore never alias a later-opened resource. There is no
confused deputy and no resource confusion — only a typed error.

Confirmed end-to-end. This program compiles clean and reaches runtime:

```lisp
(let boxed (record (h w)))
(let nested (tuple (array w w) boxed))
(stream.manage.close w)
(match boxed (record (h (bind alias)))
  (do (match (stream.write.string alias "after close\n")
        (result.result.ok) (do (io.stdout.println "WROTE THROUGH CLOSED ALIAS"))
        _                 (do (io.stdout.println "alias write failed at runtime")))) …)
```

Output: `alias write failed at runtime`. The write through the closed alias is
caught, cleanly, with a typed `stream.error`. This is already the documented
contract (`docs/reference/wasm-abi.md:63-71`: "Duplicate close and every
operation through an alias after close return `fs-error.resource-closed`").

So the analysis would upgrade a precise runtime error into a compile-time error.
That is worth something — but it is a diagnostics improvement, not a soundness
fix, and it must be priced as one.

### Why the risk asymmetry closes the case

The plan states the asymmetry itself: "false positives are worse here than
missed detections. A model that receives a spurious use-after-close error will
restructure working code to appease it."

Combine that with 7-of-7 reachability. An in-danger analysis must taint
everything sharing structure with a closed handle. With handles reachable
through all 15 constructors and through every `any`-bounded generic, the taint
set after a single `close` inside a `match` arm would routinely include
unrelated bindings. Making it precise enough to avoid that *is* the
whole-program alias analysis the plan wanted to avoid; shipping it imprecise
produces exactly the failure mode the plan says is the worse one. Both branches
are bad.

The plan's primary acceptance gate — "unrelated handles still usable after a
close … this is the whole point of non-linearity" — is the specific thing that
100% reachability makes hard to hit.

### Recommendation

**NO-GO on the in-danger propagation spike.** Close #255 with this finding as
the deliverable; the plan explicitly allows this ("Recommending no-go is a
successful outcome, not a failed one", and the definition of done accepts "a
written no-go recommendation with the enumerated sharing edges that justify
it"). The 15 edges are enumerated above.

Two cheap follow-ups that capture most of the value at a fraction of the cost:

1. **A local, non-aliasing lint.** Diagnose only the case where the *same
   binding* is closed and then used, or closed twice, within one function body
   and one control-flow path. No sharing analysis, no interprocedural
   propagation, no taint set — so no false positives on unrelated handles. This
   catches the common authoring mistake. `examples/fs-roundtrip.vib` remains a
   useful arm-sensitivity test for it. Ship as a warning, register the code in
   `schemas/linter-codes.json`.
2. **Promote the runtime guarantee into the contract.** Replace the deferred-
   ownership note at `docs/decisions/effect-system.md:112` with what is actually
   true and tested: handles are copyable; IDs are monotonic and never recycled;
   use-after-close and double-close are deterministic typed errors that can
   never alias another resource. Right now that guarantee lives only in
   `docs/reference/wasm-abi.md:63-71` and a comment in `src/execute.rs`, which
   is why the issue was written as if the situation were worse than it is.

If affine handles are ever revisited, the finding to carry forward is that the
blocker is **copyability plus the unrestricted `any` bound**, not the aggregate
constructors themselves. Constraining `where: (t any)` so handle types need an
explicit opt-in would shrink the reachable surface far more cheaply than any
analysis, and would be the actual prerequisite for a tractable version of this
feature.
