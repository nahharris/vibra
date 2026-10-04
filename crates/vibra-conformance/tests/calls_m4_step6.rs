//! Calls (milestone 4 Step 6): function values, closures and lambdas, indirect
//! calls, generics with run-time type arguments, a tail call to every kind of
//! callee, labelled defaults, and the activation and reclamation guarantees of
//! the runtime chapter, in WebAssembly.
//!
//! The reference interpreter is the oracle. A program here is checked from
//! source, run by both backends, and held to one result; an expectation written
//! beside it is authored from the specification's canonical value encoding, not
//! copied from a run. The tests also measure what the specification asks of
//! tail calls and of activations: a loop of tail calls holds a frame stack and
//! a live arena that do not grow, no engine stack is used per language call, a
//! depth is bounded only by memory, and running out of memory is the host event.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use std::collections::BTreeSet;

use vibra_ir::{CheckedProgram, Type};
use vibra_types::check_source;
use vibra_wasm::layout::{self, header, state};
use vibra_wasm_run::{
    Instance, LiveSizes, MemoryLimit, Observed, Outcome, Runner, Started,
};

const MIB: usize = 1024 * 1024;

// -- the harness --------------------------------------------------------------

/// The checked program of `source`.
fn checked(source: &str) -> CheckedProgram {
    let result = check_source("input.vib", source);
    assert!(result.accepted(), "{:?}\n{source}", result.diagnostics());
    result.program().expect("an accepted program").clone()
}

fn entry_type(program: &CheckedProgram) -> Type {
    program.entry().signature().result()
}

fn emitted(program: &CheckedProgram) -> Vec<u8> {
    vibra_wasm::emit(program)
        .unwrap_or_else(|error| panic!("the program lowers: {error}"))
        .into_bytes()
}

fn runner_with(limit: usize) -> Runner {
    Runner::new(MemoryLimit::new(limit)).expect("the engine configures")
}

fn runner() -> Runner {
    runner_with(64 * MIB)
}

/// The interpreter's canonical result, the oracle.
fn oracle(program: &CheckedProgram) -> String {
    vibra_interp::run(program)
        .expect("the interpreter runs the program")
        .canonical_result()
}

/// What the Wasm backend observes, with the live sizes around the run.
fn wasm(program: &CheckedProgram) -> (String, LiveSizes) {
    let ty = entry_type(program);
    match runner()
        .run_observed(&emitted(program), &ty, program.types())
        .expect("the module runs")
    {
        Observed::Completed { value, live } => (value.canonical_observation(&ty), live),
        Observed::Stopped(outcome) => panic!("the entry did not complete: {outcome:?}"),
    }
}

/// Runs `source` in both backends, asserts they agree, and returns the result
/// and the live sizes.
fn agree(source: &str) -> (String, LiveSizes) {
    let program = checked(source);
    let expected = oracle(&program);
    let (observed, live) = wasm(&program);
    assert_eq!(observed, expected, "the backends disagree on:\n{source}");
    (observed, live)
}

/// Runs `source` in both backends, and holds the result to `expected`, which
/// is written from the canonical value encoding of the runtime chapter.
fn agree_on(source: &str, expected: &str) -> LiveSizes {
    let (observed, live) = agree(source);
    assert_eq!(
        observed, expected,
        "not the specified encoding for:\n{source}"
    );
    live
}

fn word(instance: &mut Instance, address: u32) -> u32 {
    let bytes = instance
        .read_memory_for_tests(address, 4)
        .expect("the state is in memory");
    u32::from_le_bytes(bytes.try_into().expect("four bytes"))
}

/// A started instance of `program`.
fn start(program: &CheckedProgram, limit: usize) -> Instance {
    let bytes = emitted(program);
    let Started::Ready(instance) =
        runner_with(limit).start(&bytes).expect("a v1 module")
    else {
        panic!("the module's own memory exceeds the limit");
    };
    instance
}

/// What a run left: the bytes of live arena after the host released the result,
/// the high-water mark of the arena, and the frames that were left.
struct Footprint {
    /// The live size when the entry completed, while the host held the result.
    with_result: u64,
    live: u64,
    /// The bytes the module values reach, which an instance keeps for its life.
    kept: u64,
    arena_used: u32,
    frame_depth: u32,
}

/// Runs the entry once on a fresh instance and reads the result, then measures.
fn footprint(program: &CheckedProgram, limit: usize) -> Footprint {
    let mut instance = start(program, limit);
    let Outcome::Completed {
        result,
        live_size: with_result,
    } = instance.call_entry().expect("runs")
    else {
        panic!("the entry did not complete");
    };
    instance
        .observe(result, &entry_type(program), program.types())
        .expect("the host reads the result");
    let live = instance.live_size().expect("live size");
    let kept = module_values_bytes(&mut instance);
    Footprint {
        with_result,
        live,
        kept,
        arena_used: word(&mut instance, state::ARENA_USED),
        frame_depth: word(&mut instance, state::FRAME_DEPTH),
    }
}

/// The bytes of every block the instance's module-value table reaches, the
/// table's own block included: what an instance legitimately keeps for its
/// lifetime after the host has released everything it held.
fn module_values_bytes(instance: &mut Instance) -> u64 {
    let table = word(instance, state::MODULE_VALUES);
    if table == 0 {
        return 0;
    }
    let mut seen = BTreeSet::new();
    let mut pending = vec![table];
    let mut bytes = 0_u64;
    while let Some(block) = pending.pop() {
        if !seen.insert(block) {
            continue;
        }
        let size = word(instance, block + header::BLOCK_SIZE);
        bytes += u64::from(size);
        let kind_and_stride = word(instance, block + header::KIND);
        let stride = (kind_and_stride >> 8) & 0xFF;
        if stride != layout::STRIDE_CELLS {
            continue;
        }
        let len = word(instance, block + header::LEN);
        let classes = instance
            .read_memory_for_tests(block + header::SIZE, len as usize)
            .expect("class bytes");
        let cells = block + layout::cells_offset(len);
        for (position, class) in (0_u32..).zip(classes) {
            if class == layout::CellClass::Ref.code() {
                pending.push(word(instance, cells + 8 * position));
            }
        }
    }
    bytes
}

fn read_u64(instance: &mut Instance, address: u32) -> u64 {
    let bytes = instance
        .read_memory_for_tests(address, 8)
        .expect("memory is readable");
    u64::from_le_bytes(bytes.try_into().expect("eight bytes"))
}

/// The header word `field` of the arena object the completed entry returned,
/// found through the handle table. The test reads layout the host never does.
fn result_header(instance: &mut Instance, field: u32) -> u32 {
    let id = read_u64(instance, state::RESULT);
    let slot = u32::try_from(id & 0xFFFF_FFFF).expect("a slot");
    let table = word(instance, state::TABLE);
    let offset = word(instance, table + slot * layout::TABLE_ENTRY_SIZE + 8);
    word(instance, offset + field)
}

// -- function values, closures, and indirect calls --------------------------------

#[test]
fn a_returned_closure_owns_what_it_captured() {
    agree_on(
        "(defn answer () i32\n  ((make)))\n(defn make () (fn () i32)\n  (let value 41i32)\n  (lambda () i32 value))\n",
        "(record type: @i32 value: 41i32)\n",
    );
}

#[test]
fn a_closure_captures_values_of_every_class() {
    let source = "(deftype box (record n i32))\n\
(defn main () (tuple i8 i64 f32 f64 str box bool)\n\
  (let a -3i8)\n  (let b 9000000000i64)\n  (let c 1.5f32)\n  (let d 2.25f64)\n  (let e \"text\")\n  (let f (box n: 7i32))\n  (let g true)\n\
  (let make (lambda () (tuple i8 i64 f32 f64 str box bool) (tupleof a b c d e f g)))\n  (make))\n";
    agree(source);
}

#[test]
fn a_closure_with_no_capture_and_one_with_many() {
    agree_on(
        "(defn main () (tuple i32 i32)\n  (let a 1i32)\n  (let b 2i32)\n  (let c 3i32)\n  (let d 4i32)\n  (let none (lambda () i32 9i32))\n  (let many (lambda () i32 (do a b c) d))\n  (tupleof (none) (many)))\n",
        "(record type: (record type: @tuple arguments: (array @i32 @i32)) value: (record kind: @tuple values: (array 9i32 4i32)))\n",
    );
}

#[test]
fn a_closure_captures_a_closure_across_two_lambdas() {
    agree_on(
        "(defn main () i32\n  (let base 40i32)\n  (let inner (lambda () i32 base))\n  (let outer (lambda () i32 (let again (lambda () i32 (inner))) (again)))\n  (outer))\n",
        "(record type: @i32 value: 40i32)\n",
    );
}

#[test]
fn a_function_value_is_called_through_a_parameter_a_binding_and_a_field() {
    // The first function of a source is its entry.
    let source = "(deftype holder (record run (fn (i32) i32)))\n\
(defn main () (tuple i32 i32 i32)\n\
  (let bound id)\n\
  (let held (holder run: id))\n\
  (tupleof (apply id 1i32) (bound 2i32) ((held @run) 3i32)))\n\
(defn id (value i32) i32 value)\n\
(defn apply (f (fn (i32) i32) value i32) i32 (f value))\n";
    agree_on(
        source,
        "(record type: (record type: @tuple arguments: (array @i32 @i32 @i32)) value: (record kind: @tuple values: (array 1i32 2i32 3i32)))\n",
    );
}

#[test]
fn an_application_evaluates_its_callee_once_before_its_operands() {
    // The callee is a call whose result is a function; both backends return the
    // one value of the callee that the operand's evaluation cannot change.
    agree_on(
        "(defn main () i32 ((pick) 5i32))\n(defn pick () (fn (i32) i32) (lambda (value i32) i32 value))\n",
        "(record type: @i32 value: 5i32)\n",
    );
}

#[test]
fn a_function_value_of_a_module_function_carries_the_functions_defaults() {
    let source = "(defn main () (tuple atom atom)\n  (let f level)\n  (tupleof (f \"a\") (f \"b\" severity: @error)))\n\
(defn level (message str) atom\n  labelled: (severity atom @info)\n  severity)\n";
    agree_on(
        source,
        "(record type: (record type: @tuple arguments: (array @atom @atom)) value: (record kind: @tuple values: (array @info @error)))\n",
    );
}

// -- a counter without arithmetic ------------------------------------------------

/// The bits of a digit of the counter: a digit is one of `2^DIGIT_BITS` shared
/// module values.
const DIGIT_BITS: u32 = 6;

/// How many digits a counter that reaches `n` needs.
fn counter_digits(n: u32) -> u32 {
    (32 - n.leading_zeros()).div_ceil(DIGIT_BITS).max(1)
}

/// A decision tree over the bits `bit..` of the digit `d`, with the bits chosen
/// so far in `value`, whose leaves are `leaf(value)`.
fn digit_tree(bit: u32, value: u32, leaf: &dyn Fn(u32) -> String) -> String {
    if bit == DIGIT_BITS {
        return leaf(value);
    }
    let set = digit_tree(bit + 1, value | 1 << bit, leaf);
    let clear = digit_tree(bit + 1, value, leaf);
    format!("(if (d @b{bit}) {set} {clear})")
}

/// The definitions of a counter with no arithmetic, which counts to `n`:
/// `state`, `(zero)`, `(inc s)`, which makes the next state, and `(done s)`,
/// true exactly when the state is `n`. The language has no arithmetic until
/// Step 8a, and a counter still needs only `if`, a record, and a call, so it
/// drives a loop of any length. A state is a tuple of digits, and a digit is a
/// record of bools that is one of sixty-four module values, so a state of a long
/// recursion holds a few references and not a few dozen bools, which keeps the
/// reference interpreter's accounting of a recursion a hundred thousand deep
/// inside the runner's memory limit. No expression nests more than five `if`s.
fn counter(n: u32) -> String {
    use std::collections::BTreeSet;
    use std::fmt::Write as _;
    let digits = counter_digits(n);
    let values = 1_u32 << DIGIT_BITS;
    let mut text = String::new();
    let fields = (0..DIGIT_BITS)
        .map(|bit| format!(" b{bit} bool"))
        .collect::<String>();
    writeln!(text, "(deftype digit (record{fields}))").expect("a string");
    writeln!(
        text,
        "(deftype state (tuple{}))",
        " digit".repeat(digits as usize)
    )
    .expect("a string");
    for value in 0..values {
        let bits = (0..DIGIT_BITS)
            .map(|bit| format!(" b{bit}: {}", value >> bit & 1 == 1))
            .collect::<String>();
        writeln!(text, "(def d{value} digit (digit{bits}))").expect("a string");
    }
    let zeros = " d0".repeat(digits as usize);
    writeln!(text, "(defn zero () state (state{zeros}))").expect("a string");
    writeln!(
        text,
        "(defn is-max (d digit) bool {})",
        digit_tree(0, 0, &|value| (value == values - 1).to_string())
    )
    .expect("a string");
    writeln!(
        text,
        "(defn next-digit (d digit) digit {})",
        digit_tree(0, 0, &|value| format!("d{}", (value + 1) % values))
    )
    .expect("a string");
    writeln!(text, "(defn inc (s state) state (carry-0 s))").expect("a string");
    for place in 0..digits {
        let carry = if place + 1 == digits {
            format!("(state{zeros})")
        } else {
            format!("(carry-{} s)", place + 1)
        };
        let set = (0..digits)
            .map(|other| match other.cmp(&place) {
                std::cmp::Ordering::Less => " d0".to_owned(),
                std::cmp::Ordering::Equal => format!(" (next-digit (s {place}))"),
                std::cmp::Ordering::Greater => format!(" (s {other})"),
            })
            .collect::<String>();
        writeln!(
            text,
            "(defn carry-{place} (s state) state\n  (if (is-max (s {place})) {carry} (state{set})))"
        )
        .expect("a string");
    }
    writeln!(text, "(defn done (s state) bool (done-0 s))").expect("a string");
    let mut wanted = BTreeSet::new();
    for place in 0..digits {
        let value = (n >> (DIGIT_BITS * place)) & (values - 1);
        wanted.insert(value);
        let rest = if place + 1 == digits {
            "true".to_owned()
        } else {
            format!("(done-{} s)", place + 1)
        };
        writeln!(
            text,
            "(defn done-{place} (s state) bool\n  (if (is-digit-{value} (s {place})) {rest} false))"
        )
        .expect("a string");
    }
    for value in wanted {
        writeln!(
            text,
            "(defn is-digit-{value} (d digit) bool {})",
            digit_tree(0, 0, &|other| (other == value).to_string())
        )
        .expect("a string");
    }
    text
}

/// A loop of `n` rounds in tail position through one kind of callee, and the
/// result it ends with.
#[derive(Clone, Copy, Debug)]
enum Loop {
    /// A module function calls itself.
    Direct,
    /// Two module functions call each other.
    Mutual,
    /// A function calls the function value it was given, which calls it back.
    Parameter,
    /// A closure calls its creator's loop through a captured function value.
    Closure,
    /// A method of a declared type calls itself.
    Method,
    /// A function calls a function value it read from a record field into a
    /// binding.
    FieldAndBinding,
    /// A loop hands its activation on to another loop, and that one to a last
    /// function neither is reachable from.
    HandOff,
}

impl Loop {
    const ALL: [Self; 7] = [
        Self::Direct,
        Self::Mutual,
        Self::Parameter,
        Self::Closure,
        Self::Method,
        Self::FieldAndBinding,
        Self::HandOff,
    ];

    fn result(self) -> i32 {
        match self {
            Self::Direct | Self::Mutual => 7,
            Self::Parameter => 11,
            Self::Closure => 13,
            Self::Method => 17,
            Self::FieldAndBinding => 19,
            Self::HandOff => 23,
        }
    }

    fn source(self, n: u32) -> String {
        let body = match self {
            Self::Direct => "(defn main () i32 (run (zero)))\n\
(defn run (s state) i32\n  (if (done s) 7i32 (run (inc s))))\n"
                .to_owned(),
            Self::Mutual => "(defn main () i32 (ping (zero)))\n\
(defn ping (s state) i32\n  (if (done s) 7i32 (pong (inc s))))\n\
(defn pong (s state) i32\n  (if (done s) 7i32 (ping (inc s))))\n"
                .to_owned(),
            Self::Parameter => "(defn main () i32 (apply countdown (zero)))\n\
(defn apply (next (fn (state) i32) s state) i32 (next s))\n\
(defn countdown (s state) i32\n  (if (done s) 11i32 (apply countdown (inc s))))\n"
                .to_owned(),
            Self::Closure => "(defn main () i32 (spin (zero)))\n\
(defn spin (s state) i32 ((again spin) s))\n\
(defn again (back (fn (state) i32)) (fn (state) i32)\n  (lambda (s state) i32\n    (if (done s) 13i32 (back (inc s)))))\n"
                .to_owned(),
            Self::Method => "(defn main () i32 (walker.go (walker s: (zero))))\n\
(deftype walker (record s state)\n  (defn go (value self) i32\n    (if (done (value @s)) 17i32 (walker.go (walker s: (inc (value @s)))))))\n"
                .to_owned(),
            Self::FieldAndBinding => "(defn main () i32 (again (zero)))\n\
(deftype stepper (record next (fn (state) i32)))\n\
(defn again (s state) i32 (drive (stepper next: again) s))\n\
(defn drive (h stepper s state) i32\n  (if (done s)\n    19i32\n    (do (let go (h @next)) (go (inc s)))))\n"
                .to_owned(),
            Self::HandOff => "(defn main () i32 (first (zero)))\n\
(defn first (s state) i32\n  (if (done s) (second (zero)) (first (inc s))))\n\
(defn second (s state) i32\n  (if (done s) (last 23i32) (second (inc s))))\n\
(defn last (value i32) i32 value)\n"
                .to_owned(),
        };
        format!("{body}{}", counter(n))
    }
}

/// Non-tail recursion `n` activations deep with a base case, over the counter.
fn depth_source(n: u32) -> String {
    format!(
        "(defn main () i32 (depth (zero)))\n\
(defn depth (s state) i32\n  (if (done s) 0i32 (do (depth (inc s)) 5i32)))\n{}",
        counter(n)
    )
}

#[test]
fn the_counter_counts_without_arithmetic() {
    // Seven rounds of a direct loop end where the counter says, and a count
    // that is a power of two and one that is not both terminate.
    for n in [1, 2, 3, 7, 8, 100] {
        let (result, _) = agree(&Loop::Direct.source(n));
        assert_eq!(result, "(record type: @i32 value: 7i32)\n", "{n}");
    }
}

#[test]
fn every_kind_of_callee_is_called_in_tail_position() {
    for kind in Loop::ALL {
        let (result, _) = agree(&Loop::source(kind, 50));
        assert_eq!(
            result,
            format!("(record type: @i32 value: {}i32)\n", kind.result()),
            "{kind:?}"
        );
    }
}

#[test]
fn a_hundred_thousand_tail_calls_through_each_kind_of_callee_complete() {
    for kind in Loop::ALL {
        let (result, live) = agree(&Loop::source(kind, 100_000));
        assert_eq!(
            result,
            format!("(record type: @i32 value: {}i32)\n", kind.result()),
            "{kind:?}"
        );
        assert_eq!(live.start, 0, "{kind:?}");
    }
}

/// The most the live arena may grow between two iteration counts of a loop,
/// and the most the arena's high-water mark may: constants of this program,
/// which allocates a fresh `state` record in every round. A loop that grew with
/// its rounds would grow by megabytes between ten thousand and a hundred
/// thousand of them.
const LIVE_GROWTH_BOUND: u64 = 64;
const ARENA_GROWTH_BOUND: u32 = 4 * 1024;

#[test]
fn a_tail_loop_that_allocates_each_round_holds_a_bounded_live_arena() {
    // The live size through `vibra_v1_live_size` at two counts a factor of ten
    // apart, while the host still holds the result and after it released it,
    // and the arena's high-water mark, which is what a frame stack or a garbage
    // heap that grew with the rounds would show even after the run freed it.
    for kind in Loop::ALL {
        let measure = |n| footprint(&checked(&kind.source(n)), 64 * MIB);
        let small = measure(10_000);
        let large = measure(100_000);
        println!(
            "{kind:?}: live with the result {} then {}, after release {} then {}, arena {} then {}",
            small.with_result,
            large.with_result,
            small.live,
            large.live,
            small.arena_used,
            large.arena_used
        );
        assert!(
            large.with_result.abs_diff(small.with_result) <= LIVE_GROWTH_BOUND,
            "{kind:?}: live {} against {}",
            small.with_result,
            large.with_result
        );
        assert!(
            large.live.abs_diff(small.live) <= LIVE_GROWTH_BOUND,
            "{kind:?}: after release {} against {}",
            small.live,
            large.live
        );
        assert!(
            large.arena_used.abs_diff(small.arena_used) <= ARENA_GROWTH_BOUND,
            "{kind:?}: arena {} against {}",
            small.arena_used,
            large.arena_used
        );
        assert_eq!(large.frame_depth, 0);
        assert_eq!(large.live, large.kept, "{kind:?}: only the module values");
    }
}

#[test]
fn a_hundred_thousand_tail_calls_run_in_a_few_pages_of_memory() {
    // The same loops under a limit far below what a hundred thousand frames or
    // a hundred thousand unreleased records would need: the frame is reused and
    // the garbage is freed in every round.
    let limit = 256 * 1024;
    for kind in Loop::ALL {
        let program = checked(&kind.source(100_000));
        let bytes = emitted(&program);
        let ty = entry_type(&program);
        let outcome = runner_with(limit)
            .run_observed(&bytes, &ty, program.types())
            .expect("runs");
        assert!(
            matches!(outcome, Observed::Completed { .. }),
            "{kind:?} under {limit} bytes: {outcome:?}"
        );
    }
}

#[test]
fn the_same_loop_without_a_tail_call_does_not_run_in_that_memory() {
    // The control: when the call that repeats is not in tail position the
    // frames pile up, the limit is exhausted, and both are the host event.
    let source = format!(
        "(defn main () i32 (run (zero)))\n\
(defn run (s state) i32\n  (if (done s) 7i32 (do (run (inc s)) 7i32)))\n{}",
        counter(100_000)
    );
    let program = checked(&source);
    let bytes = emitted(&program);
    let ty = entry_type(&program);
    assert_eq!(
        runner_with(256 * 1024)
            .run_observed(&bytes, &ty, program.types())
            .expect("runs"),
        Observed::Stopped(Outcome::MemoryExhausted)
    );
    let (result, _) = agree(&source);
    assert_eq!(result, "(record type: @i32 value: 7i32)\n");
}

// -- activations: depth is bounded only by memory --------------------------------------

#[test]
fn non_tail_recursion_a_hundred_thousand_deep_completes_in_both_backends() {
    // The engine's stack is 64 KiB, which a nested WebAssembly call per
    // activation could not use for more than a few thousand.
    let program = checked(&depth_source(100_000));
    let expected = oracle(&program);
    assert_eq!(expected, "(record type: @i32 value: 5i32)\n");
    let ty = entry_type(&program);
    let small = Runner::with_wasm_stack(MemoryLimit::new(64 * MIB), 64 * 1024)
        .expect("a runner with a small engine stack");
    let Observed::Completed { value, live } = small
        .run_observed(&emitted(&program), &ty, program.types())
        .expect("runs")
    else {
        panic!("the recursion did not complete");
    };
    assert_eq!(value.canonical_observation(&ty), expected);
    assert_eq!(live.start, 0);
    let used = footprint(&program, 64 * MIB);
    assert_eq!(used.frame_depth, 0);
    assert_eq!(live.end, used.kept, "the module values and nothing else");
    assert_eq!(used.live, used.kept);
    println!(
        "depth 100000: arena high-water {} bytes, live after release {}",
        used.arena_used, used.live
    );
    assert!(
        used.arena_used > 100_000 * 64,
        "every activation of the recursion is live at the base case: {}",
        used.arena_used
    );
}

#[test]
fn the_engine_stack_a_deep_recursion_needs_does_not_depend_on_its_depth() {
    // The smallest engine stack, in KiB, under which a recursion runs: the same
    // for a recursion a hundred times deeper, because no activation takes any.
    let smallest = |n: u32| {
        let program = checked(&depth_source(n));
        let bytes = emitted(&program);
        let ty = entry_type(&program);
        [8, 12, 16, 24, 32, 48, 64].into_iter().find(|kib| {
            let runner =
                Runner::with_wasm_stack(MemoryLimit::new(64 * MIB), kib * 1024)
                    .expect("a runner");
            matches!(
                runner.run_observed(&bytes, &ty, program.types()),
                Ok(Observed::Completed { .. })
            )
        })
    };
    let shallow = smallest(1_000).expect("some stack up to 64 KiB runs it");
    let deep = smallest(100_000).expect("some stack up to 64 KiB runs it");
    println!(
        "engine stack needed: {shallow} KiB at 1,000 activations, {deep} KiB at 100,000"
    );
    assert_eq!(shallow, deep);
}

/// Non-tail recursion that never reaches a base case, with no arithmetic.
const FOREVER: &str = "(defn main () void (do (forever) void))\n(defn forever () void (do (forever) void))\n";

/// Whether `program` completes under `limit` bytes of memory, or ends in the
/// host event.
fn completes_under(program: &CheckedProgram, limit: usize) -> bool {
    let ty = entry_type(program);
    match runner_with(limit)
        .run_observed(&emitted(program), &ty, program.types())
        .expect("runs")
    {
        Observed::Completed { .. } => true,
        Observed::Stopped(Outcome::MemoryExhausted) => false,
        Observed::Stopped(other) => {
            panic!("neither a result nor the host event: {other:?}")
        }
    }
}

#[test]
fn a_recursion_with_no_base_case_is_the_host_event_and_a_larger_limit_recovers() {
    let program = checked(FOREVER);
    let bytes = emitted(&program);
    for limit in [MIB, 4 * MIB, 64 * MIB] {
        assert_eq!(
            runner_with(limit).run_entry(&bytes).expect("runs"),
            Outcome::MemoryExhausted,
            "{limit}"
        );
    }
    // Recovery: the program that recurses to a depth the first limit does not
    // hold completes on a new instance with a larger one.
    let program = checked(&depth_source(20_000));
    assert!(!completes_under(&program, 512 * 1024));
    assert!(completes_under(&program, 64 * MIB));
    agree_on(&depth_source(20_000), "(record type: @i32 value: 5i32)\n");
}

#[test]
fn a_recursion_exactly_at_the_memory_limit_runs_and_a_page_under_does_not() {
    let program = checked(&depth_source(2_000));
    // Pages of 64 KiB: the smallest limit that completes, by bisection.
    let page = 64 * 1024;
    let (mut low, mut high) = (1, 1024);
    assert!(completes_under(&program, high * page));
    while low < high {
        let middle = (low + high) / 2;
        if completes_under(&program, middle * page) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    assert!(completes_under(&program, low * page), "{low} pages");
    assert!(
        !completes_under(&program, (low - 1) * page),
        "{} pages",
        low - 1
    );
    println!("2000 activations need exactly {low} pages");
}

// -- tail calls: the frame is replaced in place, whatever its size --------------------

#[test]
fn a_tail_call_to_a_callee_with_more_locals_than_its_caller_replaces_the_frame() {
    // `big` binds thousands of values, so its frame is larger than the room the
    // small caller's segment leaves: the frame is popped and pushed, again and
    // again, and nothing leaks.
    // One flat tuple of three thousand operands, each a temporary of the frame.
    let operands = "1i32 ".repeat(3_000);
    let source = format!(
        "(defn main () i32 (small (zero)))\n\
(defn small (s state) i32\n  (if (done s) 29i32 (big (inc s))))\n\
(defn big (s state) i32\n  (let wide (tupleof {operands}))\n  (small s))\n{}",
        counter(200)
    );
    let (result, _) = agree(&source);
    assert_eq!(result, "(record type: @i32 value: 29i32)\n");
    let program = checked(&source);
    let used = footprint(&program, 64 * MIB);
    assert_eq!(used.frame_depth, 0);
    let longer = footprint(
        &checked(&source.replace(&counter(200), &counter(2_000))),
        64 * MIB,
    );
    assert!(
        longer.arena_used.abs_diff(used.arena_used) <= ARENA_GROWTH_BOUND,
        "{} against {}",
        used.arena_used,
        longer.arena_used
    );
}

#[test]
fn a_return_of_a_call_is_a_tail_call() {
    // The operand of `return` is in tail position whether or not the `return` is.
    let source = format!(
        "(defn main () i32 (run (zero)))\n\
(defn run (s state) i32\n  (if (done s) void (return (run (inc s))))\n  31i32)\n{}",
        counter(50_000)
    );
    let (result, _) = agree(&source);
    assert_eq!(result, "(record type: @i32 value: 31i32)\n");
    assert!(completes_under(&checked(&source), 256 * 1024));
}

// -- reclamation: every program ends at its baseline ------------------------------------

#[test]
fn programs_of_every_call_kind_return_the_live_size_to_the_module_values() {
    let sources = [
        "(defn main () i32 ((make)))\n(defn make () (fn () i32) (let value 41i32) (lambda () i32 value))\n".to_owned(),
        "(defn main () str ((make \"kept\")))\n(defn make (value str) (fn () str) (lambda () str value))\n".to_owned(),
        "(defn main () str (let f (make \"a\")) (let g (make-again f)) (g))\n(defn make (value str) (fn () str) (lambda () str value))\n(defn make-again (inner (fn () str)) (fn () str) (lambda () str (inner)))\n".to_owned(),
        format!(
            "(defn main () (maybe str) (wrap \"x\"))\n(defn wrap (value t) (maybe t)\n  where: (t any)\n  (maybe.some value))\n{MAYBE}"
        ),
        Loop::Closure.source(2_000),
        Loop::FieldAndBinding.source(2_000),
        depth_source(1_000),
    ];
    for source in &sources {
        let program = checked(source);
        let mut instance = start(&program, 64 * MIB);
        for run in 1..=2 {
            let Outcome::Completed { result, .. } =
                instance.call_entry().expect("runs")
            else {
                panic!("the entry did not complete");
            };
            instance
                .observe(result, &entry_type(&program), program.types())
                .expect("the host reads the result");
            assert_eq!(word(&mut instance, state::FRAME_TOP), 0, "no frame is left");
            assert_eq!(word(&mut instance, state::FRAME_DEPTH), 0, "no activation");
            let live = instance.live_size().expect("live size");
            let kept = module_values_bytes(&mut instance);
            assert_eq!(
                live, kept,
                "run {run}: the live size is exactly what the module values keep:\n{source}"
            );
        }
    }
}

// -- negative: a forged function value is the toolchain's defect -------------------------

#[test]
fn a_function_value_that_takes_another_number_of_operands_is_a_defect() {
    // The call checks the arity the value records against the operands it
    // passes, before it enters any frame. The test forges the constant the call
    // compares with: the one indirect call of this program passes one operand.
    let program =
        checked("(defn main () i32 (let f (lambda (n i32) i32 n)) (f 1i32))\n");
    let mut bytes = emitted(&program);
    // local.get CELLS; i32.load offset=16; i32.const 1; i32.ne; if
    let pattern = [0x20, 0x08, 0x28, 0x02, 0x10, 0x41, 0x01, 0x47, 0x04, 0x40];
    let at = bytes
        .windows(pattern.len())
        .position(|window| window == pattern)
        .expect("the arity guard of the indirect call");
    assert_eq!(
        bytes
            .windows(pattern.len())
            .filter(|window| *window == pattern)
            .count(),
        1,
        "one indirect call"
    );
    bytes[at + 6] = 0x02;
    let outcome = runner()
        .run_entry(&bytes)
        .expect("a valid module that traps");
    assert_eq!(
        outcome,
        Outcome::Trapped {
            code: vibra_ir::boundary::TrapCode::InvalidCheckedProgram,
            origin: None
        }
    );
}

// -- generics: type arguments at run time ------------------------------------------

const MAYBE: &str = "(deftype maybe (enum some t none void)\n  where: (t any))\n";

#[test]
fn a_generic_function_is_called_at_two_instantiations_and_every_class() {
    let source = "(defn main () (tuple i32 str bool f64 i64 (tuple i8 u16))\n\
  (tupleof (identity 1i32) (identity \"s\") (identity true) (identity 2.5f64) (identity 3i64) (identity (tupleof 4i8 5u16))))\n\
(defn identity (value t) t\n  where: (t any)\n  value)\n";
    agree(source);
}

#[test]
fn a_generic_enum_payload_is_none_when_its_type_argument_is_void() {
    // Built in generic code, `(maybe.some value)` has no payload at `void` and one
    // at every other type: the type argument decides, as it does in the
    // reference interpreter.
    let source = format!(
        "(defn main () (tuple (maybe void) (maybe i32) (maybe str) (maybe (maybe void)))\n\
  (tupleof (wrap void) (wrap 5i32) (wrap \"x\") (wrap (maybe.some))))\n\
(defn wrap (value t) (maybe t)\n  where: (t any)\n  (maybe.some value))\n{MAYBE}"
    );
    agree(&source);
}

/// How many components the arena object `main` returns has, which the host's
/// reader does not look at for an enum and the encoding states: `0` for a
/// `void` payload and `1` for any other.
fn returned_len(source: &str) -> u32 {
    let program = checked(source);
    let mut instance = start(&program, 64 * MIB);
    let Outcome::Completed { .. } = instance.call_entry().expect("runs") else {
        panic!("the entry did not complete");
    };
    result_header(&mut instance, header::LEN)
}

#[test]
fn an_enum_built_in_generic_code_has_the_payload_cells_of_its_instantiation() {
    let wrap = |t: &str, value: &str| {
        format!(
            "(defn main () (maybe {t}) (wrap {value}))\n\
(defn wrap (value u) (maybe u)\n  where: (u any)\n  (maybe.some value))\n{MAYBE}"
        )
    };
    assert_eq!(returned_len(&wrap("void", "void")), 0, "no payload at void");
    assert_eq!(returned_len(&wrap("i32", "5i32")), 1, "a payload at i32");
    assert_eq!(returned_len(&wrap("str", "\"s\"")), 1, "a payload at str");
    // The same through a closure, which runs at its creator's type arguments.
    let closure = |t: &str, value: &str| {
        format!(
            "(defn main () (maybe {t}) ((make {value})))\n\
(defn make (value u) (fn () (maybe u))\n  where: (u any)\n  (lambda () (maybe u) (maybe.some value)))\n{MAYBE}"
        )
    };
    assert_eq!(returned_len(&closure("void", "void")), 0);
    assert_eq!(returned_len(&closure("i32", "5i32")), 1);
}

#[test]
fn a_type_argument_is_passed_down_through_generic_calls() {
    let source = format!(
        "(defn main () (tuple (maybe void) (maybe i64))\n  (tupleof (outer void) (outer 7i64)))\n\
(defn outer (value t) (maybe t)\n  where: (t any)\n  (middle value))\n\
(defn middle (value u) (maybe u)\n  where: (u any)\n  (wrap value))\n\
(defn wrap (value t) (maybe t)\n  where: (t any)\n  (maybe.some value))\n{MAYBE}"
    );
    agree(&source);
}

#[test]
fn a_closure_runs_at_the_type_arguments_of_the_activation_that_made_it() {
    let source = format!(
        "(defn main () (tuple (maybe void) (maybe i32))\n  (tupleof ((make void)) ((make 5i32))))\n\
(defn make (value t) (fn () (maybe t))\n  where: (t any)\n  (lambda () (maybe t) (maybe.some value)))\n{MAYBE}"
    );
    agree(&source);
}

#[test]
fn a_generic_lambda_binds_its_own_type_arguments_at_each_call() {
    let source = format!(
        "(defn main () (tuple (maybe void) (maybe i32) (maybe str))\n\
  (let wrap (lambda (value t) (maybe t) where: (t any) (maybe.some value)))\n\
  (tupleof (wrap void) (wrap 5i32) (wrap \"s\")))\n{MAYBE}"
    );
    agree(&source);
}

#[test]
fn a_generic_function_used_as_a_value_is_called_at_the_type_it_was_given() {
    let source = format!(
        "(defn main () (tuple (maybe void) (maybe i32))\n\
  (let at-void (wrap-at-void wrap))\n\
  (tupleof (at-void void) (apply-i32 wrap 5i32)))\n\
(defn wrap-at-void (f (fn (void) (maybe void))) (fn (void) (maybe void)) f)\n\
(defn apply-i32 (f (fn (i32) (maybe i32)) value i32) (maybe i32) (f value))\n\
(defn wrap (value t) (maybe t)\n  where: (t any)\n  (maybe.some value))\n{MAYBE}"
    );
    agree(&source);
}

#[test]
fn a_union_widened_in_generic_code_has_the_discriminant_of_its_instantiation() {
    // The risk Step 5b noted: the host reads a union member by its position in
    // the instantiated member list, and generic code attaches the position it
    // knows. They agree for every union a program can write. An anonymous union
    // is canonically ordered, and its members must be concrete, so a member
    // that mentions a type parameter is rejected before any program exists. A
    // declared union may mention one, and it takes its written member order,
    // which substitution does not change.
    let anonymous = "(deftype box (record inner t)\n  where: (t any))\n(defn main () i32 1i32)\n(defn inject (value (box t)) (union (box t) str)\n  where: (t any)\n  value)\n";
    let result = check_source("input.vib", anonymous);
    assert!(!result.accepted(), "{anonymous}");
    assert!(
        result.diagnostics().iter().any(|diagnostic| {
            diagnostic.code()
                == vibra_diagnostics::DiagnosticCode::TypeUnionMemberNotConcrete
        }),
        "{:?}",
        result.diagnostics()
    );
    let declared = "(defn main () (tuple (either i32) (either str) (either str))\n\
  (tupleof (left (box inner: 1i32)) (left (box inner: \"s\")) (right \"t\")))\n\
(deftype box (record inner t)\n  where: (t any))\n\
(deftype either (union (box t) str)\n  where: (t any))\n\
(defn left (value (box t)) (either t)\n  where: (t any)\n  value)\n\
(defn right (value str) (either t)\n  where: (t any)\n  value)\n";
    agree(declared);
    let variant = |value: &str| {
        result_variant(&checked(&format!(
            "(defn main () (either i32) (left (box inner: {value})))\n\
(deftype box (record inner t)\n  where: (t any))\n\
(deftype either (union (box t) str)\n  where: (t any))\n\
(defn left (value (box t)) (either t)\n  where: (t any)\n  value)\n"
        )))
    };
    assert_eq!(variant("1i32"), 0, "the first written member");
}

/// The union variant of the arena object `main` returns.
fn result_variant(program: &CheckedProgram) -> u32 {
    let mut instance = start(program, 64 * MIB);
    let Outcome::Completed { result, .. } = instance.call_entry().expect("runs") else {
        panic!("the entry did not complete");
    };
    let id = instance.result_id(result);
    let variant = instance.variant(&id).expect("an enum or union");
    instance.release(id).expect("release");
    variant
}

#[test]
fn a_closure_captures_a_value_of_a_generic_type_of_either_class() {
    let source = "(defn main () (tuple str i64 f64 bool)\n\
  (tupleof ((keep \"text\")) ((keep 9i64)) ((keep 2.5f64)) ((keep true))))\n\
(defn keep (value t) (fn () t)\n  where: (t any)\n  (lambda () t value))\n";
    agree(source);
}

#[test]
fn a_lambda_inside_a_generic_lambda_sees_both_activations_type_arguments() {
    let source = format!(
        "(defn main () (tuple (maybe void) (maybe i32) (maybe str))\n\
  (let outer (lambda (value t) (fn () (maybe t))\n\
    where: (t any)\n\
    (lambda () (maybe t) (maybe.some value))))\n\
  (tupleof ((outer void)) ((outer 4i32)) ((outer \"s\"))))\n{MAYBE}"
    );
    agree(&source);
}

#[test]
fn a_module_value_holds_a_function_value_that_a_tail_call_enters() {
    let source = "(def seven (fn () i32) (lambda () i32 7i32))\n\
(def pick (fn (i32 i32) i32) (lambda (left i32 right i32) i32 right))\n\
(defn main () (tuple i32 i32) (tupleof (via-value) (seven)))\n\
(defn via-value () i32 (pick 1i32 (seven)))\n";
    agree_on(
        source,
        "(record type: (record type: @tuple arguments: (array @i32 @i32)) value: (record kind: @tuple values: (array 7i32 7i32)))\n",
    );
}

#[test]
fn a_tail_call_through_a_function_value_takes_the_default_of_its_callee() {
    let source = "(defn main () (tuple atom atom) (tupleof (via-value) (via-given)))\n\
(defn via-value () atom\n  (let f level)\n  (f \"a\"))\n\
(defn via-given () atom\n  (let f level)\n  (f \"b\" severity: @error))\n\
(defn level (message str) atom\n  labelled: (severity atom @info)\n  severity)\n";
    agree_on(
        source,
        "(record type: (record type: @tuple arguments: (array @atom @atom)) value: (record kind: @tuple values: (array @info @error)))\n",
    );
}

#[test]
fn a_generic_function_that_wraps_its_own_type_recurses_with_a_new_type_each_time() {
    // Polymorphic recursion: each activation is passed a type that names the
    // one before, and the descriptors are counted and freed with the frames.
    // A generic function whose type argument changes at every call has no closed
    // set of instantiations, which a monomorphizing backend could not emit.
    let source = format!(
        "(defn main () (maybe void) (nest (zero) void))\n\
(defn nest (s state value t) (maybe void)\n  where: (t any)\n  (if (done s) (maybe.some) (nest (inc s) (maybe.some value))))\n{MAYBE}{}",
        counter(5)
    );
    agree(&source);
    assert_balanced_source(&source);
}

/// The live size of a finished run is exactly what the module values keep.
fn assert_balanced_source(source: &str) {
    let program = checked(source);
    let mut instance = start(&program, 64 * MIB);
    for _ in 0..2 {
        let Outcome::Completed { result, .. } = instance.call_entry().expect("runs")
        else {
            panic!("the entry did not complete");
        };
        instance
            .observe(result, &entry_type(&program), program.types())
            .expect("the host reads the result");
        let live = instance.live_size().expect("live size");
        assert_eq!(live, module_values_bytes(&mut instance), "{source}");
    }
}

#[test]
fn a_generic_record_holds_components_of_either_class() {
    let source = "(defn main () (tuple (pair i32 str) (pair str i32) (pair i64 f64))\n\
  (tupleof (pair.of 1i32 \"x\") (pair.swap (pair.of 2i32 \"y\")) (pair.swap (pair.of 2.5f64 3i64))))\n\
(deftype pair (record left a right b)\n  where: (a any b any)\n  (defn of (left a right b) (pair a b) (pair left: left right: right))\n  (defn swap (value self) (pair b a) (pair left: (value @right) right: (value @left))))\n";
    agree(source);
}
