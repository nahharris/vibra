# Migration from `vibra-ir-v1`

Every public variant of `Type`, `Expr`, `Value`, `CallTarget`, and `IrError`
of `vibra-ir-v1`, with its successor in `vibra-ir` or `removed`. Step 27 and
X2-01 use this table to port the runtime crates and the checker. Each v1
variant appears exactly once; the host test `migration` checks that.

A library type is a `Type::Declared` whose identity the checker binds by its
role ([`Library`](src/library.rs)): `Bool`, `Str`, `Bytes`, `Dict`, `Option`,
and `Result` have no case of their own.

## `Type`

| v1 variant | Successor | Note |
| --- | --- | --- |
| `Type::Bool` | `Type::Declared` | the type playing `@bool` |
| `Type::Void` | `Type::Void` | |
| `Type::Never` | `Type::Never` | |
| `Type::Char` | `Type::Char` | |
| `Type::Str` | `Type::Declared` | the type playing `@str` |
| `Type::Bytes` | `Type::Declared` | the type playing `@bytes` |
| `Type::Atom` | `Type::Atom` | |
| `Type::AtomSingleton` | `Type::AtomSingleton` | |
| `Type::I8` | `Type::I8` | |
| `Type::I16` | `Type::I16` | |
| `Type::I32` | `Type::I32` | |
| `Type::I64` | `Type::I64` | |
| `Type::U8` | `Type::U8` | |
| `Type::U16` | `Type::U16` | |
| `Type::U32` | `Type::U32` | |
| `Type::U64` | `Type::U64` | |
| `Type::F32` | `Type::F32` | |
| `Type::F64` | `Type::F64` | |
| `Type::Function` | `Type::Fn` | a procedure is `Type::Proc`, a plan `Type::Intent` |
| `Type::Declared` | `Type::Declared` | it carries its type arguments |
| `Type::Record` | `Type::Record` | structural only; a declared record is `TypeBody::Record` |
| `Type::Enum` | removed | no anonymous variant; a variant type is `TypeBody::Variant` |
| `Type::Param` | `Type::Binder` | by position in the signature record, never by name |
| `Type::Applied` | `Type::Declared` | the arguments are part of the declared type |
| `Type::Tuple` | `Type::Tuple` | |
| `Type::Array` | `Type::List` | |
| `Type::Dict` | `Type::Declared` | the type playing `@dict` |
| `Type::Union` | removed | no union type |
| `Type::Interface` | `Type::Interface` | it fixes every associated type in its choices |
| `Type::Any` | `Type::Any` | |

## `Expr`

| v1 variant | Successor | Note |
| --- | --- | --- |
| `Expr::Literal` | `Expr::Literal` | |
| `Expr::External` | `Expr::Intrinsic` | a bound function is `FunctionBody::Intrinsic` |
| `Expr::Host` | removed | host rows return with the host registry, step 27 and M2 |
| `Expr::Default` | `Expr::Default` | valid only as a labelled argument |
| `Expr::Sequence` | `Expr::Sequence` | |
| `Expr::Variable` | `Expr::Variable` | |
| `Expr::Global` | `Expr::Global` | |
| `Expr::Function` | `Expr::Function` | at its type arguments |
| `Expr::Closure` | `Expr::Closure` | |
| `Expr::Captured` | `Expr::Captured` | |
| `Expr::Let` | `Expr::Let` | a refutable `let … else` is `Expr::LetElse` |
| `Expr::Match` | `Expr::Match` | |
| `Expr::Widen` | `Expr::WidenToInterface` | an atom singleton widens with `Expr::WidenAtom`; no union widening |
| `Expr::Try` | `Expr::Try` | |
| `Expr::Return` | `Expr::Return` | |
| `Expr::If` | `Expr::If` | |
| `Expr::Call` | `Expr::Call` | a call in tail position is `Expr::TailCall` |
| `Expr::Record` | `Expr::Construct` | `Construction::Record` |
| `Expr::Variant` | `Expr::Construct` | `Construction::Case` |
| `Expr::Wrap` | `Expr::Construct` | `Construction::Wrapper` |
| `Expr::Project` | `Expr::Field` | |
| `Expr::Tuple` | `Expr::Construct` | `Construction::Tuple` |
| `Expr::TupleProject` | removed | no tuple index; components are read by pattern |
| `Expr::Array` | `Expr::Construct` | `Construction::List` |
| `Expr::Dict` | `Expr::Construct` | `Construction::Dict` |
| `Expr::Lookup` | `Expr::Lookup` | its value is an `Option` |

## `Value`

| v1 variant | Successor | Note |
| --- | --- | --- |
| `Value::Bool` | `Value::Variant` | the case `false` or `true`, built by `Value::bool` |
| `Value::Void` | `Value::Void` | |
| `Value::Char` | `Value::Char` | |
| `Value::Str` | `Value::Str` | |
| `Value::Bytes` | `Value::Bytes` | |
| `Value::Atom` | `Value::Atom` | |
| `Value::I8` | `Value::I8` | |
| `Value::I16` | `Value::I16` | |
| `Value::I32` | `Value::I32` | |
| `Value::I64` | `Value::I64` | |
| `Value::U8` | `Value::U8` | |
| `Value::U16` | `Value::U16` | |
| `Value::U32` | `Value::U32` | |
| `Value::U64` | `Value::U64` | |
| `Value::F32` | `Value::F32` | |
| `Value::F64` | `Value::F64` | |

## `CallTarget`

| v1 variant | Successor | Note |
| --- | --- | --- |
| `CallTarget::Direct` | `CallTarget::Direct` | at its type arguments |
| `CallTarget::Indirect` | `CallTarget::Value` | |
| `CallTarget::Contract` | `CallTarget::Contract` | it carries the receiver, the arguments, and the choices |

## `IrError`

| v1 variant | Successor | Note |
| --- | --- | --- |
| `IrError::NoFunctions` | removed | a module set may have no function; `CheckedProgram::for_entry` names the entry |
| `IrError::InvalidEntry` | `IrError::InvalidEntry` | also for an entry that is generic or takes inputs |
| `IrError::DuplicateFunction` | removed | a function is named by its index; repeated identities are `IrError::Duplicate` |
| `IrError::ResultTypeMismatch` | `IrError::TypeMismatch` | for every expected type, not only a result |
| `IrError::InvalidSlotCount` | `IrError::InvalidSlotCount` | |
| `IrError::InvalidExpression` | `IrError::InvalidExpression` | slot and arity defects have their own variants |
| `IrError::RecursiveCall` | removed | an indirect call needs no static bound; the call-flow analysis follows it |
| `IrError::GlobalInitializerCycle` | `IrError::InitializerCycle` | it names the module value |
