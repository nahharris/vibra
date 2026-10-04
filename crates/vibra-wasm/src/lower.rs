//! Lowers checked IR to the functions of a module.
//!
//! Every language function, and every module-value initializer, becomes one
//! WebAssembly function `(frame) -> ()` that the dispatcher of
//! [`crate::layout`] ("Activations and the dispatcher") runs. This module
//! decides what such a function looks like inside.
//!
//! # The shape of a function
//!
//! A function is a set of numbered **basic blocks**. A block is straight-line
//! code that ends in one **terminator**: a jump, a branch on a `bool`, a call,
//! or a return. The function enters its blocks through a `loop` over a
//! `br_table` on the `pc` local, which it reads from the frame's `resume` word
//! when it is entered, so a call can end the WebAssembly function and a later
//! entry can continue at the block after it. Nothing on the operand stack or in
//! a local survives a terminator that returns to the dispatcher, so **every
//! value lives in a frame slot**: the parameters first, then the bindings of
//! the checked IR, then the temporaries this module allocates, each as one cell
//! with the ownership rule of the layout chapter (a slot owns a reference
//! exactly while its class byte is the reference class).
//!
//! # Evaluating an expression
//!
//! An expression is evaluated **into a destination slot** the caller chose.
//! Its operands are evaluated into temporaries, which are allocated in a stack
//! discipline and so released by resetting a counter, and then the expression's
//! own operation moves them out: a constructor copies each operand's cell into
//! the new object and clears the temporary's byte, and a call copies them into
//! the callee's frame. A read of a local copies the cell and takes a `dup`; a
//! projection takes a `dup` of the component and a `drop` of the aggregate; a
//! `let` evaluates straight into its binding's slot and drops the binding when
//! its body has finished. Nothing is elided, so a program's counts balance by
//! construction and by the scan of a leaving frame, and not by analysis.
//!
//! An expression of type `never` (`return` and what contains it) writes no
//! destination; the code that follows it is reachable from no block.
//!
//! # Passes
//!
//! A call pushes a frame of the callee's size, and the callee's size is known
//! only after the callee is lowered, so the module is lowered twice: the first
//! pass finds each function's frame size, which depends on nothing but its own
//! body, and the second emits the code with every size known.

use std::borrow::Cow;
use std::collections::BTreeMap;

use vibra_ir::{CallTarget, CheckedProgram, Expr, SourceOrigin, Type, Value};
use wasm_encoder::{BlockType, Instruction as I, ValType};

use crate::classify;
use crate::encode::FunctionCode;
use crate::form::NotLowered;
use crate::layout::{self, CellClass, Kind, frame, header, state};
use crate::runtime::{
    Ins, Routines, ValueClass, c32, c64, get, index, ld32, ld64, set, st8, st32, st64,
};
use crate::types::{Shape, TypeEnv, class_of};

/// The WebAssembly function's parameter: the offset of its frame.
const FRAME: u32 = 0;
/// The block the function runs next.
const PC: u32 = 1;
/// The frame a call has just pushed.
const CALLEE: u32 = 2;
/// The object a constructor is building.
const OBJECT: u32 = 3;
/// A scratch word.
const SCRATCH: u32 = 4;
/// The locals a function declares after its parameter.
const LOCALS: [ValType; 4] = [ValType::I32; 4];

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

/// A lowered function and the number of slots its frame needs.
#[derive(Debug)]
pub(crate) struct Lowered {
    pub(crate) code: FunctionCode,
    pub(crate) slots: u32,
}

/// Lowers the functions of one program against the routine indices of one
/// module.
#[derive(Debug)]
pub(crate) struct Lowering<'a> {
    fns: &'a Routines,
    program: &'a CheckedProgram,
    env: TypeEnv<'a>,
    /// The frame size of every language function and then every initializer,
    /// as the first pass found them (all `0` during the first pass).
    sizes: Vec<u32>,
    segments: Segments,
    built_object: bool,
}

impl<'a> Lowering<'a> {
    pub(crate) fn new(
        fns: &'a Routines,
        program: &'a CheckedProgram,
        sizes: Vec<u32>,
    ) -> Self {
        Self {
            fns,
            program,
            env: TypeEnv::new(program.types()),
            sizes,
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

    /// The number of language functions the program has, which is the index
    /// of the first module-value initializer.
    fn function_count(&self) -> usize {
        self.program.functions().len()
    }

    /// Lowers language function `index`, or the initializer of module value
    /// `index - function_count` when the index is past the functions.
    ///
    /// # Errors
    ///
    /// The forms the function uses that this step does not lower.
    pub(crate) fn function(&mut self, index: usize) -> Result<Lowered, NotLowered> {
        let program = self.program;
        let known = self.sizes.get(index).copied().unwrap_or(0);
        let functions = self.function_count();
        let (body, ir_slots, result): (&Expr, usize, Type) =
            match program.functions().get(index) {
                Some(function) => (
                    function.body(),
                    function.slot_count(),
                    function.signature().result(),
                ),
                None => {
                    let global = program
                        .globals()
                        .get(index.saturating_sub(functions))
                        .ok_or_else(|| NotLowered::single(crate::Form::ModuleSize))?;
                    (
                        global.initializer(),
                        global.slot_count(),
                        global.value_type(),
                    )
                }
            };
        let ir_slots = u32::try_from(ir_slots)
            .map_err(|_| NotLowered::single(crate::Form::ModuleSize))?;
        let mut builder = Builder::new(self, known, ir_slots);
        let class = builder.class(&result, body.origin())?;
        let first = builder.new_block();
        builder.start(first);
        let dest = builder.temp();
        builder.expr(body, dest)?;
        builder.end(Term::Return { slot: dest, class });
        Ok(builder.finish())
    }
}

/// A numbered block's final instruction.
#[derive(Debug)]
enum Term {
    /// Continues at a block.
    Goto(u32),
    /// Continues at one of two blocks, by the `i32` on the operand stack, which
    /// is nonzero for the first.
    Branch { then_block: u32, else_block: u32 },
    /// Pushes a frame for `function` of `slots` slots, moves the operands into
    /// its first slots, and returns to the dispatcher, which continues the
    /// caller at `resume`.
    Call {
        function: u32,
        slots: u32,
        arguments: Vec<Operand>,
        resume: u32,
    },
    /// Hands the cell of a slot to the caller and leaves the frame.
    Return { slot: u32, class: ValueClass },
}

/// An operand of a call: a slot of the caller, and whether it owns a reference.
#[derive(Clone, Copy, Debug)]
struct Operand {
    slot: u32,
    reference: bool,
}

/// One part of an object being built.
#[derive(Clone, Copy, Debug)]
enum Part {
    /// The cell of a slot, moved into the object.
    Slot { slot: u32, class: ValueClass },
    /// A cell of zero: a `void` component.
    Zero,
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
    next_temp: u32,
    max_slots: u32,
    blocks: Vec<Option<Block>>,
    current: u32,
    code: Vec<Ins>,
}

impl<'l, 'a> Builder<'l, 'a> {
    fn new(lowering: &'l mut Lowering<'a>, known: u32, ir_slots: u32) -> Self {
        Self {
            lowering,
            known,
            next_temp: ir_slots,
            max_slots: ir_slots,
            blocks: Vec::new(),
            current: 0,
            code: Vec::new(),
        }
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

    // -- slots ----------------------------------------------------------------

    const fn cell(&self, slot: u32) -> u32 {
        frame::cell_offset(self.known, slot)
    }

    fn push(&mut self, instructions: &[Ins]) {
        self.code.extend_from_slice(instructions);
    }

    /// The `i32` offset in `slot`, a reference.
    fn load_ref(&mut self, slot: u32) {
        let at = self.cell(slot);
        self.push(&[get(FRAME), ld32(at)]);
    }

    /// Marks the slot as owning a reference.
    fn own(&mut self, slot: u32) {
        self.push(&[
            get(FRAME),
            c32(i32::from(CellClass::Ref.code())),
            st8(frame::class_offset(slot)),
        ]);
    }

    /// Marks the slot as owning nothing.
    fn disown(&mut self, slot: u32) {
        self.push(&[get(FRAME), c32(0), st8(frame::class_offset(slot))]);
    }

    /// Stores constant bits in a slot's cell.
    fn store_bits(&mut self, slot: u32, bits: u64) {
        let at = self.cell(slot);
        self.push(&[get(FRAME), c64(bits.cast_signed()), st64(at)]);
    }

    /// Copies the cell of `from` to the cell of `to`, which does not change
    /// who owns what.
    fn copy_cell(&mut self, to: u32, from: u32) {
        let (to, from) = (self.cell(to), self.cell(from));
        self.push(&[get(FRAME), get(FRAME), ld64(from), st64(to)]);
    }

    /// Drops the reference a slot owns, and disowns it.
    fn drop_slot(&mut self, slot: u32) {
        self.load_ref(slot);
        let drop = self.lowering.fns.drop;
        self.push(&[I::Call(drop)]);
        self.disown(slot);
    }

    /// Drops what a slot owns, when it can own a reference.
    fn discard(&mut self, slot: u32, class: ValueClass) {
        if class == ValueClass::Ref {
            self.drop_slot(slot);
        }
    }

    /// The class of a value of `ty`, or the form that reports it.
    fn class(
        &self,
        ty: &Type,
        origin: &SourceOrigin,
    ) -> Result<ValueClass, NotLowered> {
        class_of(ty).map_err(|kind| NotLowered::type_of(kind, Some(origin.clone())))
    }

    // -- expressions ----------------------------------------------------------

    /// Evaluates `expr` into `dest`, which then owns the value if it is a
    /// reference. An expression of type `never` writes nothing.
    fn expr(&mut self, expr: &Expr, dest: u32) -> Result<(), NotLowered> {
        match expr {
            Expr::Literal { value, .. } => {
                self.literal(value, dest);
                Ok(())
            }
            Expr::Sequence { expressions, .. } => self.sequence(expressions, dest),
            Expr::Variable {
                slot, value_type, ..
            } => {
                let class = self.class(value_type, expr.origin())?;
                let slot = u32::try_from(*slot)
                    .map_err(|_| NotLowered::single(crate::Form::ModuleSize))?;
                self.copy_cell(dest, slot);
                self.take_copy(dest, class);
                Ok(())
            }
            Expr::Global {
                index, value_type, ..
            } => {
                let class = self.class(value_type, expr.origin())?;
                self.global(*index, class, dest)
            }
            Expr::Let {
                slot, value, body, ..
            } => self.binding(*slot, value, body, dest),
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.branch(condition, then_branch, else_branch, dest),
            Expr::Return { value, .. } => {
                let class = self.class(&value.result_type(), value.origin())?;
                let slot = self.temp();
                self.expr(value, slot)?;
                self.end(Term::Return { slot, class });
                let dead = self.new_block();
                self.start(dead);
                Ok(())
            }
            Expr::Call {
                target: CallTarget::Direct(function),
                arguments,
                tail: false,
                ..
            } => self.call(*function, arguments, dest),
            Expr::Record {
                value_type, fields, ..
            } => self.record(value_type, fields, expr.origin(), dest),
            Expr::Variant {
                value_type,
                variant,
                payload,
                ..
            } => self.variant(
                value_type,
                variant,
                payload.as_deref(),
                expr.origin(),
                dest,
            ),
            Expr::Wrap {
                value_type, value, ..
            } => self.wrap(value_type, value, expr.origin(), dest),
            Expr::Widen {
                value,
                value_type,
                member,
                ..
            } => self.widen(value, value_type, *member, expr.origin(), dest),
            Expr::Project {
                record,
                field,
                value_type,
                ..
            } => self.project(
                record,
                Projection::Field(field),
                value_type,
                expr.origin(),
                dest,
            ),
            Expr::Tuple {
                value_type,
                components,
                ..
            } => self.tuple(value_type, components, expr.origin(), dest),
            Expr::TupleProject {
                tuple,
                index,
                value_type,
                ..
            } => self.project(
                tuple,
                Projection::Index(*index),
                value_type,
                expr.origin(),
                dest,
            ),
            other => Err(classify::forms_of(other)),
        }
    }

    /// A sequence evaluates each expression but the last for its effects and
    /// drops the value, and the last into the destination. An empty sequence is
    /// `void`.
    fn sequence(&mut self, expressions: &[Expr], dest: u32) -> Result<(), NotLowered> {
        let Some((last, leading)) = expressions.split_last() else {
            self.store_bits(dest, 0);
            return Ok(());
        };
        for expression in leading {
            let mark = self.next_temp;
            let class = self.class(&expression.result_type(), expression.origin())?;
            let slot = self.temp();
            self.expr(expression, slot)?;
            self.discard(slot, class);
            self.release_to(mark);
        }
        self.expr(last, dest)
    }

    /// A `let`: the value is evaluated into the binding's slot, which the body
    /// reads, and the binding is dropped when the body has finished.
    fn binding(
        &mut self,
        slot: Option<usize>,
        value: &Expr,
        body: &Expr,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let class = self.class(&value.result_type(), value.origin())?;
        match slot {
            Some(slot) => {
                let slot = u32::try_from(slot)
                    .map_err(|_| NotLowered::single(crate::Form::ModuleSize))?;
                self.expr(value, slot)?;
                self.expr(body, dest)?;
                self.discard(slot, class);
            }
            None => {
                let mark = self.next_temp;
                let slot = self.temp();
                self.expr(value, slot)?;
                self.discard(slot, class);
                self.release_to(mark);
                self.expr(body, dest)?;
            }
        }
        Ok(())
    }

    /// An `if`: the condition is a `bool`, an enum whose `true` has the
    /// discriminant `1`, read and dropped before the branch.
    fn branch(
        &mut self,
        condition: &Expr,
        then_branch: &Expr,
        else_branch: &Expr,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let mark = self.next_temp;
        let test = self.temp();
        self.expr(condition, test)?;
        self.load_ref(test);
        self.push(&[ld32(header::VARIANT), set(SCRATCH)]);
        self.drop_slot(test);
        self.push(&[get(SCRATCH)]);
        self.release_to(mark);
        let (then_block, else_block, join) =
            (self.new_block(), self.new_block(), self.new_block());
        self.end(Term::Branch {
            then_block,
            else_block,
        });
        self.start(then_block);
        self.expr(then_branch, dest)?;
        self.goto(join, else_block);
        self.expr(else_branch, dest)?;
        self.goto(join, join);
        Ok(())
    }

    /// A direct call: the operands are evaluated left to right into
    /// temporaries, which the call moves into the callee's frame, and the
    /// caller continues at a block that takes the result.
    fn call(
        &mut self,
        function: usize,
        arguments: &[Expr],
        dest: u32,
    ) -> Result<(), NotLowered> {
        let mark = self.next_temp;
        let mut operands = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let class = self.class(&argument.result_type(), argument.origin())?;
            let slot = self.temp();
            self.expr(argument, slot)?;
            operands.push(Operand {
                slot,
                reference: class == ValueClass::Ref,
            });
        }
        let class = self.result_class(function)?;
        self.release_to(mark);
        let slots = self.lowering.sizes.get(function).copied().unwrap_or(0);
        let resume = self.new_block();
        self.end(Term::Call {
            function: u32::try_from(function)
                .map_err(|_| NotLowered::single(crate::Form::ModuleSize))?,
            slots,
            arguments: operands,
            resume,
        });
        self.start(resume);
        self.take_return(dest, class);
        Ok(())
    }

    /// The class of the result of language function `function`.
    fn result_class(&self, function: usize) -> Result<ValueClass, NotLowered> {
        let callee = self
            .lowering
            .program
            .functions()
            .get(function)
            .ok_or_else(|| NotLowered::single(crate::Form::ModuleSize))?;
        self.class(&callee.signature().result(), callee.origin())
    }

    /// Takes the cell a returning activation left in `ret` into `dest`.
    fn take_return(&mut self, dest: u32, class: ValueClass) {
        let to = self.cell(dest);
        self.push(&[get(FRAME), c32(0), ld64(state::RET), st64(to)]);
        if class == ValueClass::Ref {
            self.own(dest);
        }
    }

    /// A read of module value `index`: the cell of its table when the flag is
    /// set, and otherwise a call of its initializer, whose result the reader
    /// moves into the table before it keeps a count of its own.
    fn global(
        &mut self,
        global: usize,
        class: ValueClass,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let values = u32::try_from(self.lowering.program.globals().len())
            .map_err(|_| NotLowered::single(crate::Form::ModuleSize))?;
        let global = u32::try_from(global)
            .map_err(|_| NotLowered::single(crate::Form::ModuleSize))?;
        let base = layout::cells_offset(values.saturating_mul(2));
        let flag = base + 16 * global;
        let value = flag + 8;
        let class_flag = header::SIZE + 2 * global;
        let class_value = class_flag + 1;
        let (ready, init, finish, join) = (
            self.new_block(),
            self.new_block(),
            self.new_block(),
            self.new_block(),
        );
        self.push(&[
            c32(0),
            ld32(state::MODULE_VALUES),
            ld64(flag),
            I::I32WrapI64,
        ]);
        self.end(Term::Branch {
            then_block: ready,
            else_block: init,
        });

        // Read: the table keeps its count and the reader takes one more.
        self.start(ready);
        let to = self.cell(dest);
        self.push(&[
            get(FRAME),
            c32(0),
            ld32(state::MODULE_VALUES),
            ld64(value),
            st64(to),
        ]);
        self.take_copy(dest, class);
        self.goto(join, init);

        // First read: the initializer is an activation like any other.
        let functions = self.lowering.function_count();
        let function = u32::try_from(functions)
            .map_err(|_| NotLowered::single(crate::Form::ModuleSize))?
            + global;
        let slots = self
            .lowering
            .sizes
            .get(function as usize)
            .copied()
            .unwrap_or(0);
        self.end(Term::Call {
            function,
            slots,
            arguments: Vec::new(),
            resume: finish,
        });
        self.start(finish);
        let fns = self.lowering.fns;
        self.push(&[
            c32(0),
            ld32(state::MODULE_VALUES),
            set(OBJECT),
            get(OBJECT),
            c32(0),
            ld64(state::RET),
            st64(value),
            get(OBJECT),
            c32(i32::from(cell_class(class).code())),
            st8(class_value),
        ]);
        if class == ValueClass::Ref {
            self.push(&[c32(0), ld32(state::RET), I::Call(fns.dup)]);
        }
        self.push(&[get(OBJECT), c64(1), st64(flag)]);
        self.take_return(dest, class);
        self.goto(join, join);
        Ok(())
    }

    /// Takes a count for the copy of a cell that was just made: the slot owns a
    /// reference when the value is one.
    fn take_copy(&mut self, slot: u32, class: ValueClass) {
        if class == ValueClass::Ref {
            self.load_ref(slot);
            let dup = self.lowering.fns.dup;
            self.push(&[I::Call(dup)]);
            self.own(slot);
        }
    }

    // -- literals -------------------------------------------------------------

    fn literal(&mut self, value: &Value, dest: u32) {
        match value {
            Value::Void => self.store_bits(dest, 0),
            Value::Char(value) => self.store_bits(dest, u64::from(u32::from(*value))),
            Value::I8(value) => self.store_bits(dest, narrow(i32::from(*value))),
            Value::I16(value) => self.store_bits(dest, narrow(i32::from(*value))),
            Value::I32(value) => self.store_bits(dest, narrow(*value)),
            Value::U8(value) => self.store_bits(dest, u64::from(*value)),
            Value::U16(value) => self.store_bits(dest, u64::from(*value)),
            Value::U32(value) => self.store_bits(dest, u64::from(*value)),
            Value::I64(value) => self.store_bits(dest, value.cast_unsigned()),
            Value::U64(value) => self.store_bits(dest, *value),
            Value::F32(bits) => self.store_bits(dest, u64::from(*bits)),
            Value::F64(bits) => self.store_bits(dest, *bits),
            // `bool` is the enum whose variants are `false` and `true`, in
            // declaration order, with `void` payloads.
            Value::Bool(value) => {
                self.literal_object(Kind::Enum, 0, u32::from(*value), None, dest);
            }
            Value::Str(text) => {
                let (length, bytes) = utf32(text);
                self.literal_object(Kind::Str, length, 0, Some(bytes), dest);
            }
            Value::Atom(name) => {
                let (length, bytes) = utf32(name);
                self.literal_object(Kind::Atom, length, 0, Some(bytes), dest);
            }
            Value::Bytes(bytes) => {
                let length = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
                self.literal_object(Kind::Bytes, length, 0, Some(bytes.clone()), dest);
            }
        }
    }

    /// Builds an object of `kind` with `len` components, and copies `data`,
    /// when there is any, into its payload from a passive data segment.
    fn literal_object(
        &mut self,
        kind: Kind,
        len: u32,
        variant: u32,
        data: Option<Vec<u8>>,
        dest: u32,
    ) {
        self.new_object(kind, len, variant);
        if let Some(data) = data {
            let count = u32::try_from(data.len()).unwrap_or(u32::MAX);
            let segment = self.lowering.segments.intern(data);
            self.push(&[
                get(OBJECT),
                c32(index(header::SIZE)),
                I::I32Add,
                c32(0),
                c32(count.cast_signed()),
                I::MemoryInit {
                    mem: 0,
                    data_index: segment,
                },
            ]);
        }
        self.store_object(dest);
    }

    /// Calls the constructor and leaves the new object in the `OBJECT` local.
    fn new_object(&mut self, kind: Kind, len: u32, variant: u32) {
        self.lowering.built_object = true;
        let new = self.lowering.fns.new;
        self.push(&[
            c32(kind.code().cast_signed()),
            c32(index(layout::row(kind).stride)),
            c32(len.cast_signed()),
            c32(variant.cast_signed()),
            I::Call(new),
            set(OBJECT),
        ]);
    }

    /// Hands the object in the `OBJECT` local to `dest`.
    fn store_object(&mut self, dest: u32) {
        let to = self.cell(dest);
        self.push(&[get(FRAME), get(OBJECT), I::I64ExtendI32U, st64(to)]);
        self.own(dest);
    }

    // -- data forms -----------------------------------------------------------

    /// Evaluates `operands` left to right into temporaries, each in the class of
    /// the component it fills.
    fn parts(
        &mut self,
        operands: &[&Expr],
        types: &[Type],
        origin: &SourceOrigin,
    ) -> Result<Vec<Part>, NotLowered> {
        let mut parts = Vec::with_capacity(operands.len());
        for (operand, component) in operands.iter().zip(types) {
            let class = self.class(component, origin)?;
            let slot = self.temp();
            self.expr(operand, slot)?;
            parts.push(Part::Slot { slot, class });
        }
        Ok(parts)
    }

    /// Builds an object whose components are `parts`, in order, and hands it to
    /// `dest`: each component's cell moves into the object, and a reference's
    /// slot is disowned.
    fn build(&mut self, kind: Kind, variant: u32, parts: &[Part], dest: u32) {
        let len = u32::try_from(parts.len()).unwrap_or(u32::MAX);
        self.new_object(kind, len, variant);
        let cells = layout::cells_offset(len);
        for (position, part) in (0_u32..).zip(parts) {
            let at = cells + 8 * position;
            match *part {
                Part::Zero => {
                    self.push(&[get(OBJECT), c64(0), st64(at)]);
                }
                Part::Slot { slot, class } => {
                    let from = self.cell(slot);
                    self.push(&[get(OBJECT), get(FRAME), ld64(from), st64(at)]);
                    let code = cell_class(class).code();
                    if code != 0 {
                        self.push(&[
                            get(OBJECT),
                            c32(i32::from(code)),
                            st8(header::SIZE + position),
                        ]);
                    }
                    if class == ValueClass::Ref {
                        self.disown(slot);
                    }
                }
            }
        }
        self.store_object(dest);
    }

    fn shape(&self, ty: &Type, origin: &SourceOrigin) -> Result<Shape, NotLowered> {
        self.lowering
            .env
            .shape(ty)
            .ok_or_else(|| classify::forms_of_type(ty, origin))
    }

    /// A declared or anonymous record: the operands are evaluated in the order
    /// the construction gives, and stored in the order of the record's type.
    fn record(
        &mut self,
        value_type: &Type,
        fields: &[(String, Expr)],
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let shape = self.shape(value_type, origin)?;
        let types = shape.components();
        let mark = self.next_temp;
        let mut slots = vec![None; types.len()];
        for (name, operand) in fields {
            let position = shape
                .field(name)
                .ok_or_else(|| classify::forms_of_type(value_type, origin))?;
            let component = types
                .get(position)
                .ok_or_else(|| classify::forms_of_type(value_type, origin))?;
            let class = self.class(component, origin)?;
            let slot = self.temp();
            self.expr(operand, slot)?;
            if let Some(entry) = slots.get_mut(position) {
                *entry = Some(Part::Slot { slot, class });
            }
        }
        let parts = slots
            .into_iter()
            .map(|part| part.unwrap_or(Part::Zero))
            .collect::<Vec<_>>();
        self.build(Kind::Record, 0, &parts, dest);
        self.release_to(mark);
        Ok(())
    }

    /// A declared or anonymous tuple.
    fn tuple(
        &mut self,
        value_type: &Type,
        components: &[Expr],
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let shape = self.shape(value_type, origin)?;
        let types = shape.components();
        let mark = self.next_temp;
        let operands = components.iter().collect::<Vec<_>>();
        let parts = self.parts(&operands, &types, origin)?;
        self.build(Kind::Tuple, 0, &parts, dest);
        self.release_to(mark);
        Ok(())
    }

    /// A variant of an enum: its discriminant is the variant's position in the
    /// type, and its payload is one cell, or none when the slot is `void`.
    fn variant(
        &mut self,
        value_type: &Type,
        variant: &str,
        payload: Option<&Expr>,
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let mark = self.next_temp;
        if *value_type == Type::Bool {
            let discriminant = u32::from(variant == "true");
            self.build(Kind::Enum, discriminant, &[], dest);
            return Ok(());
        }
        let shape = self.shape(value_type, origin)?;
        let Shape::Enum(variants) = &shape else {
            return Err(classify::forms_of_type(value_type, origin));
        };
        let discriminant = shape
            .variant(variant)
            .ok_or_else(|| classify::forms_of_type(value_type, origin))?;
        let slot_type = variants
            .get(discriminant)
            .map(|(_, ty)| ty.clone())
            .ok_or_else(|| classify::forms_of_type(value_type, origin))?;
        let class = self.class(&slot_type, origin)?;
        let mut parts = Vec::new();
        if let Some(payload) = payload {
            let slot = self.temp();
            self.expr(payload, slot)?;
            if slot_type == Type::Void {
                // A `void` payload is no payload; its operand is evaluated for
                // its effects only.
            } else {
                parts.push(Part::Slot { slot, class });
            }
        }
        self.build(
            Kind::Enum,
            u32::try_from(discriminant).unwrap_or(u32::MAX),
            &parts,
            dest,
        );
        self.release_to(mark);
        Ok(())
    }

    /// A wrapper: one cell holding the representation.
    fn wrap(
        &mut self,
        value_type: &Type,
        value: &Expr,
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let shape = self.shape(value_type, origin)?;
        let types = shape.components();
        let mark = self.next_temp;
        let parts = self.parts(&[value], &types, origin)?;
        self.build(Kind::Wrapper, 0, &parts, dest);
        self.release_to(mark);
        Ok(())
    }

    /// The widening of a member into a union attaches the member's
    /// discriminant, its position in the union's member list. Widening an atom
    /// singleton to `atom` is erased.
    fn widen(
        &mut self,
        value: &Expr,
        value_type: &Type,
        member: Option<usize>,
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let Some(member) = member else {
            return self.expr(value, dest);
        };
        let shape = self.shape(value_type, origin)?;
        let types = shape.components();
        let member_type = types
            .get(member)
            .ok_or_else(|| classify::forms_of_type(value_type, origin))?;
        let mark = self.next_temp;
        let parts = self.parts(&[value], std::slice::from_ref(member_type), origin)?;
        self.build(
            Kind::Union,
            u32::try_from(member).unwrap_or(u32::MAX),
            &parts,
            dest,
        );
        self.release_to(mark);
        Ok(())
    }

    /// A projection is a direct read of a component: the component's cell is
    /// copied out and a reference takes a `dup`, and the aggregate is dropped.
    /// Ownership changes hands here and nowhere else.
    fn project(
        &mut self,
        aggregate: &Expr,
        selector: Projection<'_>,
        value_type: &Type,
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let aggregate_type = aggregate.result_type();
        let shape = self.shape(&aggregate_type, origin)?;
        let types = shape.components();
        let position = match selector {
            Projection::Field(name) => shape
                .field(name)
                .ok_or_else(|| classify::forms_of_type(&aggregate_type, origin))?,
            Projection::Index(index) => index,
        };
        let component = types
            .get(position)
            .ok_or_else(|| classify::forms_of_type(&aggregate_type, origin))?;
        let class = self.class(component, origin)?;
        debug_assert_eq!(class_of(value_type), Ok(class));
        let len = u32::try_from(types.len()).unwrap_or(u32::MAX);
        let at = layout::cells_offset(len) + 8 * u32::try_from(position).unwrap_or(0);
        let mark = self.next_temp;
        let slot = self.temp();
        self.expr(aggregate, slot)?;
        let (to, from) = (self.cell(dest), self.cell(slot));
        self.push(&[get(FRAME), get(FRAME), ld32(from), ld64(at), st64(to)]);
        self.take_copy(dest, class);
        self.drop_slot(slot);
        self.release_to(mark);
        Ok(())
    }

    // -- the function ---------------------------------------------------------

    /// Lays the blocks out in the dispatching loop.
    fn finish(self) -> Lowered {
        let slots = self.max_slots;
        let fns = *self.lowering.fns;
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
        Lowered {
            code: FunctionCode {
                params: vec![ValType::I32],
                results: Vec::new(),
                locals: LOCALS.to_vec(),
                body,
            },
            slots,
        }
    }
}

/// What a projection selects: a record field or a tuple position.
#[derive(Clone, Copy, Debug)]
enum Projection<'a> {
    Field(&'a str),
    Index(usize),
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
            function,
            slots,
            arguments,
            resume,
        } => {
            body.extend([
                get(FRAME),
                c32(resume.cast_signed()),
                st32(frame::RESUME),
                c32(function.cast_signed()),
                c32(slots.cast_signed()),
                I::Call(fns.push_frame),
                set(CALLEE),
            ]);
            for (position, operand) in (0_u32..).zip(arguments) {
                body.extend([
                    get(CALLEE),
                    get(FRAME),
                    ld64(frame::cell_offset(known, operand.slot)),
                    st64(frame::cell_offset(*slots, position)),
                ]);
                if operand.reference {
                    body.extend([
                        get(CALLEE),
                        c32(i32::from(CellClass::Ref.code())),
                        st8(frame::class_offset(position)),
                        get(FRAME),
                        c32(0),
                        st8(frame::class_offset(operand.slot)),
                    ]);
                }
            }
            body.push(I::Return);
        }
        Term::Return { slot, class } => {
            body.extend([
                c32(0),
                get(FRAME),
                ld64(frame::cell_offset(known, *slot)),
                st64(state::RET),
            ]);
            if *class == ValueClass::Ref {
                body.extend([get(FRAME), c32(0), st8(frame::class_offset(*slot))]);
            }
            body.extend([get(FRAME), I::Call(fns.leave), I::Return]);
        }
    }
}

/// The class byte a value of `class` has in an object, as the host reads it.
const fn cell_class(class: ValueClass) -> CellClass {
    match class {
        ValueClass::Void | ValueClass::I32 => CellClass::I32,
        ValueClass::I64 => CellClass::I64,
        ValueClass::F32 => CellClass::F32,
        ValueClass::F64 => CellClass::F64,
        ValueClass::Ref => CellClass::Ref,
    }
}

/// The 32-bit pattern of an integer of at most 32 bits, as a cell holds it.
fn narrow(value: i32) -> u64 {
    u64::from(value.cast_unsigned())
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
