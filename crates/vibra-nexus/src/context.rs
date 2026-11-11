//! One execution context: the runtime frames and the transfers between them.
//!
//! Every way control moves between activations is a method here, written
//! once: [`Context::call`], [`Context::tail_call`], [`Context::ret`],
//! [`Context::global`], and [`Context::host`]. Whatever executes the program's
//! blocks (the reference stepper, or generated code through the bridge) only
//! decides *which* transfer happens next. That is what keeps the frame
//! discipline, the exactly-once rule of a host call, and the memory accounting
//! identical however the code runs.
//!
//! A frame is `[parameters][environment][everything else]` in the context's
//! cell stack. Its `resume` field names the block that continues it, so a
//! frame is all the state a suspended activation has.

use vibra_host::{AuditEvent, HostCall, HostProvider, build_reply, perform};
use vibra_ir::arena::{STRIDE_CELLS, kind};
use vibra_ir::boundary::TrapCode;
use vibra_ir::external::host::HostRow;

use crate::host::Values;
use crate::store::{Cell, Store, StoreError};

/// The bytes one frame and one of its slots count against the memory limit.
const FRAME_BYTES: u64 = 32;
const SLOT_BYTES: u64 = 16;

/// The cells of a function value before its captures: the function, its frame
/// size, and its parameter count.
const FUNCTION_HEADER: u32 = 3;

/// A recorded trap: its code and the ordinal of its origin, `0` for none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trap {
    /// The registered trap code.
    pub code: TrapCode,
    /// The origin ordinal in the program's origin table, `0` for no origin.
    pub origin: u32,
}

/// Why a transfer did not complete. The detail is recorded in the context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Halt {
    /// A trap was recorded.
    Trap,
    /// The memory limit was reached: the host event.
    Exhausted,
}

/// Counters a test or a baseline reads. They have no effect on a run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// The greatest number of frames held at once.
    pub max_depth: usize,
    /// Tail transfers performed.
    pub tail_transfers: u64,
    /// Times the dispatcher yielded.
    pub yields: u64,
}

#[derive(Clone, Copy, Debug)]
struct Frame {
    function: u32,
    resume: u32,
    /// The index of the frame's first cell in the cell stack.
    base: usize,
    slots: u32,
    /// The slot of the caller's frame that receives the result.
    dst: u32,
    /// The module value this frame initializes, if it runs an initializer.
    global: Option<u32>,
}

#[derive(Clone, Copy, Debug)]
enum Global {
    Unread,
    Evaluating,
    Ready(Cell),
}

/// One isolated execution of a program.
pub struct Context<'h> {
    store: Store,
    frames: Vec<Frame>,
    cells: Vec<Cell>,
    globals: Vec<Global>,
    result: Cell,
    trap: Option<Trap>,
    host: &'h mut dyn HostProvider,
    audit: Vec<AuditEvent>,
    /// Quota charged by the step in progress, beyond the checkpoint itself.
    charge: u64,
    stats: Stats,
}

impl std::fmt::Debug for Context<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Context")
            .field("depth", &self.frames.len())
            .field("live_size", &self.store.live_size())
            .field("trap", &self.trap)
            .finish_non_exhaustive()
    }
}

type Done<T> = Result<T, Halt>;

impl<'h> Context<'h> {
    /// A fresh context that admits `limit` bytes of values and frames, has
    /// `globals` module values, none of them read, and performs its host
    /// operations on `host`.
    pub fn new(limit: u64, globals: usize, host: &'h mut dyn HostProvider) -> Self {
        Self {
            store: Store::new(limit),
            frames: Vec::new(),
            cells: Vec::new(),
            globals: vec![Global::Unread; globals],
            result: Cell::EMPTY,
            trap: None,
            host,
            audit: Vec::new(),
            charge: 0,
            stats: Stats::default(),
        }
    }

    /// The value store, for the instructions that build and read values.
    pub const fn store(&mut self) -> &mut Store {
        &mut self.store
    }

    /// The bytes of values and frames now live.
    #[must_use]
    pub const fn live_size(&self) -> u64 {
        self.store.live_size()
    }

    /// The most bytes of values and frames that have been live at once.
    #[must_use]
    pub const fn peak_size(&self) -> u64 {
        self.store.peak_size()
    }

    /// The number of frames now held.
    #[must_use]
    pub const fn depth(&self) -> usize {
        self.frames.len()
    }

    /// The events of the host calls that completed, in call order.
    #[must_use]
    pub fn audit(&self) -> &[AuditEvent] {
        &self.audit
    }

    /// The entry's result, once a step answered [`crate::Step::Done`]. The
    /// caller owns the reference it holds.
    #[must_use]
    pub const fn result(&self) -> Cell {
        self.result
    }

    /// The trap that stopped the program, if one did.
    #[must_use]
    pub const fn trap_record(&self) -> Option<Trap> {
        self.trap
    }

    /// The counters of this run.
    #[must_use]
    pub const fn stats(&self) -> Stats {
        self.stats
    }

    /// Charges quota for proportional work inside the step in progress, so a
    /// long primitive is paid for at the next checkpoint.
    pub const fn add_charge(&mut self, units: u64) {
        self.charge = self.charge.saturating_add(units);
    }

    pub(crate) const fn take_charge(&mut self) -> u64 {
        std::mem::replace(&mut self.charge, 0)
    }

    pub(crate) const fn note_yield(&mut self) {
        self.stats.yields = self.stats.yields.saturating_add(1);
    }

    /// Records a trap and answers the halt that reports it.
    pub const fn trap(&mut self, code: TrapCode, origin: u32) -> Halt {
        if self.trap.is_none() {
            self.trap = Some(Trap { code, origin });
        }
        Halt::Trap
    }

    /// Turns a store failure into the halt it is: the host event, or the trap
    /// of a toolchain defect.
    ///
    /// # Errors
    ///
    /// The halt, when `value` is an error.
    pub fn check<T>(&mut self, value: Result<T, StoreError>) -> Done<T> {
        value.map_err(|error| match error {
            StoreError::Exhausted => Halt::Exhausted,
            StoreError::Fault => self.trap(TrapCode::InvalidCheckedProgram, 0),
        })
    }

    fn defect(&mut self) -> Halt {
        self.trap(TrapCode::InvalidCheckedProgram, 0)
    }

    fn top(&mut self) -> Done<Frame> {
        match self.frames.last() {
            Some(frame) => Ok(*frame),
            None => Err(self.defect()),
        }
    }

    fn cell_index(&mut self, slot: u32) -> Done<usize> {
        let frame = self.top()?;
        if slot >= frame.slots {
            return Err(self.defect());
        }
        Ok(frame.base + usize::try_from(slot).unwrap_or(usize::MAX))
    }

    /// The function of the top frame.
    ///
    /// # Errors
    ///
    /// A trap when the context has no frame.
    pub fn function(&mut self) -> Done<u32> {
        Ok(self.top()?.function)
    }

    /// The block that continues the top frame.
    ///
    /// # Errors
    ///
    /// A trap when the context has no frame.
    pub fn resume(&mut self) -> Done<u32> {
        Ok(self.top()?.resume)
    }

    fn set_resume(&mut self, block: u32) -> Done<()> {
        match self.frames.last_mut() {
            Some(frame) => {
                frame.resume = block;
                Ok(())
            }
            None => Err(self.defect()),
        }
    }

    /// The cell in a slot of the top frame, borrowed.
    ///
    /// # Errors
    ///
    /// A trap for a slot outside the frame.
    pub fn slot(&mut self, slot: u32) -> Done<Cell> {
        let index = self.cell_index(slot)?;
        match self.cells.get(index) {
            Some(cell) => Ok(*cell),
            None => Err(self.defect()),
        }
    }

    /// Writes a slot of the top frame, which takes over the cell's reference.
    /// Verified code writes only an empty slot.
    ///
    /// # Errors
    ///
    /// A trap for a slot outside the frame.
    pub fn set_slot(&mut self, slot: u32, cell: Cell) -> Done<()> {
        let index = self.cell_index(slot)?;
        match self.cells.get_mut(index) {
            Some(target) => {
                *target = cell;
                Ok(())
            }
            None => Err(self.defect()),
        }
    }

    /// Moves the cell out of a slot of the top frame, leaving it empty.
    ///
    /// # Errors
    ///
    /// A trap for a slot outside the frame.
    pub fn take_slot(&mut self, slot: u32) -> Done<Cell> {
        let cell = self.slot(slot)?;
        self.set_slot(slot, Cell::EMPTY)?;
        Ok(cell)
    }

    fn take_slots(&mut self, slots: &[u32]) -> Done<Vec<Cell>> {
        slots.iter().map(|slot| self.take_slot(*slot)).collect()
    }

    fn push_frame(
        &mut self,
        function: u32,
        slots: u32,
        dst: u32,
        global: Option<u32>,
    ) -> Done<()> {
        let bytes = FRAME_BYTES + u64::from(slots) * SLOT_BYTES;
        let charged = self.store.charge(bytes);
        self.check(charged)?;
        let count = usize::try_from(slots).unwrap_or(usize::MAX);
        if self.cells.try_reserve(count).is_err() || self.frames.try_reserve(1).is_err()
        {
            self.store.uncharge(bytes);
            return Err(Halt::Exhausted);
        }
        let base = self.cells.len();
        self.cells.resize(base + count, Cell::EMPTY);
        self.frames.push(Frame {
            function,
            resume: 0,
            base,
            slots,
            dst,
            global,
        });
        self.stats.max_depth = self.stats.max_depth.max(self.frames.len());
        Ok(())
    }

    /// Releases what the top frame still owns and removes it.
    fn pop_frame(&mut self) -> Done<Frame> {
        let frame = self.top()?;
        self.release_from(frame.base)?;
        self.frames.pop();
        self.store
            .uncharge(FRAME_BYTES + u64::from(frame.slots) * SLOT_BYTES);
        Ok(frame)
    }

    /// Drops every cell of the cell stack from `base` up, and the cells.
    fn release_from(&mut self, base: usize) -> Done<()> {
        while self.cells.len() > base {
            if let Some(cell) = self.cells.pop() {
                let dropped = self.store.drop_cell(cell);
                self.check(dropped)?;
            }
        }
        Ok(())
    }

    fn fill(&mut self, operands: Vec<Cell>, env: Option<Cell>) -> Done<()> {
        for (slot, cell) in (0_u32..).zip(operands.into_iter().chain(env)) {
            self.set_slot(slot, cell)?;
        }
        Ok(())
    }

    /// Pushes the entry frame of a program.
    ///
    /// # Errors
    ///
    /// The host event when the limit does not admit the frame.
    pub fn enter(&mut self, function: u32, slots: u32) -> Done<()> {
        self.push_frame(function, slots, 0, None)
    }

    /// A non-tail call of a known function: the consumed `args` become the
    /// parameters of a new frame, and the caller continues at `resume` with
    /// the result in `dst`.
    ///
    /// # Errors
    ///
    /// The host event when the limit does not admit the frame, and a trap for
    /// operands the frames do not hold.
    pub fn call(
        &mut self,
        function: u32,
        slots: u32,
        args: &[u32],
        dst: u32,
        resume: u32,
    ) -> Done<()> {
        let operands = self.take_slots(args)?;
        self.push_call(function, slots, operands, None, dst, resume)
    }

    /// A non-tail call through the function value in the borrowed slot
    /// `callee`.
    ///
    /// # Errors
    ///
    /// As [`Context::call`], and the trap `@runtime.invalid-checked-program`
    /// for a callee that is no function value or takes another number of
    /// operands.
    pub fn call_value(
        &mut self,
        callee: u32,
        args: &[u32],
        dst: u32,
        resume: u32,
    ) -> Done<()> {
        let (function, slots, env) = self.callee(callee, args.len())?;
        let operands = self.take_slots(args)?;
        self.push_call(function, slots, operands, Some(env), dst, resume)
    }

    fn push_call(
        &mut self,
        function: u32,
        slots: u32,
        operands: Vec<Cell>,
        env: Option<Cell>,
        dst: u32,
        resume: u32,
    ) -> Done<()> {
        self.set_resume(resume)?;
        self.push_frame(function, slots, dst, None)?;
        self.fill(operands, env)
    }

    /// A tail transfer to a known function: the callee replaces the top
    /// frame, which releases everything but the consumed `args`.
    ///
    /// # Errors
    ///
    /// As [`Context::call`].
    pub fn tail_call(&mut self, function: u32, slots: u32, args: &[u32]) -> Done<()> {
        let operands = self.take_slots(args)?;
        self.replace_frame(function, slots, operands, None)
    }

    /// A tail transfer through the function value in the borrowed slot
    /// `callee`.
    ///
    /// # Errors
    ///
    /// As [`Context::call_value`].
    pub fn tail_call_value(&mut self, callee: u32, args: &[u32]) -> Done<()> {
        let (function, slots, env) = self.callee(callee, args.len())?;
        let operands = self.take_slots(args)?;
        self.replace_frame(function, slots, operands, Some(env))
    }

    fn replace_frame(
        &mut self,
        function: u32,
        slots: u32,
        operands: Vec<Cell>,
        env: Option<Cell>,
    ) -> Done<()> {
        let old = self.pop_frame()?;
        self.push_frame(function, slots, old.dst, old.global)?;
        self.stats.tail_transfers = self.stats.tail_transfers.saturating_add(1);
        self.fill(operands, env)
    }

    /// Reads the function value in a slot: the function, its frame size, and
    /// one more reference to the value, which becomes the callee's
    /// environment.
    fn callee(&mut self, slot: u32, operands: usize) -> Done<(u32, u32, Cell)> {
        let cell = self.slot(slot)?;
        let read = (|| {
            let value = cell.as_ref().ok_or(StoreError::Fault)?;
            if self.store.kind(value)? != kind::FUNCTION {
                return Err(StoreError::Fault);
            }
            let field = |index| {
                let bits = self.store.cell(value, index)?.bits;
                u32::try_from(bits).map_err(|_| StoreError::Fault)
            };
            let header = (field(0)?, field(1)?, field(2)?);
            self.store.dup(value)?;
            Ok(header)
        })();
        let (function, slots, params) = self.check(read)?;
        if usize::try_from(params).ok() != Some(operands) {
            let dropped = self.store.drop_cell(cell);
            self.check(dropped)?;
            return Err(self.defect());
        }
        Ok((function, slots, cell))
    }

    /// Builds a function value over `function`, whose frame has `slots` slots
    /// and which takes `params` operands, holding the consumed `captures`.
    ///
    /// # Errors
    ///
    /// The host event when the limit does not admit the value.
    pub fn function_value(
        &mut self,
        function: u32,
        slots: u32,
        params: u32,
        captures: &[u32],
    ) -> Done<Cell> {
        let cells = self.take_slots(captures)?;
        let built = (|| {
            let len = u32::try_from(cells.len())
                .ok()
                .and_then(|len| len.checked_add(FUNCTION_HEADER))
                .ok_or(StoreError::Exhausted)?;
            let value = self.store.alloc(kind::FUNCTION, STRIDE_CELLS, len, 0)?;
            let header =
                [function, slots, params].map(|field| Cell::i64(u64::from(field)));
            for (index, cell) in (0..len).zip(header.into_iter().chain(cells)) {
                self.store.set_cell(value, index, cell)?;
            }
            Ok(Cell::reference(value))
        })();
        self.check(built)
    }

    /// Leaves the top frame with the consumed value of `slot`, and answers
    /// whether that frame was the entry's, so the program is done.
    ///
    /// # Errors
    ///
    /// A trap for a slot the frame does not hold.
    pub fn ret(&mut self, slot: u32) -> Done<bool> {
        let value = self.take_slot(slot)?;
        let frame = self.pop_frame()?;
        if let Some(global) = frame.global {
            // The module state keeps one reference and the reader the other.
            let kept = self.store.dup_cell(value);
            self.check(kept)?;
            let index = usize::try_from(global).unwrap_or(usize::MAX);
            match self.globals.get_mut(index) {
                Some(state) => *state = Global::Ready(value),
                None => return Err(self.defect()),
            }
        }
        if self.frames.is_empty() {
            self.result = value;
            return Ok(true);
        }
        self.set_slot(frame.dst, value)?;
        Ok(false)
    }

    /// Reads a module value into `dst`. The first read of an execution pushes
    /// the initializer `function` instead, and the answer is `true`: the
    /// caller continues at `resume` once it returns. A later read is no
    /// transfer, and the answer is `false`.
    ///
    /// # Errors
    ///
    /// The host event when the limit does not admit the frame, and the trap
    /// `@runtime.invalid-checked-program` for a module value that is read
    /// while it is being initialized, which the checker rules out.
    pub fn global(
        &mut self,
        global: u32,
        function: u32,
        slots: u32,
        dst: u32,
        resume: u32,
    ) -> Done<bool> {
        let index = usize::try_from(global).unwrap_or(usize::MAX);
        match self.globals.get(index).copied() {
            Some(Global::Ready(cell)) => {
                let kept = self.store.dup_cell(cell);
                self.check(kept)?;
                self.set_slot(dst, cell)?;
                Ok(false)
            }
            Some(Global::Unread) => {
                if let Some(state) = self.globals.get_mut(index) {
                    *state = Global::Evaluating;
                }
                self.set_resume(resume)?;
                self.push_frame(function, slots, dst, Some(global))?;
                Ok(true)
            }
            Some(Global::Evaluating) | None => Err(self.defect()),
        }
    }

    /// Performs one host operation on the context's provider, exactly once:
    /// reads the borrowed `args`, runs the operation, writes its
    /// `(result t e)` value to `dst`, appends its audit event, and sets the
    /// frame to continue at `resume`.
    ///
    /// A call that ends in a trap or in the host event appends no event, as
    /// "The audit trace" requires.
    ///
    /// # Errors
    ///
    /// The host event when the provider or the store cannot hold the answer,
    /// and the trap `@runtime.invalid-checked-program` at `origin` for
    /// operands that are not what the row takes.
    pub fn host(
        &mut self,
        row: &'static HostRow,
        args: &[u32],
        dst: u32,
        resume: u32,
        origin: u32,
    ) -> Done<()> {
        let operands = args
            .iter()
            .map(|slot| self.slot(*slot))
            .collect::<Done<Vec<_>>>()?;
        let mut values = Values::new(&mut self.store);
        let Ok(call) = HostCall::read(row.symbol, &operands, &mut values) else {
            return Err(self.trap(TrapCode::InvalidCheckedProgram, origin));
        };
        let Ok(reply) = perform(&call, self.host) else {
            return Err(Halt::Exhausted);
        };
        let answer = build_reply(&reply, &mut values);
        if let Some(error) = values.failure() {
            return self.check(Err(error));
        }
        self.set_slot(dst, answer)?;
        self.set_resume(resume)?;
        self.audit.push(AuditEvent::completed(&call, &reply));
        Ok(())
    }
}
