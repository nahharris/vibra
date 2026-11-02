# Step 21b — A total indirect call-flow analysis

Prerequisite: Step 21 merged. A defect-fix step found by Step 22's demo sweep. It
claims no new language behavior: it makes the checker accept, and both backends
run, a valid program that the specification already admits (a closure is a value;
a call in tail position reuses its activation). It changes no specification.

## The defect

A continuation-passing loop that wraps the continuation it was given in a new
closure on each round was refused. The checked-IR construction reported
`@tool.unavailable` (`indirect call-flow analysis did not converge`), and `vibra
check` on the default thread aborted with a host stack overflow. Step 6 had met it
and routed around it (guide 06, "Found"); nothing owned it.

## Cause

The call-flow analysis summarizes what a function value may be. A closure's
summary held the summaries of its captured values. Wrapping a closure in a closure
nests that summary one level deeper, so the summary domain had no finite height, the
worklist never reached a fixed point, and its budget ran out, or the clone,
comparison, and drop of a deeply nested summary used the host stack first.

## Fix

A summary names a closure expression by identity. What an expression captures is a
single joined summary per expression, kept in a table beside the summaries. A call
reads it, and growth of an entry re-analyzes the owners that read it. A closure that
escaped to unknown code keeps unknown captures, as gap G12 defines. The domain is
finite, summaries only grow, and the analysis needs no budget, so
`IrError::CallFlowDidNotConverge` no longer exists. Ledger D35.1.

## Evidence

| Cases and tests | Shows |
| --- | --- |
| `V1-RUNTIME-continuation-chain` | A hundred-thousand-closure chain, run by tail calls, in both backends |
| `V1-RUNTIME-continuation-chain-non-tail` | The same chain called in non-tail position: a result, not a crash |
| `V1-RUNTIME-continuation-chain-mutual` | Two functions wrapping with two closure expressions |
| `call_flow_m4_step21b` | The checker and the interpreter, including the release of the chain, on a 512 KiB stack |
| `process_m4_call_flow` | `vibra check` and `vibra run` on the default thread |

## Excluded

Any change to the specification, the runtime, or either backend.
