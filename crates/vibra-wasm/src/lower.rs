//! The lowering of checked IR to the functions of a transition module.
//!
//! # Instances
//!
//! The unit of lowering is an **instance**: a function of the module set, or
//! the body of a closure, at one list of closed type arguments. The entry (or
//! each test) is the first instance, and every function value, closure, and
//! direct call the lowering meets names another, which is lowered in turn, so
//! a module holds exactly the instances the program can reach. A generic
//! callable is therefore specialized at each instantiation, which the runtime
//! chapter permits because no observation changes ("Generic instantiation"),
//! and no type descriptor exists at run time.
//!
//! Lowering runs twice. The first pass numbers the instances and the native
//! imports and measures each instance's frame; the second emits the code,
//! which needs every frame size and import index, and is otherwise the same
//! walk, so both passes see the same instances in the same order.
//!
//! # The frame
//!
//! Every instance is one WebAssembly function `(frame) -> ()` run by the
//! dispatcher (`crate::layout`, "Activations and the dispatcher"). Its slots
//! are the parameters, the environment, the other slots of the checked IR,
//! and the temporaries: slot `s` of the checked IR is frame slot `s` below the
//! arity and `s + 1` above it, so the environment sits between them. Every
//! value lives in a slot, and a slot's class byte says whether it owns a
//! reference, so leaving a frame balances every count, however the
//! activation ends.
//!
//! An expression is lowered into a destination slot, which then holds the
//! value and owns it when it is a reference. Reading a variable copies it and
//! takes a count; consuming an operand moves it (into an object, a callee, or
//! the caller) or drops it.
//!
//! # Fail-closed
//!
//! A node the transition profile does not run is
//! [`EmitError::Unsupported`]: a module-value read, a widening to an interface
//! value or a contract call, a guard consequence, a fallible constructor, a
//! `Dict` literal or lookup, the rows that apply a function value (`list.fold`)
//! or return a remainder (`list.next`, `dict.next`), a procedure, and a plan.
//! The whole module fails, so no program is partly executed.

mod assertion;
mod dispatch;
mod intrinsic;
mod pattern;

use std::borrow::Cow;

use vibra_ir::external::{CompilerIntrinsic, NATIVE_ROWS};
use vibra_ir::{
    CheckedModuleSet, ClosureBody, Construction, Expr, FunctionBody, Library, Node,
    SourceOrigin, Type, Value,
};
use wasm_encoder::{BlockType, Instruction as I, ValType};

use crate::encode::FunctionCode;
use crate::error::EmitError;
use crate::layout::{self, CellClass, Kind, cells_offset, frame, header, state};
use crate::runtime::{
    INVALID_CHECKED_PROGRAM, Ins, Routine, Routines, ValueClass, c32, c64, get, index,
    ld8, ld32, ld64, set, st8, st32, st64,
};
use crate::types::{Shape, Types, field_position};

pub(crate) use assertion::deep_equal;

/// The frame, the one parameter of a lowered function.
pub(crate) const FRAME: u32 = 0;
/// The block the dispatching loop runs next.
const PC: u32 = 1;
/// The callee's frame while a call moves its operands in.
const CALLEE: u32 = 2;
/// The object being built.
const OBJECT: u32 = 3;
/// A scratch word.
pub(crate) const SCRATCH: u32 = 4;
/// The function value of the call being made.
const FV: u32 = 5;
/// The table index the function value names.
const FUNC: u32 = 6;
/// The frame size the function value names.
const SLOTS: u32 = 7;
/// The cells of the function value.
const CELLS: u32 = 8;
/// The first operand of a primitive row, as an exact integer.
pub(crate) const WA: u32 = 9;
/// The second operand of a primitive row.
pub(crate) const WB: u32 = 10;
/// The value of a primitive row.
pub(crate) const WR: u32 = 11;

/// The locals every lowered function declares after its parameter.
const FIXED: [ValType; 11] = [
    ValType::I32,
    ValType::I32,
    ValType::I32,
    ValType::I32,
    ValType::I32,
    ValType::I32,
    ValType::I32,
    ValType::I32,
    ValType::I64,
    ValType::I64,
    ValType::I64,
];

/// The first local of the pairs a tail call stages its operands in: a cell
/// (`i64`) and its class byte (`i32`).
const STAGE: u32 = 12;

/// The most instances a module holds; a program that reaches more, through
/// polymorphic recursion, does not fit.
const MAX_INSTANCES: usize = 1 << 16;

/// The cells of a function value.
mod fv {
    /// The table index of the instance.
    pub(super) const FUNCTION: u32 = 0;
    /// The frame size of the instance.
    pub(super) const SLOTS: u32 = 1;
    /// The operands the instance takes.
    pub(super) const ARITY: u32 = 2;
    /// The first capture.
    pub(super) const CAPTURES: u32 = 3;
}

/// What an instance runs.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Callable<'a> {
    /// A function of the module set, by index.
    Function(usize),
    /// The body of a closure.
    Closure(&'a ClosureBody),
}

impl Callable<'_> {
    fn same(self, other: Self) -> bool {
        match (self, other) {
            (Self::Function(left), Self::Function(right)) => left == right,
            (Self::Closure(left), Self::Closure(right)) => std::ptr::eq(left, right),
            _ => false,
        }
    }
}

/// A callable at closed type arguments.
#[derive(Clone, Debug)]
pub(crate) struct Instance<'a> {
    callable: Callable<'a>,
    arguments: Vec<Type>,
}

/// The passive data segments, deduplicated in the order they are first used.
#[derive(Debug, Default)]
struct Segments {
    list: Vec<Vec<u8>>,
}

impl Segments {
    fn intern(&mut self, bytes: Vec<u8>) -> u32 {
        let position = self
            .list
            .iter()
            .position(|known| *known == bytes)
            .unwrap_or_else(|| {
                self.list.push(bytes);
                self.list.len() - 1
            });
        u32::try_from(position).unwrap_or(u32::MAX)
    }
}

/// One entry of the origin table: where a stop was written, and the type the
/// runner reads a failed `assert.equal`'s operands at.
pub(crate) type OriginEntry = (SourceOrigin, Option<Type>);

/// The lowering of one module.
pub(crate) struct Lowering<'a> {
    set: &'a CheckedModuleSet,
    types: Types<'a>,
    instances: Vec<Instance<'a>>,
    /// The frame size of each instance, from the first pass; empty in it.
    slots: Vec<u32>,
    final_pass: bool,
    fns: Routines,
    /// The native imports in index order, from the first pass.
    imports: Vec<CompilerIntrinsic>,
    /// The native rows this pass used.
    used: Vec<CompilerIntrinsic>,
    segments: Segments,
    origins: Vec<OriginEntry>,
    /// Whether the module runs tests, where an assertion records a failure.
    tests: bool,
}

/// What the second pass hands to the encoder.
pub(crate) struct Lowered {
    /// The code of each instance, in table order.
    pub(crate) functions: Vec<FunctionCode>,
    /// The frame size of each instance.
    pub(crate) slots: Vec<u32>,
    /// The native imports, in index order.
    pub(crate) imports: Vec<CompilerIntrinsic>,
    pub(crate) segments: Vec<Vec<u8>>,
    pub(crate) origins: Vec<OriginEntry>,
    /// The routines after the object constructor.
    pub(crate) extra: Vec<Routine>,
}

impl<'a> Lowering<'a> {
    /// The first pass over `set`.
    pub(crate) fn new(set: &'a CheckedModuleSet, tests: bool) -> Self {
        Self {
            set,
            types: Types::new(set),
            instances: Vec::new(),
            slots: Vec::new(),
            final_pass: false,
            fns: Routines::plan(0),
            imports: Vec::new(),
            used: Vec::new(),
            segments: Segments::default(),
            origins: Vec::new(),
            tests,
        }
    }

    /// The instance of `callable` at `arguments`, numbered on first use.
    pub(crate) fn instance(
        &mut self,
        callable: Callable<'a>,
        arguments: Vec<Type>,
        origin: &SourceOrigin,
    ) -> Result<u32, EmitError> {
        let found = self.instances.iter().position(|instance| {
            instance.callable.same(callable) && instance.arguments == arguments
        });
        let position = match found {
            Some(position) => position,
            None if self.final_pass => {
                return Err(EmitError::malformed(
                    "an instance the first pass did not reach",
                    origin,
                ));
            }
            None if self.instances.len() >= MAX_INSTANCES => {
                return Err(EmitError::ModuleSize);
            }
            None => {
                self.instances.push(Instance {
                    callable,
                    arguments,
                });
                self.instances.len() - 1
            }
        };
        u32::try_from(position).map_err(|_| EmitError::ModuleSize)
    }

    fn slots_of(&self, instance: u32) -> u32 {
        self.slots.get(instance as usize).copied().unwrap_or(0)
    }

    /// The function index of the import of `intrinsic`.
    fn import(&mut self, intrinsic: CompilerIntrinsic) -> u32 {
        if !self.used.contains(&intrinsic) {
            self.used.push(intrinsic);
        }
        self.imports
            .iter()
            .position(|known| *known == intrinsic)
            .and_then(|position| u32::try_from(position).ok())
            .unwrap_or(0)
    }

    /// The ordinal of a new entry of the origin table.
    fn origin_ordinal(&mut self, origin: &SourceOrigin, operand: Option<Type>) -> u32 {
        self.origins.push((origin.clone(), operand));
        u32::try_from(self.origins.len()).unwrap_or(u32::MAX)
    }

    /// The index of the canonical equality routine, the first extra routine.
    const fn deep_equal(&self) -> u32 {
        self.fns.new + 1
    }

    /// Lowers every instance the roots reach, numbering new ones as they are
    /// met, and gives each frame size.
    pub(crate) fn measure(&mut self) -> Result<(), EmitError> {
        let mut position = 0;
        while position < self.instances.len() {
            let (_, slots) = self.lower(position)?;
            self.slots.push(slots);
            position += 1;
        }
        Ok(())
    }

    /// The second pass, with the frame sizes and imports of this one.
    pub(crate) fn emit(self) -> Result<Lowered, EmitError> {
        let imports = NATIVE_ROWS
            .iter()
            .map(|row| row.intrinsic)
            .filter(|intrinsic| self.used.contains(intrinsic))
            .collect::<Vec<_>>();
        let count = u32::try_from(imports.len() + self.instances.len())
            .map_err(|_| EmitError::ModuleSize)?;
        let mut second = Self {
            set: self.set,
            types: self.types,
            instances: self.instances,
            slots: self.slots,
            final_pass: true,
            fns: Routines::plan(count),
            imports,
            used: Vec::new(),
            segments: Segments::default(),
            origins: Vec::new(),
            tests: self.tests,
        };
        let mut functions = Vec::with_capacity(second.instances.len());
        for position in 0..second.instances.len() {
            let (code, slots) = second.lower(position)?;
            if Some(&slots) != second.slots.get(position) {
                return Err(EmitError::Malformed {
                    what: "a frame that changed between the passes".to_owned(),
                    origin: None,
                });
            }
            functions.push(code);
        }
        let extra = vec![deep_equal(&second.fns)];
        Ok(Lowered {
            functions,
            slots: second.slots,
            imports: second.imports,
            segments: second.segments.list,
            origins: second.origins,
            extra,
        })
    }

    /// Lowers the instance at `position`: its code and its frame size.
    fn lower(&mut self, position: usize) -> Result<(FunctionCode, u32), EmitError> {
        let instance = self
            .instances
            .get(position)
            .cloned()
            .ok_or(EmitError::ModuleSize)?;
        let known = if self.final_pass {
            self.slots.get(position).copied().unwrap_or(0)
        } else {
            0
        };
        let set = self.set;
        match instance.callable {
            Callable::Function(index) => {
                let function =
                    set.functions()
                        .get(index)
                        .ok_or_else(|| EmitError::Malformed {
                            what: format!("no function {index}"),
                            origin: None,
                        })?;
                if let Some(guard) = &function.entry_guard {
                    return Err(EmitError::unsupported(
                        "entryGuard",
                        &guard.consequence.origin,
                    ));
                }
                if let Some(guard) = &function.exit_guard {
                    return Err(EmitError::unsupported(
                        "exitGuard",
                        &guard.guard.consequence.origin,
                    ));
                }
                let signature = match function
                    .signature
                    .type_info()
                    .instantiate(&instance.arguments)
                {
                    Type::Fn(signature) => *signature,
                    Type::Proc(_) => {
                        return Err(EmitError::unsupported("proc", &function.origin));
                    }
                    other => {
                        return Err(EmitError::malformed(
                            format!("a function of type {other}"),
                            &function.origin,
                        ));
                    }
                };
                let shape = FrameShape {
                    arity: signature.slot_count(),
                    slots: function.slot_count,
                    result: signature.result,
                    captures: None,
                };
                let mut builder = Builder::new(
                    self,
                    known,
                    instance.arguments,
                    shape,
                    &function.origin,
                )?;
                match &function.body {
                    FunctionBody::Expr(body) => builder.body(body)?,
                    FunctionBody::Intrinsic {
                        intrinsic,
                        type_arguments,
                    } => builder.intrinsic_body(
                        *intrinsic,
                        type_arguments,
                        &function.origin,
                    )?,
                }
                Ok(builder.finish())
            }
            Callable::Closure(closure) => {
                let origin = &closure.body.origin;
                let signature =
                    match closure.value_type.instantiate(&instance.arguments) {
                        Type::Fn(signature) => *signature,
                        Type::Proc(_) => {
                            return Err(EmitError::unsupported("proc", origin));
                        }
                        other => {
                            return Err(EmitError::malformed(
                                format!("a closure of type {other}"),
                                origin,
                            ));
                        }
                    };
                let shape = FrameShape {
                    arity: signature.slot_count(),
                    slots: closure.slot_count,
                    result: signature.result,
                    captures: Some(&closure.captures),
                };
                let mut builder =
                    Builder::new(self, known, instance.arguments, shape, origin)?;
                builder.body(&closure.body)?;
                Ok(builder.finish())
            }
        }
    }
}

/// What a lowered function's frame is made of.
struct FrameShape<'a> {
    /// The operands the instance takes.
    arity: usize,
    /// The slots of the checked IR, the parameters included.
    slots: usize,
    /// The closed result type.
    result: Type,
    /// The types of a closure's captures, open in the instance's binders.
    captures: Option<&'a [Type]>,
}

/// A numbered block's final instruction.
#[derive(Debug)]
enum Term {
    /// Continues at a block.
    Goto(u32),
    /// Continues at one of two blocks, by the `i32` on the operand stack, which
    /// is nonzero for the first.
    Branch { then_block: u32, else_block: u32 },
    /// Pushes the callee's frame, moves the operands into it, and returns to the
    /// dispatcher, which continues the caller at `resume`.
    Call {
        callee: Callee,
        moves: Vec<Move>,
        arity: u32,
        resume: u32,
    },
    /// Replaces the current frame by the callee's, with the operands moved in.
    Tail {
        callee: Callee,
        moves: Vec<Move>,
        arity: u32,
    },
    /// Hands the cell of a slot to the caller and leaves the frame.
    Return { slot: u32 },
}

/// What a call enters.
#[derive(Clone, Copy, Debug)]
enum Callee {
    /// An instance, of a known frame size.
    Named { function: u32, slots: u32 },
    /// The function value in a slot of the caller, whose instance and frame
    /// size are read from the value.
    Value { slot: u32 },
}

/// One operand of a call: a slot of the caller, and the slot of the callee it
/// moves into.
#[derive(Clone, Copy, Debug)]
struct Move {
    from: u32,
    to: u32,
}

/// One part of an object being built.
#[derive(Clone, Copy, Debug)]
enum Part {
    /// The cell of a slot of this class, moved into the object.
    Slot { slot: u32, class: ValueClass },
}

/// The case index of an object that is being built.
#[derive(Clone, Copy, Debug)]
enum Variant {
    /// A case index the code knows.
    Fixed(u32),
    /// An `i32` local, plus `bias`: a case index that a row computed.
    Local { local: u32, bias: i32 },
}

#[derive(Debug)]
struct Block {
    code: Vec<Ins>,
    term: Term,
}

/// The function being lowered.
struct Builder<'l, 'a> {
    lowering: &'l mut Lowering<'a>,
    /// The slot count the frame has: known in the second pass, `0` in the first.
    known: u32,
    /// The instance's closed type arguments.
    arguments: Vec<Type>,
    arity: u32,
    /// The closed result type.
    result: Type,
    captures: Option<&'a [Type]>,
    next_temp: u32,
    max_slots: u32,
    blocks: Vec<Option<Block>>,
    current: u32,
    code: Vec<Ins>,
    /// The most operands a tail call stages.
    stage: usize,
}

/// The class byte a value of `class` has in a cell, as the runner reads it.
const fn cell_class(class: ValueClass) -> CellClass {
    match class {
        ValueClass::Void | ValueClass::I32 | ValueClass::Dyn => CellClass::I32,
        ValueClass::I64 => CellClass::I64,
        ValueClass::F32 => CellClass::F32,
        ValueClass::F64 => CellClass::F64,
        ValueClass::Ref => CellClass::Ref,
    }
}

/// The count and the 4-byte little-endian encoding of a string's Unicode
/// scalars.
fn utf32(text: &str) -> (u32, Vec<u8>) {
    let mut bytes = Vec::with_capacity(text.len() * 4);
    let mut count = 0_u32;
    for character in text.chars() {
        bytes.extend(u32::from(character).to_le_bytes());
        count = count.saturating_add(1);
    }
    (count, bytes)
}

/// The bits of the cell of a scalar value.
fn scalar_bits(value: &Value) -> Option<u64> {
    let low = |pattern: i32| u64::from(pattern.cast_unsigned());
    Some(match value {
        Value::I8(value) => low(i32::from(*value)),
        Value::I16(value) => low(i32::from(*value)),
        Value::I32(value) => low(*value),
        Value::U8(value) => u64::from(*value),
        Value::U16(value) => u64::from(*value),
        Value::U32(value) | Value::F32(value) => u64::from(*value),
        Value::Char(value) => u64::from(u32::from(*value)),
        Value::I64(value) => value.cast_unsigned(),
        Value::U64(value) | Value::F64(value) => *value,
        Value::Void => 0,
        _ => return None,
    })
}

impl<'l, 'a> Builder<'l, 'a> {
    fn new(
        lowering: &'l mut Lowering<'a>,
        known: u32,
        arguments: Vec<Type>,
        shape: FrameShape<'a>,
        origin: &SourceOrigin,
    ) -> Result<Self, EmitError> {
        if shape.slots < shape.arity {
            return Err(EmitError::malformed("a slot count below the arity", origin));
        }
        let arity = u32::try_from(shape.arity).map_err(|_| EmitError::ModuleSize)?;
        let first_temp = u32::try_from(shape.slots)
            .ok()
            .and_then(|slots| slots.checked_add(1))
            .ok_or(EmitError::ModuleSize)?;
        Ok(Self {
            lowering,
            known,
            arguments,
            arity,
            result: shape.result,
            captures: shape.captures,
            next_temp: first_temp,
            max_slots: first_temp,
            blocks: Vec::new(),
            current: 0,
            code: Vec::new(),
            stage: 0,
        })
    }

    // -- blocks ---------------------------------------------------------------

    fn new_block(&mut self) -> u32 {
        self.blocks.push(None);
        u32::try_from(self.blocks.len() - 1).unwrap_or(u32::MAX)
    }

    fn start(&mut self, block: u32) {
        self.current = block;
        self.code = Vec::new();
    }

    fn end(&mut self, term: Term) {
        let code = std::mem::take(&mut self.code);
        if let Some(slot) = self.blocks.get_mut(self.current as usize) {
            *slot = Some(Block { code, term });
        }
    }

    /// Ends the block with a jump and starts `next`.
    fn goto(&mut self, target: u32, next: u32) {
        self.end(Term::Goto(target));
        self.start(next);
    }

    /// Ends the block with a branch on the `i32` the code leaves, to a new
    /// block that continues when it is nonzero, and to `fail` otherwise.
    fn continue_unless(&mut self, fail: u32) {
        let next = self.new_block();
        self.end(Term::Branch {
            then_block: next,
            else_block: fail,
        });
        self.start(next);
    }

    /// Starts a block nothing jumps to, after a return or a tail call.
    fn dead(&mut self) {
        let block = self.new_block();
        self.start(block);
    }

    /// Allocates a temporary slot.
    fn temp(&mut self) -> u32 {
        let slot = self.next_temp;
        self.next_temp += 1;
        self.max_slots = self.max_slots.max(self.next_temp);
        slot
    }

    /// Releases every temporary allocated since `mark`.
    const fn release_to(&mut self, mark: u32) {
        self.next_temp = mark;
    }

    // -- types ----------------------------------------------------------------

    /// `ty` at the instance's type arguments.
    fn close(&self, ty: &Type) -> Type {
        if self.arguments.is_empty() {
            ty.clone()
        } else {
            ty.instantiate(&self.arguments)
        }
    }

    /// The closed type of `node`.
    fn type_of(&self, node: &Node) -> Result<Type, EmitError> {
        Ok(self.close(&self.lowering.types.type_of(node)?))
    }

    fn shape(&self, ty: &Type, origin: &SourceOrigin) -> Result<Shape, EmitError> {
        self.lowering.types.shape(ty, origin)
    }

    fn class(&self, ty: &Type, origin: &SourceOrigin) -> Result<ValueClass, EmitError> {
        self.lowering.types.class(ty, origin)
    }

    fn node_class(&self, node: &Node) -> Result<ValueClass, EmitError> {
        self.class(&self.type_of(node)?, &node.origin)
    }

    /// The closed library type `library` at `arguments`.
    fn library_type(
        &self,
        library: Library,
        arguments: Vec<Type>,
        origin: &SourceOrigin,
    ) -> Result<Type, EmitError> {
        self.lowering.types.library_type(library, arguments, origin)
    }

    /// The case index of `case` in the variant type `ty`.
    fn case_index(
        &self,
        ty: &Type,
        case: &str,
        origin: &SourceOrigin,
    ) -> Result<u32, EmitError> {
        self.lowering.types.case_index(ty, case, origin)
    }

    // -- slots ----------------------------------------------------------------

    const fn cell(&self, slot: u32) -> u32 {
        frame::cell_offset(self.known, slot)
    }

    /// The frame slot of slot `slot` of the checked IR.
    fn ir_slot(&self, slot: usize) -> Result<u32, EmitError> {
        let slot = u32::try_from(slot).map_err(|_| EmitError::ModuleSize)?;
        Ok(if slot < self.arity { slot } else { slot + 1 })
    }

    /// The environment slot: the function value being run.
    const fn env_slot(&self) -> u32 {
        self.arity
    }

    fn push(&mut self, instructions: &[Ins]) {
        self.code.extend_from_slice(instructions);
    }

    /// The `i32` offset in `slot`, a reference.
    fn load_ref(&mut self, slot: u32) {
        let at = self.cell(slot);
        self.push(&[get(FRAME), ld32(at)]);
    }

    /// Writes the class byte of `slot`.
    fn set_byte(&mut self, slot: u32, code: u8) {
        self.push(&[
            get(FRAME),
            c32(i32::from(code)),
            st8(frame::class_offset(slot)),
        ]);
    }

    /// Marks the slot as owning a reference.
    fn own(&mut self, slot: u32) {
        self.set_byte(slot, CellClass::Ref.code());
    }

    /// Marks the slot as owning nothing.
    fn disown(&mut self, slot: u32) {
        self.set_byte(slot, 0);
    }

    /// Stores constant bits in a slot's cell, with the class byte of `class`.
    fn store_bits(&mut self, slot: u32, bits: u64, class: ValueClass) {
        let at = self.cell(slot);
        self.push(&[get(FRAME), c64(bits.cast_signed()), st64(at)]);
        self.set_byte(slot, cell_class(class).code());
    }

    /// Copies the cell of `from` to the cell of `to`, which does not change
    /// who owns what.
    fn copy_cell(&mut self, to: u32, from: u32) {
        let (to, from) = (self.cell(to), self.cell(from));
        self.push(&[get(FRAME), get(FRAME), ld64(from), st64(to)]);
    }

    /// Moves the value of `from` to `to`: the cell and the class byte, and
    /// `from` is left owning nothing.
    fn move_slot(&mut self, to: u32, from: u32) {
        self.copy_cell(to, from);
        self.push(&[
            get(FRAME),
            get(FRAME),
            ld8(frame::class_offset(from)),
            st8(frame::class_offset(to)),
        ]);
        self.disown(from);
    }

    /// Drops the reference a slot owns, and disowns it.
    fn drop_slot(&mut self, slot: u32) {
        self.load_ref(slot);
        let drop = self.lowering.fns.drop;
        self.push(&[I::Call(drop)]);
        self.disown(slot);
    }

    /// Drops what a slot owns, when its class is a reference.
    fn discard(&mut self, slot: u32, class: ValueClass) {
        if class == ValueClass::Ref {
            self.drop_slot(slot);
        }
    }

    /// Drops the reference a slot owns, if it owns one: a slot of the checked
    /// IR is written again by a later binding.
    fn drop_if_owned(&mut self, slot: u32) {
        self.push(&[
            get(FRAME),
            ld8(frame::class_offset(slot)),
            c32(i32::from(CellClass::Ref.code())),
            I::I32Eq,
            I::If(BlockType::Empty),
        ]);
        self.drop_slot(slot);
        self.push(&[I::End]);
    }

    /// Takes ownership of the copy of a cell that was just made into `slot`:
    /// a reference takes a `dup`, and the class byte is written.
    fn claim(&mut self, slot: u32, class: ValueClass) {
        if class == ValueClass::Ref {
            self.load_ref(slot);
            let dup = self.lowering.fns.dup;
            self.push(&[I::Call(dup)]);
            self.own(slot);
        } else {
            self.set_byte(slot, cell_class(class).code());
        }
    }

    /// Stops with the trap `@runtime.invalidCheckedProgram` at `origin`.
    fn stop_invalid(&mut self, origin: &SourceOrigin) {
        let ordinal = self.lowering.origin_ordinal(origin, None);
        self.push(&[
            c32(0),
            c32(1),
            st32(state::STATUS),
            c32(0),
            c32(INVALID_CHECKED_PROGRAM),
            st32(state::TRAP_CODE),
            c32(0),
            c32(ordinal.cast_signed()),
            st32(state::ORIGIN),
            I::Unreachable,
        ]);
    }

    // -- objects --------------------------------------------------------------

    /// Builds an object of `kind` from `parts`, moving each into it, and
    /// stores it in `dest`, which then owns it.
    fn build(&mut self, kind: Kind, variant: Variant, parts: &[Part], dest: u32) {
        let len = u32::try_from(parts.len()).unwrap_or(u32::MAX);
        let new = self.lowering.fns.new;
        self.push(&[
            c32(kind.code().cast_signed()),
            c32(index(layout::row(kind).stride)),
            c32(index(len)),
        ]);
        match variant {
            Variant::Fixed(case) => self.push(&[c32(case.cast_signed())]),
            Variant::Local { local, bias } => {
                self.push(&[get(local), c32(bias), I::I32Add]);
            }
        }
        self.push(&[I::Call(new), set(OBJECT)]);
        let cells = cells_offset(len);
        for (position, part) in (0_u32..).zip(parts) {
            let Part::Slot { slot, class } = *part;
            let at = self.cell(slot);
            self.push(&[
                get(OBJECT),
                get(FRAME),
                ld64(at),
                st64(cells + 8 * position),
                get(OBJECT),
                c32(i32::from(cell_class(class).code())),
                st8(header::SIZE + position),
            ]);
            self.disown(slot);
        }
        self.store_object(dest);
    }

    /// Stores the object in `OBJECT` in `dest`, which then owns it.
    fn store_object(&mut self, dest: u32) {
        let at = self.cell(dest);
        self.push(&[get(FRAME), get(OBJECT), I::I64ExtendI32U, st64(at)]);
        self.own(dest);
    }

    /// Builds an object of the characters or bytes `bytes`, `len` units.
    fn build_units(&mut self, kind: Kind, len: u32, bytes: Vec<u8>, dest: u32) {
        let new = self.lowering.fns.new;
        let size = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        self.push(&[
            c32(kind.code().cast_signed()),
            c32(index(layout::row(kind).stride)),
            c32(index(len)),
            c32(0),
            I::Call(new),
            set(OBJECT),
        ]);
        if size != 0 {
            let segment = self.lowering.segments.intern(bytes);
            self.push(&[
                get(OBJECT),
                c32(index(header::SIZE)),
                I::I32Add,
                c32(0),
                c32(index(size)),
                I::MemoryInit {
                    mem: 0,
                    data_index: segment,
                },
            ]);
        }
        self.store_object(dest);
    }

    /// Copies component `position` of the object of `len` cells in `object`
    /// into `dest`, which borrows it: the class byte says it owns nothing.
    fn borrow_component(&mut self, object: u32, position: u32, len: u32, dest: u32) {
        let (object, at) = (self.cell(object), self.cell(dest));
        self.push(&[
            get(FRAME),
            get(FRAME),
            ld32(object),
            ld64(cells_offset(len) + 8 * position),
            st64(at),
        ]);
        self.disown(dest);
    }

    /// Copies component `position` of the object in `object` into `dest`,
    /// which owns a count of it.
    fn take_component(
        &mut self,
        object: u32,
        position: u32,
        len: u32,
        class: ValueClass,
        dest: u32,
    ) {
        self.borrow_component(object, position, len, dest);
        self.claim(dest, class);
    }

    /// Leaves on the operand stack whether the object in `slot` is the case
    /// `case`.
    fn is_case(&mut self, slot: u32, case: u32) {
        self.load_ref(slot);
        self.push(&[ld32(header::VARIANT), c32(case.cast_signed()), I::I32Eq]);
    }

    /// Leaves on the operand stack whether the `Bool` in `slot` is `true`,
    /// and drops it.
    fn truth(&mut self, slot: u32, origin: &SourceOrigin) -> Result<(), EmitError> {
        let boolean = self.library_type(Library::Bool, Vec::new(), origin)?;
        let truth = self.case_index(&boolean, "true", origin)?;
        self.is_case(slot, truth);
        self.push(&[set(SCRATCH)]);
        self.drop_slot(slot);
        self.push(&[get(SCRATCH)]);
        Ok(())
    }

    // -- the function ---------------------------------------------------------

    /// Lowers an expression body.
    fn body(&mut self, body: &'a Node) -> Result<(), EmitError> {
        let first = self.new_block();
        self.start(first);
        let result = self.temp();
        self.expr(body, result)?;
        self.end(Term::Return { slot: result });
        Ok(())
    }

    /// Lowers the body of an external function: its registry row applied to
    /// its parameters.
    fn intrinsic_body(
        &mut self,
        intrinsic: CompilerIntrinsic,
        type_arguments: &[Type],
        origin: &SourceOrigin,
    ) -> Result<(), EmitError> {
        let first = self.new_block();
        self.start(first);
        let operands = (0..self.arity).collect::<Vec<_>>();
        let type_arguments = type_arguments
            .iter()
            .map(|ty| self.close(ty))
            .collect::<Vec<_>>();
        let result = self.temp();
        self.intrinsic_on(intrinsic, &type_arguments, &operands, origin, result)?;
        self.end(Term::Return { slot: result });
        Ok(())
    }

    // -- expressions ----------------------------------------------------------

    /// Evaluates `node` into `dest`, which must own nothing, and which then
    /// owns the value if it is a reference. A node of type `Never` writes
    /// nothing.
    fn expr(&mut self, node: &'a Node, dest: u32) -> Result<(), EmitError> {
        let origin = &node.origin;
        match &node.expr {
            Expr::Literal(constant) => {
                let ty = self.close(constant.value_type());
                self.constant(constant.value(), &ty, origin, dest)
            }
            Expr::Intrinsic {
                intrinsic,
                type_arguments,
                arguments,
            } => {
                let mark = self.next_temp;
                let operands = self.operands(arguments)?;
                let type_arguments = type_arguments
                    .iter()
                    .map(|ty| self.close(ty))
                    .collect::<Vec<_>>();
                self.intrinsic_on(
                    *intrinsic,
                    &type_arguments,
                    &operands,
                    origin,
                    dest,
                )?;
                self.release_to(mark);
                Ok(())
            }
            Expr::Sequence(statements) => self.sequence(statements, origin, dest),
            Expr::Variable { slot, value_type } => {
                let class = self.class(&self.close(value_type), origin)?;
                let from = self.ir_slot(*slot)?;
                self.copy_cell(dest, from);
                self.claim(dest, class);
                Ok(())
            }
            Expr::Global(_) => Err(EmitError::unsupported("global", origin)),
            Expr::Function {
                function,
                type_arguments,
            } => self.function_value(*function, type_arguments, origin, dest),
            Expr::Closure { closure, captures } => {
                self.closure_value(closure, captures, origin, dest)
            }
            Expr::Captured { slot, value_type } => {
                let class = self.class(&self.close(value_type), origin)?;
                let captures = self.captures.ok_or_else(|| {
                    EmitError::malformed("a capture outside a closure", origin)
                })?;
                let len = u32::try_from(captures.len())
                    .ok()
                    .and_then(|len| len.checked_add(fv::CAPTURES))
                    .ok_or(EmitError::ModuleSize)?;
                let position = u32::try_from(*slot)
                    .ok()
                    .and_then(|slot| slot.checked_add(fv::CAPTURES))
                    .filter(|position| *position < len)
                    .ok_or_else(|| {
                        EmitError::malformed("a capture out of range", origin)
                    })?;
                let env = self.env_slot();
                self.take_component(env, position, len, class, dest);
                Ok(())
            }
            Expr::Let { slot, value, body } => {
                let mark = self.next_temp;
                let held = self.temp();
                self.expr(value, held)?;
                match slot {
                    Some(slot) => {
                        let to = self.ir_slot(*slot)?;
                        self.drop_if_owned(to);
                        self.move_slot(to, held);
                    }
                    None => {
                        let class = self.node_class(value)?;
                        self.discard(held, class);
                    }
                }
                self.release_to(mark);
                self.expr(body, dest)
            }
            Expr::LetElse {
                pattern,
                value,
                fallback,
                body,
            } => {
                let mark = self.next_temp;
                let ty = self.type_of(value)?;
                let class = self.class(&ty, origin)?;
                let subject = self.temp();
                self.expr(value, subject)?;
                let (fail, join) = (self.new_block(), self.new_block());
                self.pattern(pattern, subject, &ty, fail, origin)?;
                self.discard(subject, class);
                let rest = self.new_block();
                self.goto(rest, fail);
                // The fallback leaves the activation: its type is `Never`.
                self.discard(subject, class);
                let ignored = self.temp();
                self.expr(fallback, ignored)?;
                self.stop_invalid(origin);
                self.goto(join, rest);
                self.release_to(mark);
                self.expr(body, dest)?;
                self.goto(join, join);
                Ok(())
            }
            Expr::Match {
                subject,
                arms,
                value_type,
            } => {
                let mark = self.next_temp;
                let ty = self.type_of(subject)?;
                let class = self.class(&ty, origin)?;
                self.class(&self.close(value_type), origin)?;
                let held = self.temp();
                self.expr(subject, held)?;
                let join = self.new_block();
                for arm in arms {
                    let next = self.new_block();
                    let arm_mark = self.next_temp;
                    self.pattern(&arm.pattern, held, &ty, next, origin)?;
                    self.discard(held, class);
                    self.release_to(arm_mark);
                    self.expr(&arm.body, dest)?;
                    self.goto(join, next);
                }
                // A checked `match` is exhaustive.
                self.stop_invalid(origin);
                self.goto(join, join);
                self.release_to(mark);
                Ok(())
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let mark = self.next_temp;
                let held = self.temp();
                self.expr(condition, held)?;
                self.truth(held, origin)?;
                self.release_to(mark);
                let (then_block, else_block, join) =
                    (self.new_block(), self.new_block(), self.new_block());
                self.end(Term::Branch {
                    then_block,
                    else_block,
                });
                self.start(then_block);
                match else_branch {
                    Some(else_branch) => {
                        self.expr(then_branch, dest)?;
                        self.goto(join, else_block);
                        self.expr(else_branch, dest)?;
                    }
                    None => {
                        let class = self.node_class(then_branch)?;
                        let ignored = self.temp();
                        self.expr(then_branch, ignored)?;
                        self.discard(ignored, class);
                        self.release_to(mark);
                        self.store_bits(dest, 0, ValueClass::Void);
                        self.goto(join, else_block);
                        self.store_bits(dest, 0, ValueClass::Void);
                    }
                }
                self.goto(join, join);
                Ok(())
            }
            Expr::Construct {
                value_type,
                construction,
            } => self.construct(value_type, construction, origin, dest),
            Expr::Field { record, field } => {
                let mark = self.next_temp;
                let ty = self.type_of(record)?;
                let Shape::Record(fields) = self.shape(&ty, origin)? else {
                    return Err(EmitError::malformed(
                        format!("a field of {ty}"),
                        origin,
                    ));
                };
                let position = field_position(&fields, field).ok_or_else(|| {
                    EmitError::malformed(format!("{ty} has no `{field}`"), origin)
                })?;
                let class = fields
                    .get(position as usize)
                    .map(|(_, field)| self.class(field, origin))
                    .transpose()?
                    .unwrap_or(ValueClass::Void);
                let len =
                    u32::try_from(fields.len()).map_err(|_| EmitError::ModuleSize)?;
                let held = self.temp();
                self.expr(record, held)?;
                self.take_component(held, position, len, class, dest);
                self.drop_slot(held);
                self.release_to(mark);
                Ok(())
            }
            Expr::Lookup { collection, key } => {
                self.lookup(collection, key, origin, dest)
            }
            Expr::Call { target, arguments } => {
                self.call(target, arguments, false, origin, dest)
            }
            Expr::TailCall { target, arguments } => {
                self.call(target, arguments, true, origin, dest)
            }
            Expr::Return(value) => {
                let mark = self.next_temp;
                let held = self.temp();
                self.expr(value, held)?;
                self.end(Term::Return { slot: held });
                self.dead();
                self.release_to(mark);
                Ok(())
            }
            Expr::Try(value) => self.try_carrier(value, origin, dest),
            Expr::WidenToInterface { .. } => {
                Err(EmitError::unsupported("widenToInterface", origin))
            }
            Expr::WidenAtom(value) => self.expr(value, dest),
            Expr::Default(_) => Err(EmitError::malformed(
                "a default outside a labelled argument",
                origin,
            )),
        }
    }

    /// Evaluates `nodes` in order into new temporaries.
    fn operands(&mut self, nodes: &'a [Node]) -> Result<Vec<u32>, EmitError> {
        let mut slots = Vec::with_capacity(nodes.len());
        for node in nodes {
            let slot = self.temp();
            self.expr(node, slot)?;
            slots.push(slot);
        }
        Ok(slots)
    }

    fn sequence(
        &mut self,
        statements: &'a [Node],
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), EmitError> {
        let Some((last, first)) = statements.split_last() else {
            return Err(EmitError::malformed("an empty sequence", origin));
        };
        for statement in first {
            let mark = self.next_temp;
            let class = self.node_class(statement)?;
            let held = self.temp();
            self.expr(statement, held)?;
            self.discard(held, class);
            self.release_to(mark);
        }
        self.expr(last, dest)
    }

    /// Builds the constant `value` of the closed type `ty` into `dest`.
    fn constant(
        &mut self,
        value: &Value,
        ty: &Type,
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), EmitError> {
        let malformed =
            || EmitError::malformed(format!("a constant {value:?} of {ty}"), origin);
        let shape = self.shape(ty, origin)?;
        let mark = self.next_temp;
        match (&shape, value) {
            (Shape::Scalar(class), _) => {
                let bits = scalar_bits(value).ok_or_else(malformed)?;
                self.store_bits(dest, bits, *class);
            }
            (Shape::Atom, Value::Atom(text)) => {
                let (len, bytes) = utf32(text);
                self.build_units(Kind::Atom, len, bytes, dest);
            }
            (Shape::Str, Value::Str(text)) => {
                let (len, bytes) = utf32(text);
                self.build_units(Kind::Str, len, bytes, dest);
            }
            (Shape::Bytes, Value::Bytes(bytes)) => {
                let len =
                    u32::try_from(bytes.len()).map_err(|_| EmitError::ModuleSize)?;
                self.build_units(Kind::Bytes, len, bytes.clone(), dest);
            }
            (Shape::Tuple(types), Value::Tuple(values))
                if types.len() == values.len() =>
            {
                let parts = self.constants(values.iter().zip(types), origin)?;
                self.build(Kind::Tuple, Variant::Fixed(0), &parts, dest);
            }
            (Shape::List(element), Value::List(values)) => {
                let parts = self
                    .constants(values.iter().map(|value| (value, element)), origin)?;
                self.build(Kind::List, Variant::Fixed(0), &parts, dest);
            }
            (Shape::Record(fields), Value::Record(values))
                if fields.len() == values.len() =>
            {
                let ordered = fields
                    .iter()
                    .map(|(name, field)| {
                        values
                            .iter()
                            .find(|(written, _)| written == name)
                            .map(|(_, value)| (value, field))
                            .ok_or_else(malformed)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let parts = self.constants(ordered.into_iter(), origin)?;
                self.build(Kind::Record, Variant::Fixed(0), &parts, dest);
            }
            (
                Shape::Variant(_),
                Value::Variant {
                    case,
                    fields: values,
                },
            ) => {
                let (position, fields) = shape.case(case).ok_or_else(malformed)?;
                if fields.len() != values.len() {
                    return Err(malformed());
                }
                let ordered = fields
                    .iter()
                    .map(|(name, field)| {
                        values
                            .iter()
                            .find(|(written, _)| written == name)
                            .map(|(_, value)| (value, field))
                            .ok_or_else(malformed)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let parts = self.constants(ordered.into_iter(), origin)?;
                self.build(Kind::Variant, Variant::Fixed(position), &parts, dest);
            }
            (Shape::Wrapper(representation), Value::Wrapper(inner)) => {
                let parts = self.constants(
                    std::iter::once((inner.as_ref(), representation)),
                    origin,
                )?;
                self.build(Kind::Wrapper, Variant::Fixed(0), &parts, dest);
            }
            (Shape::Dict(key, item), Value::Dict(entries)) => {
                let entry = Type::Tuple(vec![key.clone(), item.clone()]);
                let mut parts = Vec::with_capacity(entries.len());
                for (key_value, item_value) in entries {
                    let slot = self.temp();
                    let pair =
                        Value::Tuple(vec![key_value.clone(), item_value.clone()]);
                    self.constant(&pair, &entry, origin, slot)?;
                    parts.push(Part::Slot {
                        slot,
                        class: ValueClass::Ref,
                    });
                }
                self.build(Kind::Dict, Variant::Fixed(0), &parts, dest);
            }
            _ => return Err(malformed()),
        }
        self.release_to(mark);
        Ok(())
    }

    /// Builds each constant into a new temporary.
    fn constants<'v>(
        &mut self,
        values: impl Iterator<Item = (&'v Value, &'v Type)>,
        origin: &SourceOrigin,
    ) -> Result<Vec<Part>, EmitError> {
        let mut parts = Vec::new();
        for (value, ty) in values {
            let class = self.class(ty, origin)?;
            let slot = self.temp();
            self.constant(value, ty, origin, slot)?;
            parts.push(Part::Slot { slot, class });
        }
        Ok(parts)
    }

    fn construct(
        &mut self,
        value_type: &Type,
        construction: &'a Construction,
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), EmitError> {
        let ty = self.close(value_type);
        if self.lowering.types.is_fallible(&ty) {
            return Err(EmitError::unsupported("fallibleConstructor", origin));
        }
        let shape = self.shape(&ty, origin)?;
        let mark = self.next_temp;
        let malformed =
            || EmitError::malformed(format!("a construction of {ty}"), origin);
        match (construction, &shape) {
            (Construction::Tuple(items), Shape::Tuple(types))
                if items.len() == types.len() =>
            {
                let parts = self.parts(items.iter().zip(types))?;
                self.build(Kind::Tuple, Variant::Fixed(0), &parts, dest);
            }
            (Construction::List(items), Shape::List(element)) => {
                let parts = self.parts(items.iter().map(|item| (item, element)))?;
                self.build(Kind::List, Variant::Fixed(0), &parts, dest);
            }
            (Construction::Record(written), Shape::Record(fields)) => {
                let parts = self.fields(written, fields, origin)?;
                self.build(Kind::Record, Variant::Fixed(0), &parts, dest);
            }
            (
                Construction::Case {
                    case,
                    fields: written,
                },
                Shape::Variant(_),
            ) => {
                let (position, fields) = shape.case(case).ok_or_else(malformed)?;
                let fields = fields.to_vec();
                let parts = self.fields(written, &fields, origin)?;
                self.build(Kind::Variant, Variant::Fixed(position), &parts, dest);
            }
            (Construction::Wrapper(inner), Shape::Str | Shape::Bytes) => {
                let intrinsic = if shape == Shape::Str {
                    CompilerIntrinsic::StrFromChars
                } else {
                    CompilerIntrinsic::BytesFromList
                };
                let held = self.temp();
                self.expr(inner, held)?;
                self.intrinsic_on(intrinsic, &[], &[held], origin, dest)?;
            }
            (Construction::Wrapper(_) | Construction::Dict(_), Shape::Dict(..)) => {
                return Err(EmitError::unsupported("dictConstruct", origin));
            }
            (Construction::Wrapper(inner), Shape::Wrapper(representation)) => {
                let parts =
                    self.parts(std::iter::once((inner.as_ref(), representation)))?;
                self.build(Kind::Wrapper, Variant::Fixed(0), &parts, dest);
            }
            _ => return Err(malformed()),
        }
        self.release_to(mark);
        Ok(())
    }

    /// Evaluates each node, in order, into a new temporary of the class of
    /// its closed type.
    fn parts<'t>(
        &mut self,
        items: impl Iterator<Item = (&'a Node, &'t Type)>,
    ) -> Result<Vec<Part>, EmitError> {
        let mut parts = Vec::new();
        for (item, ty) in items {
            let class = self.class(ty, &item.origin)?;
            let slot = self.temp();
            self.expr(item, slot)?;
            parts.push(Part::Slot { slot, class });
        }
        Ok(parts)
    }

    /// Evaluates the written fields in written order and lays them out in
    /// the order of `fields`.
    fn fields(
        &mut self,
        written: &'a [(String, Node)],
        fields: &[(String, Type)],
        origin: &SourceOrigin,
    ) -> Result<Vec<Part>, EmitError> {
        if written.len() != fields.len() {
            return Err(EmitError::malformed("a construction's fields", origin));
        }
        let mut slots = Vec::with_capacity(written.len());
        for (name, node) in written {
            let slot = self.temp();
            self.expr(node, slot)?;
            slots.push((name, slot));
        }
        fields
            .iter()
            .map(|(name, ty)| {
                let slot = slots
                    .iter()
                    .find(|(written, _)| *written == name)
                    .map(|(_, slot)| *slot)
                    .ok_or_else(|| {
                        EmitError::malformed(format!("no field `{name}`"), origin)
                    })?;
                Ok(Part::Slot {
                    slot,
                    class: self.class(ty, origin)?,
                })
            })
            .collect()
    }

    /// `collection[key]`: `some` of the item at the index, or `none`.
    fn lookup(
        &mut self,
        collection: &'a Node,
        key: &'a Node,
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), EmitError> {
        let collection_type = self.type_of(collection)?;
        let (item, stride) = match self.shape(&collection_type, origin)? {
            Shape::List(element) => (element, layout::STRIDE_CELLS),
            Shape::Str => (Type::Char, layout::STRIDE_CHARS),
            Shape::Bytes => (Type::U8, layout::STRIDE_BYTES),
            Shape::Dict(..) => {
                return Err(EmitError::unsupported("dictLookup", origin));
            }
            _ => {
                return Err(EmitError::malformed(
                    format!("a lookup on {collection_type}"),
                    origin,
                ));
            }
        };
        let option = self.library_type(Library::Option, vec![item.clone()], origin)?;
        let (some, none) = (
            self.case_index(&option, "some", origin)?,
            self.case_index(&option, "none", origin)?,
        );
        let class = self.class(&item, origin)?;
        let mark = self.next_temp;
        let held = self.temp();
        self.expr(collection, held)?;
        let position = self.temp();
        self.expr(key, position)?;
        let (held_at, position_at) = (self.cell(held), self.cell(position));
        self.push(&[
            get(FRAME),
            ld64(position_at),
            get(FRAME),
            ld32(held_at),
            ld32(header::LEN),
            I::I64ExtendI32U,
            I::I64LtU,
        ]);
        let (present, absent, join) =
            (self.new_block(), self.new_block(), self.new_block());
        self.end(Term::Branch {
            then_block: present,
            else_block: absent,
        });
        self.start(present);
        let element = self.temp();
        let element_at = self.cell(element);
        // The item's address: the payload, past the class bytes of a cells
        // object, plus the index times the stride.
        self.push(&[
            get(FRAME),
            get(FRAME),
            ld32(held_at),
            set(SCRATCH),
            get(SCRATCH),
        ]);
        if stride == layout::STRIDE_CELLS {
            self.push(&[
                get(SCRATCH),
                ld32(header::LEN),
                c32(7),
                I::I32Add,
                c32(-8),
                I::I32And,
                I::I32Add,
            ]);
        }
        self.push(&[
            get(FRAME),
            ld64(position_at),
            I::I32WrapI64,
            c32(index(stride)),
            I::I32Mul,
            I::I32Add,
        ]);
        match stride {
            layout::STRIDE_CELLS => self.push(&[ld64(header::SIZE)]),
            layout::STRIDE_CHARS => self.push(&[ld32(header::SIZE), I::I64ExtendI32U]),
            _ => self.push(&[ld8(header::SIZE), I::I64ExtendI32U]),
        }
        self.push(&[st64(element_at)]);
        self.claim(element, class);
        self.build(
            Kind::Variant,
            Variant::Fixed(some),
            &[Part::Slot {
                slot: element,
                class,
            }],
            dest,
        );
        self.goto(join, absent);
        self.build(Kind::Variant, Variant::Fixed(none), &[], dest);
        self.goto(join, join);
        self.drop_slot(held);
        self.release_to(mark);
        Ok(())
    }

    /// `try value`: the payload of `some` or `ok`, or a return of `none` or
    /// of `err` with the same error.
    fn try_carrier(
        &mut self,
        value: &'a Node,
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), EmitError> {
        let carrier = self.type_of(value)?;
        let types = self.lowering.types;
        let (success, failure) = match types.library_of(&carrier) {
            Some(Library::Option) => ("some", "none"),
            Some(Library::Result) => ("ok", "err"),
            _ => {
                return Err(EmitError::malformed(format!("a try on {carrier}"), origin));
            }
        };
        let shape = self.shape(&carrier, origin)?;
        let (success_index, success_fields) = shape
            .case(success)
            .ok_or_else(|| EmitError::malformed("a carrier", origin))?;
        let payload_class = success_fields
            .first()
            .map(|(_, ty)| self.class(ty, origin))
            .transpose()?
            .unwrap_or(ValueClass::Void);
        let (_, failure_fields) = shape
            .case(failure)
            .ok_or_else(|| EmitError::malformed("a carrier", origin))?;
        let error_class = failure_fields
            .first()
            .map(|(_, ty)| self.class(ty, origin))
            .transpose()?;
        let result = self.result.clone();
        let failure_index = self.case_index(&result, failure, origin)?;
        let mark = self.next_temp;
        let held = self.temp();
        self.expr(value, held)?;
        self.is_case(held, success_index);
        let (present, absent, join) =
            (self.new_block(), self.new_block(), self.new_block());
        self.end(Term::Branch {
            then_block: present,
            else_block: absent,
        });
        self.start(present);
        self.take_component(held, 0, 1, payload_class, dest);
        self.drop_slot(held);
        self.goto(join, absent);
        let returned = self.temp();
        match error_class {
            Some(class) => {
                let error = self.temp();
                self.take_component(held, 0, 1, class, error);
                self.drop_slot(held);
                self.build(
                    Kind::Variant,
                    Variant::Fixed(failure_index),
                    &[Part::Slot { slot: error, class }],
                    returned,
                );
            }
            None => {
                self.drop_slot(held);
                self.build(Kind::Variant, Variant::Fixed(failure_index), &[], returned);
            }
        }
        self.end(Term::Return { slot: returned });
        self.start(join);
        self.release_to(mark);
        Ok(())
    }

    /// Lays the blocks out in the dispatching loop.
    fn finish(self) -> (FunctionCode, u32) {
        let slots = self.max_slots;
        let fns = self.lowering.fns;
        let known = self.known;
        let blocks = self
            .blocks
            .into_iter()
            .map(|block| {
                block.unwrap_or(Block {
                    code: Vec::new(),
                    term: Term::Goto(0),
                })
            })
            .collect::<Vec<_>>();
        let count = u32::try_from(blocks.len()).unwrap_or(u32::MAX);
        let mut body = vec![
            get(FRAME),
            ld32(frame::RESUME),
            set(PC),
            I::Loop(BlockType::Empty),
        ];
        for _ in 0..count {
            body.push(I::Block(BlockType::Empty));
        }
        body.push(get(PC));
        body.push(I::BrTable(
            Cow::Owned((0..count).collect()),
            count.saturating_sub(1),
        ));
        for (position, block) in (0_u32..).zip(blocks) {
            // The block's own label closes, and its code follows it, inside
            // the labels of the blocks after it and the loop.
            body.push(I::End);
            body.extend(block.code);
            let loop_depth = count - 1 - position;
            expand(&fns, known, &block.term, loop_depth, &mut body);
        }
        body.push(I::End);
        body.push(I::End);
        let mut locals = FIXED.to_vec();
        for _ in 0..self.stage {
            locals.push(ValType::I64);
            locals.push(ValType::I32);
        }
        (
            FunctionCode {
                params: vec![ValType::I32],
                results: Vec::new(),
                locals,
                body,
            },
            slots,
        )
    }
}

/// Writes the instructions of a terminator. `loop_depth` is the label depth of
/// the dispatching loop from the end of this block.
fn expand(
    fns: &Routines,
    known: u32,
    term: &Term,
    loop_depth: u32,
    body: &mut Vec<Ins>,
) {
    match term {
        Term::Goto(block) => {
            body.extend([c32(block.cast_signed()), set(PC), I::Br(loop_depth)]);
        }
        Term::Branch {
            then_block,
            else_block,
        } => {
            body.extend([
                I::If(BlockType::Empty),
                c32(then_block.cast_signed()),
                set(PC),
                I::Else,
                c32(else_block.cast_signed()),
                set(PC),
                I::End,
                I::Br(loop_depth),
            ]);
        }
        Term::Call {
            callee,
            moves,
            arity,
            resume,
        } => {
            body.extend([get(FRAME), c32(resume.cast_signed()), st32(frame::RESUME)]);
            let slots = enter(fns, known, *callee, *arity, body);
            for operand in moves {
                move_in(known, operand, slots, body);
            }
            body.push(I::Return);
        }
        Term::Tail {
            callee,
            moves,
            arity,
        } => {
            let slots = match *callee {
                Callee::Named { slots, .. } => Some(slots),
                Callee::Value { slot } => {
                    value_header(known, slot, *arity, fns, body);
                    None
                }
            };
            // The operands leave the frame before it is replaced.
            for (position, operand) in (0_u32..).zip(moves) {
                let (cell, class) = (STAGE + 2 * position, STAGE + 2 * position + 1);
                body.extend([
                    get(FRAME),
                    ld64(frame::cell_offset(known, operand.from)),
                    I::LocalSet(cell),
                    get(FRAME),
                    ld8(frame::class_offset(operand.from)),
                    I::LocalSet(class),
                    get(FRAME),
                    c32(0),
                    st8(frame::class_offset(operand.from)),
                ]);
            }
            match *callee {
                Callee::Named { function, slots } => body.extend([
                    get(FRAME),
                    c32(function.cast_signed()),
                    c32(slots.cast_signed()),
                    I::Call(fns.reframe),
                    set(CALLEE),
                ]),
                Callee::Value { .. } => body.extend([
                    get(FRAME),
                    get(FUNC),
                    get(SLOTS),
                    I::Call(fns.reframe),
                    set(CALLEE),
                    get(CALLEE),
                    c32(index(frame::HEADER)),
                    I::I32Add,
                    get(SLOTS),
                    c32(7),
                    I::I32Add,
                    c32(-8),
                    I::I32And,
                    I::I32Add,
                    set(SCRATCH),
                ]),
            }
            for (position, operand) in (0_u32..).zip(moves) {
                let (cell, class) = (STAGE + 2 * position, STAGE + 2 * position + 1);
                match slots {
                    Some(slots) => body.extend([
                        get(CALLEE),
                        I::LocalGet(cell),
                        st64(frame::cell_offset(slots, operand.to)),
                    ]),
                    None => body.extend([
                        get(SCRATCH),
                        I::LocalGet(cell),
                        st64(8 * operand.to),
                    ]),
                }
                body.extend([
                    get(CALLEE),
                    I::LocalGet(class),
                    st8(frame::class_offset(operand.to)),
                ]);
            }
            body.push(I::Return);
        }
        Term::Return { slot } => {
            body.extend([
                c32(0),
                get(FRAME),
                ld64(frame::cell_offset(known, *slot)),
                st64(state::RET),
                c32(0),
                get(FRAME),
                ld8(frame::class_offset(*slot)),
                st32(state::RET_CLASS),
                get(FRAME),
                c32(0),
                st8(frame::class_offset(*slot)),
                get(FRAME),
                I::Call(fns.leave),
                I::Return,
            ]);
        }
    }
}

/// Reads the function value in `slot` for a call of `arity` operands: its
/// address in `FV`, its cells in `CELLS`, and its instance and frame size in
/// `FUNC` and `SLOTS`. A value that takes another number of operands is the
/// toolchain's defect, never a program result.
fn value_header(
    known: u32,
    slot: u32,
    arity: u32,
    fns: &Routines,
    body: &mut Vec<Ins>,
) {
    body.extend([
        get(FRAME),
        ld32(frame::cell_offset(known, slot)),
        set(FV),
        get(FV),
        get(FV),
        ld32(header::LEN),
        c32(7),
        I::I32Add,
        c32(-8),
        I::I32And,
        I::I32Add,
        c32(index(header::SIZE)),
        I::I32Add,
        set(CELLS),
        get(CELLS),
        ld32(8 * fv::ARITY),
        c32(arity.cast_signed()),
        I::I32Ne,
        I::If(BlockType::Empty),
        c32(INVALID_CHECKED_PROGRAM),
        I::Call(fns.stop_trap),
        I::End,
        get(CELLS),
        ld32(8 * fv::FUNCTION),
        set(FUNC),
        get(CELLS),
        ld32(8 * fv::SLOTS),
        set(SLOTS),
    ]);
}

/// Pushes the callee's frame into `CALLEE`. For a function value the header is
/// read first, and the cells of the new frame are found in `SCRATCH`. The
/// result is the frame size when it is known.
fn enter(
    fns: &Routines,
    known: u32,
    callee: Callee,
    arity: u32,
    body: &mut Vec<Ins>,
) -> Option<u32> {
    match callee {
        Callee::Named { function, slots } => {
            body.extend([
                c32(function.cast_signed()),
                c32(slots.cast_signed()),
                I::Call(fns.push_frame),
                set(CALLEE),
            ]);
            Some(slots)
        }
        Callee::Value { slot } => {
            value_header(known, slot, arity, fns, body);
            body.extend([
                get(FUNC),
                get(SLOTS),
                I::Call(fns.push_frame),
                set(CALLEE),
                get(CALLEE),
                c32(index(frame::HEADER)),
                I::I32Add,
                get(SLOTS),
                c32(7),
                I::I32Add,
                c32(-8),
                I::I32And,
                I::I32Add,
                set(SCRATCH),
            ]);
            None
        }
    }
}

/// Moves one operand into the callee's frame: its cell and its class byte, and
/// the source is left owning nothing.
fn move_in(known: u32, operand: &Move, slots: Option<u32>, body: &mut Vec<Ins>) {
    let from_cell = frame::cell_offset(known, operand.from);
    let from_class = frame::class_offset(operand.from);
    match slots {
        Some(slots) => body.extend([
            get(CALLEE),
            get(FRAME),
            ld64(from_cell),
            st64(frame::cell_offset(slots, operand.to)),
        ]),
        None => body.extend([
            get(SCRATCH),
            get(FRAME),
            ld64(from_cell),
            st64(8 * operand.to),
        ]),
    }
    body.extend([
        get(CALLEE),
        get(FRAME),
        ld8(from_class),
        st8(frame::class_offset(operand.to)),
        get(FRAME),
        c32(0),
        st8(from_class),
    ]);
}
