# Harness module contract

Experimental Python 3.14 standard-library harness; not the Vibra implementation.
Import modules directly when `harness/` is on sys.path. No model calls from the
grader. No participant source is executed as Python.

## Common tree (plain dictionaries; no spans/comments)

Types are strings or `{name: "list"|"option", args: [type]}`.

- Program: `{kind:"program", decls:[declaration]}`.
- Type: `{kind:"type", name, shape:"wrap", inner:type, where:[expression]}`;
  or shape `record`, fields `[ {name,type} ]`; or shape `variant`, cases
  `[ {name,fields:[{name,type}]} ]`.
- Function: `{kind:"fn",name,params:[{name,type}],returns:type,
  requires:[expr],ensures:[expr],examples:[{call:expr,value:expr}],body:[stmt]}`.
  Nested functions have exactly this shape and occur as statements.
- Literal: `{kind:"literal",type:"int"|"bool"|"text",value:...}`.
- Name: `{kind:"name",name}`.
- Field: `{kind:"field",value:expr,field:name}`.
- Call: `{kind:"call",name:qname,args:[{label:null|string,value:expr}]}`.
  Labeled arguments are sorted by label; positional argument order is retained.
  Duplicate labels are retained for rejection, not silently overwritten.
- List: `{kind:"list",items:[expr]}`.
- Value if: `{kind:"if-expr",condition:expr,then:expr,else:expr}`.
- Try: `{kind:"try",value:expr}` (only whole let/return RHS).
- Let: `{kind:"let",name,type:type,value:expr}`.
- Return: `{kind:"return",value:expr}`.
- Statement if: `{kind:"if",branches:[{condition:expr,body:[stmt]}],else:[stmt]}`;
  absent else is an empty list. All elif arms are retained in order.
- Match: `{kind:"match",subject:expr,cases:[{case:name,body:[stmt]}]}`.

Field/type/case and declaration order is otherwise retained. Expression callee
names are qualified strings; field paths become nested Field nodes.

## Interfaces

- `common.py`: HarnessError(code,message,phase="evaluate",details=None),
  diagnostic(code,message,phase,...); canonical_json(tree); decode_value(tag),
  encode_value(runtime_value).
- `parsers.py`: `parse(surface, source) -> program`; aliases D/D0 accepted;
  bad source raises HarnessError with phase parse and a stable code.
- `checker.py`: `check(program) -> list[diagnostic]`; static types, field/arity,
  scope, exhaustive matches and return paths. No solver/termination proof claim.
- `evaluator.py`: `evaluate(program, entry, args, fuel=100000) -> runtime_value`;
  `check_examples(program, fuel=100000) -> list[diagnostic]`.
  Raises HarnessError for runtime guards/fuel/type/input faults.
  Entry precondition code `requires-violated`, details includes function name.
  Postcondition code `ensures-violated`; refinement code `where-violated`;
  invalid partial primitive `primitive-guard`; exhausted fuel `fuel-exhausted`.
- `taskcheck.py`: `check_task(program, task) -> list[diagnostic]` checks explicit
  public structural constraints independently of private functional test cases.
- `runner.py`: CLI `--manifest FILE --tests-dir EXPLICIT_DIR --output FILE`.
  Manifest `{schema:1,trials:[{id,surface,task,raw_path,prefix_path?,usage?}]}`;
  paths relative to manifest. t10 prepends EXACT prefix bytes; t9 replaces.
  Accept exactly one fenced code block (record and ignore surrounding prose),
  or use the whole text as plain source if there are zero fences; multiple
  fences fail extraction. Preserve raw/extracted/assembled hashes, phase/status, diagnostics,
  per-case ids/outcomes/pass, task_pass; do not print hidden expected values.
  Parse/check/examples failures count as failed assigned trials. Structural
  taskcheck findings are a SEPARATE SECONDARY score, not primary task failure.

## Runtime values

Scalars are Python int/bool/str (strict bool-versus-int), list is Python list.
Records/variants/wrapped values are dictionaries with the test-schema kind,
type/case metadata, and recursively DECODED fields/value. Options have type
`option`. Closure values are evaluator-private, never fixture-serialized.
Encoding restores tagged scalar values. Integers use canonical decimal strings
in fixtures. Unicode surrogate code points are invalid text.

Closures capture preceding lexical bindings plus self; top-level functions are
forward-visible. No local variable hoisting. Nested function parameters may
shadow outer names; duplicate bindings in one lexical frame are rejected.

Fuel is charged for expression/statement evaluation, calls, and collection
iteration/callbacks, so list.build cannot allocate arbitrary work in one step.
Use preflight collection-size and integer-bit limits if needed; classify them
as resource faults. Examples use fresh fuel, execute requires/ensures, and are
tested evidence only. Static checking inspects unreachable code as well.

Tests use unittest and synthetic public fixtures; no discovery of peer private
tests. Each owning agent writes/observes failing tests before implementation.
