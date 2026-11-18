# Current Vibra: construct inventory

Status: factual inventory of docs/spec chapters 01 to 04; working notes, not normative

File names in the `Where` column: `01` = `01-source-language.md`, `02` = `02-type-system.md`, `03` = `03-effects.md`, `04` = `04-programs-and-packages.md`. Line numbers are those of the files as read on 2026-11-16.

## Lexical

| Id | Construct | Spelling in current Vibra | Rule in one or two sentences | Where |
| --- | --- | --- | --- | --- |
| LEX-01 | File encoding and forms | `(import io @std.io)` | A `.vib` file is UTF-8 with zero or more forms; parentheses delimit lists and whitespace separates tokens. | 01-source-language.md:15 |
| LEX-02 | Absent reader features | (none) | No reader macros, quote syntax, commas, brackets, braces, raw strings, heredocs, or interpolation. | 01-source-language.md:17 |
| LEX-03 | Line comment | `; text` | `;` starts a comment that runs to end of line; comments are trivia and the formatter preserves them. | 01-source-language.md:16, 860 |
| LEX-04 | String literal | `"hello"` | Double quoted; escapes `\"`, `\\`, `\n`, `\r`, `\t`, `\u{HEX}` (1 to 6 hex digits, no surrogates, max U+10FFFF). | 01-source-language.md:80 |
| LEX-05 | String errors | `"\q"` | Unsupported escape or bad `\u{}` is `@syntax.invalid-string-literal`; an unterminated string is `@syntax.unmatched-delimiter`. | 01-source-language.md:82 |
| LEX-06 | Character literal | `\c`, `\newline`, `c` | EDN spelling: backslash plus one non-whitespace scalar, or `newline`/`return`/`space`/`tab`, or `u` plus exactly four hex digits; denotes one Unicode scalar. | 01-source-language.md:90 |
| LEX-07 | Character token boundaries | `\newline-x` (invalid) | The whole non-delimited token belongs to the character literal; a backslash followed by whitespace is invalid. | 01-source-language.md:95 |
| LEX-08 | Integer literal | `42`, `-1`, `1u8`, `25i32` | Signed decimal; optional suffix `i8 i16 i32 i64 u8 u16 u32 u64` fixes the exact type; unsuffixed gets its type from context. | 01-source-language.md:100 |
| LEX-09 | Float literal | `2.5`, `1e3`, `2f64`, `2.5f32` | Contains a decimal point, an exponent, or an `f32`/`f64` suffix; suffix fixes the type. | 01-source-language.md:36, 100 |
| LEX-10 | Numeric restrictions | `-1u8` | No separators, non-decimal bases, `isize`, or `usize`; range is checked by the type checker, so `-1u8` is one literal rejected as out of range. | 01-source-language.md:106 |
| LEX-11 | Minus sign vs discard | `-1` vs `-` | `-` followed immediately by a digit starts a number; a standalone `-` is the discard symbol. | 01-source-language.md:110 |
| LEX-12 | `void` literal | `void` | The single value of type `void`, also the type name; there is no `unit` literal. | 01-source-language.md:114 |
| LEX-13 | Symbol | `a`, `a--b`, `a.b2-c` | One or more dot-separated kebab segments: lowercase ASCII start, then lowercase, digit, or hyphen; no `?`, `!`, `_`, `/`, uppercase. | 01-source-language.md:120 |
| LEX-14 | Dotted symbol | `user.name-length` | A qualified name (path through owners or an import alias); never field-access syntax. | 01-source-language.md:125, 523 |
| LEX-15 | Atom name | `@ok`, `@std.io` | `@` plus a symbol; an ordinary value of type `atom` unless the position expects an entity reference. | 01-source-language.md:45, 174 |
| LEX-16 | Label | `level:` | A symbol followed by `:`; consumes the one following form. | 01-source-language.md:47, 189 |
| LEX-17 | Discard | `-`, `@-`, `-:` | Three equivalent spellings; create no binding, may repeat, are rejected in every non-discard position and cannot be namespaced (`-.x` invalid). | 01-source-language.md:136 |
| LEX-18 | Boolean values | `true`, `false` | Not literals in source; ordinary symbols naming the public values of `@std.bool`, part of the prelude. In `.vibon` they are literals. | 01-source-language.md:149 |
| LEX-19 | Reserved heads | `do let let-else if match return as try lambda tupleof recordof enumof` | Recognized before application; cannot be bound as names. Other reserved form heads: `dict`, `array`, `fn`, `tuple`, `record`, `enum`, `union`, `intrinsic-type`. | 01-source-language.md:161, 02-type-system.md:252 |
| LEX-20 | Non-rebindable names | `any`, `never`, `option`, `result`, `iter`, `equatable`, `ordered`, `array`, `dict`, primitive names | A binder spelled as keyword, `any`, or prelude name is `@name.reserved-declaration`; a module value, function, or alias so spelled is `@name.reserved-value-spelling`. | 01-source-language.md:156, 02-type-system.md:683 |
| LEX-21 | Unqualified `local-name` | `name-length` | Declaration names are one unqualified kebab segment; no declaration spells its own path. | 01-source-language.md:475, 02-type-system.md:961 |
| LEX-22 | Invalid name diagnostic | `a?` | A nonliteral leaf failing the symbol, label, atom, or discard production is `@syntax.invalid-name`. | 01-source-language.md:129 |
| LEX-23 | Application head | `((choose f cond) arg)` | Any form, including a list, may head a list; a nonempty executable list is an application classified from its statically resolved callee. | 01-source-language.md:193 |
| LEX-24 | Empty list | `()` | Valid only where a grammar admits it (empty parameter list, empty effect row); never an executable application. | 01-source-language.md:196 |

## Top-level declarations

| Id | Construct | Spelling in current Vibra | Rule in one or two sentences | Where |
| --- | --- | --- | --- | --- |
| TOP-01 | Closed top-level set | `import deftype defint deffect def defn test` | These are exactly the user top-level forms. | 01-source-language.md:250 |
| TOP-02 | `import` | `(import io @std.io)` | One alias symbol plus one atom entity reference; resolved entity must be a module or one public top-level declaration. | 01-source-language.md:254, 04-programs-and-packages.md:330 |
| TOP-03 | `deftype` | `(deftype user-id u64 visibility: @public)` | Declares a type with a body type expression, then attributes, nested methods, and `impl` blocks. | 01-source-language.md:255 |
| TOP-04 | `defint` | `(defint printable (defn render (value self) str))` | Declares an interface of method signatures over `self`, with attributes, members, and `impl` blocks. | 01-source-language.md:258 |
| TOP-05 | `deffect` | `(deffect read (defn file ...))` | Declares a nominal effect root with operation members. | 01-source-language.md:261, 03-effects.md:44 |
| TOP-06 | `def` | `(def default-retries u8 3u8)` | Immutable module value: name, type, expression, optional attributes; no separate `const`. | 01-source-language.md:263, 326 |
| TOP-07 | `defn` | `(defn greet (name str) str ...)` | Name, parameter list, result type, function attributes, body. | 01-source-language.md:265 |
| TOP-08 | `test` | `(test "name" body...)` | String name, optional `effects:` row, one or more body elements. | 01-source-language.md:267 |
| TOP-09 | Native AST forms | `deftype`, `defint`, `deffect`, nested `impl` | MUST NOT be parser desugarings into a generic definition node. | 01-source-language.md:413 |
| TOP-10 | Top-level spelling uniqueness | two forms named `x` | One top-level form may not reuse a spelling declared by another in the module, across all four namespaces. | 02-type-system.md:615 |
| TOP-11 | Forward reference | `defn` calling a later `defn` | Forward reference within a module is permitted; same-module mutual recursion among module-level `defn`s is valid. | 02-type-system.md:721 |
| TOP-12 | Visibility | `visibility: @public` | All declarations are private unless they contain `visibility: @public`; visibility is an attribute, not a wrapper. | 01-source-language.md:366 |
| TOP-13 | `doc:` attribute | `doc: "text"` | A string attribute accepted by every declaration kind and by effect/interface members; canonical position is last. | 01-source-language.md:296, 308, 354 |
| TOP-14 | Attribute rules | `where: ... visibility: ... doc: ...` | Attributes may be parsed in any unambiguous order, occur at most once, are formatted canonically; nested methods follow attributes and `impl` blocks follow methods. | 01-source-language.md:356 |
| TOP-15 | Function attributes | `where: labelled: variadic: visibility: effects: external: symbol: native: doc:` | Canonical order; `lambda` uses the same relative order for its subset. | 01-source-language.md:300, 353 |
| TOP-16 | Type attributes | `where: role: visibility: doc:` | Canonical order for `deftype`/`defint`. | 01-source-language.md:297, 356 |
| TOP-17 | `role:` attribute | `role: @bool` | Names the language role a standard-library type plays; embedded standard library only. | 01-source-language.md:321, 02-type-system.md:77 |
| TOP-18 | `native:` attribute | `native: "str.concat"` | Names a toolchain native for a library function that keeps its Vibra body; embedded standard library only. | 01-source-language.md:321 |
| TOP-19 | `external:` and `symbol:` | `external: @compiler symbol: "i32.add-checked"` | Occur together; body omitted; providers are `@compiler` (pure, empty row) and `@host` (only on a `deffect` member); embedded stdlib only. | 01-source-language.md:425, 439 |
| TOP-20 | `intrinsic-type` body | `(deftype i32 (intrinsic-type @i32))` | Binds a declaration to a builtin type identity; embedded standard library only. | 02-type-system.md:404 |
| TOP-21 | Attribute placement rule | header, then labelled attributes, then body | A label never interrupts a fixed header; labelled forms follow fixed positional forms and precede variadic body or member forms. | 01-source-language.md:360 |
| TOP-22 | Canonical formatting | `vibra fmt` | The only canonical representation: LF, two-space indent, 88 columns, one blank line between top-level forms, fixed/labelled/variadic operand order. | 01-source-language.md:792 |

## Types

| Id | Construct | Spelling in current Vibra | Rule in one or two sentences | Where |
| --- | --- | --- | --- | --- |
| TYP-01 | Primitive type names | `bool void never char str bytes atom i8 i16 i32 i64 u8 u16 u32 u64 f32 f64` | The listed primitive names; `bool`, `str`, `bytes` are standard-library types playing roles, not compiler-owned. | 02-type-system.md:27, 49 |
| TYP-02 | Compiler-owned core | `void atom char i8..u64 f32 f64 never (array t) fn tuple record enum union` | Everything else is an ordinary `deftype` in the embedded standard library. | 02-type-system.md:36 |
| TYP-03 | No implicit conversions | (none) | No null, truthiness, implicit numeric widening, or implicit string conversion; no platform-sized numerics. | 02-type-system.md:137, 188 |
| TYP-04 | Numeric ranges and floats | `i32`, `u64`, `f32` | `iN` and `uN` ranges are fixed; floats are IEEE 754; no source infinity or NaN; arithmetic is checked and returns typed results. | 02-type-system.md:190, 1351 |
| TYP-05 | `atom` and singleton atom types | `@ok` has singleton type, widens to `atom` | Every written atom has a singleton type that widens to `atom`. | 02-type-system.md:136, 1545 |
| TYP-06 | Tuple type | `(tuple i32 str)` | Zero or more component types; immutable. | 02-type-system.md:244, 289 |
| TYP-07 | Record type | `(record name str id u64)` | One or more distinct field name/type pairs; closed fields. | 02-type-system.md:245, 281 |
| TYP-08 | Enum type | `(enum some t none void)` | One or more variant name/type pairs; payload `void` means nullary variant; generic payload instantiated to `void` is nullary. | 02-type-system.md:247, 284 |
| TYP-09 | Union type | `(union i32 f32)` | Two or more concrete member types, no member names; members pairwise non-unifiable; no nested unions, interfaces, or bare generic parameters. | 02-type-system.md:443, 449, 454 |
| TYP-10 | Anonymous union members with generics | `(union (tuple t i32) str)` | An anonymous union member may not name a generic parameter anywhere (`@type.union-member-not-concrete`). | 02-type-system.md:462 |
| TYP-11 | Applied type | `(array t)`, `(dict k v)`, `(option t)` | `(type-name type-expr+)`; supplies the complete generic parameter list; wrong count is `@type.type-argument-mismatch`. | 02-type-system.md:240, 274 |
| TYP-12 | Function type | `(fn (i32 str) bool labelled: (a i32) variadic: (array i32) effects: (io.stdout))` | Positional types, result, optional labelled names/types, variadic type, closed effect row; defaults not in type; omitted row is empty; types equal only when rows equal. | 02-type-system.md:254, 260 |
| TYP-13 | Function types never generic | `(fn (t) t)` with generic `t` binder on the type | Function types themselves are never generic. | 02-type-system.md:832 |
| TYP-14 | Wrapper type | `(deftype celsius f64)` | A `deftype` whose body is not a structural form declares a distinct identity over one representation; no implicit conversion; no `newtype` form. | 02-type-system.md:395 |
| TYP-15 | Nominal identity | `(deftype pair (tuple i32 str))` | Every `deftype` is a new identity even with identical structure; there are no transparent aliases. | 02-type-system.md:230 |
| TYP-16 | Anonymous structural identity | `(tuple i32 str)` outside `deftype` | Identity is structure (record fields and enum variants unordered; union members unordered); an anonymous type has no owner, methods, or `impl`. | 02-type-system.md:291, 307 |
| TYP-17 | Canonical order of anonymous types | fields sorted by UTF-8 name bytes | Fixes discriminants, rendering, key order; formatter sorts anonymous types unless they contain a comment; declared types keep declaration order. | 02-type-system.md:297 |
| TYP-18 | Recursive types | `(deftype tree (enum leaf void node (array tree)))` | Recursion needs a `deftype` name; allowed through array, dict, or function type; direct infinite expansion is `@type.infinite-size`. | 02-type-system.md:309 |
| TYP-19 | Structural types in every type position | `(tuple ...)` as parameter, result, field, payload, member, `def` annotation, `as` type, `types:` argument | Each structural form MUST be accepted wherever a type is. | 02-type-system.md:270 |
| TYP-20 | `any` | `(t any)`, `(values any)` | Predeclared empty interface; as a bound constrains nothing; as a type it is an interface value that erases the type and can only be passed along. | 02-type-system.md:900, 939 |
| TYP-21 | `never` | `(defn fail (m str) never ...)` | Uninhabited type; writable wherever a type is, including generic arguments such as `(result t never)`. | 02-type-system.md:141, 1449 |
| TYP-22 | `void` | `(defn f () void ...)` | Type with one value `void`; result of functions with no meaningful value; `(return void)`. | 02-type-system.md:134, 01-source-language.md:114 |
| TYP-23 | Interface value type | `(iter item)`, `printable` in type position | A symbol in type position resolves across type and interface namespaces; interface values are reached by explicit widening. | 02-type-system.md:891 |
| TYP-24 | `self` type | `(defn render (value self) str)` | Names the receiver type inside a contract, `deftype` method, or `impl`; not visible in module-level functions; may not be a generic name. | 02-type-system.md:1064, 440 |
| TYP-25 | Generic declaration | `where: (t any u printable)` | Flat generic-name/bound pairs on `deftype`, `defint`, `defn`, nested method, `lambda`; the only declaration site of generic names; one bound per parameter. | 01-source-language.md:344, 02-type-system.md:783 |
| TYP-26 | Generic inheritance | nested method inside generic `deftype` | A nested method inherits its owner's generic names, a lambda those of all enclosing declarations; redeclaring is `@name.generic-redeclaration`. | 01-source-language.md:349, 02-type-system.md:812 |
| TYP-27 | Generic invariance and exclusions | `(array i32)` vs `(array number)` | Generic arguments are invariant; no partial application, named type arguments, specialization by value, multiple bounds, or runtime type tests. | 02-type-system.md:795 |
| TYP-28 | Bound rules | `where: (t iter)` (error) | A bound is one nominal interface name (no applied bound); each type argument MUST implement its bound (`@type.unsatisfied-bound`). | 02-type-system.md:801, 806 |
| TYP-29 | `types:` call-site list | `(ring.empty types: (str))` | Supplies every type argument in `where:` order (type's parameters then the method's own); length mismatch is `@type.type-argument-mismatch`; reserved label (`@name.reserved-label` for a parameter named `types`). | 02-type-system.md:795, 857 |
| TYP-30 | Dict key admissibility | `(dict str i32)` | Keys must implement `ordered`; closed key set (bool, char, str, bytes, atom, integers, structural of keys); `f32`, `f64`, `fn`, arrays, dicts, `option`, `void`, `never` not admissible (`@type.invalid-dict-key`). | 02-type-system.md:317, 357, 378 |
| TYP-31 | Overlap rule | `(union (array t) (array i32))` | Union members, applied interface targets, and `from`/`try-from` sources of one receiver must be pairwise non-unifiable, bound-agnostic. | 02-type-system.md:198 |
| TYP-32 | Type namespaces | type, value, interface, effect | A module has four namespaces, but one flat top-level spelling space; position selects the namespace. | 02-type-system.md:615 |
| TYP-33 | Reserved type heads | `tuple record enum union intrinsic-type fn` | The head of an applied type is any symbol except these six; a `deftype`, `defint`, or generic name spelled so is `@name.reserved-declaration`. | 02-type-system.md:251, 431 |
| TYP-34 | Language roles | `role: @bool @str @bytes @option @result @dict @iter @equatable @ordered` | Closed set of compiler-known positions filled by library types; each declared exactly once. | 02-type-system.md:62 |
| TYP-35 | Prelude | `true false bool str option result iter ...` | Closed, spec-fixed set of declarations visible in every module without import; cannot be shadowed or extended. | 02-type-system.md:109 |
| TYP-36 | Builtin standard types | `bool`, `str`, `bytes`, `dict`, `option`, `result`, `ordering`, `conversion-error` | Ordinary `deftype`s: `bool` is `(enum false void true void)`, `str` is `(array char)`, `bytes` is `(array u8)`, `dict` is sorted `(array (tuple k v))`. | 02-type-system.md:49, 83, 102 |
| TYP-37 | Uninhabited types | `(result t never)` | A type is uninhabited when `never`, tuple/record with an uninhabited field, wrapper of one, or enum/union with all payloads/members uninhabited. | 02-type-system.md:1487 |
| TYP-38 | Host values | (none) | No `resource` type, host handle scope, or close protocol; host operations take and return ordinary values. | 02-type-system.md:1720 |
| TYP-39 | Value semantics | (none) | Bindings and values are immutable; no references, pointers, object identity, destructors, or shared mutable cells. | 02-type-system.md:1728 |

## Construction and projection

| Id | Construct | Spelling in current Vibra | Rule in one or two sentences | Where |
| --- | --- | --- | --- | --- |
| CON-01 | Declared record construction | `(user name: "Ada" id: (user-id 1))` | Apply the type name to all fields as labelled operands. | 02-type-system.md:538, 01-source-language.md:536 |
| CON-02 | Declared tuple construction | `(pair 1 "x")` | Apply the type name with one positional operand per component in order. | 02-type-system.md:539 |
| CON-03 | Enum constructor | `(option.some "value")`, `(option.none)` | Qualified `type.variant`; zero operands for `void` payload, else one operand. | 02-type-system.md:540, 01-source-language.md:537 |
| CON-04 | Union injection | `(number 1i32)` | Apply the declared union to one operand whose type is exactly one member; ambiguous operand is `@type.ambiguous-inference`. | 02-type-system.md:541, 544 |
| CON-05 | Wrapper construction | `(celsius 21.5f64)` | Apply the wrapper name to one operand of the representation type. | 02-type-system.md:542 |
| CON-06 | Wrapper unwrapping | pattern `(celsius c)` | The unwrapping constructor pattern reads the representation; constructor and pattern are available only where visibility permits. | 02-type-system.md:398, 1339 |
| CON-07 | `str`/`bytes` wrap and unwrap | `(str scalars)`, `(bytes items)` | The wrapper constructor and pattern are the only conversion between `str` and `(array char)` or `bytes` and `(array u8)`. | 02-type-system.md:88 |
| CON-08 | Bool construction | `(bool.true)`, `(bool.false)` | Variants of the `bool` enum; `true` and `false` are module values of the same. | 02-type-system.md:93 |
| CON-09 | Anonymous tuple | `(tupleof "Ada" 42u64)` | Reserved form; type is the operand types in order; evaluates left to right. | 01-source-language.md:710, 02-type-system.md:557 |
| CON-10 | Anonymous record | `(recordof name: "Ada" id: 42u64)` | Reserved form; labels must be distinct. | 01-source-language.md:711, 02-type-system.md:558 |
| CON-11 | Anonymous enum | `(enumof some: 1i32)` | Reserved form selecting one variant; MUST be checked against a written expected anonymous enum type or it is `@type.ambiguous-inference`; payloadless is `(enumof a: void)`. | 02-type-system.md:560 |
| CON-12 | Anonymous union | `(as number 1i32)` | An anonymous union has no constructor; values reach it by widening at a written boundary or through `as`. | 02-type-system.md:563 |
| CON-13 | Array construction | `(array.of 1i32 2i32 3i32)` | Static method of builtin `array`; one exact element type; empty call needs an expected type. | 01-source-language.md:712, 02-type-system.md:575 |
| CON-14 | Dict construction | `(dict.of "name" "Ada" "role" "x")` | Alternating key/value operands, even arity; later duplicate key replaces earlier; keys and values each share one exact type. | 01-source-language.md:713, 716 |
| CON-15 | No other literals | (none) | No other collection-literal form and no `entry` wrapper; `array.of` and `dict.of` are first-class function values, `tupleof` and `recordof` are not. | 01-source-language.md:706, 721 |
| CON-16 | Tuple projection | `(pair 0)` | Apply a tuple value to an unsuffixed decimal index literal, checked at compile time against arity; no `value.0`; not from a variable. | 02-type-system.md:499, 509 |
| CON-17 | Record projection | `(person @name)` | Apply a record value to an unqualified atom selector naming a visible field; no `field` form, no generated accessor. | 02-type-system.md:500, 512, 01-source-language.md:532 |
| CON-18 | Array lookup | `(items 0u64)` | Apply an array to a `u64`; returns `(option t)`; out of bounds never traps. | 02-type-system.md:501, 518 |
| CON-19 | Dict lookup | `(table "key")` | Apply a dict to a key of exact type `k`; returns `(option v)`. | 02-type-system.md:502 |
| CON-20 | String and bytes lookup | `(text 0u64)` | `str` index is a `u64` scalar index returning `(option char)`; `bytes` returns `(option u8)`. | 02-type-system.md:503, 520 |
| CON-21 | Projection/lookup arity | `(r @a @b)` (error) | Exactly one unlabelled operand, no labelled or variadic operands; pure; contributes no effect or call edge. | 02-type-system.md:506, 521 |
| CON-22 | Application kinds | `@function @tuple-projection @record-projection @collection-lookup @constructor` | The closed set of applicable categories; enums, unions, atoms, numbers, `void`, wrapper values, and unconstrained generics are not applicable. | 02-type-system.md:492, 525 |
| CON-23 | Same shape, static classification | `(value @name)` | A function call when `value` has a matching `fn` type, a record projection when it has a record type; literal-headed `(1 2)` not applicable. | 01-source-language.md:234 |
| CON-24 | Not `fn` values | lambda needed | Constructors, projections, lookups, and enum tags are not function values; a higher-order use needs an explicit `lambda`. | 02-type-system.md:703, 530 |
| CON-25 | Conversion call | `(from.convert 42u8)` | Destination-dispatched interface call; destination from the written expected type. | 02-type-system.md:1630 |
| CON-26 | Fallible conversion | `(try-from.convert value)` | Returns `(result self conversion-error)`; `conversion-error` is `(enum out-of-range void invalid-format void unrepresentable void)`. | 02-type-system.md:1643, 1683 |
| CON-27 | Conversion interfaces | `(impl (from f64) (defn convert (value f64) self ...))` | `from` and `try-from` are generic over the source, implemented on the destination; no `into`; both pure (`effects: ()`). | 02-type-system.md:1637, 1675 |
| CON-28 | Builtin numeric conversions | `(as u32 (from.convert 42u8))` | Between integers: `from` if the destination holds every source value, else `try-from`; every number implements `(try-from str)`; `str` implements `(from n)`; floats convert through no contract; no `to-U`, `parse`, `to-str`. | 02-type-system.md:1699 |
| CON-29 | Conversion overlap | `(from t)` with `(try-from i32)` | A receiver's `from`/`try-from` sources must be pairwise non-unifiable (`@type.redundant-conversion`). | 02-type-system.md:1713 |
| CON-30 | `as` expression | `(as number 1i32)` | Static ascription: no-op, widening, or constraining an ambiguous inference; never narrows, never converts, no runtime form. | 01-source-language.md:756, 02-type-system.md:1598 |
| CON-31 | Three widening relations | concrete to interface, member to union, atom singleton to `atom` | The only widenings; each applies once at a written expected type, never inside a container, never chained. | 02-type-system.md:1545, 1593 |
| CON-32 | Written expected types | parameter, result, `def`, field/payload/representation, `types:`, `as` | The complete list of positions where widening fires; no least upper bound is computed. | 02-type-system.md:1564, 1577 |
| CON-33 | Method call by path | `(user.name-length value)`, `(printable.render value)` | A method is applied through a dotted path naming the entity; no receiver-first syntax, no method-call operator. | 02-type-system.md:1069 |

## Patterns and destructuring

| Id | Construct | Spelling in current Vibra | Rule in one or two sentences | Where |
| --- | --- | --- | --- | --- |
| PAT-01 | Binder pattern | `name` | A bare unqualified local name always introduces a binding; never compares. | 01-source-language.md:636, 02-type-system.md:1342 |
| PAT-02 | Discard pattern | `-`, `@-`, `-:` | Binds nothing; repeatable. | 01-source-language.md:282, 642 |
| PAT-03 | Literal pattern | `0`, `"a"`, `@ok`, `\c` | Admitted for every primitive except `f32`, `f64`, `void`, `never`; literal arm sets never close a type. | 01-source-language.md:582, 02-type-system.md:1265 |
| PAT-04 | Constructor pattern | `(option.some head)`, `(user name: n)`, `(pair a b)` | `(symbol pattern-or-label-pattern...)`; a dotted/constructor symbol never binds a local. | 01-source-language.md:583, 638 |
| PAT-05 | Tuple pattern | `(tupleof left right)` | Exact arity; a declared tuple uses `(z p...)`. | 01-source-language.md:584, 735 |
| PAT-06 | Record pattern | `(recordof name: n)` | Labelled fields; omitted fields ignored; declared records use `(z a: p)`. | 01-source-language.md:585, 736 |
| PAT-07 | Enum pattern | `(enumof a: p)` | Mirrors the `enumof` constructor. | 01-source-language.md:586, 02-type-system.md:567 |
| PAT-08 | Array pattern | `(array a b)` | Exact length; refutable for the variable-length array type; never a value constructor. | 01-source-language.md:587, 737, 02-type-system.md:586 |
| PAT-09 | Narrowing pattern | `(as i32 n)` | Matches one member of a union and binds the payload at that type; scrutinee must be a union (`@type.narrowing-non-union`) and the type a member (`@type.not-a-union-member`). | 01-source-language.md:588, 774, 02-type-system.md:1316 |
| PAT-10 | Constant pattern | `limit`, `limits.max`, `true` | A name that resolves to a module `def` with a constant initializer is a value pattern and binds nothing; adding such a `def` changes existing binder patterns of that name. | 02-type-system.md:1356, 01-source-language.md:169 |
| PAT-11 | Constant expression | literal, atom, `void`, constructor/`tupleof`/`recordof`/`enumof` of constants, constant `def` name | A function call is never constant; decided from the initializer, not by evaluation. | 02-type-system.md:1371 |
| PAT-12 | Pattern positions | parameters, `let`, `let-else`, `match` arms | Positional function and lambda parameters are written `pattern type` pairs. | 01-source-language.md:285, 329 |
| PAT-13 | Irrefutable required | `let`, positional parameters | The single pattern must cover every value of the expected type, else `@pattern.refutable-binding`. | 02-type-system.md:1331 |
| PAT-14 | Refutable required | `let-else` | An irrefutable pattern is `@pattern.irrefutable-let-else`. | 01-source-language.md:653, 02-type-system.md:1414 |
| PAT-15 | Refutability rules | tuple, record, wrapper, enum | Tuple/record/wrapper patterns are irrefutable when all subpatterns are; an enum constructor is refutable unless the enum has exactly that one inhabited variant. | 02-type-system.md:1336 |
| PAT-16 | Refutable `as` pattern | `(as i32 n)` | Valid in `match` and `let-else`; invalid in `let`, positional parameters, and lambda parameters when refutable. | 02-type-system.md:1324 |
| PAT-17 | Match exhaustiveness | `@pattern.non-exhaustive` | Checked over bools, closed atoms, enums, unions, finite structural patterns; the diagnostic names one uncovered shape using `-`. | 02-type-system.md:1258, 1273 |
| PAT-18 | Unreachable arm | `@pattern.unreachable-arm` | An arm covered by earlier arms or requiring an uninhabited payload is an error. | 02-type-system.md:1285 |
| PAT-19 | Atom scrutinee | `match` on `atom` | Type `atom` is never closed (needs a binder or discard); a singleton atom type is closed by its one arm. | 02-type-system.md:1262 |
| PAT-20 | Pattern binder rules | duplicate or visible name | Every named binder must not shadow a visible name and must not repeat within one pattern. | 01-source-language.md:639, 02-type-system.md:660 |
| PAT-21 | Dotted name in pattern | `limits.max` | A dotted symbol names a resolved entity (constructor or constant), never introduces a local. | 01-source-language.md:638, 02-type-system.md:1358 |

## Functions

| Id | Construct | Spelling in current Vibra | Rule in one or two sentences | Where |
| --- | --- | --- | --- | --- |
| FUN-01 | `defn` | `(defn greet (name str) str visibility: @public (str.concat "hello, " name))` | Name, flat parameter list of `pattern type` pairs, result type, attributes, body; final element is the result. | 01-source-language.md:265, 377 |
| FUN-02 | Parameter list | `(a i32 b i32)` | Flat pattern/type pairs with no wrapper list; each pattern irrefutable. | 01-source-language.md:285, 329 |
| FUN-03 | Destructuring parameter | `((tupleof left right) (tuple i32 i32))` | A parameter pattern may destructure when irrefutable. | 01-source-language.md:691 |
| FUN-04 | Labelled parameters | `labelled: (level atom @info)` | One flat list of name/type/default triples; real unqualified names; each default a constant expression of any type (`@type.not-constant` otherwise). | 01-source-language.md:286, 330 |
| FUN-05 | Labelled call | `(log.write "m" level: @info)` | Label then value; canonical order is fixed, then labelled in declaration order, then variadic; other order is a style diagnostic. | 01-source-language.md:226, 214 |
| FUN-06 | Reserved label `types:` | `labelled: (types ...)` | A declaration may not name a labelled parameter `types` (`@name.reserved-label`). | 02-type-system.md:857 |
| FUN-07 | Variadic parameter | `variadic: (fields (array (tuple str str)))` | Exactly one name/type pair; type must be `(array t)` or `(dict k v)`; at most one per function; any discard spelling allowed as name. | 01-source-language.md:288, 339 |
| FUN-08 | Array variadic call | `(collect-values v1 v2 v3)` | Every remaining unlabelled form is collected into one array. | 01-source-language.md:238 |
| FUN-09 | Dict variadic call | `(collect-fields k1 v1 k2 v2)` | Alternating key and value forms, even count, later duplicate key replaces. | 01-source-language.md:239 |
| FUN-10 | Defaults | `labelled: (n u8 3u8)` | Only labelled parameters have defaults; defaults belong to the function value, not its type. | 01-source-language.md:333, 02-type-system.md:264 |
| FUN-11 | `lambda` | `(lambda (value i32) i32 (i32.add value 1i32))` | Anonymous function: parameters, result, `where:`/`labelled:`/`variadic:`/`effects:`, body; no name or visibility; `fn` is never an anonymous declaration. | 01-source-language.md:553, 580 |
| FUN-12 | Lambda self-reference | lambda calling itself | A lambda has no self-name and MUST NOT refer to itself. | 02-type-system.md:723 |
| FUN-13 | Generic lambda | `(lambda (left t right t) t where: (t any) left)` | Generic like `defn`; stays generic when bound by `let`; instantiated from inference, `types:`, or an expected `fn` type. | 02-type-system.md:822, 834 |
| FUN-14 | Functions as values | `(let f greet)` | A module-level `defn` path or nested method path in expression position is a `fn` value. | 02-type-system.md:701, 01-source-language.md:541 |
| FUN-15 | Function application | `(f a b)` | When `f` has a function type; `(f)` is a nullary call; operands evaluated in fixed, labelled, variadic order. | 01-source-language.md:221, 230 |
| FUN-16 | Function value row | `(fn (i32) i32 effects: (io.stdout))` | A call through a function value contributes the row of its type; `fn` values are not `equatable` and not dict keys (`@type.function-not-equatable`). | 03-effects.md:170, 02-type-system.md:718 |
| FUN-17 | Atoms not applicable | `(@some-atom v)` | A static error. | 01-source-language.md:231 |
| FUN-18 | No call operator | (none) | No user-defined call operator or callable interface. | 01-source-language.md:211 |
| FUN-19 | Contract member as value | `printable.render` with expected `fn` type | Instantiated from a written expected `fn` type; otherwise `@type.ambiguous-inference`. | 02-type-system.md:707 |
| FUN-20 | Recursion | `(defn fact ...)` calling itself | Self and mutual recursion among module-level `defn`s is valid. | 02-type-system.md:721 |
| FUN-21 | Tail calls | call in tail position | A call in tail position MUST reuse the activation; non-tail recursion has no language depth limit. | 02-type-system.md:724 |
| FUN-22 | Return type for `void`/`never` | `(return void)` | A `void`-result function that returns early writes `(return void)`. | 01-source-language.md:667 |
| FUN-23 | Extern function | `(defn add-checked (l i32 r i32) (result i32 core.arithmetic-error) external: @compiler symbol: "i32.add-checked")` | Body-less binding to toolchain registry; embedded stdlib only; applied through its Vibra name. | 01-source-language.md:433, 462 |
| FUN-24 | Complete written signatures | public function, `def`, type parameter | Every public function, `def`, interface member, and effect operation has a complete written type; callers never need a callee body. | 02-type-system.md:777, 24 |
| FUN-25 | Inference limits | unsuffixed literal, empty `array.of` | Inference is local; ambiguity is `@type.ambiguous-inference`; inference never invents bounds, implementations, conversions, ceilings, or error conversions. | 02-type-system.md:733, 743 |
| FUN-26 | Entry function | `(defn main () (result void e))` | A binary target's `entry` names a module-level `defn` with no parameters returning `void` or `(result void e)`; need not be public or named `main`. | 04-programs-and-packages.md:276, 285 |

## Interfaces and methods

| Id | Construct | Spelling in current Vibra | Rule in one or two sentences | Where |
| --- | --- | --- | --- | --- |
| INT-01 | Interface declaration | `(defint printable (defn render (value self) str))` | Nominal set of method signatures over `self`; conformance always explicit. | 02-type-system.md:888 |
| INT-02 | Abstract member | `(defn next (value self) (option (tuple item self)))` | An interface member without a body. | 01-source-language.md:272, 517 |
| INT-03 | Default method | `(defn describe (value self) str "x")` in a `defint` | A contract member with a body (at least one expression); implementations must not redeclare it (`@type.default-override`). | 01-source-language.md:517, 02-type-system.md:1087 |
| INT-04 | Interface members cannot be external | `external:` in `defint` | An interface contract cannot be external. | 01-source-language.md:443 |
| INT-05 | `impl` inside `deftype` | `(deftype user ... (impl printable (defn render (value self) str ...)))` | The positional target is the interface; members are unqualified. | 01-source-language.md:486, 02-type-system.md:965 |
| INT-06 | `impl` inside `defint` | `(defint printable ... (impl i32 (defn render (value self) str ...)))` | The positional target is the concrete type; allowed when the package owns the interface; same-module target is `@type.redundant-implementation`. | 01-source-language.md:506, 02-type-system.md:987 |
| INT-07 | No top-level `impl` | (none) | No top-level implementation form, no `for:`, no `implements:`; `impl` is a native child only. | 01-source-language.md:513, 02-type-system.md:969 |
| INT-08 | Orphan rule | `impl` placement | An implementation can be written only where the package owns the type or the interface. | 02-type-system.md:984 |
| INT-09 | Impl target kinds | `deftype` target interface, `defint` target concrete type | The other kind is `@name.wrong-entity-kind`; an anonymous structural type or `never` target is the same error. | 02-type-system.md:974, 920 |
| INT-10 | Implementation completeness | every abstract member exactly once | Missing member is `@type.missing-abstract-member`; signature, labelled names, defaults, variadic shape must match after `self` substitution; extra or duplicate members are errors. | 02-type-system.md:1077, 1089 |
| INT-11 | Implementation effects | member's `effects:` | An implementation writes exactly its contract member's row; its performed row must be within it. | 03-effects.md:140 |
| INT-12 | Implementation identity | `(from i16)` and `(from i8)` on one receiver | A block is keyed by applied interface target plus receiver; a member by contract member, target, receiver; applied targets pairwise non-unifiable (`@type.overlapping-implementation`). | 02-type-system.md:995, 1010 |
| INT-13 | Ambiguous implementation | two candidates at a call site | `@type.ambiguous-implementation` rather than picking an order. | 02-type-system.md:1016 |
| INT-14 | Nested (static) method | `(deftype user ... (defn name-length (value self) u64 ...))` | A `defn` nested in a `deftype`; one unqualified name; may use `self`; may declare extra generics; reached by `type.method`. | 01-source-language.md:270, 486 |
| INT-15 | Static method without receiver | `(defn empty () (ring t) ...)` in a `deftype` | A nested method need not take `self`; called as `(ring.empty types: (str))`. | 02-type-system.md:845 |
| INT-16 | Member namespace | fields, variants, methods, effect operations | One flat namespace per owner; collision is `@name.member-collision`; interface implementations contribute no names. | 02-type-system.md:638 |
| INT-17 | Resolution paths | `user.name-length`, `printable.render`, `@app.m.user.name-length` | A reference is a dotted path through owners; a declaration name is one segment. | 01-source-language.md:523, 02-type-system.md:624 |
| INT-18 | `self` rule | `self` | Resolves to the declared type in a `deftype` or its `impl`, to the positional target in a `defint` `impl`; not visible in module-level functions. | 02-type-system.md:1064 |
| INT-19 | Receiver dispatch | contract member with fixed positional `self` | Implementation selected from the receiver value; variadic or labelled `self` does not qualify. | 02-type-system.md:1020 |
| INT-20 | Destination dispatch | `(defn empty () self)` | A member with no fixed `self` but `self` in its result is selected from the written expected type; no expected type is `@type.ambiguous-destination`. | 02-type-system.md:1027, 1034, 1044 |
| INT-21 | Undispatchable member | `(defn version () str)` in a `defint` | No fixed `self` parameter and no `self` in the result is `@type.undispatchable-contract-member`; no interface-level static member. | 02-type-system.md:1053 |
| INT-22 | Interface value calls | call through `(iter item)` | `self` in the result gives the interface value type; `self` in another parameter or a function-typed result is `@type.mismatch`; an interface value implements no interface. | 02-type-system.md:946, 955 |
| INT-23 | `any` satisfaction | every type | Every type satisfies `any` with no `impl`; `(impl any ...)` is unwritable; `never` satisfies only `any`. | 02-type-system.md:907, 915, 933 |
| INT-24 | No inheritance | (none) | No inheritance between concrete types, no operator overloading, no lifted union members; dispatch is static and closed-world. | 02-type-system.md:1098, 475 |
| INT-25 | Comparison interfaces | `(equatable.equal a b)`, `(ordered.compare a b)` | Prelude interfaces with laws; `ordered` compare returns `ordering`; dict keys follow `ordered`; no `hashable`. | 02-type-system.md:317, 326, 337 |
| INT-26 | `iter` interface | `(defint iter where: (item any) (defn next (value self) (option (tuple item self))))` | Abstract `next`; defaults `map filter skip take collect`, called as `(iter.map xs f)`; defaults must not be redeclared. | 02-type-system.md:1116, 1130, 1142 |
| INT-27 | `iter` user implementation | `(impl (iter u64) (defn next ...))` | Nested in a `deftype`; target must be the full application `(iter item)`; supplies only `next`. | 02-type-system.md:1239 |
| INT-28 | Closed `iter` registry | `array`, `dict`, `str`, `option` | Closed toolchain `iter` conformance; users cannot add methods or `impl` to `array` or `dict`; `result` and heterogeneous tuples are not iterable. | 02-type-system.md:1222, 1232, 1235 |
| INT-29 | Builtin members | `i32.add-checked`, `str.concat`, `array.of` | Builtin types own static methods reached through the type path with no import; users cannot declare members on builtin types. | 02-type-system.md:411, 585 |

## Control flow and failure

| Id | Construct | Spelling in current Vibra | Rule in one or two sentences | Where |
| --- | --- | --- | --- | --- |
| CTL-01 | `if` | `(if cond then else)` | Exactly three operands; condition must be `bool`; branches have one common type after `never` branches are skipped. | 01-source-language.md:571, 02-type-system.md:1249 |
| CTL-02 | `match` | `(match value (option.some t) (str.length t) (option.none) 0)` | Alternating pattern/result forms flat, no `case` wrapper; one result expression per arm; exhaustiveness and reachability checked. | 01-source-language.md:572, 725 |
| CTL-03 | `do` | `(do e1 e2)` | A body sequence of its own; empty `do` is `void`; its bindings end with it; used to group several expressions in an arm or branch. | 01-source-language.md:570, 114, 727 |
| CTL-04 | `let` | `(let x 1 y 2)` | One or more flat pattern/value pairs in a body sequence; names visible to later pairs and later elements; odd operand count is `@syntax.invalid-form`; type `void`. | 01-source-language.md:623 |
| CTL-05 | `let` value checking | `(let x 1)` | The value is checked with no written expected type, so a `let` never widens its value. | 02-type-system.md:1405 |
| CTL-06 | `let` position rule | `(f (let ...))` | A `let` or `let-else` is valid only as a direct element of a body sequence; elsewhere `@syntax.misplaced-binding`. | 01-source-language.md:610 |
| CTL-07 | Ignored result | `(let - (risky))` | A fallible value in a non-final body element must be bound, matched, passed, or discarded, else `@type.unhandled-fallible`; `option` is not fallible. | 01-source-language.md:644, 02-type-system.md:1307 |
| CTL-08 | `let-else` | `(let-else (option.some head) (items 0u64) (return 0i32))` | Exactly three operands; refutable pattern; fallback must have type `never`; pattern bindings visible to later elements only. | 01-source-language.md:651, 661 |
| CTL-09 | `return` | `(return expr)` | One operand; exits the innermost `defn`, nested method, or `lambda`; type `never`; operand checked at the written result type. | 01-source-language.md:667, 02-type-system.md:1426 |
| CTL-10 | `return` restrictions | `return` in tail position | A tail-position `return` is `@type.redundant-return`; outside a function body (`def` initializer, test not inside lambda) is `@type.invalid-return`. | 01-source-language.md:675, 02-type-system.md:1433, 1439 |
| CTL-11 | `try` | `(try expr)` | Operand `(option t)` or `(result t e)`, enclosing result must be the same container (same `e` for result); expression type is `t`; early exit with `none` or `err`. | 01-source-language.md:744, 02-type-system.md:1293 |
| CTL-12 | `try` restrictions | `try` in a test | Any other context or container is `@type.invalid-try`; always invalid in a test (result `void`); operand must be a value, not a diverging expression. | 02-type-system.md:1299, 1303 |
| CTL-13 | `option` | `(option.some v)`, `(option.none)` | Standard-library enum `(enum some t none void)`; absence is a value; used for all lookups. | 01-source-language.md:381, 02-type-system.md:1345 |
| CTL-14 | `result` | `(result.ok v)`, `(result.err e)` | Standard-library enum; fallible unless `e` is uninhabited; no exceptions or implicit error conversion. | 02-type-system.md:1307, 1345, 01-source-language.md:744 |
| CTL-15 | `never` expressions | call of a `never` function, `return` | Admitted at any expected type; allowed only as a body element, `if` branch, `match` arm result, `let-else` fallback, or final function expression; elsewhere `@type.unreachable-code`. | 02-type-system.md:1455, 1463, 1473 |
| CTL-16 | `never` function bodies | `(defn fail (m str) never (fail m))` | Final expression must be `never`; `never` is never inferred; an operand of type `never` fixes no generic argument. | 02-type-system.md:1458, 757 |
| CTL-17 | Unreachable following element | element after a `never` element | `@type.unreachable-code` at the following element. | 02-type-system.md:1480 |
| CTL-18 | Retired loops and assignment | `while for break continue` | Not forms; emit `@syntax.retired-form`; no assignment form; early exit is `return` or `try`. | 01-source-language.md:591 |
| CTL-19 | Checked arithmetic | `i32.add-checked` | Overflow, division by zero, invalid shifts, failed conversions return typed results; no wrap or trap. | 02-type-system.md:1351 |
| CTL-20 | Branch join | `(if c 1i32 2.0f32)` | Differing branch types are an error unless an enclosing written boundary names a union; no least upper bound. | 02-type-system.md:1255, 1578 |

## Collections and iteration

| Id | Construct | Spelling in current Vibra | Rule in one or two sentences | Where |
| --- | --- | --- | --- | --- |
| COL-01 | Array type | `(array i32)` | The one compiler-owned generic immutable sequence, indexed storage; lookups return `option`. | 02-type-system.md:44, 501 |
| COL-02 | Array constructor | `(array.of 1i32 2i32)` | Variadic static method with `where: (t any)`; empty call needs an expected type. | 02-type-system.md:575 |
| COL-03 | Dict type | `(dict str i32)` | Standard-library `deftype` role `@dict`: entries sorted by `ordered.compare`, one entry per key; `dict` is a reserved form head and has no written constructor or pattern. | 02-type-system.md:102 |
| COL-04 | Dict constructor | `(dict.of k v k v)` | Variadic `dict.of` with `variadic: (entries (dict k v))`; sorts and merges entries; odd count is a binding error. | 02-type-system.md:579, 716 |
| COL-05 | Dict reading | `(table key)`, `dict.entries` | Lookup by application; entries read through `dict.entries`; traversal only in canonical key order. | 02-type-system.md:107, 393 |
| COL-06 | Immutability | all collections | Collections have immutable value semantics. | 01-source-language.md:701, 02-type-system.md:289 |
| COL-07 | Pure iteration | `(iter.map xs f)` | Uses the `iter` interface and its default methods `map filter skip take collect`; callbacks must have `effects: ()`. | 01-source-language.md:702, 02-type-system.md:1208 |
| COL-08 | Iterator laws | `next`, `collect`, `map`, `filter`, `take`/`skip` | `next` is a function of its iterator; `map` `filter` `skip` `take` are lazy; `collect` drains eagerly into `(array item)`. | 02-type-system.md:1169 |
| COL-09 | Iterator adapter types | `mapped-iter filtered-iter skipped-iter taken-iter` | Public stdlib `deftype`s with nested `impl (iter ...)`; results seen as `(iter item)`. | 02-type-system.md:1188 |
| COL-10 | `(iter item)` in type position | `(values (iter item))` | Interface value reached by explicit widening; MUST NOT be written bare as `iter`. | 02-type-system.md:1148 |
| COL-11 | Effectful iteration | recursive `defn` over `(iter.next it)` | Written as a recursive module-level function with an explicit effect ceiling; no loop, foreach, or effectful combinator. | 02-type-system.md:1107, 03-effects.md:186 |
| COL-12 | `option` iteration | `(iter.collect some-option)` | `option` is iterable with at most one item. | 02-type-system.md:1230 |
| COL-13 | `str` iteration and lookup | `(iter.collect text)` | Iterates `char` in scalar order; indices count Unicode scalars, not bytes. | 02-type-system.md:1229, 520 |
| COL-14 | `str`/`bytes` operations | `str.concat`, `bytes.slice` | Operations are static methods of `str` and `bytes`; comparison via their `equatable`/`ordered` implementations. | 02-type-system.md:90 |

## Effects

| Id | Construct | Spelling in current Vibra | Rule in one or two sentences | Where |
| --- | --- | --- | --- | --- |
| EFF-01 | `deffect` | `(deffect read visibility: @public (defn file (at path) (result bytes fs-error) ...))` | Introduces a nominal effect root plus operations; each operation has its own visibility (private unless `@public`). | 03-effects.md:44, 66 |
| EFF-02 | Effect operation | `(defn text (at path) (result str fs-error) external: @host symbol: "fs.read-text")` | A member `defn` of a `deffect`; referenced as `module.root.operation`; the owner root is implicit in its row. | 03-effects.md:63, 77 |
| EFF-03 | Operation body | Vibra-bodied operation | Operations may have Vibra bodies (ordinary function run by both backends) or be `@host` externals. | 03-effects.md:36, 91 |
| EFF-04 | Operation additive row | `effects: (fs.write)` on an operation | Lists additional roots its body performs; complete row is owner root plus additive row; a `@host` member has no additive row. | 03-effects.md:77, 81 |
| EFF-05 | `deffect` has no `effects:` | `(deffect x effects: ...)` | `@syntax.invalid-attribute`; a root is a name, not a row. | 03-effects.md:84, 99 |
| EFF-06 | `@host` external | `external: @host symbol: "fs.read-file"` | Valid only on a `deffect` member; symbol must be in the closed host registry and owner must equal the enclosing `deffect`; embedded stdlib only (`@external.owner-mismatch`). | 03-effects.md:91, 99 |
| EFF-07 | Effect row | `effects: (io.stdout fs.read)` | A flat, duplicate-free set of symbols resolved in the effect namespace; unqualified means current module, dotted begins with an import alias; atoms are not accepted (`@effect.invalid-reference`). | 03-effects.md:115, 01-source-language.md:417 |
| EFF-08 | Duplicate root | `(io.stdout io.stdout)` | `@effect.duplicate-root` at the second entry. | 03-effects.md:121 |
| EFF-09 | Default empty row | omitted `effects:` | Omission is `effects: ()` on every `defn`, `lambda`, function type, `test`, interface member; never inferred. | 01-source-language.md:402, 03-effects.md:124 |
| EFF-10 | Written ceiling required | effectful `defn` | An effectful `defn`, `lambda`, or test MUST write its complete `effects:`; an exceeding body is `@effect.outside-ceiling`. | 01-source-language.md:405, 03-effects.md:128 |
| EFF-11 | `def` has no effects | `(def x t (io-call))` | A `def` initializer cannot write `effects:`; its performed row must be empty. | 03-effects.md:126 |
| EFF-12 | Performed row | transitive closure | Computed as the union of complete rows reached over the function-call graph; `let`, `let-else`, `return` operands count; creating a function value performs nothing. | 03-effects.md:133, 203, 178 |
| EFF-13 | Complete row of a call | defn, operation, function value, contract member | A `defn` contributes its performed row; an operation its owner root plus additive row; a function value the row of its type; a contract member the contract ceiling, never the implementation's. | 03-effects.md:165 |
| EFF-14 | No row subtyping or polymorphism | `fn` types with different rows | Function types are equal only if rows are equal; no effect variables; a higher-order function declares the exact callback row; a caller wraps in a `lambda` to adjust. | 03-effects.md:147, 180 |
| EFF-15 | No sub-effect relation | roots `audit` and `fs.write` | Naming an owner root never discharges an additive root; containment is a plain subset test. | 03-effects.md:212 |
| EFF-16 | Pure applications | constructors, projections, lookups, `tupleof`/`recordof`/`enumof` | Add no root and no call edge; evaluation of their operands still contributes. | 03-effects.md:190 |
| EFF-17 | Interface ceiling | `(defn render (value self) str effects: ())` in `defint` | Every contract member has a written or default-empty ceiling; implementations must write the same row. | 03-effects.md:130, 140 |
| EFF-18 | Witness diagnostics | `@effect.outside-ceiling` | One diagnostic per offending function and missing root, with a chain of call-site spans noting `performs <root> via <callee>`. | 03-effects.md:250 |
| EFF-19 | Unused declared root | `@contract.unused-effect` | An unused root in a ceiling or target array is a warning. | 03-effects.md:139, 345 |
| EFF-20 | Target consent | `effects: (array @std.fs.read @std.io.stdout)` in `project.vibon` | A binary target lists canonical root atoms that must contain the entry's performed row; entity-reference slot; blanket consent for the run. | 03-effects.md:290, 336 |
| EFF-21 | Effectful tests | `(test "x" effects: (io.stdout) ...)` | A test writes its complete row; running it is consent; no target array applies; a pure test may omit the row. | 03-effects.md:365 |
| EFF-22 | Standard roots | `io.stdin io.stdout io.stderr fs.read fs.write fs.metadata env.read time.now random.generate` | Reserved roots declared by embedded modules with operations `text bytes file entries stat var args wall monotonic`; every operation returns `(result t e)`. | 03-effects.md:372, 402 |
| EFF-23 | No terminating source | (none) | No host operation or standard function returns `never`. | 03-effects.md:396 |
| EFF-24 | Host world | default in-memory world for `vibra test` | Time and random come from the host provider; tests use a deterministic host world. | 03-effects.md:408 |
| EFF-25 | Not exceptions or handlers | (none) | Effects are static and erased; not exceptions, algebraic handlers, resumptions, values, permission prompts, path filters, or capabilities; no sealed or dominance relations. | 03-effects.md:19, 434 |

## Modules, imports, visibility, packages

| Id | Construct | Spelling in current Vibra | Rule in one or two sentences | Where |
| --- | --- | --- | --- | --- |
| MOD-01 | Module identity | `src/hello/model.vib` is `@hello.model` | Every `.vib` file is one module; identity is its target-relative path without extension; the file does not redeclare it. | 04-programs-and-packages.md:306 |
| MOD-02 | Path segments | `text.vib`, `text/` | Every segment is a `kebab-name`; a file and a directory of the same name may not coexist (`@module.file-directory-collision`); `.vib` is the only extension; no index module. | 04-programs-and-packages.md:310 |
| MOD-03 | Module import | `(import text @std.str)` | Binds only the alias; public declarations referenced as `alias.name`, effect operations `alias.root.operation`. | 04-programs-and-packages.md:326, 355 |
| MOD-04 | Declaration import | `(import ordering @std.core.ordering)` | Binds the alias to one declaration in every namespace it occupies; members reached through the alias. | 04-programs-and-packages.md:358 |
| MOD-05 | Import resolution | `@unit.c1...cn` | First component names a unit, then a module path, optionally one declaration; never guessed from the importing file. | 04-programs-and-packages.md:330, 317 |
| MOD-06 | Import prohibitions | string path, relative, glob, re-export | String paths, relative, absolute, glob imports, extension search, index fallback, re-exports, cycles are errors; no wildcard or open namespaces. | 04-programs-and-packages.md:349, 02-type-system.md:600 |
| MOD-07 | Import cycle | back edge | `@module.import-cycle` at the later edge. | 04-programs-and-packages.md:346 |
| MOD-08 | Duplicate alias | two imports of one alias | `@name.redeclaration`; an alias spelled as a builtin type name, `true`, or `false` is `@name.reserved-value-spelling`. | 04-programs-and-packages.md:344, 02-type-system.md:690 |
| MOD-09 | Visibility | `visibility: @public` | Private by default; an import exposes only public declarations; private access is `@name.private-access`; resolution is total but access is not. | 01-source-language.md:366, 04-programs-and-packages.md:339 |
| MOD-10 | Constructor visibility | wrapper constructor/pattern | A wrapper's constructor and unwrapping pattern are available only where visibility permits. | 02-type-system.md:399 |
| MOD-11 | Effect root visibility | `visibility: @public` on `deffect` | Another module names a root in a row only when the root is public, and applies an operation only when the operation is public. | 03-effects.md:67 |
| MOD-12 | Name shadowing | repeated name | Forbidden for top-level declarations, import aliases, and lexical bindings (`@name.redeclaration`); sibling scopes may reuse a name. | 02-type-system.md:650, 666 |
| MOD-13 | Unit | local target, `@tests`, or dependency alias | The root of code reference; the first component of a code-reference atom. | 04-programs-and-packages.md:250 |
| MOD-14 | Entity-reference atoms | `@app.m.fs.read.file` | Each code entity has one canonical atom path; an atom is a reference only in a slot that expects one; wrong kind is `@name.wrong-entity-kind`. | 02-type-system.md:624, 644 |
| MOD-15 | Prelude without import | `bool`, `str`, `option`, ... | Only the prelude is visible without import; all other stdlib declarations need an import. | 02-type-system.md:109, 04-programs-and-packages.md:363 |
| MOD-16 | Standard library modules | `@std.core @std.option @std.result @std.bool @std.str @std.bytes @std.iter @std.builtin @std.assert @std.io @std.fs @std.env @std.time @std.random` | Embedded in the toolchain; `@std.builtin` is never imported; unit `@std`, package `vibra-stdlib`. | 04-programs-and-packages.md:457, 469 |
| MOD-17 | Project file | `project.vibon` | One `@project.v1` record with `package`, `targets`, `dependencies`; no `project.vib` and no executable project declaration. | 04-programs-and-packages.md:76, 144 |
| MOD-18 | Targets | `kind: @bin` / `@lib` | Unique one-component kebab name, kind, root, disjoint roots; binary requires `entry` and `effects`, library omits both; a library surface is every public declaration. | 04-programs-and-packages.md:243, 256 |
| MOD-19 | Entry rules | `entry: @hello.main.main` | Declaration reference whose first component is the target itself; must be a no-parameter `defn` returning `void` or `(result void e)`; an `err` ends `run` with exit 6. | 04-programs-and-packages.md:264, 276 |
| MOD-20 | Dependencies | `kind: @path` / `@git` | Local path or HTTPS Git with 40-hex `rev`; alias binds one `@lib` target; exact versions; no registry, ranges, scripts; `project-lock.vibon` generated by `vibra project sync`. | 04-programs-and-packages.md:371, 384, 393 |
| MOD-21 | VIBON data | `(record ...)`, `(array ...)`, `(tuple ...)`, `(dict ...)` | Non-executable `.vibon` documents with literals including `true`/`false`; bare symbols, applications, and declarations are rejected. | 04-programs-and-packages.md:16 |

## Tests and documentation

| Id | Construct | Spelling in current Vibra | Rule in one or two sentences | Where |
| --- | --- | --- | --- | --- |
| TST-01 | `test` declaration | `(test "greets by name" (assert.equal-str (greet "Ada") "hello, Ada"))` | A string name, optional `effects:`, then a body sequence; result type is `void`. | 01-source-language.md:267, 04-programs-and-packages.md:511, 590 |
| TST-02 | Test location | files under `tests/` | Modules in the reserved `@tests` unit (`tests/math/add.vib` is `@tests.math.add`); a `test` elsewhere is `@tool.unavailable`; targets cannot import `@tests`. | 04-programs-and-packages.md:493, 504 |
| TST-03 | Test names | unique string | Unique within a module (`@name.redeclaration`); compared by exact scalar sequence; discovery order is module identity then source order. | 04-programs-and-packages.md:515 |
| TST-04 | Assertion module | `(import assert @std.assert)` | A module declaring tests MUST import `@std.assert` (`@module.missing-required-import`); no test-only prelude or private-access exception. | 04-programs-and-packages.md:548, 563 |
| TST-05 | Assertion members | `assert.true assert.false assert.equal-bool assert.equal-char assert.equal-str assert.equal-i32 assert.equal-u64` | Closed set of seven; other assertions are `@tool.unavailable`; admitted only from the `@tests` unit. The spec's own example at line 512 writes `assert.equal`, which is not in this table. | 04-programs-and-packages.md:571, 581, 587 |
| TST-06 | `return` and `try` in tests | `(return x)` in a test | `return` directly in a test body is `@type.invalid-return`; inside a nested `lambda` it exits the lambda; `try` is always invalid. | 04-programs-and-packages.md:592, 02-type-system.md:1303 |
| TST-07 | Test effects | `(test "x" effects: (io.stdout) ...)` | Omitted row is empty; effectful tests run in the default host world. | 04-programs-and-packages.md:553 |
| TST-08 | `doc:` attribute | `(defn f ... doc: "text" body)` | A string attribute on `deftype`, `defint`, `deffect`, `def`, `defn`, and members; the chapters give no further rules for its content or rendering. | 01-source-language.md:296, 308 |
| TST-09 | Comments | `; text` | Line comments only; no block comments; preserved and attached to forms by the formatter. | 01-source-language.md:16, 860 |

## Constants

| Id | Construct | Spelling in current Vibra | Rule in one or two sentences | Where |
| --- | --- | --- | --- | --- |
| CST-01 | `def` | `(def default-retries u8 3u8)` | Immutable module value with a written type; initializer may be any expression but cannot perform effects. | 01-source-language.md:263, 03-effects.md:126 |
| CST-02 | `def` attributes | `visibility:` `doc:` | Only declaration attributes are accepted on a `def`. | 01-source-language.md:264, 296 |
| CST-03 | No `const` | (none) | There is no separate `const` form. | 01-source-language.md:326 |
| CST-04 | Constant `def` as pattern | `(match x limit 1 - 0)` | A `def` whose initializer is a constant expression is usable as a constant pattern and as a labelled default. | 02-type-system.md:1356, 1380 |
| CST-05 | Prelude values | `true`, `false` | `(def true bool (bool.true) visibility: @public)` and likewise `false` in `@std.bool`. | 02-type-system.md:94 |
| CST-06 | Initializer cycles | mutually referring `def`s | Rejected. | 02-type-system.md:1376 |
| CST-07 | `return` in `def` | `(def x t (return 1))` | `@type.invalid-return`. | 02-type-system.md:1433 |

## Anything else

| Id | Construct | Spelling in current Vibra | Rule in one or two sentences | Where |
| --- | --- | --- | --- | --- |
| ETC-01 | Labels as operands in anonymous forms | `(recordof a: 1)` | The same label syntax is used for record construction, record patterns, labelled arguments, and `types:`. | 01-source-language.md:189, 02-type-system.md:857 |
| ETC-02 | Operand evaluation order | fixed, labelled, variadic | Evaluated in resolved fixed-parameter order, then labelled in declaration order, then variadic in source order; the callee once, first. | 01-source-language.md:209, 221 |
| ETC-03 | Reserved call-site label | `types:` | Not an operand of the callee, placed before labelled operands; noncanonical position is `@style.argument-order`. | 02-type-system.md:870 |
| ETC-04 | Order tolerance | interleaved operands | The recovery parser accepts unambiguous orders; noncanonical order is a style diagnostic. | 01-source-language.md:214 |
| ETC-05 | `.vibon` boolean literals | `true`, `false` in data | Only in data documents; source uses prelude values. | 01-source-language.md:152, 04-programs-and-packages.md:23 |
| ETC-06 | No WebAssembly FFI | (none) | No Wasm external provider and no source-level Wasm FFI; no `external`, `primitive`, or `host-op` expression. | 01-source-language.md:467 |
| ETC-07 | Specialization unobservable | monomorphization | Implementations may monomorphize; strategy not observable. | 02-type-system.md:883 |
| ETC-08 | Generic implementations unscheduled | `(impl (array t) ...)` in a `defint` | An `impl` target in a `defint` must be a closed type; a free generic name is `@name.unknown-symbol`; generic implementations are an unscheduled proposal. | 02-type-system.md:817, 1244 |
| ETC-09 | Associated types | (none) | No associated types on interfaces; `conversion-error` is fixed. | 02-type-system.md:1693 |
| ETC-10 | Implementation capability boundary | M2/M3 profiles | Valid-but-unavailable forms produce `@tool.unavailable`; this is a capability boundary, not a second dialect. | 02-type-system.md:149, 160 |
| ETC-11 | Fixed-arity `fn` type vs variadic | M2 profile | M2 admits no variadic slot or call. | 02-type-system.md:176 |

## Things the chapters mark as retired or forbidden

- `while`, `for`, `break`, `continue`: retired forms, `@syntax.retired-form` (01-source-language.md:591).
- Assignment form and shared mutable state (01-source-language.md:591, 631; 02-type-system.md:1728).
- Boolean literal in source; `true` and `false` are prelude values (01-source-language.md:149).
- `unit` literal (01-source-language.md:117).
- `const` form (01-source-language.md:326).
- `bind` pattern form and compatibility spelling (01-source-language.md:646).
- `case` wrapper in `match` (01-source-language.md:725); wrapper list around each parameter (01-source-language.md:329).
- `field` form and generated accessor functions (01-source-language.md:532); `value.0` postfix tuple index (02-type-system.md:512).
- Top-level `impl`, `for:` implementation attribute, `implements:` attribute (01-source-language.md:513; 02-type-system.md:969).
- `external`, `primitive`, `host-op` expressions; WebAssembly external provider or FFI (01-source-language.md:467).
- `newtype` form and transparent type aliases (02-type-system.md:230, 399).
- Subtyping, row subtyping, least upper bound, effect variables, polymorphic rows (02-type-system.md:1559, 1577; 03-effects.md:147, 180).
- Callable interface or user-defined call operator (01-source-language.md:211; 02-type-system.md:528).
- `into` interface (02-type-system.md:1681); `hashable` interface (02-type-system.md:354); operator overloading (02-type-system.md:1100).
- `to-U`, `parse`, `to-str` number static methods (02-type-system.md:1707).
- Multiple bounds on one generic parameter, partial application, named type arguments, specialization by value, runtime type tests (02-type-system.md:797).
- Wildcard imports, re-exports, open namespaces, extensible prelude, string-path and relative imports, import cycles (02-type-system.md:600; 04-programs-and-packages.md:349).
- Exceptions, algebraic handlers, resumptions, runtime permission prompts, sealed roots, sub-effect relations (03-effects.md:434).
- `resource` type constructor, host handles, close protocol; long-lived streams (02-type-system.md:1720).
- User-declared members on builtin types; `impl` blocks on `array` or `dict`; anonymous types with methods or `impl` (02-type-system.md:307, 585, 1235).
- `isize`, `usize`, numeric separators, non-decimal bases; source spellings for infinity or NaN (01-source-language.md:109; 02-type-system.md:194).
- Reader macros, quote, commas, brackets, braces, raw strings, heredocs, string interpolation (01-source-language.md:17).
- Null value, truthiness conversion, implicit numeric widening, implicit string conversion (02-type-system.md:137).
- Implicit error conversions (01-source-language.md:745).
- Recursive `lambda` self-reference (02-type-system.md:723).
- Terminating host operations with result `never` (03-effects.md:396).
- `project.vib` project file, legacy project format, version ranges, registry, lifecycle scripts (04-programs-and-packages.md:144, 488).
