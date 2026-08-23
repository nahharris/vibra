# Vibra v1 source language

Status: normative target
Implementation status: not started

## Reader

A `.vib` file is UTF-8 and contains zero or more forms. Parentheses delimit
lists; whitespace separates tokens; `;` starts a line comment. There are no
reader macros, quote syntax, commas, brackets, braces, raw strings, heredocs,
or interpolation.

```ebnf
module       = trivia, { top-form, trivia }, EOF ;
form         = atom | list ;
list         = "(", trivia, symbol, { required-trivia, form }, trivia, ")" ;
atom         = string | boolean | integer | float | unit | atom-name
             | label | symbol ;
boolean      = "true" | "false" ;
unit         = "unit" ;
atom-name    = "@", kebab-name, { ".", kebab-name } ;
label        = kebab-name, ":" ;
symbol       = symbol-start, { symbol-rest } ;
symbol-start = lowercase-letter | "_" ;
symbol-rest  = symbol-start | digit | "-" | "." | "/" | "?" | "!" ;
trivia       = { whitespace | line-comment } ;
```

Strings are double quoted and support `\"`, `\\`, `\n`, `\r`, `\t`, and
`\u{HEX}`. Integers are signed decimal. A float contains a decimal point or an
exponent. Numeric range belongs to type checking, not lexing.

User-defined names MUST be kebab-case. `_` is the wildcard. `/` is reserved in
user declarations and separates compiler-owned name components. A dotted name
is a qualified symbol, never field-access syntax. Keywords, booleans, `unit`,
and atom names cannot be rebound.

## Labels and calls

A label consumes one following form. Required ordered input is positional;
optional or named input is labelled. A call may contain fixed positional,
labelled, and variadic operands.

The recovery parser accepts labelled operands interleaved with a variadic tail
when every operand can be bound unambiguously to the resolved signature. The
canonical formatter emits fixed positional operands first, labelled operands in
declaration order, and the variadic tail last. Ambiguous or duplicate binding
is an error. Noncanonical but unambiguous order is a style diagnostic, not a
compile error.

Call arguments are evaluated in resolved parameter order, followed by the
variadic tail from left to right. Evaluation therefore does not change when the
formatter normalizes operand order.

```vibra
(log.write "build finished" level: @info field: "target" field: "app")
```

Every executable list not reserved by the grammar is a call. A function value
is `f`; a nullary call is `(f)`.

## Top-level forms

V1 has exactly these user top-level forms:

```ebnf
top-form = import | deftype | defint | impl | deffect | const | defn | test ;
import   = "(", "import", symbol, string, ")" ;
deftype  = "(", "deftype", symbol, type-expr, { type-attribute }, ")" ;
defint   = "(", "defint", symbol, { declaration-attribute },
           interface-member+, ")" ;
impl     = "(", "impl", symbol, "for:", type-expr,
           implementation-member+, ")" ;
deffect  = "(", "deffect", symbol, { declaration-attribute },
           effect-member+, ")" ;
const    = "(", "const", symbol, type-expr, expr,
           { declaration-attribute }, ")" ;
defn     = "(", "defn", symbol, parameters, type-expr,
           { function-attribute }, { expr }, ")" ;
test     = "(", "test", string, "effects:", effect-row,
           { expr }, ")" ;

parameters = "(", { parameter }, ")" ;
parameter  = "(", symbol, type-expr,
             [ "kind:", ("@labelled" | "@variadic") ],
             [ "default:", literal ], ")" ;
```

All declarations are private unless they contain `visibility: @public`.
Visibility is part of the declaration, not a wrapper form.

`type-attribute` is `params:`, `where:`, `visibility:`, or `doc:`.
`declaration-attribute` is `visibility:` or `doc:`. `function-attribute` is
`params:`, `where:`, `visibility:`, `effects:`, or `doc:`. Each may appear at
most once. Attribute labels are parsed in any order and formatted in the order
listed here.

A parameter is positional when `kind:` is absent. A labelled parameter uses
its parameter name as its call label and MUST have a literal `default:`. A
variadic parameter is the final parameter, has no default, and binds an array
of its written element type. A function has at most one variadic parameter.

```vibra
(import io "@std/io.vib")

(deftype user-id (newtype uint64)
  visibility: @public)

(const default-retries uint8 3)

(defn greet ((name str)) str
  visibility: @public
  effects: ()
  (text.concat "hello, " name))
```

The following signature has one optional labelled parameter and one variadic
tail:

```vibra
(defn write-log
  ((message str)
   (level atom kind: @labelled default: @info)
   (fields (tuple str str) kind: @variadic))
  void
  effects: (io.stdout)
  (log.write message level fields))
```

`deftype`, `defint`, and `deffect` are native AST forms. They MUST NOT be
parser desugarings into a generic definition node. The full static rules are
in the type and effect chapters.

## Types, interfaces, and implementations

```vibra
(deftype user
  (record (name str) (id user-id))
  visibility: @public)

(deftype option
  (enum (some t) (none))
  params: ((t type))
  visibility: @public)

(defint printable
  visibility: @public
  (defn render ((value self)) str effects: ()))

(impl printable for: user
  (defn render ((value user)) str effects: ()
    (field value name)))
```

An `impl` names one interface and one nominal type with `for:`. Interface
members are signatures and have no body. Implementation members have bodies
and MUST match the corresponding signature.

Records are constructed by calling their type with labelled fields. Enum tags
are constructors qualified by the type name. `field` reads a statically known
record field.

```vibra
(user name: "Ada" id: (user-id 1))
(option.some "value")
(field person name)
```

## Functions and expressions

A function declares a name, a parenthesized parameter list, and a result type.
Public functions MUST declare `effects:`. Private functions MAY omit it and use
the inferred row. Function bodies are direct expression sequences; their final
expression is the result.

```ebnf
expr = atom | call
     | "(", "do", { expr }, ")"
     | "(", "let", symbol, expr, { expr }, ")"
     | "(", "if", expr, expr, expr, ")"
     | "(", "match", expr, case+, ")"
     | "(", "while", expr, expr, ")"
     | "(", "for", symbol, expr, expr, ")"
     | "(", "break", ")" | "(", "continue", ")"
     | "(", "return", [ expr ], ")"
     | "(", "try", expr, ")"
     | "(", "discard", expr, ")"
     | "(", "with-resource", resource-binding, { expr }, ")"
     | "(", "field", expr, symbol, ")"
     | "(", "primitive", atom-name, { expr }, ")"
     | "(", "host-op", atom-name, { expr }, ")"
     | collection ;
case = "(", "case", pattern, { expr }, ")" ;
resource-binding = "(", symbol, expr, ")" ;
collection = "(", "tuple", { expr }, ")"
           | "(", "array", { expr }, ")"
           | "(", "map", { "(", "entry", expr, expr, ")" }, ")" ;
pattern = literal | "_" | "(", "bind", symbol, ")"
        | "(", qualified-symbol, { pattern | label, pattern }, ")"
        | "(", "tuple", { pattern }, ")"
        | "(", "array", { pattern }, ")" ;
```

`let` introduces an immutable name for the remainder of its form. There is no
assignment or shared mutable state in v1. `while` and `for` exist for bounded
local iteration; mutation needed by their library implementation is
compiler/runtime internal. Collections have immutable value semantics.

`match` uses explicit `case` lists. Arms are checked for reachability and
exhaustiveness. Patterns are literals, `_`, `(bind name)`, record patterns,
tuple patterns, and qualified enum constructors.

```vibra
(match value
  (case (option.some (bind text)) (text.length text))
  (case (option.none) 0))
```

`try` propagates the error of `result` or the absence of `option` through a
function returning the same container and error type. There are no exceptions
or implicit error conversions. A fallible value MUST be matched, propagated,
returned, bound for later use, or explicitly consumed with `discard`.

`with-resource` evaluates an operation that yields a host resource, binds it
for the lexical body, and closes it on every exit path in reverse nesting
order. A resource cannot escape, be stored, returned, or captured.

`primitive` and `host-op` are closed compiler forms accepted only in
toolchain-signed standard-library modules. `primitive` selects a typed, pure
operation such as checked integer addition. `host-op` selects a typed host
registry entry and is governed by the effects and authority chapter. A normal
package that uses either form is rejected during name checking.

## Canonical format

`vibra fmt` defines the only canonical representation:

- UTF-8, LF endings, and one trailing newline;
- two-space indentation and no trailing whitespace;
- one blank line between top-level forms;
- leaf lists on one line when they fit within 88 columns;
- declaration headers before labelled attributes and bodies;
- fixed, labelled, then variadic call operands;
- one `case` per line in a multiline `match`; and
- preserved comments attached to the following form when possible.

Formatting MUST be idempotent and semantics-preserving. The formatter MAY
normalize recoverable presentation but MUST NOT guess through a syntax,
binding, or type ambiguity.
