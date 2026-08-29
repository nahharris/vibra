# Vibra v1 source language

Status: normative target
Implementation status: not started

## Reader

A `.vib` file is UTF-8 and contains zero or more forms. Parentheses delimit
lists; whitespace separates tokens; `;` starts a line comment. There are no
reader macros, quote syntax, commas, brackets, braces, raw strings, heredocs,
or interpolation.

```ebnf
module         = trivia, { top-form, trivia }, EOF ;
form           = atom | list ;
list           = "(", trivia,
                 [ form, { required-trivia, form } ], trivia, ")" ;
atom           = string | character | boolean | integer | float | void
               | atom-name
               | label | symbol ;
literal        = string | character | boolean | integer | float | void
               | atom-name ;
boolean        = "true" | "false" ;
void           = "void" ;
character      = backslash,
                 ( character-name | unicode-character | character-scalar ) ;
character-name = "newline" | "return" | "space" | "tab" ;
unicode-character = "u", hex-digit, hex-digit, hex-digit, hex-digit ;
integer        = [ "-" ], digits, [ integer-suffix ] ;
float          = [ "-" ],
                 ( digits, ".", digits, [ exponent ], [ float-suffix ]
                 | digits, exponent, [ float-suffix ]
                 | digits, float-suffix ) ;
integer-suffix = "i8" | "i16" | "i32" | "i64"
               | "u8" | "u16" | "u32" | "u64" ;
float-suffix   = "f32" | "f64" ;
exponent       = ( "e" | "E" ), [ "+" | "-" ], digits ;
digits         = digit, { digit } ;
symbol         = "-" | ( kebab-name, { ".", kebab-name } ) ;
atom-name      = "@", symbol ;
label          = symbol, ":" ;
kebab-name     = lowercase-letter,
                 { lowercase-letter | digit | "-" } ;
lowercase-letter = "a" | "b" | "c" | "d" | "e" | "f" | "g"
                 | "h" | "i" | "j" | "k" | "l" | "m" | "n"
                 | "o" | "p" | "q" | "r" | "s" | "t" | "u"
                 | "v" | "w" | "x" | "y" | "z" ;
digit          = "0" | "1" | "2" | "3" | "4"
               | "5" | "6" | "7" | "8" | "9" ;
hex-digit      = digit | "a" | "b" | "c" | "d" | "e" | "f"
               | "A" | "B" | "C" | "D" | "E" | "F" ;
backslash      = U+005C ;
character-scalar = ? one Unicode scalar other than whitespace ? ;
trivia         = { whitespace | line-comment } ;
required-trivia = ( whitespace | line-comment ), trivia ;
```

Strings are double quoted and support `\"`, `\\`, `\n`, `\r`, `\t`, and
`\u{HEX}`.

A character literal follows EDN spelling and denotes exactly one Unicode scalar
value. A backslash may be followed by one non-whitespace scalar, one of
`newline`, `return`, `space`, or `tab`, or `u` and exactly four hexadecimal
digits. Thus `\c`, `\0`, `\newline`, and `\u0063` denote `c`, `0`, line feed,
and `c`. A `u` escape whose value is a surrogate code point is invalid. The
complete non-delimited token belongs to the character literal: `\newline-x`
and `\ab` are invalid rather than a character followed by a symbol. A
backslash followed by whitespace is invalid; whitespace characters use their
named or Unicode spelling.

Integers are signed decimal and floats contain a decimal point, an exponent, or
an `f32`/`f64` suffix. An adjacent suffix is part of the numeric token. The
integer suffixes are `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, and `u64`;
the float suffixes are `f32` and `f64`. A suffix fixes the literal's exact type,
so `1u8`, `25i32`, and `2.5f64` have types `u8`, `i32`, and `f64`. `2f64` is
also a float. Unsuffixed literals receive a type from their expression context.
An unknown suffix, an integer suffix on a float body, or trailing token text is
an invalid numeric literal. Numeric range belongs to type checking, not lexing;
for example, `-1u8` is lexically one `u8` literal and is rejected as out of
range. V1 has no numeric separators, non-decimal bases, `isize`, or `usize`.
Character and maximal numeric recognition take precedence over symbol
recognition; `-` followed immediately by a digit starts a numeric token, while
the standalone `-` remains the discard symbol.

`void` is both the primitive type name in type position and its single value in
expression or data position. It is the result of a function or form that
completes successfully but has no meaningful value to return. Empty `do` and
an empty function body evaluate to `void`. There is no `unit` literal.

Every named symbol consists of one or more dot-separated segments, and every
segment independently satisfies `kebab-name`. Its first character MUST be a
lowercase ASCII letter; every later character is a lowercase ASCII letter,
digit, or hyphen. Consecutive and trailing hyphens are therefore valid, while a
leading hyphen is not. `?`, `!`, `_`, `/`, an empty segment, and a segment that
starts with a digit are invalid in a symbol. A dotted symbol is a qualified
name, never field-access syntax. For example, `a`, `a1`, `a-`, `a--b`, and
`a.b2-c` are symbols; `1a`, `-a`, `a..b`, and `a?` are not.

Labels and atom names derive mechanically from symbols: `some.name:` is a
label and `@some.name` is an atom name. The `"-"` symbol alternative therefore
also derives `@-` and `-:` without separate lexical rules. These three forms
are the semantic `discard` subset. Discard classification takes precedence over
the ordinary symbol, atom-name, or label role.

The three discard spellings cannot be namespaced or extended: `-.name`,
`@-.name`, and `-:.name` are invalid. All are semantically equivalent to Rust's
`_`: in a binder or wildcard position they create no identity, may repeat, and
do not participate in redeclaration or shadowing checks. Despite being derived
through the ordinary name grammar, a discard never denotes a value, label,
atom, or reference and is rejected in every non-discard position.

Keywords, booleans, `void`, and atom names cannot be rebound.
Boolean, `void`, and reserved-form recognition takes precedence when their
spelling also satisfies the symbol production.

An atom is an ordinary value by default. Only a source grammar or `.vibon`
schema position that explicitly expects an entity reference resolves an atom
as one. The module locator in `import` is such a source position; writing
`@std.io` in an ordinary expression produces an atom value rather than loading
or invoking code. Effect rows do not use atoms: they contain lexical symbols
resolved through the source module's imports.

A position that does expect an entity reference declares the entity kind it
requires; it never changes how the atom's path is read. The type-system chapter
defines that one reading, under which each path denotes at most one code entity
in every position, and names the one entity kind that carries a pair identity
rather than a path.

## Labels and applications

A label consumes one following form. Required ordered input is positional;
optional or named input is labelled. An application may contain fixed
positional, labelled, and variadic operands.

The reader permits any form, including another list, at the head of a list.
After reserved-form recognition, every nonempty executable list is an
application. The checker classifies it from the statically resolved callee;
an application is not necessarily a function call. An empty list is valid
only where a contextual grammar explicitly admits it, such as an empty
parameter or effect list. It is never an executable application.

```vibra
((choose-function condition) argument)
((make-tuple) 0)
((tuple-of-functions 0) argument)
```

The first example calls the function selected by `choose-function`. The second
projects component zero from the tuple returned by `make-tuple`. The third
projects a function from a tuple and calls it. The callee expression is
evaluated exactly once before any runtime operand; compile-time tuple and
record selectors are not evaluated as values. The type chapter defines the
closed set of applicable categories and their operand rules; v1 has no
user-defined call operator or callable interface.

The recovery parser accepts labelled operands interleaved with a variadic tail
when every operand can be bound unambiguously to a resolved function or
constructor signature. The canonical formatter emits fixed positional
operands first, labelled operands in declaration order, and the variadic tail
last. Ambiguous or duplicate binding is an error. Noncanonical but
unambiguous order is a style diagnostic, not a compile error.

Function and constructor operands are evaluated in resolved fixed-parameter
order, then labelled-parameter declaration order, then variadic source order.
Evaluation therefore does not change when the formatter normalizes operand
order.

```vibra
(log.write "build finished" level: @info "target" "app")
```

A function value is `f`; its nullary application `(f)` is a function call.
Atoms are ordinary values and are not applicable, so `(@some-atom value)` is a
static error rather than an indirect lookup or call.
The head spelling alone never turns an operand into a selector: `(value @name)`
is a function call when `value` has a matching `fn` type and a record
projection when `value` has a record type. `(value value)` is classified by the
same static rule. A literal-headed form such as `(1 2)` is not applicable.

An array variadic parameter receives every remaining unlabelled form as one
array. A map variadic parameter receives alternating key and value forms. The
call MUST contain an even number of remaining forms, and construction follows
the ordinary `map.of` rules.

```vibra
(collect-values v1 v2 v3)
(collect-fields k1 v1 k2 v2 k3 v3)
```

## Declarations

V1 has exactly these user top-level forms:

```ebnf
top-form = import | deftype | defint | deffect | def | defn | test ;
import   = "(", "import", symbol, atom-name, ")" ;
deftype  = "(", "deftype", symbol, type-expr,
           { type-attribute | nested-method
           | interface-implementation }, ")" ;
defint   = "(", "defint", symbol,
           { declaration-attribute | interface-member
           | interface-implementation }, ")" ;
deffect  = "(", "deffect", symbol,
           { declaration-attribute | effect-member }, ")" ;
def      = "(", "def", symbol, type-expr, expr,
           { declaration-attribute }, ")" ;
defn     = "(", "defn", symbol, parameters, type-expr,
           { function-attribute }, { expr }, ")" ;
test     = "(", "test", string, [ "effects:", effect-row ], expr+, ")" ;

nested-method = "(", "defn", local-name, parameters, type-expr,
                { function-attribute }, { expr }, ")" ;
interface-member = "(", "defn", local-name, parameters, type-expr,
                   { function-attribute }, ")" ;
interface-implementation = "(", "impl", type-expr,
                           implementation-member+, ")" ;
implementation-member = "(", "defn", local-name, parameters, type-expr,
                        { function-attribute }, { expr }, ")" ;
effect-member = "(", "defn", local-name, parameters, type-expr,
                { function-attribute }, { expr }, ")" ;

discard              = "-" | "@-" | "-:" ;
local-name           = kebab-name ;
binding-name         = local-name | discard ;
parameters           = "(", { pattern, type-expr }, ")" ;
labelled-parameters  = "(", { local-name, type-expr, literal }, ")" ;
variadic-parameter   = "(", binding-name, variadic-type, ")" ;
variadic-type        = "(", "array", type-expr, ")"
                     | "(", "map", type-expr, type-expr, ")" ;
effect-row            = "(", { effect-reference }, ")" ;
effect-reference      = symbol ;
where-clause         = "(", { symbol, generic-bound }, ")" ;
generic-bound        = "any" | symbol ;

declaration-attribute = "visibility:", atom-name | "doc:", string ;
type-attribute = "where:", where-clause
               | declaration-attribute ;
function-attribute = "where:", where-clause
                   | "labelled:", labelled-parameters
                   | "variadic:", variadic-parameter
                   | "visibility:", atom-name
                   | "effects:", effect-row
                   | "external:", atom-name
                   | "symbol:", string
                   | "doc:", string ;
lambda-attribute = "labelled:", labelled-parameters
                 | "variadic:", variadic-parameter
                 | "effects:", effect-row ;
```

`def` introduces an immutable module value. There is no separate `const` form
in v1.

Every required positional parameter is written directly as a pattern/type
pair; there is no wrapper list around each parameter. The pattern MUST be
irrefutable for the written type. `labelled:` is one flat list of
name/type/default triples. Every labelled parameter MUST have a literal default
value and a real unqualified local name because its name is part of the call
contract. `variadic:` contains exactly one name/type pair, and its type MUST be
an `array` or `map`. A function has at most one variadic parameter. A variadic
parameter may use any discard spelling when its value is intentionally unused.

`where:` is a flat list of generic-name/bound pairs. It is the only declaration
of generic names: every generic name used by a declaration MUST occur exactly
once in its `where:` clause, unless an enclosing declaration already binds it.
`any` states that no interface bound is required. A nested method inherits its
owner's generic names and MUST NOT redeclare one; the type chapter defines that
inheritance and the call-site `types:` list built from it.

The canonical function-attribute order is `where:`, `labelled:`, `variadic:`,
`visibility:`, `effects:`, `external:`, `symbol:`, then `doc:`. The canonical
type-attribute order is `where:`, `visibility:`, then `doc:`. Attributes may be
parsed in any unambiguous order, occur at most once, and are formatted
canonically. Nested methods follow attributes, and `impl` blocks follow methods.

Every labelled argument or attribute follows all fixed positional forms of its
enclosing form and precedes its variadic body or member forms. The parser MAY
recover another unambiguous order for function or constructor applications as
described above, but the declaration grammar and formatter never use a label
to interrupt a fixed header.

All declarations are private unless they contain `visibility: @public`.
Visibility is part of the declaration, not a wrapper form.

```vibra
(import io @std.io)

(deftype user-id (newtype u64)
  visibility: @public)

(def default-retries u8 3u8)

(defn greet (name str) str
  visibility: @public
  (text.concat "hello, " name))

(deftype option
  (enum some t none void)
  where: (t any)
  visibility: @public)
```

The following declaration has labelled defaults, an array variadic tail, and
an imported effect reference:

```vibra
(import io @std.io)
(import log @std.log)

(defn write-log (message str) void
  labelled: (level atom @info)
  variadic: (fields (array (tuple str str)))
  visibility: @public
  effects: (io.stdout)
  (log.write message level fields))
```

Omitted `effects:` always means `effects: ()`. This default is identical for
every `defn`, `lambda`, function type, and test; omission never requests effect
inference. The checker computes the effects performed by a body and requires
them to fit within its written or default-empty ceiling. An ordinary effectful
`defn`, `lambda`, or test MUST therefore write `effects:` explicitly. For a
`deffect` member, the owner root remains implicit and `effects: ()` means that
the operation performs no additional roots. `deffect` itself accepts no
`effects:` attribute. An operation's owner root and additive row together form
the row performed by each of its callers, so a caller writes both; the effects
chapter defines that propagation.

`deftype`, `defint`, and `deffect` are native AST forms. They MUST NOT be
parser desugarings into a generic definition node. A nested `impl` is likewise
a native child of its `defint`, not a generic call or a method-list desugaring.

An effect reference is a symbol resolved only in the effect namespace. An
unqualified symbol names an effect root in the current module; a dotted symbol
starts with an explicit import alias and names a root in that module. Thus
`io.stdout` resolves through `(import io @std.io)`. The resolved AST stores the
root's canonical nominal identity, not the alias spelling. An atom such as
`@std.io.stdout` in a source `effects:` row is an error and is never accepted
as an alternate reference syntax; it emits `@effect.invalid-reference`.

## External definitions

Vibra code calls external behavior through ordinary typed functions. A
toolchain-owned declaration binds a signature to one of exactly two providers.
`external:` and `symbol:` MUST occur together, and such a declaration MUST omit
its body:

```vibra
(defn add-checked (left i32 right i32) (result i32 overflow)
  visibility: @public
  external: @compiler
  symbol: "integer.add-checked")
```

`@compiler` selects a pure compiler intrinsic. Such a declaration MUST have an
empty effect ceiling. `@host` selects a typed host operation and is valid only
on a member of the `deffect` that owns the operation's nominal effect root. A
`@host` external member MUST have no additive effects. An interface contract
cannot be external.

The provider and string symbol are checked against closed, versioned toolchain
registries. External declarations are accepted only in toolchain-signed
standard-library modules; ordinary packages cannot declare them. Every
external function is applied through its declared Vibra name and ordinary
function-application syntax—there is no `external`, `primitive`, or `host-op`
expression. V1 has no WebAssembly external provider and no source-level Wasm
FFI.

## Types, interfaces, and methods

Every `defn` name is one unqualified `local-name`, whether the declaration is
module-level or nested. The enclosing form names the owner, so a nested name
never repeats it. An implementation is written as an `impl` block whose
positional target supplies whichever half of the interface/type pair its parent
does not.

```vibra
(defint printable
  visibility: @public
  (defn render (value self) str))

(deftype user
  (record name str id user-id)
  visibility: @public
  (defn name-length (value self) u64
    (text.length (value @name)))
  (impl printable
    (defn render (value self) str
      (value @name))))
```

The package that owns an interface may implement it for a type declared in
another module by placing an `impl` block in the owning `defint`:

```vibra
(defint printable
  (defn render (value self) str)
  (impl i32
    (defn render (value self) str
      (integer.to-str value))))
```

`impl` is valid only as a direct child of the `deftype` that owns the type or
the `defint` that owns the interface; there is no top-level `impl` and no `for:`
implementation attribute. A contract member has no body. A method implementing
an interface has a body and MUST match its contract after substituting the
receiver type for `self`. The type chapter defines completeness, conflict, and
ownership rules.

A declaration name and a reference are distinct. A name is always one segment
in its owner's scope; a reference is a dotted path through owners, so
`user.name-length` and `printable.render` are resolved paths at a use site and
never the spelling of a declaration.

Records are constructed by applying their nominal type to labelled fields.
Enum tags and newtypes expose qualified constructors. A record value is
applied to an atom selector to read one statically known field; there is no
`field` form and no generated source accessor function.

```vibra
(user name: "Ada" id: (user-id 1))
(option.some "value")
(person @name)
```

## Functions and expressions

A named function declares a name, a flat parameter list, and a result type.
Function bodies are direct expression sequences; their final expression is the
result. `lambda` declares an anonymous function with the same parameter,
result, labelled, variadic, and effect syntax, but no name or visibility.
`fn` is reserved for function types and is never an anonymous declaration.

```vibra
(lambda (value i32) i32
  (integer.increment value))
```

```ebnf
expr = atom | application | lambda
     | "(", "do", { expr }, ")"
     | "(", "let", pattern, expr, { expr }, ")"
     | "(", "if", expr, expr, expr, ")"
     | "(", "match", expr, pattern, expr, { pattern, expr }, ")"
     | "(", "while", expr, expr, ")"
     | "(", "for", pattern, expr, expr, ")"
     | "(", "break", ")" | "(", "continue", ")"
     | "(", "return", [ expr ], ")"
     | "(", "try", expr, ")" ;
application = "(", expr, { expr }, ")" ;
lambda = "(", "lambda", parameters, type-expr,
         { lambda-attribute }, { expr }, ")" ;
pattern = binding-name | literal
        | "(", symbol, { pattern | label, pattern }, ")"
        | "(", "tuple", { pattern }, ")"
        | "(", "array", { pattern }, ")" ;
```

Reserved forms are recognized before the general application production. A
list headed by `let`, for example, cannot be reinterpreted as application of a
value named `let`.

`let` introduces immutable bindings for the remainder of its form. There is no
assignment or shared mutable state in v1. `let`, `for`, and positional
function or lambda parameters accept patterns, but each such binding pattern
MUST be irrefutable for its expected type. `match` accepts refutable patterns.

A bare unqualified local name in a pattern always introduces a binding; it
never compares against an existing value. A dotted symbol in a pattern names a
constructor or other resolved entity and never introduces a local. Every named
binder MUST NOT shadow a visible name, and the same pattern MUST NOT bind one
name more than once. A discard never binds and may be used as
`(let - expression)`, `(let @- expression)`, or `(let -: expression)` to state
that a result is intentionally ignored. Repeating a discard is always valid
because it creates no declaration to redeclare or shadow. There is no `bind`
pattern form and no compatibility spelling for it.

```vibra
(let (tuple name id) pair
  (text.concat name (integer.to-str id)))

(for (tuple key value) entries
  (visit key value))

(lambda ((tuple left right) (tuple i32 i32)) i32
  (integer.add left right))
```

`while` and `for` exist for local iteration; mutation needed by their library
implementation is compiler/runtime internal. Collections have immutable value
semantics. Source values use the closed, pure constructor entities `tuple.of`,
`array.of`, and `map.of`; the unqualified `tuple`, `array`, and `map` forms are
reserved for types and patterns. There is no source collection-literal form
and no `entry` wrapper.

```vibra
(tuple.of "Ada" 42u64)
(array.of 1i32 2i32 3i32)
(map.of "name" "Ada" "role" "maintainer")
```

`tuple.of` may contain heterogeneous values. Every `array.of` element has one
exact type. `map.of` contains alternating key and value expressions and MUST
have even arity; its keys share one exact type and its values share one exact
type. An empty `array.of` or `map.of` requires an expected collection type.
All constructor operands are evaluated, and a later duplicate map key replaces
the earlier value. These names are closed native constructor entities, not
ordinary user declarations and not overloadable qualified functions.

`match` contains alternating pattern/result forms directly; there is no `case`
wrapper. Every arm has exactly one result expression, so `do` groups multiple
expressions. Arms are checked for reachability and exhaustiveness.

```vibra
(match value
  (option.some text) (text.length text)
  (option.none) 0)
```

A tuple pattern has exact arity. A named-record pattern uses labelled fields;
omitted fields are ignored. Array patterns have exact length. Irrefutability is
a type-system property: tuple and record patterns may be irrefutable when all
of their subpatterns are, while a fixed-length array pattern is refutable for
the variable-length array type. Enum constructors are normally refutable, but
the exhaustiveness checker may prove a constructor pattern irrefutable for a
single-variant type.

`try` propagates the error of `result` or the absence of `option` through a
function returning the same container and error type. There are no exceptions
or implicit error conversions. A fallible value MUST be matched, propagated,
returned, bound for later use, or explicitly ignored with a discard binding.

## Canonical format

`vibra fmt` defines the only canonical representation:

- UTF-8, LF endings, and one trailing newline;
- two-space indentation and no trailing whitespace;
- one blank line between top-level forms;
- `\newline`, `\return`, `\space`, and `\tab` for their four characters,
  uppercase four-digit `\uNNNN` spelling for every other control or whitespace
  scalar in the Basic Multilingual Plane, and direct EDN character spelling for
  every remaining scalar;
- adjacent lowercase numeric suffixes, preserved exactly when written;
- leaf lists on one line when they fit within 88 columns;
- declaration headers before labelled attributes and bodies;
- fixed, labelled, then variadic function or constructor operands;
- one pattern/result arm per line in a multiline `match`; and
- preserved comments attached to the following form when possible.

Formatting MUST be idempotent and semantics-preserving. The formatter MAY
normalize recoverable presentation but MUST NOT guess through a syntax,
binding, or type ambiguity.
