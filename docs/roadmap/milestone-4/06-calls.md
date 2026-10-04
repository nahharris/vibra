# Step 6 — calls, closures, generics, tail calls, and depth

Prerequisite: Step 5b merged. Stage 4A behavior step, WebAssembly backend. It
claims the Wasm halves of two exit clauses: deep non-tail recursion has the
same specified outcome in both backends, and a tail-recursive loop that
allocates a fresh compound value per iteration holds a bounded live arena.

## Read before editing

- [Runtime](../../spec/06-runtime.md): **Tail calls**, **Activations and
  memory**, **Reclamation**, **Generic instantiation**, **Evaluation**.
- [Types](../../spec/02-type-system.md): **Functions as values**, **Generics**.
- [M4 ledger](decision-ledger.md) rows D1.1, D1.2, D3.2, D9.2.

## Scope

Calls of every kind and the activation machinery that Step 5b only began:
generic instantiation by run-time type arguments, function values, lambdas and
closures with captures, indirect calls through a function table, a tail call to
every kind of callee, the deep non-tail recursion outcome, and a bounded live
arena across a long allocating tail loop.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-ir/src/lib.rs`: `CallTarget`, `Expr::Closure`, `Expr::Function`, `Expr::Call` and its `tail` flag | The shapes to lower; the checker already marks tail position, and `validate_tail_calls` is the invariant to preserve |
| `crates/vibra-wasm` | The dispatcher, the frame replacement for a tail call, the function table, and the type-argument passing convention |
| `crates/vibra-interp/src/lib.rs`: `Callable`, `TailTransferAction` | The behavior to match, kind by kind |
| The `V1-RUNTIME` tail-call, generic, lambda, and closure cases under `conformance/cases/` | The cases that move |

## Ordered tasks

1. Type arguments at run time: one function per source function, each taking the
   type arguments of its activation as a value ([ledger D9.2](decision-ledger.md)),
   so a value built in generic code carries its instantiated type.
2. Function values and the table: named functions, constructors as values where
   the checker admits them, and `call_indirect` guarded by the recorded
   signature; a mismatch is the toolchain defect, never a program result.
3. Closures and lambdas: captures by value with `dup`, type arguments of the
   creating activation, and a generic `lambda`'s own arguments at each call.
4. Tail calls: the callee replaces the current frame, with its own captures and
   type arguments, for a module function of any module, a method, a lambda, a
   function value, and (in Step 9) a contract member. No Wasm call is made per
   language activation, so no engine feature is needed.
5. Depth: a non-tail recursion a hundred thousand deep completes; one with no base case
   exhausts the runner's memory limit and is reported as the host event, in both
   backends against one `expect.host_event`.
6. Bounded arena: the allocating tail loop measured through
   `vibra_v1_live_size` at two iteration counts a factor of ten apart; add the
   case that asserts the difference is within the constant the case states.
7. Move the matched cases.

Invariants preserved: an application evaluates its callee once before its
operands; a tail call reuses the activation, including from `return`; generics
stay observable; engine stack use is independent of language depth.

## Test matrix

- Positive: each callee kind in tail and non-tail position; a closure capturing
  across two lambdas; a generic function called at two instantiations; mutual
  recursion; a loop of a hundred thousand tail calls through each kind of callee.
- Negative: no new source diagnostic; the exhaustion case and its host-event
  expectation; a forged indirect-call signature in a host test is a defect.
- Recovery: after an exhaustion stop, a new instance runs the same program with
  a larger limit.
- Boundary: zero and many captures; a closure capturing a closure; a tail call
  whose callee has more locals than its caller; recursion exactly at the memory
  limit.
- Formatter: no change.

## Diagnostic and schema changes

None.

## Validation

The [Step 4–12 row](validation.md#focused-checks) and the full
[pre-merge list](validation.md#before-merging-each-step). Record the two live
sizes, the depth reached, and the engine's peak stack.

## Excluded

Contract members and interface values (Step 9); patterns (Step 7); any monomorphization or
tail-call proposal use (M7); borrow inference.

## Completion evidence

The two exit clauses named above, in the
[coverage table](README.md#deliverable-and-gate-coverage), have a Wasm half that
passes next to Step 3's interpreter half; the bounded-arena numbers are in the
handoff.

## As built

The step landed as one change on the mechanism of Step 5b ([D2.12, D2.14,
D9.5](decision-ledger.md)); the design is written once in the documentation of
`crates/vibra-wasm/src/layout.rs` ("Activations and the dispatcher") and of
`lower.rs`.

- **One mechanism.** A call of any kind pushes a frame or replaces one; nothing
  else was added to the dispatcher. A frame is `[parameters][environment][type
  arguments][bindings][temporaries]`. The checked IR numbers a body's slots from
  its parameters on, so the lowering maps a binding's slot `s` at or past the
  parameter count to `s + k + 1`, where `k` is the number of the activation's own
  type arguments. Every move copies a cell with its class byte, and a return
  hands the byte to its caller in the state's `ret_class`.
- **Function values.** A function value is an arena object of kind `function`:
  `[function][slots][arity][own][defaults..][captures..][types..]`. A `lambda`
  and a module function used as a value each have a body of their own in the one
  function table, after the module-value initializers; the second's body is one
  tail call of the function over its own parameters. A call through a value
  checks its recorded arity against the operands it passes, reads the function
  and frame size from it, and gives it to the new frame as the environment. A
  closure reads what it captured, and its creator's type arguments, through the
  environment, so a closure holds no more than a count of its value.
- **Tail calls.** The operands move into locals with their class bytes, and
  `reframe` drops what the frame still owns, then rewrites it in place when the
  new frame ends within its segment and pops and pushes otherwise. The first
  `reframe` of a callee larger than the room left is covered by
  `a_tail_call_to_a_callee_with_more_locals_than_its_caller_replaces_the_frame`.
  A call whose operand is a `return`'s is a tail call too.
- **Generics.** Each language function exists once. A call matches the callee's
  signature against the types of its operands and result, as the interpreter's
  `bind_call` does, and builds a descriptor of each type argument; a function
  value made in a generic activation captures its descriptors; a `lambda` with
  generic parameters of its own is passed them at each call. A value of generic
  type is classed by its cell's class byte (`ValueClass::Dyn`). The one thing the
  lowering reads from a descriptor is whether a type argument is `void`, which
  decides whether an enum payload of generic type is a payload.
- **Labelled defaults.** An omitted operand is the callee's default: for a
  module function a constant built at the call, from the `vibra_ir::Constant`
  of its signature, and for any other callee a cell of the function value,
  because the callee is known only at run time.
- **Lowered.** Every form of a call, a function value, a closure, and a
  generic. `NotLowered` still names `match`, `try`, an array or dict, a
  variadic parameter, an interface value, a wrapper over `str` or `bytes`, a
  compiler external, a test assertion, and a contract call (`call:contract`,
  `call:tail-contract`). The forms `closure`, `captured`, `function`, `default`,
  `call:indirect`, `call:tail-direct`, `call:tail-indirect`, `type:param`, and
  `type:function` are no longer reported, and the `Form` variants for four of
  them are removed.
- **The deep non-tail recursion row.** `V1-RUNTIME-activation-depth` counts with
  `u64` arithmetic, so it stays Step 8a's. The language needs no arithmetic to
  count, though: a tuple of digits, `if`, and a call make a counter that counts to
  any number, and the new cases use one. `V1-RUNTIME-activation-depth-counter`
  recurses a hundred thousand activations deep with a base case and completes in
  both backends, and `V1-RUNTIME-activation-exhaustion-discarded-call` is a
  recursion with no base case that ends in `expect.host_event` in both.
- **Evidence.** Seven `V1-RUNTIME-tail-counter-*` cases run a hundred thousand
  rounds through each kind of callee (a module function, mutual recursion, a
  function-value parameter, a closure, a method, a field and a binding, and a
  hand-off between unrelated loops), and the host test measures them. A loop
  that allocates a fresh `state` tuple each round holds a live size of 10,304
  bytes (what the module values keep) at 10,000 and at 100,000 rounds, after the
  host released the result and while it held it, and an arena high-water mark that
  does not change between the two counts (26,816 to 27,008 bytes by kind). The
  same loops run in 256 KiB; the loop with the call moved out of tail position
  exhausts that and ends as the host event. The recursion a hundred thousand deep
  has an arena high-water mark of 13,635,712 bytes and runs on an 8 KiB engine
  stack, the smallest the test tries, at a thousand activations and at a hundred
  thousand; two thousand activations need exactly 5 pages, and one page less is
  the host event.
- **Counting without arithmetic.** A counter is a tuple of digits, a digit is
  one of sixty-four module values (a record of six bools), and the functions
  that make the next state and test for the last are chains of `if`. It is that
  shape, and not a record of seventeen bools, because the reference
  interpreter's accounting of a recursion a hundred thousand deep must stay
  inside the runner's 64 MiB: a record of bools costs it about 1.4 KiB a level, and
  a tuple of three references 584 bytes.
- **Found.** The reference interpreter has no defect in module initializers: the
  `InvalidBody` Step 5b saw was an entry with an operand (the first function of a
  single source is its entry, and `one` was written first). The case
  `V1-RUNTIME-module-value-initializer-call` and
  `crates/vibra-conformance/tests/initializer_calls_m4_step6.rs` hold every
  initializer shape. The checker's indirect call-flow analysis does not
  converge on a walk that passes a continuation closure through a loop
  (`checked IR construction failed: indirect call-flow analysis did not
  converge`, `@tool.unavailable`), so no case is written in continuation-passing
  style; a counter does the same work.
- **The risk Step 5b noted.** A union's member in Wasm comes from the member list
  at the discriminant, and the interpreter records the operand's static type. No
  form of Step 6 makes them differ: an anonymous union's members must be concrete,
  and a declared union keeps its written order under any instantiation
  (`a_union_widened_in_generic_code_has_the_discriminant_of_its_instantiation`).
- **Left for later steps.** Contract members and interface values (Step 9), `match`
  and every pattern (Step 7), the rows that need integer arithmetic to count
  (`V1-RUNTIME-activation-depth`, `-activation-exhaustion`,
  `-tail-closure-captured-loop`, `-tail-function-value-parameter`,
  `-tail-unrelated-function-loop`, and `-workspace-run-activation-exhaustion`,
  all Step 8a's), and variadic tails (Step 8b).
