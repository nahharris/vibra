//! Misuse robustness. The verifier normally prevents these calls, but the
//! bridge must be memory-safe when it is wrong, because that is what makes the
//! `unsafe` audit local.
//!
//! The pointers these calls pass are null or address a live array of sixteen
//! `u32`, as the routines' precondition requires; everything else (slot
//! indices, counts, blocks, functions, kinds, origins) is random, within and
//! beyond what the running frame holds.

use std::cell::Cell as Counter;

use vibra_host::{HostWorld, WorldProvider};
use vibra_nexus::{Context, Outcome, UNBOUNDED, run};
use vibra_nexus_bridge::{
    CONSTANT_ATOM, CONSTANT_BYTES, CONSTANT_STR, Generated, STATUS_PROCEED,
    STATUS_TRAP, admit,
};

use crate::fixture::{Fixture, function, text};
use crate::sys::{self, Ctx};

thread_local! {
    static STATE: Counter<u64> = const { Counter::new(0x9E37_79B9_7F4A_7C15) };
    static CALLS: Counter<u64> = const { Counter::new(0) };
    static STEPS: Counter<u64> = const { Counter::new(0) };
    static BAD: Counter<u64> = const { Counter::new(0) };
}

/// xorshift64*, deterministic.
fn next() -> u64 {
    STATE.with(|state| {
        let mut x = state.get();
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        state.set(x);
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    })
}

/// A random number below `bound`.
fn below(bound: u32) -> u32 {
    u32::try_from(next() % u64::from(bound)).unwrap()
}

/// Mostly in range of a frame of eight slots, sometimes far beyond.
fn slot() -> u32 {
    match below(10) {
        0 => below(u32::MAX),
        1 | 2 => 8 + below(8),
        _ => below(8),
    }
}

fn block() -> u32 {
    if below(4) == 0 {
        below(u32::MAX)
    } else {
        below(6)
    }
}

/// Records one routine's answer: a status of the closed set.
fn note(status: u32) {
    CALLS.with(|calls| calls.set(calls.get() + 1));
    if status > STATUS_PROCEED {
        BAD.with(|bad| bad.set(bad.get() + 1));
    }
}

fn note_packed(packed: u64) {
    note(u32::try_from(packed >> 32).unwrap_or(u32::MAX));
}

extern "C" fn fuzz_step(ctx: Ctx, _resume: u32) -> u32 {
    STEPS.with(|steps| steps.set(steps.get() + 1));
    let operands: [u32; 16] = std::array::from_fn(|_| slot());
    let list = |count: u32| -> &[u32] {
        if below(8) == 0 {
            &[]
        } else {
            &operands[..usize::try_from(count.min(16)).unwrap()]
        }
    };
    for _ in 0..8 {
        let count = below(12);
        match below(21) {
            0 => note(sys::call(ctx, below(6), list(count), slot(), block())),
            1 => note(sys::call_value(ctx, slot(), list(count), slot(), block())),
            2 => note(sys::tail_call(ctx, below(6), list(count))),
            3 => note(sys::tail_call_value(ctx, slot(), list(count))),
            4 => note(sys::ret(ctx, slot())),
            5 => note(sys::host(
                ctx,
                below(4),
                list(count),
                slot(),
                block(),
                below(4),
            )),
            6 => note(sys::global(ctx, below(3), slot(), block())),
            7 => note(sys::trap(ctx, below(8), below(4))),
            8 => note(sys::scalar(ctx, slot(), below(6), next())),
            9 => note(sys::constant(ctx, slot(), below(5))),
            10 => note(sys::copy(ctx, slot(), slot())),
            11 => note(sys::move_slot(ctx, slot(), slot())),
            12 => note(sys::drop_slot(ctx, slot())),
            13 => note(sys::op(ctx, slot(), below(6), slot(), slot())),
            14 => note(sys::build(ctx, slot(), below(8), below(4), list(count))),
            15 => note(sys::cell(ctx, slot(), slot(), below(5))),
            16 => note(sys::variant(ctx, slot(), slot())),
            17 => note(sys::function(ctx, slot(), below(6), list(count))),
            18 => note_packed(sys::branch(ctx, slot(), block(), block())),
            19 => note_packed(sys::switch(ctx, slot(), list(count), block())),
            _ => note(sys::ret(std::ptr::null_mut(), 0)),
        }
    }
    match below(12) {
        0 => 1,
        1 => 2,
        2 => 3,
        3 => 4,
        4 => below(u32::MAX),
        _ => 0,
    }
}

extern "C" fn quiet_step(_ctx: Ctx, _resume: u32) -> u32 {
    STATUS_TRAP
}

fn program() -> Fixture {
    let mut fixture = Fixture::new(vec![
        function(fuzz_step, 8, 0, false),
        function(fuzz_step, 4, 1, false),
        function(fuzz_step, 6, 2, true),
        function(fuzz_step, 3, 0, false),
        function(quiet_step, 2, 0, false),
        function(fuzz_step, 8, 0, true),
    ])
    .symbols(&["io.stdout-text", "io.stdin-text"]);
    fixture.globals = vec![3, 4];
    fixture.constants = vec![
        text(CONSTANT_STR, "text"),
        text(CONSTANT_BYTES, "bytes"),
        text(CONSTANT_ATOM, "atom"),
    ];
    fixture.origins = vec![
        crate::fixture::origin("a.vib", 0, 1),
        crate::fixture::origin("a.vib", 1, 2),
    ];
    fixture
}

#[test]
fn random_misuse_never_panics_and_leaves_the_context_consistent() {
    let fixture = program();
    let descriptor = fixture.descriptor();
    // SAFETY: the fixture owns every table the descriptor addresses.
    let admitted = unsafe { admit(&descriptor) }.expect("an admissible program");
    let code = Generated::new(admitted);
    let mut trials = 0_u64;
    let mut outcomes = std::collections::BTreeSet::new();
    while STEPS.with(Counter::get) < 20_000 && trials < 200_000 {
        trials += 1;
        let mut host = WorldProvider::new(&HostWorld::default());
        let limit = 16 * 1024 + u64::from(below(64 * 1024));
        let mut ctx = Context::new(limit, code.globals(), &mut host);
        if code.start(&mut ctx).is_err() {
            continue;
        }
        let outcome = loop {
            match run(&mut ctx, &code, UNBOUNDED) {
                Outcome::Yielded => {}
                other => break other,
            }
        };
        outcomes.insert(format!("{outcome:?}").split('(').next().unwrap().to_owned());
        assert!(ctx.live_size() <= ctx.peak_size());
        assert!(ctx.peak_size() <= limit, "the limit holds");
        // Dropping the context frees its memory; nothing escaped it.
        drop(ctx);
    }
    assert!(
        STEPS.with(Counter::get) >= 20_000,
        "at least 10,000 steps ran"
    );
    assert!(
        CALLS.with(Counter::get) >= 100_000,
        "every step made routine calls, each of which answered"
    );
    assert_eq!(BAD.with(Counter::get), 0, "every answer was a status");
    assert!(
        outcomes.len() >= 2,
        "the fuzz reached several outcomes: {outcomes:?}"
    );
}

#[test]
fn a_null_handle_is_a_trap_status_and_a_known_pattern_is_a_defect() {
    assert_eq!(sys::ret(std::ptr::null_mut(), 0), STATUS_TRAP);
    assert_eq!(sys::scalar(std::ptr::null_mut(), 0, 0, 0), STATUS_TRAP);
}
