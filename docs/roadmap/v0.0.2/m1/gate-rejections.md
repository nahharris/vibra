# Gate rejections

The constructions of the release gate's pure modules that the contract rules of
the [type chapter](../../../spec/02-type-system.md#guards) rejected, each with
its predicate, its diagnostic, the rule that would have had to apply, and its
repair.

The modules are those of
[`examples/gate-pure/`](../../../../examples/gate-pure/):

- `errors`: the error type `GateError` and its description;
- `paths`: a note's name from its file name, and a note's log file;
- `lines`: the line count of a text;
- `metric`: the summary of the counts, its mean, and its text.

## Rejections

None. The modules check with no diagnostic.

The one bare guard of the modules is `requires Ordered.greater(notes 0u64)` on
`metric.perNote`. Its one call, in `metric.meanLines`, is under
`if Ordered.greater(notes 0u64)` and is discharged by GUARD-3. The same call
written over the field reads `summary.notes` and `summary.lines`, with no
destructuring pattern, is discharged as well.

No predicate the modules needed fell outside the rules, so this list raises no
specification question.
