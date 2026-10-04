//! Lowers the checked IR that Step 5a covers: the primitive literals and the
//! sequence that evaluates them.
//!
//! An expression leaves its value on the operand stack by its
//! [`ValueClass`], and a sequence discards each value but the last with the
//! plain `drop` of the runtime for a reference and the engine's `drop` for a
//! scalar. A node the step does not lower is returned as the forms it needs,
//! computed by the classifier that already names them, so a program that does
//! not lower produces an error and never a module that omits part of it.
//! Later steps add an arm here for each form they lower.

use std::collections::BTreeMap;

use vibra_ir::{CheckedFunction, Expr, Type, Value};
use wasm_encoder::{Ieee32, Ieee64, Instruction as I, ValType};

use crate::classify;
use crate::form::{Form, NotLowered};
use crate::layout::{self, Kind, header};
use crate::runtime::{Ins, Routines, ValueClass};

/// What a value of `ty` leaves on the operand stack, or `None` for a type this
/// step does not represent.
pub(crate) fn class_of(ty: &Type) -> Option<ValueClass> {
    Some(match ty {
        Type::Void => ValueClass::Void,
        Type::Char
        | Type::I8
        | Type::I16
        | Type::I32
        | Type::U8
        | Type::U16
        | Type::U32 => ValueClass::I32,
        Type::I64 | Type::U64 => ValueClass::I64,
        Type::F32 => ValueClass::F32,
        Type::F64 => ValueClass::F64,
        // `bool` is an enum arena value, and `str`, `bytes`, and the atoms are
        // arena values of their own kinds.
        Type::Bool | Type::Str | Type::Bytes | Type::Atom | Type::AtomSingleton(_) => {
            ValueClass::Ref
        }
        Type::Never
        | Type::Function(_)
        | Type::Declared(_)
        | Type::Record(_)
        | Type::Enum(_)
        | Type::Param(_)
        | Type::Applied(..)
        | Type::Tuple(_)
        | Type::Array(_)
        | Type::Dict(..)
        | Type::Union(_)
        | Type::Interface(..)
        | Type::Any => return None,
    })
}

/// A lowered function: what it returns and its body.
#[derive(Debug)]
pub(crate) struct FunctionCode {
    pub(crate) class: ValueClass,
    pub(crate) locals: Vec<ValType>,
    pub(crate) body: Vec<Ins>,
}

/// The passive data segments a module carries, one per distinct literal
/// content, in the order the program first uses them.
#[derive(Debug, Default)]
pub(crate) struct Segments {
    order: Vec<Vec<u8>>,
    index: BTreeMap<Vec<u8>, u32>,
}

impl Segments {
    fn intern(&mut self, bytes: Vec<u8>) -> u32 {
        if let Some(index) = self.index.get(&bytes) {
            return *index;
        }
        let index = u32::try_from(self.order.len()).unwrap_or(u32::MAX);
        self.index.insert(bytes.clone(), index);
        self.order.push(bytes);
        index
    }

    pub(crate) fn into_segments(self) -> Vec<Vec<u8>> {
        self.order
    }
}

/// Lowers functions against the routine indices of one module.
#[derive(Debug)]
pub(crate) struct Lowering<'a> {
    fns: &'a Routines,
    segments: Segments,
    built_object: bool,
}

/// The one scratch local a function that copies a literal into a new object
/// uses.
const SCRATCH: u32 = 0;

impl<'a> Lowering<'a> {
    pub(crate) fn new(fns: &'a Routines) -> Self {
        Self {
            fns,
            segments: Segments::default(),
            built_object: false,
        }
    }

    /// Whether any lowered function builds an arena object, so the module needs
    /// the constructor.
    pub(crate) const fn built_object(&self) -> bool {
        self.built_object
    }

    pub(crate) fn into_segments(self) -> Vec<Vec<u8>> {
        self.segments.into_segments()
    }

    /// Lowers one function.
    ///
    /// # Errors
    ///
    /// The forms the function uses that this step does not lower.
    pub(crate) fn function(
        &mut self,
        function: &CheckedFunction,
    ) -> Result<FunctionCode, NotLowered> {
        let mut body = Vec::new();
        let mut scratch = false;
        let class = self.expr(function.body(), &mut body, &mut scratch)?;
        if class_of(&function.signature().result()) != Some(class) {
            return Err(NotLowered::single(Form::Result));
        }
        body.push(I::End);
        Ok(FunctionCode {
            class,
            locals: if scratch {
                vec![ValType::I32]
            } else {
                Vec::new()
            },
            body,
        })
    }

    fn expr(
        &mut self,
        expr: &Expr,
        code: &mut Vec<Ins>,
        scratch: &mut bool,
    ) -> Result<ValueClass, NotLowered> {
        match expr {
            Expr::Literal { value, .. } => Ok(self.literal(value, code, scratch)),
            Expr::Sequence { expressions, .. } => {
                let mut class = ValueClass::Void;
                for expression in expressions {
                    self.discard(class, code);
                    class = self.expr(expression, code, scratch)?;
                }
                Ok(class)
            }
            other => Err(classify::forms_of(other)),
        }
    }

    /// Discards the value of `class` on the stack: a reference drops its
    /// count, and a scalar is popped.
    fn discard(&self, class: ValueClass, code: &mut Vec<Ins>) {
        match class {
            ValueClass::Void => {}
            ValueClass::I32 | ValueClass::I64 | ValueClass::F32 | ValueClass::F64 => {
                code.push(I::Drop);
            }
            ValueClass::Ref => code.push(I::Call(self.fns.drop)),
        }
    }

    fn literal(
        &mut self,
        value: &Value,
        code: &mut Vec<Ins>,
        scratch: &mut bool,
    ) -> ValueClass {
        match value {
            Value::Void => ValueClass::Void,
            Value::Char(value) => scalar32(code, u32::from(*value).cast_signed()),
            Value::I8(value) => scalar32(code, i32::from(*value)),
            Value::I16(value) => scalar32(code, i32::from(*value)),
            Value::I32(value) => scalar32(code, *value),
            Value::U8(value) => scalar32(code, i32::from(*value)),
            Value::U16(value) => scalar32(code, i32::from(*value)),
            Value::U32(value) => scalar32(code, value.cast_signed()),
            Value::I64(value) => scalar64(code, *value),
            Value::U64(value) => scalar64(code, value.cast_signed()),
            Value::F32(bits) => {
                code.push(I::F32Const(Ieee32::new(*bits)));
                ValueClass::F32
            }
            Value::F64(bits) => {
                code.push(I::F64Const(Ieee64::new(*bits)));
                ValueClass::F64
            }
            // `bool` is the enum whose variants are `false` and `true`, in
            // declaration order, with `void` payloads.
            Value::Bool(value) => {
                self.object(Kind::Enum, 0, u32::from(*value), None, code, scratch)
            }
            Value::Str(text) => {
                let (length, bytes) = utf32(text);
                self.object(Kind::Str, length, 0, Some(bytes), code, scratch)
            }
            Value::Atom(name) => {
                let (length, bytes) = utf32(name);
                self.object(Kind::Atom, length, 0, Some(bytes), code, scratch)
            }
            Value::Bytes(bytes) => {
                let length = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
                self.object(Kind::Bytes, length, 0, Some(bytes.clone()), code, scratch)
            }
        }
    }

    /// Builds an object of `kind` with `len` components, and copies `data`,
    /// when there is any, into its payload from a passive data segment.
    fn object(
        &mut self,
        kind: Kind,
        len: u32,
        variant: u32,
        data: Option<Vec<u8>>,
        code: &mut Vec<Ins>,
        scratch: &mut bool,
    ) -> ValueClass {
        self.built_object = true;
        let stride = layout::row(kind).stride;
        code.extend([
            I::I32Const(kind.code().cast_signed()),
            I::I32Const(stride.cast_signed()),
            I::I32Const(len.cast_signed()),
            I::I32Const(variant.cast_signed()),
            I::Call(self.fns.new),
        ]);
        if let Some(data) = data {
            let count = u32::try_from(data.len()).unwrap_or(u32::MAX);
            let segment = self.segments.intern(data);
            *scratch = true;
            code.extend([
                I::LocalTee(SCRATCH),
                I::I32Const(header::SIZE.cast_signed()),
                I::I32Add,
                I::I32Const(0),
                I::I32Const(count.cast_signed()),
                I::MemoryInit {
                    mem: 0,
                    data_index: segment,
                },
                I::LocalGet(SCRATCH),
            ]);
        }
        ValueClass::Ref
    }
}

fn scalar32(code: &mut Vec<Ins>, value: i32) -> ValueClass {
    code.push(I::I32Const(value));
    ValueClass::I32
}

fn scalar64(code: &mut Vec<Ins>, value: i64) -> ValueClass {
    code.push(I::I64Const(value));
    ValueClass::I64
}

/// The scalar count and the 4-byte little-endian encoding of a string's
/// Unicode scalars.
fn utf32(text: &str) -> (u32, Vec<u8>) {
    let mut bytes = Vec::with_capacity(text.len() * 4);
    let mut count = 0_u32;
    for character in text.chars() {
        bytes.extend(u32::from(character).to_le_bytes());
        count = count.saturating_add(1);
    }
    (count, bytes)
}
