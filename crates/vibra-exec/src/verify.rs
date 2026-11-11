//! The verifier: what must hold of a [`Program`] before anything executes it.
//!
//! The rules are numbered so a failing program, a test, and the design
//! (`docs/roadmap/nexus/design.md`, "Verification rules") name the same thing:
//!
//! - **V1** the program has a function, and the entry and every module-value
//!   initializer name a function with no parameter and no environment;
//! - **V2** every function has a block, every block is reachable from block
//!   `0`, and the frame holds the function's entry slots;
//! - **V3** every block, slot, function, constant, module value, and origin an
//!   instruction or terminator names exists;
//! - **V4** a direct call names a function without an environment and passes
//!   exactly its parameters; a function value names a function with one; a
//!   scalar operation and a host call pass exactly their operands;
//! - **V5** no operand list that is consumed names a slot twice; and
//! - **V6** on every path, a slot is full where it is read and empty where it
//!   is written.
//!
//! V6 is a forward data-flow analysis over three states. Where two paths
//! disagree about a slot, the slot is *unknown* at the join: it may be neither
//! read nor written until an [`Inst::Drop`] empties it.

use std::fmt;

use crate::{
    Block, BlockId, Callee, FuncId, Function, Inst, OriginId, Program, Slot, Terminator,
};

/// Why a program was rejected, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyError {
    /// The function the violation is in, when it is in one.
    pub function: Option<FuncId>,
    /// The block the violation is in, when it is in one.
    pub block: Option<BlockId>,
    /// The violation.
    pub kind: VerifyErrorKind,
}

/// One violation of the verification rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerifyErrorKind {
    /// V1: the program has no function.
    NoFunctions,
    /// V1: the entry or an initializer names no function.
    MissingRoot(FuncId),
    /// V1: the entry or an initializer takes a parameter or an environment.
    RootTakesOperands(FuncId),
    /// V2: the function has no block.
    NoBlocks,
    /// V2: the frame is smaller than the parameters and the environment.
    FrameTooSmall,
    /// V2: no path from the entry block reaches the block.
    UnreachableBlock,
    /// V3: a block that does not exist.
    UnknownBlock(BlockId),
    /// V3: a slot outside the frame.
    UnknownSlot(Slot),
    /// V3: a function that does not exist.
    UnknownFunction(FuncId),
    /// V3: a constant that does not exist.
    UnknownConstant(u32),
    /// V3: a module value that does not exist.
    UnknownGlobal(u32),
    /// V3: an origin ordinal past the origin table.
    UnknownOrigin(OriginId),
    /// V4: an operand count that differs from what the callee takes.
    Arity {
        /// The count the callee, operation, or row takes.
        expected: usize,
        /// The count passed.
        found: usize,
    },
    /// V4: a direct call of a function that takes an environment.
    DirectCallNeedsEnvironment(FuncId),
    /// V4: a function value over a function that takes no environment.
    FunctionValueWithoutEnvironment(FuncId),
    /// V5: a consumed operand list names a slot twice.
    ConsumedTwice(Slot),
    /// V6: a read of a slot that is not full on every path.
    ReadOfEmpty(Slot),
    /// V6: a write of a slot that is not empty on every path.
    WriteOfFull(Slot),
}

impl fmt::Display for VerifyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid execution IR")?;
        if let Some(function) = self.function {
            write!(formatter, " in function {}", function.0)?;
        }
        if let Some(block) = self.block {
            write!(formatter, ", block {}", block.0)?;
        }
        write!(formatter, ": {:?}", self.kind)
    }
}

impl std::error::Error for VerifyError {}

/// The state of one slot at one program point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Empty,
    Full,
    Unknown,
}

/// Verifies `program` against V1 to V6.
///
/// # Errors
///
/// The first violation found, in function, block, and instruction order.
pub fn verify(program: &Program) -> Result<(), VerifyError> {
    let root = |kind| VerifyError {
        function: None,
        block: None,
        kind,
    };
    if program.functions.is_empty() {
        return Err(root(VerifyErrorKind::NoFunctions));
    }
    for id in std::iter::once(program.entry).chain(program.globals.iter().copied()) {
        let function = program
            .function(id)
            .ok_or_else(|| root(VerifyErrorKind::MissingRoot(id)))?;
        if function.entry_slots() != 0 {
            return Err(root(VerifyErrorKind::RootTakesOperands(id)));
        }
    }
    for (index, function) in program.functions.iter().enumerate() {
        let id = FuncId(u32::try_from(index).unwrap_or(u32::MAX));
        Checker {
            program,
            function,
            id,
        }
        .check()?;
    }
    Ok(())
}

struct Checker<'p> {
    program: &'p Program,
    function: &'p Function,
    id: FuncId,
}

type Check = Result<(), VerifyErrorKind>;

impl Checker<'_> {
    fn check(&self) -> Result<(), VerifyError> {
        let at = |block: Option<BlockId>| {
            move |kind| VerifyError {
                function: Some(self.id),
                block,
                kind,
            }
        };
        if self.function.blocks.is_empty() {
            return Err(at(None)(VerifyErrorKind::NoBlocks));
        }
        if self.function.slots < self.function.entry_slots() {
            return Err(at(None)(VerifyErrorKind::FrameTooSmall));
        }
        let slots = usize::try_from(self.function.slots).unwrap_or(usize::MAX);
        let entry_slots =
            usize::try_from(self.function.entry_slots()).unwrap_or(usize::MAX);
        let mut entry = vec![State::Empty; slots];
        for state in entry.iter_mut().take(entry_slots) {
            *state = State::Full;
        }
        // The state on entry to each block, once some path has reached it.
        let mut reached: Vec<Option<Vec<State>>> =
            vec![None; self.function.blocks.len()];
        if let Some(first) = reached.first_mut() {
            *first = Some(entry);
        }
        let mut work = vec![BlockId(0)];
        while let Some(id) = work.pop() {
            let Some(block) = self.function.blocks.get(id.index()) else {
                return Err(at(Some(id))(VerifyErrorKind::UnknownBlock(id)));
            };
            let Some(Some(mut state)) = reached.get(id.index()).cloned() else {
                continue;
            };
            let successors = self.block(block, &mut state).map_err(at(Some(id)))?;
            for (target, state) in successors {
                let Some(known) = reached.get_mut(target.index()) else {
                    return Err(at(Some(id))(VerifyErrorKind::UnknownBlock(target)));
                };
                match known {
                    None => {
                        *known = Some(state);
                        work.push(target);
                    }
                    Some(known) => {
                        let mut changed = false;
                        for (old, new) in known.iter_mut().zip(&state) {
                            if *old != *new && *old != State::Unknown {
                                *old = State::Unknown;
                                changed = true;
                            }
                        }
                        if changed {
                            work.push(target);
                        }
                    }
                }
            }
        }
        // A block no path reaches has no state to check it in, and a code
        // generator would still emit it, so it is rejected rather than skipped.
        for (index, state) in reached.iter().enumerate() {
            if state.is_none() {
                let id = BlockId(u32::try_from(index).unwrap_or(u32::MAX));
                return Err(at(Some(id))(VerifyErrorKind::UnreachableBlock));
            }
        }
        Ok(())
    }

    /// Checks one block from `state` and answers the state it hands to each
    /// successor.
    fn block(
        &self,
        block: &Block,
        state: &mut [State],
    ) -> Result<Vec<(BlockId, Vec<State>)>, VerifyErrorKind> {
        for inst in &block.insts {
            self.inst(inst, state)?;
        }
        let mut out = Vec::new();
        match &block.term {
            Terminator::Jump(target) => out.push((*target, state.to_vec())),
            Terminator::Branch {
                cond,
                then_block,
                else_block,
            } => {
                read(state, *cond)?;
                out.push((*then_block, state.to_vec()));
                out.push((*else_block, state.to_vec()));
            }
            Terminator::Switch {
                value,
                targets,
                default,
            } => {
                read(state, *value)?;
                for target in targets.iter().chain(std::iter::once(default)) {
                    out.push((*target, state.to_vec()));
                }
            }
            Terminator::Call {
                callee,
                args,
                dst,
                resume,
            } => {
                self.callee(state, *callee, args)?;
                write(state, *dst)?;
                out.push((*resume, state.to_vec()));
            }
            Terminator::TailCall { callee, args } => {
                self.callee(state, *callee, args)?;
            }
            Terminator::Host {
                row,
                args,
                dst,
                resume,
                origin,
            } => {
                self.origin(*origin)?;
                if args.len() != row.parameters.len() {
                    return Err(VerifyErrorKind::Arity {
                        expected: row.parameters.len(),
                        found: args.len(),
                    });
                }
                for arg in args {
                    read(state, *arg)?;
                }
                write(state, *dst)?;
                out.push((*resume, state.to_vec()));
            }
            Terminator::Global {
                global,
                dst,
                resume,
            } => {
                if self.program.globals.get(global.index()).is_none() {
                    return Err(VerifyErrorKind::UnknownGlobal(global.0));
                }
                write(state, *dst)?;
                out.push((*resume, state.to_vec()));
            }
            Terminator::Return { value } => consume(state, &[*value])?,
            Terminator::Trap { origin, .. } => self.origin(*origin)?,
        }
        for (target, _) in &out {
            if self.function.blocks.get(target.index()).is_none() {
                return Err(VerifyErrorKind::UnknownBlock(*target));
            }
        }
        Ok(out)
    }

    fn inst(&self, inst: &Inst, state: &mut [State]) -> Check {
        match inst {
            Inst::Scalar { dst, .. } => write(state, *dst),
            Inst::Constant { dst, constant } => {
                if self.program.constants.get(constant.index()).is_none() {
                    return Err(VerifyErrorKind::UnknownConstant(constant.0));
                }
                write(state, *dst)
            }
            Inst::Copy { dst, src } => {
                read(state, *src)?;
                write(state, *dst)
            }
            Inst::Move { dst, src } => {
                consume(state, &[*src])?;
                write(state, *dst)
            }
            Inst::Drop { slot } => {
                *slot_state(state, *slot)? = State::Empty;
                Ok(())
            }
            Inst::Op { dst, op, args } => {
                if args.len() != op.arity() {
                    return Err(VerifyErrorKind::Arity {
                        expected: op.arity(),
                        found: args.len(),
                    });
                }
                for arg in args {
                    read(state, *arg)?;
                }
                write(state, *dst)
            }
            Inst::Intrinsic {
                dst, args, origin, ..
            } => {
                self.origin(*origin)?;
                for arg in args {
                    read(state, *arg)?;
                }
                write(state, *dst)
            }
            Inst::Build { dst, cells, .. } => {
                consume(state, cells)?;
                write(state, *dst)
            }
            Inst::Cell { dst, object, .. } | Inst::Variant { dst, object } => {
                read(state, *object)?;
                write(state, *dst)
            }
            Inst::Function {
                dst,
                function,
                captures,
            } => {
                let body = self
                    .program
                    .function(*function)
                    .ok_or(VerifyErrorKind::UnknownFunction(*function))?;
                if !body.env {
                    return Err(VerifyErrorKind::FunctionValueWithoutEnvironment(
                        *function,
                    ));
                }
                consume(state, captures)?;
                write(state, *dst)
            }
        }
    }

    fn callee(&self, state: &mut [State], callee: Callee, args: &[Slot]) -> Check {
        match callee {
            Callee::Direct(id) => {
                let function = self
                    .program
                    .function(id)
                    .ok_or(VerifyErrorKind::UnknownFunction(id))?;
                if function.env {
                    return Err(VerifyErrorKind::DirectCallNeedsEnvironment(id));
                }
                let expected = usize::try_from(function.params).unwrap_or(usize::MAX);
                if args.len() != expected {
                    return Err(VerifyErrorKind::Arity {
                        expected,
                        found: args.len(),
                    });
                }
            }
            // The arity of a function value is known only when it is called,
            // and the runtime checks it there.
            Callee::Indirect(slot) => {
                if args.contains(&slot) {
                    return Err(VerifyErrorKind::ConsumedTwice(slot));
                }
                read(state, slot)?;
            }
        }
        consume(state, args)
    }

    fn origin(&self, origin: OriginId) -> Check {
        if origin == OriginId::NONE || self.program.origin(origin).is_some() {
            Ok(())
        } else {
            Err(VerifyErrorKind::UnknownOrigin(origin))
        }
    }
}

fn slot_state(state: &mut [State], slot: Slot) -> Result<&mut State, VerifyErrorKind> {
    state
        .get_mut(slot.index())
        .ok_or(VerifyErrorKind::UnknownSlot(slot))
}

fn read(state: &mut [State], slot: Slot) -> Check {
    match *slot_state(state, slot)? {
        State::Full => Ok(()),
        State::Empty | State::Unknown => Err(VerifyErrorKind::ReadOfEmpty(slot)),
    }
}

fn write(state: &mut [State], slot: Slot) -> Check {
    let state = slot_state(state, slot)?;
    match *state {
        State::Empty => {
            *state = State::Full;
            Ok(())
        }
        State::Full | State::Unknown => Err(VerifyErrorKind::WriteOfFull(slot)),
    }
}

fn consume(state: &mut [State], slots: &[Slot]) -> Check {
    for (position, slot) in slots.iter().enumerate() {
        if slots.iter().take(position).any(|earlier| earlier == slot) {
            return Err(VerifyErrorKind::ConsumedTwice(*slot));
        }
    }
    for slot in slots {
        read(state, *slot)?;
        *slot_state(state, *slot)? = State::Empty;
    }
    Ok(())
}
