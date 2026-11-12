//! Descriptors built in Rust, and the step functions of the foundation's proof
//! cases written by hand against the routines, the way generated code will.

use std::cell::Cell as Counter;

use vibra_host::{AuditEvent, HostWorld, WorldProvider};
use vibra_ir::boundary::TrapCode;
use vibra_nexus::{Cell, Context, Outcome, Stats, run};
use vibra_nexus_bridge as bridge;
use vibra_nexus_bridge::{
    CLASS_I64, CONSTANT_STR, Generated, KIND_ENUM, KIND_TUPLE, OP_I64_ADD, OP_I64_EQ,
    OP_I64_SUB, VibraConstant, VibraFunction, VibraOrigin, VibraProgram, VibraStep,
    VibraTable, admit,
};

use crate::sys::{self, Ctx, block, ok, status};

pub const MIB: u64 = 1024 * 1024;
pub const LIMIT: u64 = 64 * MIB;

thread_local! {
    /// How many rounds the next `main` step of the demo starts with.
    pub static ROUNDS: Counter<u64> = const { Counter::new(0) };
    /// Whether the demo ends in a trap.
    pub static TRAP: Counter<bool> = const { Counter::new(false) };
    /// The variant the entry of `entry_enum` returns.
    pub static VARIANT: Counter<u32> = const { Counter::new(0) };
    /// How many step functions were called.
    pub static STEPS: Counter<u64> = const { Counter::new(0) };
}

/// Counts a step in `STEPS`.
pub fn counted() {
    STEPS.with(|steps| steps.set(steps.get() + 1));
}

pub fn steps() -> u64 {
    STEPS.with(Counter::get)
}

/// A descriptor and the memory it points to.
pub struct Fixture {
    pub abi: u32,
    pub registry_minor: u32,
    pub entry: u32,
    pub entry_outcome: u32,
    pub functions: Vec<VibraFunction>,
    pub globals: Vec<u32>,
    pub constants: Vec<VibraConstant>,
    pub symbols: Vec<VibraTable<u8>>,
    pub origins: Vec<VibraOrigin>,
}

pub fn function(step: VibraStep, slots: u32, params: u32, env: bool) -> VibraFunction {
    VibraFunction {
        step: Some(step),
        slots,
        params,
        env: u32::from(env),
        blocks: 8,
    }
}

pub fn text(kind: u32, text: &'static str) -> VibraConstant {
    VibraConstant {
        kind,
        reserved: 0,
        data: VibraTable::new(text.as_bytes()),
    }
}

pub fn origin(source: &'static str, start: u32, end: u32) -> VibraOrigin {
    VibraOrigin {
        source: VibraTable::new(source.as_bytes()),
        start,
        end,
    }
}

impl Fixture {
    pub fn new(functions: Vec<VibraFunction>) -> Self {
        Self {
            abi: bridge::ABI_VERSION,
            registry_minor: 0,
            entry: 0,
            entry_outcome: 0,
            functions,
            globals: Vec::new(),
            constants: Vec::new(),
            symbols: Vec::new(),
            origins: Vec::new(),
        }
    }

    pub fn symbols(mut self, symbols: &[&'static str]) -> Self {
        self.symbols = symbols
            .iter()
            .map(|symbol| VibraTable::new(symbol.as_bytes()))
            .collect();
        self
    }

    /// The descriptor, borrowing the fixture's tables.
    pub fn descriptor(&self) -> VibraProgram {
        VibraProgram {
            abi: self.abi,
            registry_minor: self.registry_minor,
            entry: self.entry,
            entry_outcome: self.entry_outcome,
            functions: VibraTable::new(&self.functions),
            globals: VibraTable::new(&self.globals),
            constants: VibraTable::new(&self.constants),
            symbols: VibraTable::new(&self.symbols),
            origins: VibraTable::new(&self.origins),
        }
    }
}

/// What a run observed.
#[derive(Debug, PartialEq)]
pub struct Ran {
    pub outcome: Outcome,
    pub audit: Vec<AuditEvent>,
    pub result: Cell,
    pub stats: Stats,
    pub live: u64,
}

/// Runs the fixture through admission and the dispatcher, continuing after
/// every yield.
pub fn run_bridge(fixture: &Fixture, limit: u64, quota: u64) -> Ran {
    let descriptor = fixture.descriptor();
    // SAFETY: the fixture owns every table the descriptor addresses, and it
    // outlives the call and is not mutated.
    let admitted = unsafe { admit(&descriptor) }.expect("an admissible program");
    let code = Generated::new(admitted);
    let mut host = WorldProvider::new(&HostWorld::default());
    let mut ctx = Context::new(limit, code.globals(), &mut host);
    code.start(&mut ctx).expect("room for the entry frame");
    let outcome = loop {
        match run(&mut ctx, &code, quota) {
            Outcome::Yielded => {}
            other => break other,
        }
    };
    Ran {
        outcome,
        audit: ctx.audit().to_vec(),
        result: ctx.result(),
        stats: ctx.stats(),
        live: ctx.live_size(),
    }
}

// -- the demo ------------------------------------------------------------------

/// `main`: builds the function value of `even`, calls it with the rounds, then
/// performs one stdout host operation, then traps or returns the loop's result.
extern "C" fn demo_main(ctx: Ctx, resume: u32) -> u32 {
    counted();
    let body = || -> Result<u32, u32> {
        match resume {
            0 => {
                ok(sys::function(ctx, 0, 1, &[]))?;
                ok(sys::scalar(ctx, 1, CLASS_I64, ROUNDS.with(Counter::get)))?;
                Ok(sys::call_value(ctx, 0, &[1], 2, 1))
            }
            1 => {
                ok(sys::drop_slot(ctx, 0))?;
                ok(sys::constant(ctx, 3, 0))?;
                Ok(sys::host(ctx, 0, &[3], 1, 2, 1))
            }
            _ => {
                if TRAP.with(Counter::get) {
                    let code = u32::try_from(TrapCode::DivisionByZero.code()).unwrap();
                    return Ok(sys::trap(ctx, code, 2));
                }
                ok(sys::drop_slot(ctx, 1))?;
                ok(sys::drop_slot(ctx, 3))?;
                Ok(sys::ret(ctx, 2))
            }
        }
    };
    status(body())
}

/// One half of the mutual recursion: with `n` zero returns `n`, otherwise builds
/// a function value of `other` and tail-calls it with `n - 1`.
fn hop(ctx: Ctx, resume: u32, other: u32) -> u32 {
    counted();
    let body = || -> Result<u32, u32> {
        let mut at = resume;
        loop {
            match at {
                0 => {
                    ok(sys::scalar(ctx, 2, CLASS_I64, 0))?;
                    ok(sys::op(ctx, 3, OP_I64_EQ, 0, 2))?;
                    at = block(sys::branch(ctx, 3, 1, 2))?;
                }
                1 => return Ok(sys::ret(ctx, 0)),
                _ => {
                    ok(sys::scalar(ctx, 4, CLASS_I64, 1))?;
                    ok(sys::op(ctx, 5, OP_I64_SUB, 0, 4))?;
                    ok(sys::function(ctx, 6, other, &[]))?;
                    return Ok(sys::tail_call_value(ctx, 6, &[5]));
                }
            }
        }
    };
    status(body())
}

extern "C" fn even(ctx: Ctx, resume: u32) -> u32 {
    hop(ctx, resume, 2)
}

extern "C" fn odd(ctx: Ctx, resume: u32) -> u32 {
    hop(ctx, resume, 1)
}

/// The M1 demo: `rounds` indirect tail transfers between two functions, one
/// audited stdout operation, then a trap with origin ordinal 2 or a return.
pub fn demo(rounds: u64, trap: bool) -> Fixture {
    ROUNDS.with(|cell| cell.set(rounds));
    TRAP.with(|cell| cell.set(trap));
    let mut fixture = Fixture::new(vec![
        function(demo_main, 4, 0, false),
        function(even, 7, 1, true),
        function(odd, 7, 1, true),
    ])
    .symbols(&["io.stdout-text"]);
    fixture.constants = vec![text(CONSTANT_STR, "done\n")];
    fixture.origins = vec![
        origin("app/main.vib", 10, 11),
        origin("app/main.vib", 20, 21),
    ];
    fixture
}

/// The demo with `even` taking two operands while `main` passes one.
pub fn demo_with_arity_defect() -> Fixture {
    extern "C" fn returns_first(ctx: Ctx, _resume: u32) -> u32 {
        counted();
        sys::ret(ctx, 0)
    }
    let mut fixture = demo(1, false);
    fixture.functions.truncate(2);
    fixture.functions[1] = function(returns_first, 8, 2, true);
    fixture
}

// -- non-tail recursion --------------------------------------------------------

extern "C" fn deep_main(ctx: Ctx, resume: u32) -> u32 {
    counted();
    let body = || -> Result<u32, u32> {
        if resume == 0 {
            ok(sys::scalar(ctx, 0, CLASS_I64, ROUNDS.with(Counter::get)))?;
            return Ok(sys::call(ctx, 1, &[0], 1, 1));
        }
        Ok(sys::ret(ctx, 1))
    };
    status(body())
}

extern "C" fn deep_count(ctx: Ctx, resume: u32) -> u32 {
    counted();
    let body = || -> Result<u32, u32> {
        let mut at = resume;
        loop {
            match at {
                0 => {
                    ok(sys::scalar(ctx, 1, CLASS_I64, 0))?;
                    ok(sys::op(ctx, 2, OP_I64_EQ, 0, 1))?;
                    at = block(sys::branch(ctx, 2, 1, 2))?;
                }
                1 => return Ok(sys::ret(ctx, 1)),
                2 => {
                    ok(sys::scalar(ctx, 3, CLASS_I64, 1))?;
                    ok(sys::op(ctx, 4, OP_I64_SUB, 0, 3))?;
                    return Ok(sys::call(ctx, 1, &[4], 4, 3));
                }
                _ => {
                    ok(sys::op(ctx, 5, OP_I64_ADD, 4, 3))?;
                    return Ok(sys::ret(ctx, 5));
                }
            }
        }
    };
    status(body())
}

/// `count(n)` is `0` for zero and `count(n - 1) + 1` otherwise, with no tail
/// position.
pub fn deep(n: u64) -> Fixture {
    ROUNDS.with(|cell| cell.set(n));
    Fixture::new(vec![
        function(deep_main, 2, 0, false),
        function(deep_count, 6, 1, false),
    ])
}

// -- a module value ------------------------------------------------------------

extern "C" fn module_main(ctx: Ctx, resume: u32) -> u32 {
    counted();
    let body = || -> Result<u32, u32> {
        let mut at = resume;
        loop {
            match at {
                0 => {
                    if sys::global(ctx, 0, 0, 1) == bridge::STATUS_PROCEED {
                        at = 1;
                    } else {
                        return Ok(bridge::STATUS_CONTINUE);
                    }
                }
                1 => {
                    let status = sys::global(ctx, 0, 1, 2);
                    if status == bridge::STATUS_PROCEED {
                        at = 2;
                    } else {
                        return Ok(status);
                    }
                }
                _ => {
                    ok(sys::build(ctx, 2, KIND_TUPLE, 0, &[0, 1]))?;
                    ok(sys::scalar(ctx, 3, CLASS_I64, 7))?;
                    return Ok(sys::ret(ctx, 3));
                }
            }
        }
    };
    status(body())
}

extern "C" fn module_initializer(ctx: Ctx, resume: u32) -> u32 {
    counted();
    let body = || -> Result<u32, u32> {
        if resume == 0 {
            ok(sys::constant(ctx, 0, 0))?;
            return Ok(sys::host(ctx, 0, &[0], 1, 1, 0));
        }
        ok(sys::drop_slot(ctx, 1))?;
        Ok(sys::ret(ctx, 0))
    };
    status(body())
}

/// A module value whose initializer performs a host operation, read twice.
pub fn module_value() -> Fixture {
    let mut fixture = Fixture::new(vec![
        function(module_main, 4, 0, false),
        function(module_initializer, 2, 0, false),
    ])
    .symbols(&["io.stdout-text"]);
    fixture.globals = vec![1];
    fixture.constants = vec![text(CONSTANT_STR, "init\n")];
    fixture
}

// -- entries for the exit codes ------------------------------------------------

extern "C" fn returns_ok(ctx: Ctx, _resume: u32) -> u32 {
    counted();
    status((|| {
        ok(sys::build(ctx, 0, KIND_ENUM, 0, &[]))?;
        Ok(sys::ret(ctx, 0))
    })())
}

extern "C" fn returns_err(ctx: Ctx, _resume: u32) -> u32 {
    counted();
    status((|| {
        ok(sys::scalar(ctx, 0, bridge::CLASS_I32, 0))?;
        ok(sys::build(ctx, 1, KIND_ENUM, 1, &[0]))?;
        Ok(sys::ret(ctx, 1))
    })())
}

extern "C" fn traps(ctx: Ctx, _resume: u32) -> u32 {
    counted();
    sys::trap(
        ctx,
        u32::try_from(TrapCode::DivisionByZero.code()).unwrap(),
        1,
    )
}

/// An entry that returns `ok` of a `result` whose `err` is variant 1.
pub fn entry_ok() -> Fixture {
    let mut fixture = Fixture::new(vec![function(returns_ok, 2, 0, false)]);
    fixture.entry_outcome = 2;
    fixture
}

/// An entry that returns `err` of a `result` whose `err` is variant 1.
pub fn entry_err() -> Fixture {
    let mut fixture = Fixture::new(vec![function(returns_err, 2, 0, false)]);
    fixture.entry_outcome = 2;
    fixture
}

extern "C" fn returns_enum(ctx: Ctx, _resume: u32) -> u32 {
    counted();
    status((|| {
        ok(sys::build(
            ctx,
            0,
            KIND_ENUM,
            VARIANT.with(Counter::get),
            &[],
        ))?;
        Ok(sys::ret(ctx, 0))
    })())
}

/// An entry that returns a payload-free enum of `variant` (a `bool`-shaped
/// value for variant 1), whose `entry_outcome` the program declares.
pub fn entry_enum(variant: u32, entry_outcome: u32) -> Fixture {
    VARIANT.with(|cell| cell.set(variant));
    let mut fixture = Fixture::new(vec![function(returns_enum, 1, 0, false)]);
    fixture.entry_outcome = entry_outcome;
    fixture
}

/// An entry that traps at origin 1.
pub fn entry_trap() -> Fixture {
    let mut fixture = Fixture::new(vec![function(traps, 1, 0, false)]);
    fixture.origins = vec![origin("app/main.vib", 3, 9)];
    fixture
}
