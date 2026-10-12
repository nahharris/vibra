//! Closed key order, as data and as the routines that compare by it.
//!
//! `docs/spec/02-type-system.md`, "Nominal declarations": a dict is ordered by
//! its keys, and for a closed key type the order is fixed by the language and
//! not by a program: `false` before `true`, numeric order for integers, scalar
//! value for `char`, lexicographic for `str` and `bytes`, the spelling for an
//! atom, component by component for a tuple and a record, canonical variant and
//! then payload for an enum, and canonical member and then value for a union.
//!
//! The shape of a key type is read from its type once ([`shape`]), and a shape
//! becomes one WebAssembly routine `(a: i64, b: i64) -> i32` that answers `-1`,
//! `0`, or `1`, where `a` and `b` are the 64 bits of two cells of that type: an
//! integer's bits, or the offset of an object. A routine reads components of
//! the objects it is given and calls the routine of each component, so the
//! engine's stack depth is the nesting of the type and never of a value. The
//! rows of the registry that order text, `text.compare` and `bytes.compare`,
//! are the native imports of the module, so strings are ordered by the code that
//! the reference interpreter calls too.
//!
//! A `deftype` key is ordered by its own `compare`, which is Vibra and is called
//! as any language function is; that is the work of the dict lowering itself and
//! not of a routine here.

use vibra_ir::Type;
use wasm_encoder::{BlockType, Instruction as I, ValType};

use crate::layout::{cells_offset, header};
use crate::runtime::{Ins, Routine, c32, get, ld32, ld64, set};
use crate::types::TypeEnv;

/// How a closed key type orders its values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum KeyShape {
    /// An integer of at most 32 bits, or a `char`, which is compared as the
    /// 32-bit word its cell holds.
    Narrow { signed: bool },
    /// A 64-bit integer.
    Wide { signed: bool },
    /// The characters of a `str` or an atom.
    Text,
    /// The bytes of `bytes`.
    Bytes,
    /// The components of a tuple or a record, in the order the object stores
    /// them: the first that differs decides.
    Components(Vec<KeyShape>),
    /// The variants of an enum, `bool` included, or the members of a union: the
    /// discriminant decides, and then the payload of the one both hold, which
    /// is `None` for a variant with no payload.
    Variants(Vec<Option<KeyShape>>),
}

/// The shape of the closed key type `ty`, or `None` when `ty` is not one: a
/// declared type is ordered by its own `compare`, and an array, a dict, an
/// option, a float, and a function are not keys at all.
pub(crate) fn shape(env: &TypeEnv<'_>, ty: &Type) -> Option<KeyShape> {
    Some(match ty {
        Type::Bool => KeyShape::Variants(vec![None, None]),
        Type::Char | Type::U8 | Type::U16 | Type::U32 => {
            KeyShape::Narrow { signed: false }
        }
        Type::I8 | Type::I16 | Type::I32 => KeyShape::Narrow { signed: true },
        Type::I64 => KeyShape::Wide { signed: true },
        Type::U64 => KeyShape::Wide { signed: false },
        Type::Str | Type::Atom | Type::AtomSingleton(_) => KeyShape::Text,
        Type::Bytes => KeyShape::Bytes,
        // `void` has one value, so it orders nothing.
        Type::Void => KeyShape::Components(Vec::new()),
        Type::Tuple(components) => KeyShape::Components(
            components
                .iter()
                .map(|component| shape(env, component))
                .collect::<Option<Vec<_>>>()?,
        ),
        Type::Record(fields) => KeyShape::Components(
            fields
                .iter()
                .map(|(_, field)| shape(env, field))
                .collect::<Option<Vec<_>>>()?,
        ),
        Type::Enum(variants) => KeyShape::Variants(
            variants
                .iter()
                .map(|(_, payload)| match payload {
                    Type::Void => Some(None),
                    other => shape(env, other).map(Some),
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        Type::Union(members) => KeyShape::Variants(
            members
                .iter()
                .map(|member| shape(env, member).map(Some))
                .collect::<Option<Vec<_>>>()?,
        ),
        _ => return None,
    })
}

/// How a dict orders the keys of one type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Order {
    /// The language's own order for a closed key type.
    Closed(KeyShape),
    /// The `compare` the program writes for a declared key type: the index of
    /// the function that implements it.
    User(usize),
}

/// The order of a dict whose keys have type `key`, from the interface the
/// checker recorded for it: none for a closed key type, and the `ordered`
/// interface for any other. `None` when this step cannot order the keys: a
/// closed shape holding a declared key (which answers through its own
/// implementation, and so is dispatched from a run-time type), and a declared
/// key that no implementation of one non-generic receiver orders.
pub(crate) fn order_of(
    env: &TypeEnv<'_>,
    key: &Type,
    key_order: Option<&vibra_ir::TypeId>,
) -> Option<Order> {
    match key_order {
        None => shape(env, key).map(Order::Closed),
        Some(interface) => env
            .functions()
            .iter()
            .position(|function| {
                function.implements().is_some_and(|implements| {
                    implements.interface == *interface
                        && implements.member == "compare"
                        && implements.receiver == *key
                })
            })
            .map(Order::User),
    }
}

/// The function indices a comparer calls.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Calls {
    /// `text.compare`, when the module imports it.
    pub(crate) text: u32,
    /// `bytes.compare`, when the module imports it.
    pub(crate) bytes: u32,
}

/// The routine of `shape`: `(a: i64, b: i64) -> i32`. `children` are the
/// function indices of the routines of the component shapes, in the order the
/// shape lists them (the payloads that exist, for a variant list).
pub(crate) fn routine(shape: &KeyShape, children: &[u32], calls: Calls) -> Routine {
    // Locals after the two parameters: 2 the result of a component, 3 and 4 the
    // discriminants.
    let (result, first, second) = (2, 3, 4);
    let mut code: Vec<Ins> = Vec::new();
    let a = |code: &mut Vec<Ins>| code.extend([get(0), I::I32WrapI64]);
    let b = |code: &mut Vec<Ins>| code.extend([get(1), I::I32WrapI64]);
    match shape {
        KeyShape::Narrow { signed } => {
            let (gt, lt) = if *signed {
                (I::I32GtS, I::I32LtS)
            } else {
                (I::I32GtU, I::I32LtU)
            };
            let word = |code: &mut Vec<Ins>| {
                a(code);
                b(code);
            };
            word(&mut code);
            code.push(gt);
            word(&mut code);
            code.push(lt);
            code.push(I::I32Sub);
        }
        KeyShape::Wide { signed } => {
            let (gt, lt) = if *signed {
                (I::I64GtS, I::I64LtS)
            } else {
                (I::I64GtU, I::I64LtU)
            };
            // `greater - less`, the operands pushed once for each comparison.
            code.extend([get(0), get(1), gt, get(0), get(1), lt, I::I32Sub]);
        }
        KeyShape::Text => {
            a(&mut code);
            b(&mut code);
            code.push(I::Call(calls.text));
        }
        KeyShape::Bytes => {
            a(&mut code);
            b(&mut code);
            code.push(I::Call(calls.bytes));
        }
        KeyShape::Components(parts) => {
            let first_cell = cells_offset(u32::try_from(parts.len()).unwrap_or(0));
            for (position, child) in (0_u32..).zip(children) {
                let at = first_cell + 8 * position;
                a(&mut code);
                code.push(ld64(at));
                b(&mut code);
                code.push(ld64(at));
                code.extend([
                    I::Call(*child),
                    set(result),
                    get(result),
                    I::If(BlockType::Empty),
                    get(result),
                    I::Return,
                    I::End,
                ]);
            }
            code.push(c32(0));
        }
        KeyShape::Variants(parts) => {
            let payload = cells_offset(1);
            a(&mut code);
            code.extend([ld32(header::VARIANT), set(first)]);
            b(&mut code);
            code.extend([ld32(header::VARIANT), set(second)]);
            // Different discriminants decide.
            code.extend([
                get(first),
                get(second),
                I::I32Ne,
                I::If(BlockType::Empty),
                c32(-1),
                c32(1),
                get(first),
                get(second),
                I::I32LtU,
                I::Select,
                I::Return,
                I::End,
            ]);
            // The same discriminant: the payload, when the variant has one.
            let mut next_child = children.iter();
            for (variant, part) in (0_u32..).zip(parts) {
                if part.is_none() {
                    continue;
                }
                let Some(child) = next_child.next() else {
                    break;
                };
                code.extend([
                    get(first),
                    c32(variant.cast_signed()),
                    I::I32Eq,
                    I::If(BlockType::Empty),
                ]);
                a(&mut code);
                code.push(ld64(payload));
                b(&mut code);
                code.push(ld64(payload));
                code.extend([I::Call(*child), I::Return, I::End]);
            }
            code.push(c32(0));
        }
    }
    code.push(I::End);
    Routine {
        params: vec![ValType::I64, ValType::I64],
        results: vec![ValType::I32],
        locals: vec![ValType::I32, ValType::I32, ValType::I32],
        body: code,
        export: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_key_type_has_a_shape_and_a_declared_one_does_not() {
        let env = TypeEnv::new(&[]);
        assert_eq!(
            shape(&env, &Type::I8),
            Some(KeyShape::Narrow { signed: true })
        );
        assert_eq!(
            shape(&env, &Type::Char),
            Some(KeyShape::Narrow { signed: false })
        );
        assert_eq!(
            shape(&env, &Type::U64),
            Some(KeyShape::Wide { signed: false })
        );
        assert_eq!(
            shape(&env, &Type::Tuple(vec![Type::Str, Type::Bool])),
            Some(KeyShape::Components(vec![
                KeyShape::Text,
                KeyShape::Variants(vec![None, None])
            ]))
        );
        assert_eq!(shape(&env, &Type::F64), None);
        assert_eq!(shape(&env, &Type::Array(Box::new(Type::I32))), None);
        assert_eq!(
            shape(
                &env,
                &Type::Declared(vibra_ir::TypeId::new("point", "point"))
            ),
            None
        );
    }
}
