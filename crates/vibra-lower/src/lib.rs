//! Lowers a checked program to the execution IR of `vibra-exec`.
//!
//! `docs/roadmap/nexus/design.md`, "From checked IR", is the table this crate
//! implements, one package at a time. This is package X3-03.
//!
//! # The slice
//!
//! The checked forms of the table's rows marked X3-03, and what each becomes:
//!
//! | Checked form | Execution IR |
//! | --- | --- |
//! | `Literal` | `Scalar`; `Constant` for `str`, `bytes`, atom; `bool` is `Build Enum` of variant 0 or 1 with no cell |
//! | `Sequence` | a straight line; each discarded value is dropped |
//! | `Variable` | `Copy` from the slot |
//! | `Captured` | `Cell` of the environment at `3 + index` |
//! | `Let` | the value is evaluated into the binding's slot, which is dropped when the body ends |
//! | `If` | `Variant` of the `bool`, then `Branch` |
//! | `Call` with a `Direct` or an `Indirect` target, `tail` or not | `Call` or `TailCall` with `Direct` or `Indirect` |
//! | `Function`, `Closure` | `Function` over a body with an environment; a module function used as a value gets a wrapper body that is one tail call |
//! | `Global` | `Global` |
//! | `Host` | `Host`, which ends the block |
//! | `Return` | `Return` |
//!
//! Every other form is outside the slice, and so is anything the table gives
//! to a later package. `lower` answers [`LowerError::NotLowered`] with the
//! name of the first such form it meets, never a partial program:
//!
//! - `External`, a call to a compiler intrinsic: `External numeric` (X3-10)
//!   or `External collection` (X3-16);
//! - `Record`, `Tuple`, `Variant`, `Wrap`, `Array`, `Widen`, `Project`,
//!   `TupleProject` (X3-11);
//! - `Match` and `Try` (X3-12);
//! - a generic function, closure, or activation (X3-13);
//! - `Default`, a labelled parameter, and a variadic tail (X3-14);
//! - a `Call` with a contract target (X3-15);
//! - `Dict` and `Lookup` (X3-16);
//! - a test assertion function (X3-18);
//! - an entry that takes a parameter.
//!
//! The table does not say what a `Call` of a function whose body is an
//! `External` becomes before X3-10; it is outside the slice because the body
//! is. Whether the `Call` row includes calls to such wrappers in tail position
//! is therefore left to X3-10.
//!
//! # Whole program
//!
//! The lowering starts at the entry and follows every module function it
//! calls or reifies, every module value it reads, and every closure body. If
//! anything so reached is outside the slice the result is an error. Code that
//! can never run (after a `return`) is lowered and discarded, so it is checked
//! too.
//!
//! # Ownership
//!
//! Every slot is empty or full (rule V6). The rules, per form:
//!
//! - an expression is lowered *into* a destination slot that is empty before
//!   it and full after it, unless the expression never completes;
//! - a variable is copied, so a binding is read any number of times and is
//!   dropped once, when its `Let` body ends; a parameter is released with the
//!   frame;
//! - the operands of a call, a closure's captures, and the value of a
//!   `Return` are evaluated into fresh temporaries and consumed;
//! - the operands of a host call are temporaries that the call borrows and
//!   that the resume block drops; the callee of an indirect call is a
//!   temporary that the call borrows and, for a non-tail call, the resume
//!   block drops;
//! - a leading element of a sequence, a condition, and a `Let` binding are
//!   dropped as soon as they are no longer needed;
//! - a frame that ends by `Return` or by a tail transfer releases what it
//!   still owns, so an early exit and a tail call drop nothing explicitly.
//!
//! Temporaries are allocated like a stack above the checked slots and freed
//! when the form that took them ends, so a frame is as large as the deepest
//! nesting of live operands.
//!
//! # Identifiers
//!
//! Identifiers are assigned in order of first reference, which is
//! deterministic because nothing here iterates a hash table: the entry is
//! function 0; a module function, a module value's initializer, a closure
//! body, and the wrapper of a module function used as a value each take the
//! next function ID when first referenced. A closure body and a wrapper are
//! lowered at once, at the reference; a module function and an initializer
//! are lowered after the body that referenced them, in the order they were
//! referenced. Module values, constants, and origins are numbered in the order
//! they are first used.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;

use vibra_exec::{
    Block, BlockId, Callee, ConstId, Constant, FuncId, Function, GlobalId, Inst,
    MAX_SLOTS, ObjectKind, OriginId, Program, ScalarClass, Slot, Terminator,
};
use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::{
    CallTarget, CheckedProgram, Expr, FunctionSignature, SourceOrigin, Value,
};

/// The cell of a function value that holds its first capture: the function,
/// its frame size, and its parameter count come first.
const FIRST_CAPTURE: usize = 3;

/// Defines the names of [`form`] and the list of all of them together, so the
/// list cannot omit one.
macro_rules! forms {
    ($($(#[$meta:meta])* $name:ident = $text:literal;)*) => {
        /// The names of the forms [`lower`] refuses, as [`LowerError::NotLowered`]
        /// carries them. The lowering names a form only through these
        /// constants, so [`form::ALL`] is every name it can return, and a
        /// consumer that maps a name to the work that will lower it can be
        /// checked against the list.
        pub mod form {
            $($(#[$meta])* pub const $name: &str = $text;)*

            /// Every name of this module, in declaration order.
            pub const ALL: &[&str] = &[$($name),*];
        }
    };
}

forms! {
    /// An entry that takes a parameter.
    ENTRY_WITH_PARAMETERS = "entry with parameters";
    /// A labelled parameter or a variadic tail.
    LABELLED_OR_VARIADIC_PARAMETER = "labelled or variadic parameter";
    /// A generic function, closure, or activation.
    GENERIC_ACTIVATION = "generic activation";
    /// A test assertion function.
    TEST_ASSERTION = "test assertion";
    /// A call to a compiler intrinsic of a numeric primitive or a `char`.
    EXTERNAL_NUMERIC = "External numeric";
    /// A call to any other compiler intrinsic: text, bytes, arrays, dicts.
    EXTERNAL_COLLECTION = "External collection";
    /// An omitted labelled argument.
    DEFAULT = "Default";
    /// A `match`.
    MATCH = "Match";
    /// A `widen` to a union or an interface.
    WIDEN = "Widen";
    /// A `try`.
    TRY = "Try";
    /// A record literal.
    RECORD = "Record";
    /// An enum variant literal.
    VARIANT = "Variant";
    /// A wrapper literal.
    WRAP = "Wrap";
    /// A record projection.
    PROJECT = "Project";
    /// A tuple literal.
    TUPLE = "Tuple";
    /// A tuple projection.
    TUPLE_PROJECT = "TupleProject";
    /// An array literal.
    ARRAY = "Array";
    /// A dict literal.
    DICT = "Dict";
    /// A dict lookup.
    LOOKUP = "Lookup";
    /// A call whose target is a contract member.
    CONTRACT_CALL = "Call of a contract member";
}

/// Whether `intrinsic` is an operation of a numeric primitive or a `char`
/// (package X3-10), and not of text, bytes, arrays, or dicts (X3-16).
const fn is_numeric(intrinsic: CompilerIntrinsic) -> bool {
    matches!(
        intrinsic,
        CompilerIntrinsic::Integer(..)
            | CompilerIntrinsic::Float(..)
            | CompilerIntrinsic::Convert(..)
            | CompilerIntrinsic::CharToU32
            | CompilerIntrinsic::CharFromU32
    )
}

/// Why a checked program was not lowered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LowerError {
    /// The program reaches a form outside the slice. The text names it.
    NotLowered(&'static str),
    /// The program is not what the checker produces; the text names how.
    Malformed(&'static str),
    /// A function needs a larger frame than [`MAX_SLOTS`].
    FrameTooLarge {
        /// The function, by name.
        function: String,
    },
}

impl fmt::Display for LowerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotLowered(form) => write!(formatter, "not lowered: {form}"),
            Self::Malformed(what) => {
                write!(formatter, "malformed checked program: {what}")
            }
            Self::FrameTooLarge { function } => {
                write!(formatter, "the frame of `{function}` is too large")
            }
        }
    }
}

impl std::error::Error for LowerError {}

type Lowered<T> = Result<T, LowerError>;

/// Lowers `program` to execution IR.
///
/// The result always passes `vibra_exec::verify`.
///
/// # Errors
///
/// [`LowerError::NotLowered`] when anything reachable from the entry is
/// outside the slice.
pub fn lower(program: &CheckedProgram) -> Lowered<Program> {
    let mut lowering = Lowering::new(program);
    let entry = lowering.module_function(program.entry_index())?;
    let takes = program.entry().signature().slot_types().len();
    if takes != 0 {
        return Err(LowerError::NotLowered(form::ENTRY_WITH_PARAMETERS));
    }
    lowering.drain()?;
    lowering.finish(entry)
}

enum Job {
    Module(usize, FuncId),
    Global(usize, FuncId),
}

struct Lowering<'p> {
    program: &'p CheckedProgram,
    functions: Vec<Option<Function>>,
    module_ids: BTreeMap<usize, FuncId>,
    wrapper_ids: BTreeMap<usize, FuncId>,
    global_ids: BTreeMap<usize, GlobalId>,
    initializers: Vec<FuncId>,
    constants: Vec<Constant>,
    origins: Vec<SourceOrigin>,
    origin_ids: BTreeMap<SourceOrigin, OriginId>,
    queue: VecDeque<Job>,
}

impl<'p> Lowering<'p> {
    fn new(program: &'p CheckedProgram) -> Self {
        Self {
            program,
            functions: Vec::new(),
            module_ids: BTreeMap::new(),
            wrapper_ids: BTreeMap::new(),
            global_ids: BTreeMap::new(),
            initializers: Vec::new(),
            constants: Vec::new(),
            origins: Vec::new(),
            origin_ids: BTreeMap::new(),
            queue: VecDeque::new(),
        }
    }

    fn reserve(&mut self) -> Lowered<FuncId> {
        let id = u32::try_from(self.functions.len())
            .map_err(|_| LowerError::Malformed("too many functions"))?;
        self.functions.push(None);
        Ok(FuncId(id))
    }

    fn fill(&mut self, id: FuncId, function: Function) -> Lowered<()> {
        match self.functions.get_mut(id.index()) {
            Some(slot) => {
                *slot = Some(function);
                Ok(())
            }
            None => Err(LowerError::Malformed("a function that was never reserved")),
        }
    }

    /// The parameter count of a signature the slice can lower.
    fn parameters(signature: &FunctionSignature) -> Lowered<u32> {
        if !signature.labelled().is_empty() || signature.variadic().is_some() {
            return Err(LowerError::NotLowered(form::LABELLED_OR_VARIADIC_PARAMETER));
        }
        if signature.has_params() {
            return Err(LowerError::NotLowered(form::GENERIC_ACTIVATION));
        }
        u32::try_from(signature.parameters().len())
            .map_err(|_| LowerError::Malformed("too many parameters"))
    }

    /// The parameter count of module function `index`, once it is known to be
    /// inside the slice.
    fn module_parameters(&self, index: usize) -> Lowered<u32> {
        let function =
            self.program
                .functions()
                .get(index)
                .ok_or(LowerError::Malformed(
                    "a call of a function that does not exist",
                ))?;
        if function.test_assertion().is_some() {
            return Err(LowerError::NotLowered(form::TEST_ASSERTION));
        }
        Self::parameters(function.signature())
    }

    /// The function ID of module function `index`, queueing its body the first
    /// time it is referenced.
    fn module_function(&mut self, index: usize) -> Lowered<FuncId> {
        if let Some(id) = self.module_ids.get(&index) {
            return Ok(*id);
        }
        self.module_parameters(index)?;
        let id = self.reserve()?;
        self.module_ids.insert(index, id);
        self.queue.push_back(Job::Module(index, id));
        Ok(id)
    }

    /// The function ID of the wrapper that stands for module function `index`
    /// used as a value: one body that tail-calls it.
    fn wrapper(&mut self, index: usize) -> Lowered<FuncId> {
        if let Some(id) = self.wrapper_ids.get(&index) {
            return Ok(*id);
        }
        let params = self.module_parameters(index)?;
        let target = self.module_function(index)?;
        let id = self.reserve()?;
        self.wrapper_ids.insert(index, id);
        let name = self
            .program
            .functions()
            .get(index)
            .map_or_else(String::new, |function| {
                format!("{} as a value", function.name())
            });
        let args = (0..params).map(Slot).collect();
        let function = Function {
            name,
            params,
            env: true,
            slots: params.saturating_add(1),
            blocks: vec![Block {
                insts: Vec::new(),
                term: Terminator::TailCall {
                    callee: Callee::Direct(target),
                    args,
                },
            }],
        };
        self.fill(id, function)?;
        Ok(id)
    }

    fn global(&mut self, index: usize) -> Lowered<GlobalId> {
        if let Some(id) = self.global_ids.get(&index) {
            return Ok(*id);
        }
        if self.program.globals().get(index).is_none() {
            return Err(LowerError::Malformed(
                "a read of a module value that does not exist",
            ));
        }
        let initializer = self.reserve()?;
        let id = GlobalId(
            u32::try_from(self.initializers.len())
                .map_err(|_| LowerError::Malformed("too many module values"))?,
        );
        self.initializers.push(initializer);
        self.global_ids.insert(index, id);
        self.queue.push_back(Job::Global(index, initializer));
        Ok(id)
    }

    fn constant(&mut self, constant: Constant) -> Lowered<ConstId> {
        let position = match self.constants.iter().position(|known| *known == constant)
        {
            Some(position) => position,
            None => {
                self.constants.push(constant);
                self.constants.len() - 1
            }
        };
        u32::try_from(position)
            .map(ConstId)
            .map_err(|_| LowerError::Malformed("too many constants"))
    }

    fn origin(&mut self, origin: &SourceOrigin) -> Lowered<OriginId> {
        if let Some(id) = self.origin_ids.get(origin) {
            return Ok(*id);
        }
        self.origins.push(origin.clone());
        let id = u32::try_from(self.origins.len())
            .map(OriginId)
            .map_err(|_| LowerError::Malformed("too many origins"))?;
        self.origin_ids.insert(origin.clone(), id);
        Ok(id)
    }

    fn drain(&mut self) -> Lowered<()> {
        let program = self.program;
        while let Some(job) = self.queue.pop_front() {
            match job {
                Job::Module(index, id) => {
                    let function = program.functions().get(index).ok_or(
                        LowerError::Malformed("a function that does not exist"),
                    )?;
                    let (body, slot_count) = function.module_body();
                    let params = Self::parameters(function.signature())?;
                    let lowered = Body::lower(
                        self,
                        function.name().to_owned(),
                        Shape {
                            params,
                            env: false,
                            ir_slots: slot_count,
                        },
                        body,
                    )?;
                    self.fill(id, lowered)?;
                }
                Job::Global(index, id) => {
                    let global = program.globals().get(index).ok_or(
                        LowerError::Malformed("a module value that does not exist"),
                    )?;
                    let lowered = Body::lower(
                        self,
                        format!("module value {}", global.name()),
                        Shape {
                            params: 0,
                            env: false,
                            ir_slots: global.slot_count(),
                        },
                        global.initializer(),
                    )?;
                    self.fill(id, lowered)?;
                }
            }
        }
        Ok(())
    }

    fn finish(self, entry: FuncId) -> Lowered<Program> {
        let functions = self
            .functions
            .into_iter()
            .map(|function| {
                function
                    .ok_or(LowerError::Malformed("a function that was never lowered"))
            })
            .collect::<Lowered<Vec<_>>>()?;
        Ok(Program {
            functions,
            entry,
            globals: self.initializers,
            constants: self.constants,
            origins: self.origins,
        })
    }
}

/// The frame a body is lowered for.
struct Shape {
    params: u32,
    env: bool,
    /// The activation slots the checked body numbers.
    ir_slots: usize,
}

struct Cur {
    /// `None` for code that cannot run, which is lowered and discarded.
    id: Option<BlockId>,
    insts: Vec<Inst>,
}

struct Body<'a, 'p> {
    low: &'a mut Lowering<'p>,
    name: String,
    params: u32,
    env: bool,
    ir_slots: u32,
    base: u32,
    next_temp: u32,
    max_temp: u32,
    blocks: Vec<Block>,
    cur: Option<Cur>,
}

impl<'a, 'p> Body<'a, 'p> {
    /// Lowers `body` as the whole of one function.
    fn lower(
        low: &'a mut Lowering<'p>,
        name: String,
        shape: Shape,
        body: &Expr,
    ) -> Lowered<Function> {
        let ir_slots = u32::try_from(shape.ir_slots)
            .map_err(|_| LowerError::Malformed("too many activation slots"))?
            .max(shape.params);
        let base = ir_slots.saturating_add(u32::from(shape.env));
        let mut this = Body {
            low,
            name,
            params: shape.params,
            env: shape.env,
            ir_slots,
            base,
            next_temp: 0,
            max_temp: 0,
            blocks: Vec::new(),
            cur: None,
        };
        let entry = this.new_block();
        this.start(entry);
        let result = this.temp();
        this.expr(body, result)?;
        if this.cur.is_some() {
            this.end(Terminator::Return { value: result });
        }
        let slots = base.saturating_add(this.max_temp);
        if slots > MAX_SLOTS {
            return Err(LowerError::FrameTooLarge {
                function: this.name,
            });
        }
        Ok(Function {
            name: this.name,
            params: shape.params,
            env: shape.env,
            slots,
            blocks: this.blocks,
        })
    }

    // -- the builder ----------------------------------------------------------

    fn new_block(&mut self) -> BlockId {
        self.blocks.push(Block {
            insts: Vec::new(),
            term: Terminator::Jump(BlockId(0)),
        });
        BlockId(u32::try_from(self.blocks.len() - 1).unwrap_or(u32::MAX))
    }

    fn start(&mut self, id: BlockId) {
        self.cur = Some(Cur {
            id: Some(id),
            insts: Vec::new(),
        });
    }

    fn emit(&mut self, inst: Inst) {
        if let Some(cur) = &mut self.cur {
            cur.insts.push(inst);
        }
    }

    fn end(&mut self, term: Terminator) {
        let Some(cur) = self.cur.take() else {
            return;
        };
        if let Some(id) = cur.id
            && let Some(block) = self.blocks.get_mut(id.index())
        {
            *block = Block {
                insts: cur.insts,
                term,
            };
        }
    }

    /// Ends the current block, if there is one, at the join of an `if`,
    /// creating the join when the first arm reaches it.
    fn jump_to(&mut self, join: &mut Option<BlockId>) {
        if self.cur.is_none() {
            return;
        }
        let target = match *join {
            Some(target) => target,
            None => {
                let target = self.new_block();
                *join = Some(target);
                target
            }
        };
        self.end(Terminator::Jump(target));
    }

    fn temp(&mut self) -> Slot {
        let slot = Slot(self.base.saturating_add(self.next_temp));
        self.next_temp = self.next_temp.saturating_add(1);
        self.max_temp = self.max_temp.max(self.next_temp);
        slot
    }

    const fn release_to(&mut self, mark: u32) {
        self.next_temp = mark;
    }

    fn drop_slot(&mut self, slot: Slot) {
        self.emit(Inst::Drop { slot });
    }

    /// The execution slot of checked activation slot `slot`.
    fn ir(&self, slot: usize) -> Lowered<Slot> {
        let slot = u32::try_from(slot)
            .map_err(|_| LowerError::Malformed("a slot outside the activation"))?;
        if slot >= self.ir_slots {
            return Err(LowerError::Malformed("a slot outside the activation"));
        }
        // The environment sits after the parameters, so a binding moves up one.
        Ok(Slot(if slot < self.params {
            slot
        } else {
            slot + u32::from(self.env)
        }))
    }

    // -- expressions ----------------------------------------------------------

    /// Evaluates `expr` into `dest`, which is empty before and full after,
    /// unless the expression never completes.
    ///
    /// Where nothing can run, the expression is still lowered, so that a form
    /// outside the slice is found, and the result is discarded.
    fn expr(&mut self, expr: &Expr, dest: Slot) -> Lowered<()> {
        if self.cur.is_some() {
            return self.live(expr, dest);
        }
        let mark = self.blocks.len();
        self.cur = Some(Cur {
            id: None,
            insts: Vec::new(),
        });
        let result = self.live(expr, dest);
        self.cur = None;
        self.blocks.truncate(mark);
        result
    }

    fn live(&mut self, expr: &Expr, dest: Slot) -> Lowered<()> {
        match expr {
            Expr::Literal { value, .. } => self.literal(value, dest),
            Expr::Sequence { expressions, .. } => self.sequence(expressions, dest),
            Expr::Variable { slot, .. } => {
                let src = self.ir(*slot)?;
                self.emit(Inst::Copy { dst: dest, src });
                Ok(())
            }
            Expr::Captured { slot, .. } => {
                if !self.env {
                    return Err(LowerError::Malformed("a capture outside a closure"));
                }
                let index = u32::try_from(FIRST_CAPTURE.saturating_add(*slot))
                    .map_err(|_| LowerError::Malformed("too many captures"))?;
                self.emit(Inst::Cell {
                    dst: dest,
                    object: Slot(self.params),
                    index,
                });
                Ok(())
            }
            Expr::Global { index, .. } => {
                let global = self.low.global(*index)?;
                let resume = self.new_block();
                self.end(Terminator::Global {
                    global,
                    dst: dest,
                    resume,
                });
                self.start(resume);
                Ok(())
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
                let mark = self.next_temp;
                let held = self.temp();
                self.expr(value, held)?;
                self.end(Terminator::Return { value: held });
                self.release_to(mark);
                Ok(())
            }
            Expr::Call {
                target,
                arguments,
                tail,
                ..
            } => self.call(target, arguments, *tail, dest),
            Expr::Host {
                row,
                arguments,
                origin,
                ..
            } => self.host(row, arguments, origin, dest),
            Expr::Function { function, .. } => {
                let wrapper = self.low.wrapper(*function)?;
                self.emit(Inst::Function {
                    dst: dest,
                    function: wrapper,
                    captures: Vec::new(),
                });
                Ok(())
            }
            Expr::Closure {
                signature,
                parameters,
                captures,
                body,
                slot_count,
                ..
            } => self.closure(
                signature,
                parameters.len(),
                captures,
                body,
                *slot_count,
                dest,
            ),
            Expr::External { intrinsic, .. } => {
                Err(LowerError::NotLowered(if is_numeric(*intrinsic) {
                    form::EXTERNAL_NUMERIC
                } else {
                    form::EXTERNAL_COLLECTION
                }))
            }
            Expr::Default { .. } => Err(LowerError::NotLowered(form::DEFAULT)),
            Expr::Match { .. } => Err(LowerError::NotLowered(form::MATCH)),
            Expr::Widen { .. } => Err(LowerError::NotLowered(form::WIDEN)),
            Expr::Try { .. } => Err(LowerError::NotLowered(form::TRY)),
            Expr::Record { .. } => Err(LowerError::NotLowered(form::RECORD)),
            Expr::Variant { .. } => Err(LowerError::NotLowered(form::VARIANT)),
            Expr::Wrap { .. } => Err(LowerError::NotLowered(form::WRAP)),
            Expr::Project { .. } => Err(LowerError::NotLowered(form::PROJECT)),
            Expr::Tuple { .. } => Err(LowerError::NotLowered(form::TUPLE)),
            Expr::TupleProject { .. } => {
                Err(LowerError::NotLowered(form::TUPLE_PROJECT))
            }
            Expr::Array { .. } => Err(LowerError::NotLowered(form::ARRAY)),
            Expr::Dict { .. } => Err(LowerError::NotLowered(form::DICT)),
            Expr::Lookup { .. } => Err(LowerError::NotLowered(form::LOOKUP)),
        }
    }

    fn literal(&mut self, value: &Value, dest: Slot) -> Lowered<()> {
        let narrow = |value: i32| u64::from(value.cast_unsigned());
        let scalar = |class, bits| Inst::Scalar {
            dst: dest,
            class,
            bits,
        };
        let inst = match value {
            Value::Bool(value) => Inst::Build {
                dst: dest,
                kind: ObjectKind::Enum,
                variant: u32::from(*value),
                cells: Vec::new(),
            },
            Value::Void => scalar(ScalarClass::I32, 0),
            Value::Char(value) => {
                scalar(ScalarClass::I32, u64::from(u32::from(*value)))
            }
            Value::I8(value) => scalar(ScalarClass::I32, narrow(i32::from(*value))),
            Value::I16(value) => scalar(ScalarClass::I32, narrow(i32::from(*value))),
            Value::I32(value) => scalar(ScalarClass::I32, narrow(*value)),
            Value::U8(value) => scalar(ScalarClass::I32, u64::from(*value)),
            Value::U16(value) => scalar(ScalarClass::I32, u64::from(*value)),
            Value::U32(value) => scalar(ScalarClass::I32, u64::from(*value)),
            Value::I64(value) => scalar(ScalarClass::I64, value.cast_unsigned()),
            Value::U64(value) => scalar(ScalarClass::I64, *value),
            Value::F32(bits) => scalar(ScalarClass::F32, u64::from(*bits)),
            Value::F64(bits) => scalar(ScalarClass::F64, *bits),
            Value::Str(text) => self.constant(Constant::Str(text.clone()), dest)?,
            Value::Bytes(bytes) => {
                self.constant(Constant::Bytes(bytes.clone()), dest)?
            }
            Value::Atom(name) => self.constant(Constant::Atom(name.clone()), dest)?,
        };
        self.emit(inst);
        Ok(())
    }

    fn constant(&mut self, constant: Constant, dest: Slot) -> Lowered<Inst> {
        let constant = self.low.constant(constant)?;
        Ok(Inst::Constant {
            dst: dest,
            constant,
        })
    }

    /// Every element but the last is evaluated for its effects and dropped,
    /// and the last is the value. An empty sequence is `void`.
    fn sequence(&mut self, expressions: &[Expr], dest: Slot) -> Lowered<()> {
        let Some((last, leading)) = expressions.split_last() else {
            return self.literal(&Value::Void, dest);
        };
        for expression in leading {
            let mark = self.next_temp;
            let held = self.temp();
            self.expr(expression, held)?;
            self.drop_slot(held);
            self.release_to(mark);
        }
        self.expr(last, dest)
    }

    /// A `let`: the value is evaluated into the binding's slot, which the body
    /// reads, and the binding is dropped when the body has finished. A
    /// discarded value is dropped at once.
    fn binding(
        &mut self,
        slot: Option<usize>,
        value: &Expr,
        body: &Expr,
        dest: Slot,
    ) -> Lowered<()> {
        match slot {
            Some(slot) => {
                let slot = self.ir(slot)?;
                self.expr(value, slot)?;
                self.expr(body, dest)?;
                self.drop_slot(slot);
            }
            None => {
                let mark = self.next_temp;
                let held = self.temp();
                self.expr(value, held)?;
                self.drop_slot(held);
                self.release_to(mark);
                self.expr(body, dest)?;
            }
        }
        Ok(())
    }

    /// An `if`: the condition, a `bool`, is read for its variant and dropped
    /// before the branch, and only the selected arm is evaluated.
    fn branch(
        &mut self,
        condition: &Expr,
        then_branch: &Expr,
        else_branch: &Expr,
        dest: Slot,
    ) -> Lowered<()> {
        let mark = self.next_temp;
        let held = self.temp();
        self.expr(condition, held)?;
        let mut arms = None;
        if self.cur.is_some() {
            let test = self.temp();
            self.emit(Inst::Variant {
                dst: test,
                object: held,
            });
            self.drop_slot(held);
            let (then_block, else_block) = (self.new_block(), self.new_block());
            self.end(Terminator::Branch {
                cond: test,
                then_block,
                else_block,
            });
            arms = Some((test, then_block, else_block));
        }
        let mut join = None;
        for (arm, taken) in [(then_branch, 0), (else_branch, 1)] {
            if let Some((test, then_block, else_block)) = arms {
                self.start(if taken == 0 { then_block } else { else_block });
                self.drop_slot(test);
            }
            self.expr(arm, dest)?;
            self.jump_to(&mut join);
        }
        if let Some(join) = join {
            self.start(join);
        }
        self.release_to(mark);
        Ok(())
    }

    /// Evaluates `arguments` left to right into fresh temporaries.
    fn operands(&mut self, arguments: &[Expr]) -> Lowered<Vec<Slot>> {
        let mut slots = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let held = self.temp();
            self.expr(argument, held)?;
            slots.push(held);
        }
        Ok(slots)
    }

    /// A call. The callee is evaluated once before any operand. A call the
    /// checker marked as a tail transfer is a `TailCall`, whatever the callee
    /// is; any other is a `Call`.
    fn call(
        &mut self,
        target: &CallTarget,
        arguments: &[Expr],
        tail: bool,
        dest: Slot,
    ) -> Lowered<()> {
        let mark = self.next_temp;
        let (callee, held) = match target {
            CallTarget::Direct(index) => {
                let params = self.low.module_parameters(*index)?;
                if usize::try_from(params).ok() != Some(arguments.len()) {
                    return Err(LowerError::Malformed(
                        "a call with the wrong operand count",
                    ));
                }
                (Callee::Direct(self.low.module_function(*index)?), None)
            }
            CallTarget::Indirect { callee, .. } => {
                let held = self.temp();
                self.expr(callee, held)?;
                (Callee::Indirect(held), Some(held))
            }
            CallTarget::Contract { .. } => {
                return Err(LowerError::NotLowered(form::CONTRACT_CALL));
            }
        };
        let args = self.operands(arguments)?;
        if self.cur.is_none() {
            self.release_to(mark);
            return Ok(());
        }
        if tail {
            self.end(Terminator::TailCall { callee, args });
        } else {
            let resume = self.new_block();
            self.end(Terminator::Call {
                callee,
                args,
                dst: dest,
                resume,
            });
            self.start(resume);
            if let Some(held) = held {
                self.drop_slot(held);
            }
        }
        self.release_to(mark);
        Ok(())
    }

    /// A host operation. Its operands are borrowed, so they are dropped when
    /// the block that continues begins.
    fn host(
        &mut self,
        row: &'static vibra_ir::external::host::HostRow,
        arguments: &[Expr],
        origin: &SourceOrigin,
        dest: Slot,
    ) -> Lowered<()> {
        if arguments.len() != row.parameters.len() {
            return Err(LowerError::Malformed(
                "a host call with the wrong operand count",
            ));
        }
        let mark = self.next_temp;
        let args = self.operands(arguments)?;
        if self.cur.is_some() {
            let origin = self.low.origin(origin)?;
            let resume = self.new_block();
            self.end(Terminator::Host {
                row,
                args: args.clone(),
                dst: dest,
                resume,
                origin,
            });
            self.start(resume);
            for held in args {
                self.drop_slot(held);
            }
        }
        self.release_to(mark);
        Ok(())
    }

    /// A closure: its body is a function with an environment, and the value
    /// consumes the captures, evaluated left to right.
    fn closure(
        &mut self,
        signature: &FunctionSignature,
        parameters: usize,
        captures: &[Expr],
        body: &Expr,
        slot_count: usize,
        dest: Slot,
    ) -> Lowered<()> {
        let params = Lowering::parameters(signature)?;
        if usize::try_from(params).ok() != Some(parameters) {
            return Err(LowerError::Malformed(
                "a closure that disagrees with its signature",
            ));
        }
        let id = self.low.reserve()?;
        let name = format!("{} closure", self.name);
        let lowered = Body::lower(
            &mut *self.low,
            name,
            Shape {
                params,
                env: true,
                ir_slots: slot_count,
            },
            body,
        )?;
        self.low.fill(id, lowered)?;
        let mark = self.next_temp;
        let held = self.operands(captures)?;
        self.emit(Inst::Function {
            dst: dest,
            function: id,
            captures: held,
        });
        self.release_to(mark);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::form;

    /// The lowering names a refused form only through `form`, so the list of
    /// names cannot drift from the places that return one.
    #[test]
    fn every_not_lowered_site_names_its_form_through_the_constants() {
        let source = include_str!("lib.rs");
        let code = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(!code.contains("NotLowered(\""), "a literal form name");
        let mut seen = std::collections::BTreeSet::new();
        for name in form::ALL {
            assert!(seen.insert(*name), "{name} is listed twice");
        }
        assert_eq!(form::ALL.len(), 20);
    }
}
