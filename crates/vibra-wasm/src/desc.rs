//! Run-time type descriptors, and the table that names them.
//!
//! A generic activation and an interface value carry types at run time as
//! **descriptors** (`docs/roadmap/milestone-4/decision-ledger.md`, D9.2 and
//! D9.5, and [`crate::layout`], "Type arguments"). A descriptor is an arena
//! `tuple` whose first cell is the **head** of the type's constructor and whose
//! other cells are the descriptors of its arguments, so two descriptors are the
//! same type exactly when their heads, lengths, and arguments are the same. A
//! slot that holds none, because nothing fixed the parameter it stands for, has
//! a class byte that is not the reference class.
//!
//! A head is `class | ordinal << 8`. The **class** is the low byte, and says
//! what the type is made of: one of the constants of [`class`]. The **ordinal**
//! numbers each distinct constructor (see [`crate::types::head_key`]) in the
//! order a lowering first meets it, which is deterministic. Code that needs to
//! know only what kind of type a descriptor stands for (an interface value
//! unwraps itself, a key of a closed shape is compared by that shape) tests the
//! class, and code that selects an implementation tests the whole head. The
//! ordinals mean nothing outside the module, so the toolchain produces a
//! [`DescriptorTable`] beside it, as it produces the origin table, which maps a
//! head back to its type for a host that reads an interface value.

use vibra_ir::Type;
use wasm_encoder::{BlockType, Instruction as I, ValType};

use crate::layout::{CellClass, header};
use crate::runtime::{Ins, Routine, c32, get, index, ld8, ld32, set};

/// The low byte of a head: what a type is made of.
pub(crate) mod class {
    /// `void`. Its head is `1` in every module.
    pub(crate) const VOID: u32 = 1;
    /// An integer of at most 32 bits that is signed.
    pub(crate) const NARROW_SIGNED: u32 = 2;
    /// An unsigned integer of at most 32 bits, and `char`.
    pub(crate) const NARROW_UNSIGNED: u32 = 3;
    /// `i64`.
    pub(crate) const WIDE_SIGNED: u32 = 4;
    /// `u64`.
    pub(crate) const WIDE_UNSIGNED: u32 = 5;
    /// `f32`.
    pub(crate) const F32: u32 = 6;
    /// `f64`.
    pub(crate) const F64: u32 = 7;
    /// `bool`.
    pub(crate) const BOOL: u32 = 8;
    /// `str`.
    pub(crate) const TEXT: u32 = 9;
    /// `atom`.
    pub(crate) const ATOM: u32 = 10;
    /// The singleton type of one written atom.
    pub(crate) const SINGLETON: u32 = 11;
    /// `bytes`.
    pub(crate) const BYTES: u32 = 12;
    /// An anonymous tuple.
    pub(crate) const TUPLE: u32 = 13;
    /// An anonymous record.
    pub(crate) const RECORD: u32 = 14;
    /// An anonymous enum.
    pub(crate) const ENUM: u32 = 15;
    /// An anonymous union.
    pub(crate) const UNION: u32 = 16;
    /// `(array t)`.
    pub(crate) const ARRAY: u32 = 17;
    /// `(dict k v)`.
    pub(crate) const DICT: u32 = 18;
    /// A function type.
    pub(crate) const FUNCTION: u32 = 19;
    /// An interface type, `any` included: an interface value.
    pub(crate) const INTERFACE: u32 = 20;
    /// A declared type with no arguments.
    pub(crate) const DECLARED: u32 = 21;
    /// A declared type applied to arguments.
    pub(crate) const APPLIED: u32 = 22;
    /// `never`.
    pub(crate) const NEVER: u32 = 23;
    /// A generic parameter nothing fixed. It has no head, and is never built.
    pub(crate) const UNRESOLVED: u32 = 24;
}

/// The mask of a head's class.
pub(crate) const CLASS_MASK: u32 = 0xFF;

/// The bits a head's ordinal is shifted by.
pub(crate) const ORDINAL_SHIFT: u32 = 8;

/// The class of the constructor of `ty`.
pub(crate) fn class_of_type(ty: &Type) -> u32 {
    match ty {
        Type::Void => class::VOID,
        Type::I8 | Type::I16 | Type::I32 => class::NARROW_SIGNED,
        Type::U8 | Type::U16 | Type::U32 | Type::Char => class::NARROW_UNSIGNED,
        Type::I64 => class::WIDE_SIGNED,
        Type::U64 => class::WIDE_UNSIGNED,
        Type::F32 => class::F32,
        Type::F64 => class::F64,
        Type::Bool => class::BOOL,
        Type::Str => class::TEXT,
        Type::Atom => class::ATOM,
        Type::AtomSingleton(_) => class::SINGLETON,
        Type::Bytes => class::BYTES,
        Type::Tuple(_) => class::TUPLE,
        Type::Record(_) => class::RECORD,
        Type::Enum(_) => class::ENUM,
        Type::Union(_) => class::UNION,
        Type::Array(_) => class::ARRAY,
        Type::Dict(..) => class::DICT,
        Type::Function(_) => class::FUNCTION,
        Type::Interface(..) | Type::Any => class::INTERFACE,
        Type::Declared(_) => class::DECLARED,
        Type::Applied(..) => class::APPLIED,
        Type::Never => class::NEVER,
        Type::Param(_) => class::UNRESOLVED,
    }
}

/// The types that the heads of a module stand for, produced beside the module.
///
/// An entry is a type of the constructor, whose arguments are the
/// placeholders the host replaces by the descriptors it reads
/// (`Type::with_components`). A host that observes an interface value reads the
/// head of its descriptor, finds the entry, and reads the descriptors of the
/// arguments the same way, so no ordinal reaches an observation. The table is
/// not part of the module's bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DescriptorTable {
    entries: Vec<(u32, Type)>,
}

impl DescriptorTable {
    pub(crate) fn new(entries: Vec<(u32, Type)>) -> Self {
        Self { entries }
    }

    /// The number of heads the table names.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table names no head: a module that builds no descriptor.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The type a head stands for, with its own arguments in place of the
    /// descriptors that follow the head, or `None` for a head the module never
    /// built.
    #[must_use]
    pub fn type_of(&self, head: u32) -> Option<&Type> {
        self.entries
            .iter()
            .find(|(known, _)| *known == head)
            .map(|(_, ty)| ty)
    }

    /// Every head and the type it stands for, in head order.
    pub fn entries(&self) -> impl Iterator<Item = (u32, &Type)> {
        self.entries.iter().map(|(head, ty)| (*head, ty))
    }
}

/// The routine `(a: i32, b: i32) -> i32` that answers `1` when two descriptors
/// are the same type, where a descriptor offset of `0` stands for an argument
/// nothing fixed, which agrees with any type, as the reference interpreter's
/// binding of a repeated parameter does. `own` is the routine's own function
/// index, which it calls for each argument.
pub(crate) fn equal_routine(own: u32) -> Routine {
    // Params: 0 a, 1 b. Locals: 2 length, 3 the cells of a, 4 the cells of b,
    // 5 the index, 6 the offset of an argument of a, 7 that of b.
    let (length, cells_a, cells_b, at, pa, pb) = (2, 3, 4, 5, 6, 7);
    let reference = i32::from(CellClass::Ref.code());
    let mut code: Vec<Ins> = Vec::new();
    // An unresolved side agrees with anything.
    code.extend([
        get(0),
        I::I32Eqz,
        get(1),
        I::I32Eqz,
        I::I32Or,
        I::If(BlockType::Empty),
        c32(1),
        I::Return,
        I::End,
    ]);
    // The same number of components.
    code.extend([
        get(0),
        ld32(header::LEN),
        set(length),
        get(length),
        get(1),
        ld32(header::LEN),
        I::I32Ne,
        I::If(BlockType::Empty),
        c32(0),
        I::Return,
        I::End,
    ]);
    // The cells follow the class bytes, padded to 8.
    for (cells, object) in [(cells_a, 0), (cells_b, 1)] {
        code.extend([
            get(object),
            get(object),
            ld32(header::LEN),
            c32(7),
            I::I32Add,
            c32(-8),
            I::I32And,
            I::I32Add,
            c32(index(header::SIZE)),
            I::I32Add,
            set(cells),
        ]);
    }
    // The heads.
    code.extend([
        get(cells_a),
        ld32(0),
        get(cells_b),
        ld32(0),
        I::I32Ne,
        I::If(BlockType::Empty),
        c32(0),
        I::Return,
        I::End,
        c32(1),
        set(at),
        I::Block(BlockType::Empty),
        I::Loop(BlockType::Empty),
        get(at),
        get(length),
        I::I32GeU,
        I::BrIf(1),
    ]);
    // The offset of component `at` of each side, or zero when it is not a
    // reference.
    for (cells, object, held) in [(cells_a, 0, pa), (cells_b, 1, pb)] {
        code.extend([
            get(cells),
            get(at),
            c32(3),
            I::I32Shl,
            I::I32Add,
            ld32(0),
            c32(0),
            get(object),
            get(at),
            I::I32Add,
            ld8(header::SIZE),
            c32(reference),
            I::I32Eq,
            I::Select,
            set(held),
        ]);
    }
    code.extend([
        get(pa),
        get(pb),
        I::Call(own),
        I::I32Eqz,
        I::If(BlockType::Empty),
        c32(0),
        I::Return,
        I::End,
        get(at),
        c32(1),
        I::I32Add,
        set(at),
        I::Br(0),
        I::End,
        I::End,
        c32(1),
        I::End,
    ]);
    Routine {
        params: vec![ValType::I32, ValType::I32],
        results: vec![ValType::I32],
        locals: vec![ValType::I32; 6],
        body: code,
        export: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn void_is_the_head_one_in_every_module() {
        assert_eq!(class_of_type(&Type::Void), class::VOID);
        assert_eq!(class::VOID, 1);
    }

    #[test]
    fn an_interface_and_any_share_a_class() {
        assert_eq!(class_of_type(&Type::Any), class::INTERFACE);
    }
}
