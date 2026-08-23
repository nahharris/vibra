# 9. Diagnostics

Status: draft

This is the registry of stable diagnostic codes. A code's meaning never
changes. Retiring a code means never issuing it again; it does not mean reusing
the number.

Every code here must have at least one `vibra-bad` block somewhere in `spec/`
that triggers it, and the conformance suite fails if one does not. A code with
no triggering example is an unimplemented promise.

## Families

| Prefix | Stage |
| --- | --- |
| `E-LEX` | Reading bytes into tokens |
| `E-SYN` | Form structure and operand order |
| `E-ATOM` | Atom positions |
| `E-IMPORT` | Module resolution |
| `E-PROJ` | `project.vib` |
| `E-NAME` | Name resolution |
| `E-SCOPE` | Lexical scoping |
| `E-VIS` | Visibility |
| `E-MAIN` | Entry point |
| `E-CONST` | Constant evaluation |
| `E-TYPE` | Typing |
| `E-CALL` | Call sites |
| `E-CAST` | `cast` boundaries |
| `E-INT` | Interfaces and implementations |
| `E-MATCH` | Pattern matching |
| `E-MUT` | Mutability |
| `E-FLOW` | Control flow |
| `E-TRY` | Failure propagation |
| `E-OP` | Primitive operations |
| `E-EFFECT` | Effect declarations and inference |
| `E-ABI` | The host boundary |
| `W-*` | Warnings, same families |

## Errors

### Reading

| Code | Meaning |
| --- | --- |
| `E-LEX-001` | The file is not valid UTF-8. |
| `E-LEX-002` | A character or delimiter that is not part of the reader appears in source. |
| `E-LEX-003` | A symbol segment contains a character outside lowercase kebab-case. |
| `E-LEX-004` | `_` is used as a value reference. |
| `E-LEX-005` | A reserved word or special-form head is used as a definition name. |
| `E-LEX-006` | A string literal is unterminated, or contains an invalid escape or Unicode scalar. |

### Form structure

| Code | Meaning |
| --- | --- |
| `E-SYN-001` | A label appears in expression position. |
| `E-SYN-002` | A label appears after a body or variadic operand. |
| `E-SYN-003` | A label is not one this form accepts. |
| `E-SYN-004` | A label appears more than once in one form. |
| `E-SYN-005` | A label has no following form. |
| `E-SYN-006` | A form has too few or too many fixed positional operands. |
| `E-ATOM-001` | A bare symbol or string appears where the form requires an atom. |
| `E-ATOM-002` | An atom is not one this position accepts. |

### Modules and names

| Code | Meaning |
| --- | --- |
| `E-IMPORT-001` | An import path is absolute. |
| `E-IMPORT-002` | An import path escapes the target's root directory. |
| `E-IMPORT-003` | An import path does not resolve to a readable `.vib` file. |
| `E-IMPORT-004` | An import names a dependency not declared in `project.vib`. |
| `E-IMPORT-005` | Modules form an import cycle. |
| `E-IMPORT-006` | Two imports in one file bind the same alias. |
| `E-PROJ-001` | `project.vib` has zero or more than one `package`. |
| `E-PROJ-002` | `project.vib` declares no target. |
| `E-PROJ-003` | Two targets share a name. |
| `E-PROJ-004` | A version string is not `major.minor.patch`. |
| `E-PROJ-005` | A dependency path does not contain a readable `project.vib`. |
| `E-NAME-001` | A symbol does not resolve. The message names the longest prefix that did. |
| `E-NAME-002` | Two top-level definitions in one module share a name. |
| `E-SCOPE-001` | A binder shadows a name visible in an enclosing lexical scope. |
| `E-VIS-001` | A private definition is referenced from another module. |
| `E-VIS-002` | A private type appears in the signature of a public definition. |
| `E-MAIN-001` | `main` has a signature other than `() void` or `() (result void e)`. |
| `E-MAIN-002` | A target's entry file defines no `main`. |
| `E-CONST-001` | A `const` initializer is not a compile-time constant expression. |

### Types

| Code | Meaning |
| --- | --- |
| `E-TYPE-001` | A generic type constructor is referenced without arguments. |
| `E-TYPE-002` | A generic type is applied to the wrong number of arguments. |
| `E-TYPE-003` | A `deftype` body is a bare type reference, which would create an alias. |
| `E-TYPE-004` | A member inside a declaration block is not namespaced to that block or to a listed interface. |
| `E-TYPE-005` | A record declares the same field twice. |
| `E-TYPE-006` | A tuple has fewer than two elements. |
| `E-TYPE-007` | An enum declares the same tag twice. |
| `E-TYPE-008` | A handle is widened to a stronger access than it holds. |
| `E-TYPE-009` | Generic instantiation does not terminate. |
| `E-TYPE-010` | Type arguments are partially applied or given by name. |
| `E-TYPE-011` | `self` appears outside a `deftype` or `defint` block. |
| `E-TYPE-012` | A `ref` is stored, returned, or captured. |
| `E-TYPE-013` | A `let-as` initializer does not have the stated type. |
| `E-TYPE-014` | The branches of an `if` have different types. |
| `E-TYPE-015` | `(return)` with no operand appears in a function that does not return `void`. |
| `E-TYPE-016` | An `fn` captures a `mut` cell or a `ref`. |
| `E-TYPE-017` | A record literal has a missing, extra, or misordered field. |
| `E-TYPE-018` | An expression's type is not the type its position requires. This is the general mismatch; a more specific code takes precedence where one applies. |
| `E-TYPE-019` | `field` names a field the operand's type does not declare, or `index` is applied to a type that is not an array or map. |
| `E-TYPE-020` | A type declares an inherent member whose name collides with a listed interface of the same name. |
| `E-CALL-001` | A call passes the wrong number of arguments. |
| `E-CALL-002` | An argument's type does not match the parameter's. |
| `E-CALL-003` | The head of a call is not a function, constructor, or primitive. |
| `E-CAST-001` | `cast` is used between types that are not a newtype and its immediate underlying type. |
| `E-CAST-002` | `cast` is used on a handle-backed type. |

### Interfaces

| Code | Meaning |
| --- | --- |
| `E-INT-001` | A type declares an interface but does not implement all of its required members. |
| `E-INT-002` | An implementation's signature does not match the interface's. |
| `E-INT-003` | A type implements a member of an interface it does not declare. |
| `E-INT-004` | An interface method's first parameter is not `(self self)`. |
| `E-INT-005` | A type overrides an interface method that has a provided body. |
| `E-INT-006` | A type declares an interface without declaring that interface's superinterfaces. |

### Expressions

| Code | Meaning |
| --- | --- |
| `E-MATCH-001` | A `match` is not exhaustive. The message names an uncovered value. |
| `E-MUT-001` | `set` targets a binding that is not a `mut` cell. |
| `E-FLOW-001` | `break` or `continue` appears outside a loop. |
| `E-FLOW-002` | A `range` has a zero step. |
| `E-TRY-001` | `try` mixes a `result` operand with an `option` function, or the reverse. |
| `E-TRY-002` | A `try` operand's error type differs from the enclosing function's. |
| `E-TRY-003` | `try` appears in a function that returns neither `result` nor `option`. |
| `E-OP-001` | A primitive operation's operands have different or unsupported types. |
| `E-OP-002` | A primitive operation on literal operands would trap. |

### Effects and the host boundary

| Code | Meaning |
| --- | --- |
| `E-EFFECT-001` | A declared effect row does not cover the inferred row. The message names each uncovered operation. |
| `E-EFFECT-002` | An effect row names a root or operation that does not resolve. |
| `E-EFFECT-003` | An implementation's effect row exceeds its interface method's ceiling. |
| `E-EFFECT-004` | An effect root name collides with another top-level name in its module. |
| `E-EFFECT-005` | A `deffect` declares the same operation twice. |
| `E-EFFECT-006` | A declared effect row names the same root or operation twice. |
| `E-EFFECT-007` | An `effects:` label appears at a position where effects are inferred. |
| `E-EFFECT-008` | An explicit empty `effects: ()` is written; absence already declares the empty row. |
| `E-ABI-001` | An `intrinsic` call's arity or argument types do not match the registry entry. |
| `E-ABI-002` | An `intrinsic` names an operation not in the `vibra_v1` registry. |
| `E-ABI-003` | An authority-crossing `intrinsic` appears outside a `deffect` operation. |
| `E-ABI-004` | A value crossing the host boundary is not a scalar or an arena index. |

## Warnings

| Code | Meaning |
| --- | --- |
| `W-EFFECT-001` | A declared effect row is broader than the inferred row. The message names each unused root. |
| `W-RESULT-001` | A `result` or `option` in non-final body position is neither bound, returned, nor propagated. |
| `W-BIND-001` | A named binding is never read. |
| `W-MATCH-001` | A `match` arm can never be reached. |
| `W-NAME-001` | A public definition has no `doc:`. |
| `W-IMPORT-001` | An import is never used. |

Warnings are deterministic and ordered by primary span. `--deny-warnings`
promotes every warning to an error, and `vibra test --deny-warnings` fails a run
that produces any.

## Reserved ranges

`E-*-900` through `E-*-999` in every family are reserved for internal compiler
errors that should never reach a user. Emitting one is a bug, and the message
must say so and point at the issue tracker.
