# Pre-M4 change 02: every tail call reuses its activation

Status: landed in the pull request that adds this file. Decided by Hannah on
2026-10-02. Process record, not specification; the normative text is the
runtime chapter's **Tail calls** section.

## Rule before and after

Before: only a call in tail position whose callee was a member of the current
function's **recursive group** had to reuse the current activation. The group
of a module-level `defn` was every module-level `defn` of its module reachable
from it through static call edges, with an unbounded callee standing for every
function named as a value and every `lambda`. A tail call to a closure, or to
a function outside the group, grew language-level stack, and no `lambda` body
had a tail position at all.

After: every call in tail position MUST reuse the current activation, whatever
the callee is. That covers a module-level `defn` of any module, a method of a
declared type, a `lambda` or closure, a function value held in a parameter, a
binding, a record field, or a collection, and a contract member whether it is
dispatched statically or through an interface value. The callee runs with its
own captures and type arguments. Applications that create no language
activation (constructors, projections, lookups, `@compiler` externals, native
implementations) are not calls for this rule. The definition of tail position
is unchanged, and the operand of `try` stays non-tail.

Non-tail recursion stays legal. Exhausting the host stack through non-tail
calls is still a host event outside interpreter/Wasm parity and still stops the
reference interpreter with `@runtime.host-stack-exhausted`. Milestone 4 decides
whether to add a portable limit; this change adds neither a limit nor a ban.

## Reason

- A simpler rule. The group definition needed a call-flow analysis, a notion of
  "reachable" that was neither a cycle nor a module boundary, and an
  over-approximation for escaping functions, all to say which tail calls the
  guarantee covered. The new rule needs none of it.
- State machines and continuation-style code. A parser or protocol written as
  one function per state, split across modules or driven through callbacks,
  closures, or interface values, is exactly the code a tail-call guarantee is
  for, and it was the code the group rule left out.
- The same guarantee the WebAssembly backend must meet in Milestone 4. A
  backend can lower every tail call to a tail-call instruction or a trampoline
  without reconstructing a group relation first.

## What was removed

- Spec: the recursive-group definition and every use of it (runtime, type
  system, charter, conformance chapters). The milestone 2 and 3 records that
  mention groups carry a pointer to this file.
- `vibra-ir`: `RecursiveGroups` and its Tarjan pass, the recursive-group
  accessors on `CheckedProgram`, the call edges that only fed them (static
  edges in the module set, `CallFlow` call sets, the edge insertions in
  `validate_program_expr`), `parameter_aliases` and the target-set analysis in
  tail validation, and the group-membership check on direct tail calls. Tail
  validation is now one entry-independent pass that requires a tail marker to
  sit in an activation-relative tail position. Tests that observed
  groups were removed or turned into tests of what remains.
- `vibra-types`: the `current_function` field of the checking environment,
  which existed only to withhold tail marks inside `lambda` bodies.
- Kept: the call-flow analysis and the dependency graph. They decide
  `@type.initializer-cycle` and bound indirect call targets, and nothing about
  them depended on the group relation. `IrError::RecursiveCall` keeps its name
  and now documents only the unbounded indirect call target it reports.

## Changed behaviour

- The checker marks a call as a tail transfer in `lambda` bodies, for contract
  members (statically resolved, destination-selected, default, and dispatched
  at run time), and for the call inside the closure that a contract member
  named as a value expands to. It still withholds the mark from a call that can
  reach no source code.
- The interpreter runs every activation, function or `lambda`, in one loop and
  reuses it for a tail transfer to any named function, closure, or contract
  implementation. A compiler-intrinsic wrapper is invoked as an ordinary call.

## Existing expectations reviewed

| Test | Before | After | Why |
| --- | --- | --- | --- |
| `mixed_named_and_closure_tail_calls_reuse_the_selected_target`, closure branch | 0 transfers, depth 2 | 1 transfer, depth 1 | the tail-called closure now reuses the activation |
| `lambda_activations_reuse_for_calls_through_captured_values` | no tail mark, 0 transfers | 2 marks, 2 transfers, depth 1 | the `lambda` call and the call inside its body are both tail calls |
| `captured_callable_values_survive_repeated_tail_transfers` | 3 transfers, depth 2 | 4 transfers, depth 1 | the final call of the captured `lambda` is a transfer |
| `admits_mutual_recursive_calls_and_marks_tail_transfers` | asserted the recursive groups | asserts three tail marks | groups no longer exist; the three bodies are each one tail call |
| `V1-RUNTIME-tail-negative` | unchanged | unchanged | its call is a `let` initializer, non-tail under both rules |
| `V1-RUNTIME-tail-mutual` | unchanged | unchanged | direct mutual recursion reuses the activation under both rules |

Every other tail test, including the external-wrapper cases that expect no
transfer and depth 2, passes with its expectation untouched.

## Evidence

Conformance corpus, `vibra-conformance --root conformance/cases`: before
368 passed, 0 failed, 0 unavailable; after 373 passed, 0 failed, 0
unavailable. The five new `V1-RUNTIME-tail-*` cases each loop 100000 or more
times, against an activation bound of 4096.

| Case | Shape |
| --- | --- |
| `V1-RUNTIME-tail-cross-module-mutual` | a function of one module and a function of another call each other, one by name and one through a function value |
| `V1-RUNTIME-tail-function-value-parameter` | a call through a function-value parameter and the function it receives |
| `V1-RUNTIME-tail-closure-captured-loop` | a closure tail-calls its creator's loop through a captured value |
| `V1-RUNTIME-tail-contract-member-interface-value` | a contract member called through an interface value calls itself |
| `V1-RUNTIME-tail-unrelated-function-loop` | one loop hands its activation to a different loop, then to a function neither can be reached from |

Run against the previous `main`, the closure and contract cases fail with the
activation bound; the other three pass there because the old group was a
reachability relation, so a direct or parameter-passed call between module
functions already reused the activation. They stay as guards for the new rule.

Host tests in `crates/vibra-conformance/tests/tail_calls_step9.rs` run the
same sources and assert the exact transfer count and a maximum activation
depth of 3 (the loop body, `lower`, and its checked subtraction) for each, a
two-module workspace run, and that deep non-tail recursion through a function
value, a closure, and a contract member still stops with the host budget
error. The corpus has no way to expect a host event, so the negative case
lives in host tests and in the existing CLI process test
`deep_non_tail_recursion_is_an_operational_failure_not_an_abort`.

Commands run in the worktree on this change, all passing:

```text
cargo fetch --locked
cargo fmt --all --check
RUSTFLAGS="-D warnings" cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
RUSTFLAGS="-D warnings" cargo test --locked --offline --workspace --all-targets --all-features   # 653 passed, 0 failed
RUSTDOCFLAGS="-D warnings" cargo doc --locked --offline --workspace --no-deps --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases   # 373 passed
cargo test --locked --offline -p vibra-conformance --test evidence_step11
cargo run --locked --offline -p vibra-conformance --bin m1-fuzz -- --profile ci-smoke
```

The spec-example inventory in `docs/roadmap/milestone-1/syntax-examples.tsv`
was regenerated for the runtime and conformance chapters, whose line numbers
moved.
