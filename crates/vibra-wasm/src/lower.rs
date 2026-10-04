//! Lowers checked IR to the functions of a module.
//!
//! Every language function, every module-value initializer, and every `lambda`
//! (a closure body, or the body that stands for a module function used as a
//! value) becomes one WebAssembly function `(frame) -> ()` that the dispatcher
//! of [`crate::layout`] ("Activations and the dispatcher") runs. This module
//! decides what such a function looks like inside.
//!
//! # The shape of a function
//!
//! A function is a set of numbered **basic blocks**. A block is straight-line
//! code that ends in one **terminator**: a jump, a branch on a `bool`, a call, a
//! tail call, or a return. The function enters its blocks through a `loop` over
//! a `br_table` on the `pc` local, which it reads from the frame's `resume` word
//! when it is entered, so a call can end the WebAssembly function and a later
//! entry can continue at the block after it. Nothing on the operand stack or in
//! a local survives a terminator that returns to the dispatcher, so **every
//! value lives in a frame slot**, each as one cell with the ownership rule of
//! the layout chapter (a slot owns a reference exactly while its class byte is
//! the reference class).
//!
//! # The slots of a frame
//!
//! ```text
//! 0 .. n          the parameters, fixed then labelled
//! n               the environment: the function value being run, or empty
//! n+1 .. n+1+k    the type arguments of the activation's own generic parameters
//! ..              the bindings of the checked IR, then the temporaries
//! ```
//!
//! The checked IR numbers a body's slots from its parameters on, so a binding's
//! slot `s` at or past `n` is the frame's slot `s + k + 1`. A closure reads what
//! it captured, and the type arguments of the activation that made it, through
//! its environment: the function value is an arena object (kind `function`) of
//! cells `[function][slots][arity][own][defaults..][captures..][types..]`, and
//! the activation holds one count of it in its environment slot.
//!
//! # Evaluating an expression
//!
//! An expression is evaluated **into a destination slot** the caller chose.
//! Its operands are evaluated into temporaries, which are allocated in a stack
//! discipline and so released by resetting a counter, and then the expression's
//! own operation moves them out: a constructor copies each operand's cell and
//! class byte into the new object and clears the temporary's byte, and a call
//! copies them into the callee's frame. A read of a local copies the cell and
//! takes a `dup`; a projection takes a `dup` of the component and a `drop` of
//! the aggregate; a `let` evaluates straight into its binding's slot and drops
//! the binding when its body has finished. Nothing is elided, so a program's
//! counts balance by construction and by the scan of a leaving frame, and not by
//! analysis.
//!
//! A slot's class byte says what its cell holds, whatever the type of the value
//! is. Where the type is known, the code writes the byte the type fixes. Where
//! the type is a generic parameter, the class is a run-time property, so the
//! code copies the byte with the cell and takes a `dup` when the byte is the
//! reference class: that is [`ValueClass::Dyn`], and it is the only place the
//! code branches on a class.
//!
//! An expression of type `never` (`return` and what contains it) writes no
//! destination; the code that follows it is reachable from no block.
//!
//! # Calls
//!
//! One call, whatever the callee. A call evaluates its callee (a function value
//! once, before any operand), its operands, and the type arguments of its
//! callee's generic parameters, which it works out as the reference interpreter
//! does, by matching the callee's signature against the types of the operands
//! and of the result at the call, and builds as run-time type descriptors. A
//! non-tail call pushes a frame and returns to the dispatcher. A tail call
//! stages its operands, replaces the current frame by the callee's with
//! `reframe`, and moves them in, so the depth never grows. A callee that is a
//! function value contributes its function and frame size from the value, and
//! the value itself becomes the new frame's environment.
//!
//! # Type arguments
//!
//! A generic activation carries its type arguments as slots, each a run-time
//! **type descriptor**: an arena tuple whose first cell is a number for the
//! type's constructor (`void` is [`HEAD_VOID`]) and whose other cells are the
//! descriptors of its arguments. A parameter nothing fixed is an empty slot.
//! Descriptors are built where a call is made and are counted like any other
//! value. Code reads them for what the language needs of a type that only the
//! type argument knows: an enum payload of a generic type is no payload when
//! the type argument is `void`.
//!
//! # Passes
//!
//! A call pushes a frame of the callee's size, and the callee's size is known
//! only after the callee is lowered, so the module is lowered twice: the first
//! pass finds each function's frame size, which depends on nothing but its own
//! body, and the second emits the code with every size known.

use std::borrow::Cow;
use std::collections::BTreeMap;

use vibra_ir::{
    CallTarget, CheckedProgram, Constant, Expr, FunctionSignature, SourceOrigin, Type,
    Value,
};
use wasm_encoder::{BlockType, Instruction as I, ValType};

use crate::classify;
use crate::encode::FunctionCode;
use crate::form::NotLowered;
use crate::layout::{self, CellClass, Kind, frame, header, state};
use crate::runtime::{
    Ins, Routines, ValueClass, c32, c64, get, index, ld8, ld32, ld64, set, st8, st32,
    st64,
};
use crate::types::{
    Shape, TypeEnv, bind, class_of, head_key, signature_params, written_name,
};
use vibra_ir::boundary::TrapCode;

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
/// The function value a call invokes.
const FV: u32 = 5;
/// The function the call enters.
const FUNC: u32 = 6;
/// The frame size of that function.
const SLOTS: u32 = 7;
/// The first cell of the function value.
const CELLS: u32 = 8;
/// The locals a function declares after its parameter.
const FIXED: [ValType; 8] = [ValType::I32; 8];
/// The first staging local, an `i64` for a cell, and then an `i32` for its class
/// byte, for each operand of a tail call.
const STAGE: u32 = 9;

/// The head of the descriptor of `void`.
pub(crate) const HEAD_VOID: u32 = 1;
/// The next head a type constructor is numbered with.
const HEAD_FIRST: u32 = 2;

/// The cells of a function value that come before its captures.
mod fv {
    /// The table index of the function.
    pub(super) const FUNCTION: u32 = 0;
    /// The slot count of its frame.
    pub(super) const SLOTS: u32 = 1;
    /// The number of its parameters, which a call checks.
    pub(super) const ARITY: u32 = 2;
    /// The number of its own type arguments.
    pub(super) const OWN: u32 = 3;
    /// The first default, one cell for each labelled parameter.
    pub(super) const DEFAULTS: u32 = 4;
}

/// The name a type argument the call could not fix is bound to. No parameter
/// has it, so it resolves to nothing.
const UNRESOLVED: &str = "?";

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

// -- lambdas ------------------------------------------------------------------

/// What a lambda's body is.
#[derive(Debug)]
enum LambdaKind<'a> {
    /// A `lambda` of the program.
    Closure {
        signature: &'a FunctionSignature,
        captures: usize,
        body: &'a Expr,
        slot_count: usize,
    },
    /// A module function used as a value: a body of one tail call of it.
    Wrapper {
        function: usize,
        signature: &'a FunctionSignature,
        origin: &'a SourceOrigin,
    },
}

/// One lambda: the body that runs when a function value is called.
#[derive(Debug)]
struct Lambda<'a> {
    kind: LambdaKind<'a>,
    /// The type parameters visible where the function value is made, which the
    /// value captures the type arguments of.
    outer: Vec<String>,
    /// The type parameters the lambda binds itself, whose arguments each call
    /// passes.
    own: Vec<String>,
}

/// The lambdas of a program, in the order a traversal meets them, which is the
/// order of their indices in the function table after the module-value
/// initializers.
#[derive(Debug, Default)]
pub(crate) struct Lambdas<'a> {
    list: Vec<Lambda<'a>>,
    by_node: BTreeMap<usize, usize>,
}

impl Lambdas<'_> {
    /// The number of lambdas.
    pub(crate) fn len(&self) -> usize {
        self.list.len()
    }
}

/// The address of an expression node, which names it for the whole lowering.
fn node(expr: &Expr) -> usize {
    std::ptr::from_ref(expr).addr()
}

/// The names in `signature` that `visible` does not already bind: the type
/// parameters a lambda of that signature binds itself.
fn own_params(signature: &FunctionSignature, visible: &[String]) -> Vec<String> {
    signature_params(signature)
        .into_iter()
        .filter(|name| !visible.contains(name))
        .collect()
}

/// The lambdas of a program.
pub(crate) fn collect(program: &CheckedProgram) -> Lambdas<'_> {
    let mut lambdas = Lambdas::default();
    for function in program.functions() {
        let visible = signature_params(function.signature());
        visit(function.body(), &visible, &mut lambdas);
    }
    for global in program.globals() {
        visit(global.initializer(), &[], &mut lambdas);
    }
    lambdas
}

fn visit<'a>(expr: &'a Expr, visible: &[String], out: &mut Lambdas<'a>) {
    match expr {
        Expr::Closure {
            signature,
            captures,
            body,
            slot_count,
            ..
        } => {
            for capture in captures {
                visit(capture, visible, out);
            }
            let own = own_params(signature, visible);
            let id = out.list.len();
            out.by_node.insert(node(expr), id);
            out.list.push(Lambda {
                kind: LambdaKind::Closure {
                    signature,
                    captures: captures.len(),
                    body,
                    slot_count: *slot_count,
                },
                outer: visible.to_vec(),
                own: own.clone(),
            });
            let mut inner = visible.to_vec();
            inner.extend(own);
            visit(body, &inner, out);
        }
        Expr::Function {
            function,
            signature,
            origin,
        } => {
            let id = out.list.len();
            out.by_node.insert(node(expr), id);
            out.list.push(Lambda {
                kind: LambdaKind::Wrapper {
                    function: *function,
                    signature,
                    origin,
                },
                outer: visible.to_vec(),
                own: own_params(signature, visible),
            });
        }
        other => {
            for child in children(other) {
                visit(child, visible, out);
            }
        }
    }
}

/// The expressions directly below `expr`, in evaluation order.
fn children(expr: &Expr) -> Vec<&Expr> {
    match expr {
        Expr::Literal { .. }
        | Expr::Default { .. }
        | Expr::Variable { .. }
        | Expr::Global { .. }
        | Expr::Function { .. }
        | Expr::Captured { .. } => Vec::new(),
        Expr::External { arguments, .. } => arguments.iter().collect(),
        Expr::Sequence { expressions, .. } => expressions.iter().collect(),
        Expr::Closure { captures, .. } => captures.iter().collect(),
        Expr::Let { value, body, .. } => vec![value, body],
        Expr::Match {
            scrutinee, arms, ..
        } => {
            let mut all: Vec<&Expr> = vec![scrutinee];
            all.extend(arms.iter().map(|arm| &arm.body));
            all
        }
        Expr::Widen { value, .. }
        | Expr::Try { value, .. }
        | Expr::Return { value, .. }
        | Expr::Wrap { value, .. } => vec![value],
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => vec![condition, then_branch, else_branch],
        Expr::Call {
            target, arguments, ..
        } => {
            let mut all: Vec<&Expr> = Vec::new();
            if let CallTarget::Indirect { callee, .. } = target {
                all.push(callee);
            }
            all.extend(arguments.iter());
            all
        }
        Expr::Record { fields, .. } => fields.iter().map(|(_, field)| field).collect(),
        Expr::Variant { payload, .. } => {
            payload.iter().map(|payload| &**payload).collect()
        }
        Expr::Project { record, .. } => vec![record],
        Expr::Tuple { components, .. } => components.iter().collect(),
        Expr::TupleProject { tuple, .. } => vec![tuple],
        Expr::Array { elements, .. } => elements.iter().collect(),
        Expr::Dict { entries, .. } => entries
            .iter()
            .flat_map(|(key, value)| [key, value])
            .collect(),
        Expr::Lookup {
            collection, key, ..
        } => vec![collection, key],
    }
}

/// The body of the lambda that stands for module function `function` used as a
/// value of `signature`: one call of it, in tail position, over the lambda's
/// own parameters.
fn wrapper_body(
    function: usize,
    signature: &FunctionSignature,
    origin: &SourceOrigin,
) -> Expr {
    let arguments = signature
        .slot_types()
        .into_iter()
        .enumerate()
        .map(|(slot, value_type)| Expr::Variable {
            slot,
            value_type,
            origin: origin.clone(),
        })
        .collect();
    Expr::Call {
        target: CallTarget::Direct(function),
        arguments,
        result: signature.result(),
        tail: true,
        origin: origin.clone(),
    }
}

// -- the lowering of a module ---------------------------------------------------

/// A lowered function and the number of slots its frame needs.
#[derive(Debug)]
pub(crate) struct Lowered {
    pub(crate) code: FunctionCode,
    pub(crate) slots: u32,
}

/// What a function knows of its own frame.
#[derive(Clone, Debug, Default)]
struct Shape1 {
    /// The parameters.
    arity: u32,
    /// The slots the checked IR numbers: the parameters and the bindings.
    ir_slots: u32,
    /// The type parameters the activation is passed.
    own: Vec<String>,
    /// The type parameters of the function value's creator.
    outer: Vec<String>,
    /// The labelled parameters, which the function value holds defaults of.
    labelled: u32,
    /// The captured values.
    captures: u32,
}

impl Shape1 {
    /// The cells of the function value this function is run from.
    fn env_len(&self) -> u32 {
        fv::DEFAULTS
            + self.labelled
            + self.captures
            + u32::try_from(self.outer.len()).unwrap_or(u32::MAX)
    }

    /// The slot of the environment.
    const fn env_slot(&self) -> u32 {
        self.arity
    }

    /// The slot of the type argument of own parameter `position`.
    const fn type_slot(&self, position: u32) -> u32 {
        self.arity + 1 + position
    }
}

/// Lowers the functions of one program against the routine indices of one
/// module.
#[derive(Debug)]
pub(crate) struct Lowering<'a> {
    fns: &'a Routines,
    program: &'a CheckedProgram,
    lambdas: &'a Lambdas<'a>,
    env: TypeEnv<'a>,
    /// The frame size of every unit: the language functions, then the
    /// initializers, then the lambdas, as the first pass found them (all `0`
    /// during the first pass).
    sizes: Vec<u32>,
    segments: Segments,
    built_object: bool,
    /// The number each type constructor has in a descriptor.
    heads: BTreeMap<String, u32>,
}

impl<'a> Lowering<'a> {
    pub(crate) fn new(
        fns: &'a Routines,
        program: &'a CheckedProgram,
        lambdas: &'a Lambdas<'a>,
        sizes: Vec<u32>,
    ) -> Self {
        let mut heads = BTreeMap::new();
        heads.insert("void".to_owned(), HEAD_VOID);
        Self {
            fns,
            program,
            lambdas,
            env: TypeEnv::new(program.types()),
            sizes,
            segments: Segments::default(),
            built_object: false,
            heads,
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

    /// The index of the first lambda.
    fn lambda_base(&self) -> usize {
        self.function_count() + self.program.globals().len()
    }

    /// The number a type constructor has in a descriptor.
    fn head(&mut self, ty: &Type) -> u32 {
        let key = head_key(ty);
        let next = u32::try_from(self.heads.len())
            .unwrap_or(u32::MAX)
            .saturating_add(HEAD_FIRST - 1);
        *self.heads.entry(key).or_insert(next)
    }

    /// Lowers unit `index`: language function `index`, the initializer of
    /// module value `index - function_count` when the index is past the
    /// functions, or lambda `index - lambda_base` when it is past those.
    ///
    /// # Errors
    ///
    /// The forms the unit uses that this step does not lower.
    pub(crate) fn function(&mut self, index: usize) -> Result<Lowered, NotLowered> {
        let program = self.program;
        let known = self.sizes.get(index).copied().unwrap_or(0);
        let functions = self.function_count();
        let module_size = || NotLowered::single(crate::Form::ModuleSize);
        let owned: Expr;
        let (body, shape, result): (&Expr, Shape1, Type) =
            if let Some(function) = program.functions().get(index) {
                let signature = function.signature();
                let arity = u32::try_from(signature.slot_types().len())
                    .map_err(|_| module_size())?;
                let ir_slots =
                    u32::try_from(function.slot_count()).map_err(|_| module_size())?;
                (
                    function.body(),
                    Shape1 {
                        arity,
                        ir_slots: ir_slots.max(arity),
                        own: signature_params(signature),
                        ..Shape1::default()
                    },
                    signature.result(),
                )
            } else if index < self.lambda_base() {
                let global = program
                    .globals()
                    .get(index.saturating_sub(functions))
                    .ok_or_else(module_size)?;
                (
                    global.initializer(),
                    Shape1 {
                        ir_slots: u32::try_from(global.slot_count())
                            .map_err(|_| module_size())?,
                        ..Shape1::default()
                    },
                    global.value_type(),
                )
            } else {
                let lambda = self
                    .lambdas
                    .list
                    .get(index - self.lambda_base())
                    .ok_or_else(module_size)?;
                match &lambda.kind {
                    LambdaKind::Closure {
                        signature,
                        captures,
                        body,
                        slot_count,
                    } => {
                        let arity = u32::try_from(signature.slot_types().len())
                            .map_err(|_| module_size())?;
                        (
                            &**body,
                            Shape1 {
                                arity,
                                ir_slots: u32::try_from(*slot_count)
                                    .map_err(|_| module_size())?
                                    .max(arity),
                                own: lambda.own.clone(),
                                outer: lambda.outer.clone(),
                                labelled: u32::try_from(signature.labelled().len())
                                    .map_err(|_| module_size())?,
                                captures: u32::try_from(*captures)
                                    .map_err(|_| module_size())?,
                            },
                            signature.result(),
                        )
                    }
                    LambdaKind::Wrapper {
                        function,
                        signature,
                        origin,
                    } => {
                        let arity = u32::try_from(signature.slot_types().len())
                            .map_err(|_| module_size())?;
                        owned = wrapper_body(*function, signature, origin);
                        (
                            &owned,
                            Shape1 {
                                arity,
                                ir_slots: arity,
                                own: lambda.own.clone(),
                                outer: lambda.outer.clone(),
                                labelled: u32::try_from(signature.labelled().len())
                                    .map_err(|_| module_size())?,
                                captures: 0,
                            },
                            signature.result(),
                        )
                    }
                }
            };
        let mut builder = Builder::new(self, known, shape);
        builder.class(&result, body.origin())?;
        let first = builder.new_block();
        builder.start(first);
        let dest = builder.temp();
        builder.expr(body, dest)?;
        builder.end(Term::Return { slot: dest });
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
    /// A language function, of a known frame size.
    Named { function: u32, slots: u32 },
    /// The function value in a slot of the caller, whose function and frame
    /// size are read from the value.
    Value { slot: u32 },
}

/// One operand of a call: a slot of the caller, and the slot of the callee it
/// moves into.
#[derive(Clone, Copy, Debug)]
struct Move {
    from: u32,
    to: u32,
    /// For a type argument that a function value may take fewer of than the
    /// call passes: the position, and the argument moves only when the value
    /// has more own type arguments than this.
    only_below: Option<u32>,
}

/// One part of an object being built.
#[derive(Clone, Copy, Debug)]
enum Part {
    /// The cell of a slot, moved into the object.
    Slot { slot: u32, class: ValueClass },
    /// A cell of constant bits, a scalar.
    Const(u64),
    /// A cell of zero: a `void` component.
    Zero,
}

/// Where the class byte of a value of a generic type is read from.
#[derive(Clone, Copy, Debug)]
enum ByteSource {
    /// Another slot of the frame.
    Slot(u32),
    /// A component of the object a slot holds.
    Component { slot: u32, position: u32 },
    /// A component of the function value of the call being made, in `FV`.
    Value { position: u32 },
    /// The `value` cell of a module value of the state's table.
    Table { class_value: u32 },
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
    shape: Shape1,
    next_temp: u32,
    max_slots: u32,
    blocks: Vec<Option<Block>>,
    current: u32,
    code: Vec<Ins>,
    /// The most operands a tail call stages.
    stage: usize,
}

/// Where a type parameter's argument is found in the running activation.
#[derive(Clone, Copy, Debug)]
enum TypeSlot {
    /// The activation's own: a slot of the frame.
    Own(u32),
    /// The creator's: a cell of the function value.
    Outer(u32),
}

impl<'l, 'a> Builder<'l, 'a> {
    fn new(lowering: &'l mut Lowering<'a>, known: u32, shape: Shape1) -> Self {
        let first_temp =
            shape.ir_slots + 1 + u32::try_from(shape.own.len()).unwrap_or(0);
        Self {
            lowering,
            known,
            shape,
            next_temp: first_temp,
            max_slots: first_temp,
            blocks: Vec::new(),
            current: 0,
            code: Vec::new(),
            stage: 0,
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

    /// The frame slot of slot `slot` of the checked IR.
    fn ir_slot(&self, slot: usize) -> Result<u32, NotLowered> {
        let slot = u32::try_from(slot)
            .map_err(|_| NotLowered::single(crate::Form::ModuleSize))?;
        Ok(if slot < self.shape.arity {
            slot
        } else {
            slot + 1 + u32::try_from(self.shape.own.len()).unwrap_or(0)
        })
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

    /// Stores constant bits in a slot's cell, and marks the slot with the class
    /// of a scalar.
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

    /// Drops the reference a slot owns, and disowns it.
    fn drop_slot(&mut self, slot: u32) {
        self.load_ref(slot);
        let drop = self.lowering.fns.drop;
        self.push(&[I::Call(drop)]);
        self.disown(slot);
    }

    /// Drops what a slot owns, when it can own a reference.
    fn discard(&mut self, slot: u32, class: ValueClass) {
        match class {
            ValueClass::Ref => self.drop_slot(slot),
            ValueClass::Dyn => {
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
            ValueClass::Void
            | ValueClass::I32
            | ValueClass::I64
            | ValueClass::F32
            | ValueClass::F64 => {}
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

    /// Takes the count and the class of the copy of a cell that was just made
    /// into `slot`. Where the type fixes the class, the byte is written and a
    /// reference takes a `dup`. Where it does not, the byte is copied from
    /// `source` and a `dup` is taken when it is the reference class.
    fn claim(&mut self, slot: u32, class: ValueClass, source: ByteSource) {
        match class {
            ValueClass::Ref => {
                self.load_ref(slot);
                let dup = self.lowering.fns.dup;
                self.push(&[I::Call(dup)]);
                self.own(slot);
            }
            ValueClass::Dyn => {
                match source {
                    ByteSource::Slot(from) => {
                        self.push(&[get(FRAME), ld8(frame::class_offset(from))]);
                    }
                    ByteSource::Component { slot, position } => {
                        let at = self.cell(slot);
                        self.push(&[
                            get(FRAME),
                            ld32(at),
                            ld8(header::SIZE + position),
                        ]);
                    }
                    ByteSource::Value { position } => {
                        self.push(&[get(FV), ld8(header::SIZE + position)]);
                    }
                    ByteSource::Table { class_value } => {
                        self.push(&[
                            c32(0),
                            ld32(state::MODULE_VALUES),
                            ld8(class_value),
                        ]);
                    }
                }
                self.push(&[set(SCRATCH)]);
                self.push(&[
                    get(FRAME),
                    get(SCRATCH),
                    st8(frame::class_offset(slot)),
                    get(SCRATCH),
                    c32(i32::from(CellClass::Ref.code())),
                    I::I32Eq,
                    I::If(BlockType::Empty),
                ]);
                self.load_ref(slot);
                let dup = self.lowering.fns.dup;
                self.push(&[I::Call(dup), I::End]);
            }
            ValueClass::Void
            | ValueClass::I32
            | ValueClass::I64
            | ValueClass::F32
            | ValueClass::F64 => self.set_byte(slot, cell_class(class).code()),
        }
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
                let slot = self.ir_slot(*slot)?;
                self.copy_cell(dest, slot);
                self.claim(dest, class, ByteSource::Slot(slot));
                Ok(())
            }
            Expr::Captured {
                slot, value_type, ..
            } => {
                let class = self.class(value_type, expr.origin())?;
                self.captured(*slot, class, dest)
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
                let slot = self.temp();
                self.expr(value, slot)?;
                self.end(Term::Return { slot });
                let dead = self.new_block();
                self.start(dead);
                Ok(())
            }
            Expr::Call {
                target,
                arguments,
                result,
                tail,
                ..
            } => self.call(target, arguments, result, *tail, expr.origin(), dest),
            Expr::Closure { .. } | Expr::Function { .. } => {
                self.function_value(expr, dest)
            }
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
            self.store_bits(dest, 0, ValueClass::Void);
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
                let slot = self.ir_slot(slot)?;
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

    // -- calls ----------------------------------------------------------------

    /// A call: the callee is evaluated first, and once, when it is a function
    /// value; then the operands, left to right, into temporaries; then the type
    /// arguments of the callee's generic parameters. A non-tail call moves them
    /// into the callee's frame and continues at a block that takes the result.
    /// A tail call replaces the current frame with the callee's.
    fn call(
        &mut self,
        target: &CallTarget,
        arguments: &[Expr],
        result: &Type,
        tail: bool,
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let mark = self.next_temp;
        let program = self.lowering.program;
        // The callee: its signature, as the call matches its operands against
        // it, and the generic parameters the call passes arguments for.
        let (callee, signature, free): (Callee, FunctionSignature, Vec<String>) =
            match target {
                CallTarget::Direct(function) => {
                    let callee = program
                        .functions()
                        .get(*function)
                        .ok_or_else(|| NotLowered::single(crate::Form::ModuleSize))?;
                    let slots =
                        self.lowering.sizes.get(*function).copied().unwrap_or(0);
                    (
                        Callee::Named {
                            function: u32::try_from(*function).map_err(|_| {
                                NotLowered::single(crate::Form::ModuleSize)
                            })?,
                            slots,
                        },
                        callee.signature().clone(),
                        signature_params(callee.signature()),
                    )
                }
                CallTarget::Indirect { callee, .. } => {
                    let Type::Function(signature) = callee.result_type() else {
                        return Err(classify::forms_of(callee));
                    };
                    let slot = self.temp();
                    self.expr(callee, slot)?;
                    let free = signature_params(&signature)
                        .into_iter()
                        .filter(|name| self.resolve(name).is_none())
                        .collect();
                    (Callee::Value { slot }, (*signature).clone(), free)
                }
                CallTarget::Contract { .. } => {
                    return Err(NotLowered::from_uses(vec![
                        crate::form::UnloweredForm::new(
                            crate::Form::Call,
                            Some(origin.clone()),
                        )
                        .with_detail("contract"),
                    ])
                    .unwrap_or_else(|| NotLowered::single(crate::Form::Call)));
                }
            };

        // The operands, in order. An omitted labelled operand is the callee's
        // default.
        let positional = signature.parameters().len();
        let mut operands = Vec::with_capacity(arguments.len());
        for (position, argument) in arguments.iter().enumerate() {
            self.class(&argument.result_type(), argument.origin())?;
            let slot = self.temp();
            if matches!(argument, Expr::Default { .. }) {
                let labelled = position
                    .checked_sub(positional)
                    .ok_or_else(|| classify::forms_of(argument))?;
                self.default_operand(&callee, &signature, labelled, argument, slot)?;
            } else {
                self.expr(argument, slot)?;
            }
            operands.push(slot);
        }

        // The type arguments: what the callee's generic parameters are at this
        // call, as types over this activation's own.
        let mut bound = BTreeMap::new();
        for (slot, argument) in signature.slot_types().iter().zip(arguments) {
            bind(slot, &argument.result_type(), &mut bound);
        }
        bind(&signature.result(), result, &mut bound);
        let mut moves = Vec::with_capacity(arguments.len() + free.len() + 1);
        for (position, slot) in (0_u32..).zip(&operands) {
            moves.push(Move {
                from: *slot,
                to: position,
                only_below: None,
            });
        }
        let arity = u32::try_from(arguments.len())
            .map_err(|_| NotLowered::single(crate::Form::ModuleSize))?;
        if let Callee::Value { slot } = callee {
            moves.push(Move {
                from: slot,
                to: arity,
                only_below: None,
            });
        }
        for (position, name) in (0_u32..).zip(&free) {
            let ty = bound
                .get(name)
                .cloned()
                .unwrap_or_else(|| Type::Param(UNRESOLVED.to_owned()));
            let slot = self.temp();
            self.build_type(&ty, slot)?;
            moves.push(Move {
                from: slot,
                to: arity + 1 + position,
                only_below: matches!(callee, Callee::Value { .. }).then_some(position),
            });
        }
        self.class(&signature.result(), origin)?;
        self.release_to(mark);
        if tail {
            self.stage = self.stage.max(moves.len());
            self.end(Term::Tail {
                callee,
                moves,
                arity,
            });
            let dead = self.new_block();
            self.start(dead);
            return Ok(());
        }
        let resume = self.new_block();
        self.end(Term::Call {
            callee,
            moves,
            arity,
            resume,
        });
        self.start(resume);
        self.take_return(dest);
        Ok(())
    }

    /// An omitted labelled operand: the callee's default, which a module
    /// function declares and a function value holds.
    fn default_operand(
        &mut self,
        callee: &Callee,
        signature: &FunctionSignature,
        labelled: usize,
        argument: &Expr,
        slot: u32,
    ) -> Result<(), NotLowered> {
        match callee {
            Callee::Named { .. } => {
                let constant = signature
                    .labelled()
                    .get(labelled)
                    .and_then(|parameter| parameter.default())
                    .ok_or_else(|| classify::forms_of(argument))?;
                self.constant(constant, argument.origin(), slot)
            }
            Callee::Value { slot: value } => {
                let class = self.class(&argument.result_type(), argument.origin())?;
                let position = fv::DEFAULTS
                    + u32::try_from(labelled)
                        .map_err(|_| NotLowered::single(crate::Form::ModuleSize))?;
                self.load_value(*value);
                self.push(&[
                    get(FRAME),
                    get(CELLS),
                    ld64(8 * position),
                    st64(self.cell(slot)),
                ]);
                self.claim(slot, class, ByteSource::Value { position });
                Ok(())
            }
        }
    }

    /// A constant, built like the expression it stands for.
    fn constant(
        &mut self,
        constant: &Constant,
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let expr = constant.to_expr(origin);
        self.expr(&expr, dest)
    }

    /// Reads the function value in `slot`: its address in `FV` and its first
    /// cell in `CELLS`.
    fn load_value(&mut self, slot: u32) {
        let at = self.cell(slot);
        self.push(&[
            get(FRAME),
            ld32(at),
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
        ]);
    }

    /// Takes the cell a returning activation left in `ret` into `dest`, with
    /// its class byte, so `dest` owns what the callee returned.
    fn take_return(&mut self, dest: u32) {
        let to = self.cell(dest);
        self.push(&[
            get(FRAME),
            c32(0),
            ld64(state::RET),
            st64(to),
            get(FRAME),
            c32(0),
            ld32(state::RET_CLASS),
            st8(frame::class_offset(dest)),
        ]);
    }

    // -- function values and captures -------------------------------------------

    /// A function value: an object of the function and its frame size, the
    /// defaults of its labelled parameters, what it captured, and the type
    /// arguments of the activation that made it.
    fn function_value(&mut self, expr: &Expr, dest: u32) -> Result<(), NotLowered> {
        let lowering = &*self.lowering;
        let id = lowering
            .lambdas
            .by_node
            .get(&node(expr))
            .copied()
            .ok_or_else(|| classify::forms_of(expr))?;
        let lambdas = lowering.lambdas;
        let lambda = lambdas
            .list
            .get(id)
            .ok_or_else(|| NotLowered::single(crate::Form::ModuleSize))?;
        let unit = lowering.lambda_base() + id;
        let program = lowering.program;
        let slots = lowering.sizes.get(unit).copied().unwrap_or(0);
        let module_size = || NotLowered::single(crate::Form::ModuleSize);
        let mark = self.next_temp;
        let mut parts: Vec<Part> = Vec::new();
        let (arity, defaults, captures): (usize, Vec<Option<Constant>>, &[Expr]) =
            match (&lambda.kind, expr) {
                (
                    LambdaKind::Closure { signature, .. },
                    Expr::Closure { captures, .. },
                ) => (
                    signature.slot_types().len(),
                    signature
                        .labelled()
                        .iter()
                        .map(|parameter| parameter.default().cloned())
                        .collect(),
                    captures,
                ),
                (LambdaKind::Wrapper { function, .. }, _) => {
                    let callee =
                        program.functions().get(*function).ok_or_else(module_size)?;
                    (
                        callee.signature().slot_types().len(),
                        callee
                            .signature()
                            .labelled()
                            .iter()
                            .map(|parameter| parameter.default().cloned())
                            .collect(),
                        &[],
                    )
                }
                _ => return Err(classify::forms_of(expr)),
            };
        let own = u32::try_from(lambda.own.len()).map_err(|_| module_size())?;
        for bits in [
            u64::try_from(unit).map_err(|_| module_size())?,
            u64::from(slots),
            u64::try_from(arity).map_err(|_| module_size())?,
            u64::from(own),
        ] {
            parts.push(Part::Const(bits));
        }
        for default in &defaults {
            match default {
                Some(constant) => {
                    let class = self.class(&constant.value_type(), expr.origin())?;
                    let slot = self.temp();
                    self.constant(constant, expr.origin(), slot)?;
                    parts.push(Part::Slot { slot, class });
                }
                None => parts.push(Part::Zero),
            }
        }
        let capture_types: Vec<Type> = match expr {
            Expr::Closure { capture_types, .. } => capture_types.clone(),
            _ => Vec::new(),
        };
        for (capture, ty) in captures.iter().zip(&capture_types) {
            let class = self.class(ty, capture.origin())?;
            let slot = self.temp();
            self.expr(capture, slot)?;
            parts.push(Part::Slot { slot, class });
        }
        for name in &lambda.outer {
            let slot = self.temp();
            self.type_argument(name, slot);
            parts.push(Part::Slot {
                slot,
                class: ValueClass::Dyn,
            });
        }
        self.build(Kind::Function, 0, &parts, dest);
        self.release_to(mark);
        Ok(())
    }

    /// A read of what the running closure captured: a cell of its function
    /// value.
    fn captured(
        &mut self,
        capture: usize,
        class: ValueClass,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let position = fv::DEFAULTS
            + self.shape.labelled
            + u32::try_from(capture)
                .map_err(|_| NotLowered::single(crate::Form::ModuleSize))?;
        let env = self.shape.env_slot();
        let at = layout::cells_offset(self.shape.env_len()) + 8 * position;
        let to = self.cell(dest);
        let from = self.cell(env);
        self.push(&[get(FRAME), get(FRAME), ld32(from), ld64(at), st64(to)]);
        self.claim(
            dest,
            class,
            ByteSource::Component {
                slot: env,
                position,
            },
        );
        Ok(())
    }

    // -- type arguments ---------------------------------------------------------

    /// Where the running activation has the argument of type parameter `name`.
    fn resolve(&self, name: &str) -> Option<TypeSlot> {
        let matches = |known: &String| known == name || written_name(known) == name;
        if let Some(position) = self.shape.own.iter().position(matches) {
            return Some(TypeSlot::Own(u32::try_from(position).ok()?));
        }
        let position = self.shape.outer.iter().position(matches)?;
        Some(TypeSlot::Outer(u32::try_from(position).ok()?))
    }

    /// Copies the argument of type parameter `name` into `dest`, which then
    /// owns a count of its descriptor, or is empty when nothing fixed it.
    fn type_argument(&mut self, name: &str, dest: u32) {
        match self.resolve(name) {
            Some(TypeSlot::Own(position)) => {
                let from = self.shape.type_slot(position);
                self.copy_cell(dest, from);
                self.claim(dest, ValueClass::Dyn, ByteSource::Slot(from));
            }
            Some(TypeSlot::Outer(position)) => {
                let env = self.shape.env_slot();
                let at = layout::cells_offset(self.shape.env_len())
                    + 8 * (fv::DEFAULTS
                        + self.shape.labelled
                        + self.shape.captures
                        + position);
                let from = self.cell(env);
                let to = self.cell(dest);
                self.push(&[get(FRAME), get(FRAME), ld32(from), ld64(at), st64(to)]);
                self.claim(
                    dest,
                    ValueClass::Dyn,
                    ByteSource::Component {
                        slot: env,
                        position: fv::DEFAULTS
                            + self.shape.labelled
                            + self.shape.captures
                            + position,
                    },
                );
            }
            None => self.store_bits(dest, 0, ValueClass::Void),
        }
    }

    /// Builds the descriptor of `ty` into `dest`: a parameter is the argument
    /// the activation holds for it, and any other type is a new tuple of its
    /// constructor's number and the descriptors of its arguments.
    fn build_type(&mut self, ty: &Type, dest: u32) -> Result<(), NotLowered> {
        if let Type::Param(name) = ty {
            self.type_argument(name, dest);
            return Ok(());
        }
        let mark = self.next_temp;
        let head = self.lowering.head(ty);
        let mut parts = vec![Part::Const(u64::from(head))];
        for component in ty.components() {
            let slot = self.temp();
            self.build_type(&component, slot)?;
            parts.push(Part::Slot {
                slot,
                class: ValueClass::Dyn,
            });
        }
        self.build(Kind::Tuple, 0, &parts, dest);
        self.release_to(mark);
        Ok(())
    }

    // -- module values ----------------------------------------------------------

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
        self.claim(dest, class, ByteSource::Table { class_value });
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
            callee: Callee::Named { function, slots },
            moves: Vec::new(),
            arity: 0,
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
        self.take_return(dest);
        self.goto(join, join);
        Ok(())
    }

    // -- literals -------------------------------------------------------------

    fn literal(&mut self, value: &Value, dest: u32) {
        match value {
            Value::Void => self.store_bits(dest, 0, ValueClass::Void),
            Value::Char(value) => {
                self.store_bits(dest, u64::from(u32::from(*value)), ValueClass::I32);
            }
            Value::I8(value) => {
                self.store_bits(dest, narrow(i32::from(*value)), ValueClass::I32);
            }
            Value::I16(value) => {
                self.store_bits(dest, narrow(i32::from(*value)), ValueClass::I32);
            }
            Value::I32(value) => self.store_bits(dest, narrow(*value), ValueClass::I32),
            Value::U8(value) => {
                self.store_bits(dest, u64::from(*value), ValueClass::I32);
            }
            Value::U16(value) => {
                self.store_bits(dest, u64::from(*value), ValueClass::I32);
            }
            Value::U32(value) => {
                self.store_bits(dest, u64::from(*value), ValueClass::I32);
            }
            Value::I64(value) => {
                self.store_bits(dest, value.cast_unsigned(), ValueClass::I64);
            }
            Value::U64(value) => self.store_bits(dest, *value, ValueClass::I64),
            Value::F32(bits) => {
                self.store_bits(dest, u64::from(*bits), ValueClass::F32)
            }
            Value::F64(bits) => self.store_bits(dest, *bits, ValueClass::F64),
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
    /// `dest`: each component's cell and class byte move into the object, and a
    /// slot that owned a reference is disowned.
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
                Part::Const(bits) => {
                    self.push(&[get(OBJECT), c64(bits.cast_signed()), st64(at)]);
                }
                Part::Slot { slot, class } => {
                    let from = self.cell(slot);
                    self.push(&[get(OBJECT), get(FRAME), ld64(from), st64(at)]);
                    match class {
                        ValueClass::Dyn => {
                            self.push(&[
                                get(OBJECT),
                                get(FRAME),
                                ld8(frame::class_offset(slot)),
                                st8(header::SIZE + position),
                            ]);
                            self.disown(slot);
                        }
                        ValueClass::Ref => {
                            self.push(&[
                                get(OBJECT),
                                c32(i32::from(CellClass::Ref.code())),
                                st8(header::SIZE + position),
                            ]);
                            self.disown(slot);
                        }
                        ValueClass::Void
                        | ValueClass::I32
                        | ValueClass::I64
                        | ValueClass::F32
                        | ValueClass::F64 => {
                            let code = cell_class(class).code();
                            if code != 0 {
                                self.push(&[
                                    get(OBJECT),
                                    c32(i32::from(code)),
                                    st8(header::SIZE + position),
                                ]);
                            }
                        }
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
    /// type, and its payload is one cell, or none when the slot is `void`. A
    /// slot of a generic type is `void` when the type argument is, which only
    /// the run time knows, so the variant is built either way and the code
    /// chooses by the descriptor.
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
        let discriminant_word = u32::try_from(discriminant).unwrap_or(u32::MAX);
        let slot_type = variants
            .get(discriminant)
            .map(|(_, ty)| ty.clone())
            .ok_or_else(|| classify::forms_of_type(value_type, origin))?;
        let class = self.class(&slot_type, origin)?;
        let Some(payload) = payload else {
            self.build(Kind::Enum, discriminant_word, &[], dest);
            self.release_to(mark);
            return Ok(());
        };
        let slot = self.temp();
        self.expr(payload, slot)?;
        match &slot_type {
            // A `void` payload is no payload; its operand is evaluated for its
            // effects only.
            Type::Void => self.build(Kind::Enum, discriminant_word, &[], dest),
            // The slot of a generic type holds no payload exactly when its type
            // argument is `void`.
            Type::Param(name) => {
                let test = self.temp();
                self.type_argument(name, test);
                self.push(&[
                    get(FRAME),
                    ld8(frame::class_offset(test)),
                    c32(i32::from(CellClass::Ref.code())),
                    I::I32Eq,
                    I::If(BlockType::Result(ValType::I32)),
                ]);
                self.load_ref(test);
                self.push(&[
                    set(OBJECT),
                    get(OBJECT),
                    get(OBJECT),
                    ld32(header::LEN),
                    c32(7),
                    I::I32Add,
                    c32(-8),
                    I::I32And,
                    I::I32Add,
                    ld32(header::SIZE),
                    c32(i32::try_from(HEAD_VOID).unwrap_or(1)),
                    I::I32Eq,
                    I::Else,
                    c32(0),
                    I::End,
                    set(SCRATCH),
                ]);
                self.discard(test, ValueClass::Dyn);
                self.push(&[get(SCRATCH)]);
                let (void_block, some_block, join) =
                    (self.new_block(), self.new_block(), self.new_block());
                self.end(Term::Branch {
                    then_block: void_block,
                    else_block: some_block,
                });
                self.start(void_block);
                self.build(Kind::Enum, discriminant_word, &[], dest);
                self.goto(join, some_block);
                self.build(
                    Kind::Enum,
                    discriminant_word,
                    &[Part::Slot { slot, class }],
                    dest,
                );
                self.goto(join, join);
            }
            _ => self.build(
                Kind::Enum,
                discriminant_word,
                &[Part::Slot { slot, class }],
                dest,
            ),
        }
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
        debug_assert!(
            class_of(value_type).is_ok(),
            "a projection's type has a class"
        );
        let len = u32::try_from(types.len()).unwrap_or(u32::MAX);
        let position = u32::try_from(position).unwrap_or(0);
        let at = layout::cells_offset(len) + 8 * position;
        let mark = self.next_temp;
        let slot = self.temp();
        self.expr(aggregate, slot)?;
        let (to, from) = (self.cell(dest), self.cell(slot));
        self.push(&[get(FRAME), get(FRAME), ld32(from), ld64(at), st64(to)]);
        self.claim(dest, class, ByteSource::Component { slot, position });
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
        let stage = self.stage;
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
        for _ in 0..stage {
            locals.push(ValType::I64);
            locals.push(ValType::I32);
        }
        Lowered {
            code: FunctionCode {
                params: vec![ValType::I32],
                results: Vec::new(),
                locals,
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
            callee,
            moves,
            arity,
            resume,
        } => {
            body.extend([get(FRAME), c32(resume.cast_signed()), st32(frame::RESUME)]);
            let slots = enter(fns, known, *callee, *arity, body);
            for operand in moves {
                move_in(fns, known, operand, slots, body);
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
                let store = |body: &mut Vec<Ins>| {
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
                };
                match operand.only_below {
                    Some(below) => {
                        body.extend([
                            get(CELLS),
                            ld32(8 * fv::OWN),
                            c32(below.cast_signed()),
                            I::I32GtU,
                            I::If(BlockType::Empty),
                        ]);
                        store(body);
                        body.extend([
                            I::Else,
                            I::LocalGet(class),
                            c32(i32::from(CellClass::Ref.code())),
                            I::I32Eq,
                            I::If(BlockType::Empty),
                            I::LocalGet(cell),
                            I::I32WrapI64,
                            I::Call(fns.drop),
                            I::End,
                            I::End,
                        ]);
                    }
                    None => store(body),
                }
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
/// address in `FV`, its cells in `CELLS`, and its function and frame size in
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
        c32(TrapCode::InvalidCheckedProgram.code()),
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
/// the source is left owning nothing. A type argument a function value takes
/// fewer of than the call passes is dropped instead.
fn move_in(
    fns: &Routines,
    known: u32,
    operand: &Move,
    slots: Option<u32>,
    body: &mut Vec<Ins>,
) {
    let from_cell = frame::cell_offset(known, operand.from);
    let from_class = frame::class_offset(operand.from);
    let moving = |body: &mut Vec<Ins>| {
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
    };
    match operand.only_below {
        Some(below) => {
            body.extend([
                get(CELLS),
                ld32(8 * fv::OWN),
                c32(below.cast_signed()),
                I::I32GtU,
                I::If(BlockType::Empty),
            ]);
            moving(body);
            body.extend([
                I::Else,
                get(FRAME),
                ld8(from_class),
                c32(i32::from(CellClass::Ref.code())),
                I::I32Eq,
                I::If(BlockType::Empty),
                get(FRAME),
                ld32(from_cell),
                I::Call(fns.drop),
                I::End,
                get(FRAME),
                c32(0),
                st8(from_class),
                I::End,
            ]);
        }
        None => moving(body),
    }
}

/// The class byte a value of `class` has in an object, as the host reads it.
const fn cell_class(class: ValueClass) -> CellClass {
    match class {
        ValueClass::Void | ValueClass::I32 | ValueClass::Dyn => CellClass::I32,
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
