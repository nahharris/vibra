//! Patterns and typed failure (milestone 4 Step 7): `match` with every pattern
//! kind, destructuring, `let-else`, `as` narrowing, `try`, and `never`, in
//! WebAssembly.
//!
//! The reference interpreter is the oracle. A program here is checked from
//! source, run by both backends, and held to one result; an expectation written
//! beside it is authored from the specification's canonical value encoding, not
//! copied from a run. The tests also measure what the runtime chapter asks of
//! reclamation: every path through a `match` (each arm, a failed pattern, a
//! `try` early exit, a `let-else` fallback, a `return`) leaves the frame
//! balanced, so the live size returns to what the module values keep, and an
//! arm, a fallback, or a continuation in tail position replaces its activation.

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

fn runner() -> Runner {
    Runner::new(MemoryLimit::new(64 * MIB)).expect("the engine configures")
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
        Observed::Completed { value, live, .. } => {
            (value.canonical_observation(&ty), live)
        }
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
fn agree_on(source: &str, expected: &str) {
    let (observed, _) = agree(source);
    assert_eq!(
        observed, expected,
        "not the specified encoding for:\n{source}"
    );
}

fn word(instance: &mut Instance, address: u32) -> u32 {
    let bytes = instance
        .read_memory_for_tests(address, 4)
        .expect("the state is in memory");
    u32::from_le_bytes(bytes.try_into().expect("four bytes"))
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

/// What a run left, after the host released the result: the live bytes, what the
/// module values keep, the arena's high-water mark, and the frames left.
struct Footprint {
    /// The live size when the entry completed, while the host held the result.
    with_result: u64,
    live: u64,
    kept: u64,
    arena_used: u32,
    frame_depth: u32,
}

/// Runs the entry once on a fresh instance, reads and releases the result, and
/// measures what is still held.
fn footprint(program: &CheckedProgram) -> Footprint {
    let bytes = emitted(program);
    let Started::Ready(mut instance) = runner().start(&bytes).expect("a v1 module")
    else {
        panic!("the module's own memory exceeds the limit");
    };
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

/// Runs `source` in both backends, asserts they agree and are the specified
/// result, and asserts the frame is balanced: after the host released the
/// result nothing is live but what the module values keep, and no frame is left.
fn balanced_on(source: &str, expected: &str) {
    agree_on(source, expected);
    let after = footprint(&checked(source));
    assert_eq!(
        after.live, after.kept,
        "the live size is not at its baseline for:\n{source}"
    );
    assert_eq!(after.frame_depth, 0, "a frame is left by:\n{source}");
}

// -- the encoding of a result ---------------------------------------------------

/// The canonical encoding of a result observation of type `ty` and value `value`.
fn result(ty: &str, value: &str) -> String {
    format!("(record type: {ty} value: {value})\n")
}

/// The type encoding of an anonymous tuple.
fn tuple_type(components: &[&str]) -> String {
    format!(
        "(record type: @tuple arguments: (array {}))",
        components.join(" ")
    )
}

/// The value encoding of an anonymous tuple.
fn tuple_value(components: &[&str]) -> String {
    format!(
        "(record kind: @tuple values: (array {}))",
        components.join(" ")
    )
}

/// The result of an entry that returns a tuple of `str`.
fn strs(values: &[&str]) -> String {
    let types = vec!["@str"; values.len()];
    let quoted = values
        .iter()
        .map(|v| format!("\"{v}\""))
        .collect::<Vec<_>>();
    let quoted = quoted.iter().map(String::as_str).collect::<Vec<_>>();
    result(&tuple_type(&types), &tuple_value(&quoted))
}

// -- match: every pattern kind --------------------------------------------------------

#[test]
fn the_first_matching_arm_wins() {
    // Both arms cover 3, and the first is the one taken; a failed arm that has
    // tested part of its pattern leaves nothing for the next.
    let source = "(defn main () (tuple str str str)\n\
  (tupleof (pick (tupleof 1i32 2i32)) (pick (tupleof 1i32 3i32)) (pick (tupleof 4i32 4i32))))\n\
(defn pick (value (tuple i32 i32)) str\n\
  (match value\n\
    (tupleof 1i32 3i32) \"first\"\n\
    (tupleof 1i32 -) \"second\"\n\
    - \"third\"))\n";
    balanced_on(source, &strs(&["second", "first", "third"]));
}

#[test]
fn a_failed_arm_is_followed_by_a_covering_arm_and_nothing_leaks() {
    // The text literal of the first arm is built and compared, and the arm fails
    // after it has borrowed the parts of the pair.
    let source = "(defn main () (tuple str str)\n\
  (tupleof (pick (tupleof \"a\" \"x\")) (pick (tupleof \"b\" \"y\"))))\n\
(defn pick (value (tuple str str)) str\n\
  (match value\n\
    (tupleof \"a\" \"z\") \"no\"\n\
    (tupleof \"a\" y) y\n\
    (tupleof x -) x))\n";
    balanced_on(source, &strs(&["x", "b"]));
}

#[test]
fn a_single_arm_match_and_an_empty_payload_variant() {
    let source = "(deftype shape (enum empty void circle i32))\n\
(defn main () (tuple i32 bool bool)\n\
  (tupleof (only (tupleof 4i32 \"x\")) (empty-shape (shape.empty)) (empty-shape (shape.circle 2i32))))\n\
(defn only (value (tuple i32 str)) i32 (match value (tupleof n -) n))\n\
(defn empty-shape (value shape) bool (match value (shape.empty) true (shape.circle -) false))\n";
    balanced_on(
        source,
        &result(
            &tuple_type(&["@i32", "@bool", "@bool"]),
            &tuple_value(&["4i32", "true", "false"]),
        ),
    );
}

// -- destructuring in let, parameters, and lambdas ---------------------------------

#[test]
fn let_parameters_and_lambdas_destructure() {
    let source = "(deftype single (enum only i32))\n\
(defn main () (tuple i32 i32 str i32)\n\
  (let (single.only only) (single.only 9i32))\n\
  (let (tupleof picked -) ((lambda ((recordof a: a) (record a i32 b str)) (tuple i32 bool) (tupleof a true))\n\
                           (recordof a: 5i32 b: \"z\")))\n\
  (let (tupleof first (recordof k: second)) (tupleof \"one\" (recordof k: 2i32)))\n\
  (tupleof only picked first second))\n";
    balanced_on(
        source,
        &result(
            &tuple_type(&["@i32", "@i32", "@str", "@i32"]),
            &tuple_value(&["9i32", "5i32", "\"one\"", "2i32"]),
        ),
    );
}

#[test]
fn a_destructuring_lambda_parameter_holds_what_it_binds() {
    let source = "(defn main () (tuple str i32)\n\
  (let swap (lambda ((tupleof a b) (tuple i32 str)) (tuple str i32) (tupleof b a)))\n\
  (swap (tupleof 3i32 \"three\")))\n";
    balanced_on(
        source,
        &result(
            &tuple_type(&["@str", "@i32"]),
            &tuple_value(&["\"three\"", "3i32"]),
        ),
    );
}

// -- let-else -----------------------------------------------------------------------

#[test]
fn let_else_as_the_final_element_of_a_body() {
    let source = "(defn main () (tuple i32 i32)\n\
  (tupleof (check (option.some 5i32)) (check (option.none))))\n\
(defn check (o (option i32)) i32\n\
  (if (done o) 1i32 2i32))\n\
(defn done (o (option i32)) bool\n\
  (match o (option.some -) true (option.none) false))\n\
(defn unused (o (option i32)) void\n\
  (let-else (option.some -) o (return void)))\n";
    balanced_on(
        source,
        &result(
            &tuple_type(&["@i32", "@i32"]),
            &tuple_value(&["1i32", "2i32"]),
        ),
    );
    // The body that ends in the `let-else` is `void`: it is lowered and balanced.
    balanced_on(
        "(defn main () void (unused (option.some 1i32)) (unused (option.none)))\n\
(defn unused (o (option i32)) void\n  (let-else (option.some -) o (return void)))\n",
        &result("@void", "void"),
    );
}

#[test]
fn a_let_else_with_an_as_pattern_narrows_for_the_rest_of_the_body() {
    let source = "(deftype number (union i32 str))\n\
(defn main () (tuple i32 i32)\n\
  (tupleof (int-or (as number 5i32)) (int-or (as number \"no\"))))\n\
(defn int-or (value number) i32\n\
  (let-else (as i32 n) value (return -1i32))\n\
  n)\n";
    balanced_on(
        source,
        &result(
            &tuple_type(&["@i32", "@i32"]),
            &tuple_value(&["5i32", "-1i32"]),
        ),
    );
}

// -- try ----------------------------------------------------------------------------

#[test]
fn try_leaves_a_lambda_and_not_the_function_around_it() {
    let source = "(defn main () (tuple i32 i32)\n\
  (let run (lambda (o (option i32)) (option i32) (option.some (try o))))\n\
  (tupleof (code (run (option.some 2i32))) (code (run (option.none)))))\n\
(defn code (o (option i32)) i32 (match o (option.some n) n (option.none) -1i32))\n";
    balanced_on(
        source,
        &result(
            &tuple_type(&["@i32", "@i32"]),
            &tuple_value(&["2i32", "-1i32"]),
        ),
    );
}

#[test]
fn try_in_generic_code_with_a_payload_that_may_be_void() {
    // The payload of a generic type has a cell exactly when its type argument is
    // not `void`, so the same code propagates a nullary success, a payload, and
    // an error with and without one.
    let source = "(defn main () (tuple str str str str)\n\
  (tupleof (text (pass (save))) (text (pass (as (result i32 str) (result.err \"bad\"))))\n\
           (text (pass (as (result i32 str) (result.ok 3i32)))) (is-some (direct))))\n\
(defn save () (result void str) (result.ok))\n\
(defn direct () (option void) (option.some))\n\
(defn text (r (result t str)) str\n\
  where: (t any)\n\
  (match r (result.ok -) \"ok\" (result.err message) message))\n\
(defn pass (value (result t e)) (result t e)\n\
  where: (t any e any)\n\
  (result.ok (try value)))\n\
(defn is-some (value (option t)) str\n\
  where: (t any)\n\
  (match value (option.some present) \"some\" (option.none) \"none\"))\n";
    balanced_on(source, &strs(&["ok", "bad", "ok", "some"]));
}

#[test]
fn a_generic_binder_on_a_payload_that_may_be_void_binds_void() {
    let source = "(defn main () (tuple i32 str bool bool)\n\
  (tupleof (unwrap-or (option.some 5i32) 0i32) (unwrap-or (option.none) \"fallback\")\n\
           (kept (direct)) (kept (as (option i32) (option.none)))))\n\
(defn direct () (option void) (option.some))\n\
(defn unwrap-or (value (option t) fallback t) t\n\
  where: (t any)\n\
  (match value (option.some present) present (option.none) fallback))\n\
(defn kept (value (option t)) bool\n\
  where: (t any)\n\
  (match value (option.some present) (is-void present) (option.none) false))\n\
(defn is-void (value t) bool where: (t any) true)\n";
    balanced_on(
        source,
        &result(
            &tuple_type(&["@i32", "@str", "@bool", "@bool"]),
            &tuple_value(&["5i32", "\"fallback\"", "true", "false"]),
        ),
    );
}

// -- the risk Step 5b noted ---------------------------------------------------------

// -- reclamation: every path leaves the frame balanced ------------------------------

#[test]
fn every_arm_of_a_match_over_references_leaves_the_frame_balanced() {
    let source = "(deftype shape (enum empty void circle str rect (tuple str str)))\n\
(defn main () (tuple (tuple str str) (tuple str str) (tuple str str))\n\
  (tupleof (describe (shape.empty)) (describe (shape.circle \"c\"))\n\
           (describe (shape.rect (tupleof \"l\" \"r\")))))\n\
(defn describe (value shape) (tuple str str)\n\
  (match value\n\
    (shape.empty) (tupleof \"e\" \"e\")\n\
    (shape.circle name) (tupleof name name)\n\
    (shape.rect (tupleof a b)) (tupleof b a)))\n";
    let pair =
        |a: &str, b: &str| tuple_value(&[&format!("\"{a}\""), &format!("\"{b}\"")]);
    let pair_type = tuple_type(&["@str", "@str"]);
    balanced_on(
        source,
        &result(
            &tuple_type(&[&pair_type, &pair_type, &pair_type]),
            &tuple_value(&[&pair("e", "e"), &pair("c", "c"), &pair("r", "l")]),
        ),
    );
}

#[test]
fn a_match_as_a_subject_an_operand_and_a_nested_arm_is_balanced() {
    let source = "(defn main () (tuple str str)\n\
  (tupleof\n\
    (match (match (tupleof \"a\" \"b\") (tupleof x -) (option.some x)) (option.some s) s (option.none) \"\")\n\
    (pick (option.some (tupleof \"p\" \"q\")))))\n\
(defn pick (o (option (tuple str str))) str\n\
  (match o\n\
    (option.some (tupleof first second))\n\
      (match second \"q\" first - second)\n\
    (option.none) \"none\"))\n";
    balanced_on(source, &strs(&["a", "p"]));
}

#[test]
fn a_return_inside_an_arm_with_live_bindings_and_temporaries_is_balanced() {
    let source = "(defn main () (tuple str str)\n\
  (tupleof (early (option.some \"k\")) (early (option.none))))\n\
(defn early (o (option str)) str\n\
  (let held (tupleof \"x\" \"y\"))\n\
  (let picked (match o\n\
    (option.some text) (do (let inner (tupleof text text)) (return text))\n\
    (option.none) (held 0)))\n\
  picked)\n";
    // `some` returns its text from inside the arm, with `held`, `inner`, and
    // the binder live; `none` leaves the arm with the first of `held`.
    balanced_on(source, &strs(&["k", "x"]));
}

#[test]
fn a_try_early_exit_drops_the_temporaries_and_bindings_the_frame_holds() {
    let source = "(defn main () (tuple str str)\n\
  (tupleof (text (pair true)) (text (pair false))))\n\
(defn parse (flag bool) (result i32 str)\n\
  (if flag (result.ok 1i32) (result.err \"bad\")))\n\
(defn pair (flag bool) (result (tuple str i32) str)\n\
  (let held (tupleof \"x\" \"y\"))\n\
  (result.ok (tupleof \"p\" (try (parse flag)))))\n\
(defn text (r (result (tuple str i32) str)) str\n\
  (match r (result.ok (tupleof s -)) s (result.err message) message))\n";
    balanced_on(source, &strs(&["p", "bad"]));
}

#[test]
fn a_let_else_fallback_that_returns_leaves_the_frame_balanced() {
    let source = "(defn main () (tuple (tuple str str) (tuple str str))\n\
  (tupleof (f (option.some \"t\")) (f (option.none))))\n\
(defn f (o (option str)) (tuple str str)\n\
  (let held (tupleof \"x\" \"y\"))\n\
  (let-else (option.some text) o (return held))\n\
  (tupleof text text))\n";
    let pair =
        |a: &str, b: &str| tuple_value(&[&format!("\"{a}\""), &format!("\"{b}\"")]);
    let pair_type = tuple_type(&["@str", "@str"]);
    balanced_on(
        source,
        &result(
            &tuple_type(&[&pair_type, &pair_type]),
            &tuple_value(&[&pair("t", "t"), &pair("x", "y")]),
        ),
    );
}

#[test]
fn two_matches_in_one_body_sequence_each_release_what_they_bind() {
    // The binders of each arm are dropped when its result is done.
    let source = "(defn main () (tuple str str)\n\
  (let a (match (tupleof \"a1\" \"a2\") (tupleof x y) (tupleof y x)))\n\
  (let b (match (tupleof \"b1\" \"b2\") (tupleof x y) (tupleof y x)))\n\
  (tupleof (a 0) (b 1)))\n";
    balanced_on(source, &strs(&["a2", "b1"]));
}

// -- tail position: arms, fallbacks, and the rest of a sequence ---------------------------

/// -- a counter ----------------------------------------------------------------

/// The definitions of a counter that counts to `n`: `state`, `(zero)`,
/// `(inc s)`, which makes the next state, and `(done s)`, true exactly when the
/// state is `n`. A state is a record of one `u32`, so every round allocates a
/// fresh record, which the loops below rely on to show that the garbage is
/// freed.
fn counter(n: u32) -> String {
    format!(
        "(deftype state (record n u32))\n\
(defn zero () state (state n: 0u32))\n\
(defn inc (s state) state\n  (state n: (match (u32.add-checked (s @n) 1u32) (result.ok next) next (result.err -) 0u32)))\n\
(defn done (s state) bool (u32.equal (s @n) {n}u32))\n"
    )
}

/// A loop of `n` rounds whose repeating call is in tail position through one of
/// the forms of this step.
#[derive(Clone, Copy, Debug)]
enum Loop {
    /// An arm of a `match`, with a binder that the call must not keep.
    Arm,
    /// The rest of the body sequence after a `let-else`.
    LetElse,
    /// The rest of the body sequence after a `try`.
    Try,
    /// The operand of a `return` in a `let-else` fallback.
    Return,
}

impl Loop {
    const ALL: [Self; 4] = [Self::Arm, Self::LetElse, Self::Try, Self::Return];

    fn source(self, n: u32) -> String {
        let body = match self {
            Self::Arm => {
                "(defn main () i32 (run (zero)))\n\
(defn run (s state) i32\n  (match (done s) true 7i32 false (match (inc s) next (run next))))\n"
            }
            Self::LetElse => {
                "(defn main () i32 (run (zero)))\n\
(defn run (s state) i32\n  (let-else false (done s) (return 7i32))\n  (run (inc s)))\n"
            }
            Self::Try => {
                "(defn main () i32 (code (run (zero))))\n\
(defn code (o (option i32)) i32 (match o (option.some n) n (option.none) 0i32))\n\
(defn step (s state) (option state) (option.some (inc s)))\n\
(defn run (s state) (option i32)\n  (let next (try (step s)))\n  (if (done s) (option.some 7i32) (run next)))\n"
            }
            Self::Return => {
                "(defn main () i32 (run (zero)))\n\
(defn check (s state) (option i32) (option.none))\n\
(defn run (s state) i32\n  (match (done s)\n    true 7i32\n    false (do (let-else (option.some -) (check s) (return (run (inc s)))) 9i32)))\n"
            }
        };
        format!("{body}{}", counter(n))
    }
}

#[test]
fn a_tail_position_through_each_form_ends_where_the_counter_says() {
    for kind in Loop::ALL {
        let (observed, _) = agree(&kind.source(50));
        assert_eq!(observed, result("@i32", "7i32"), "{kind:?}");
    }
}

/// The most the live arena may grow between two iteration counts of a loop, and
/// the most the arena's high-water mark may: constants of the program, which
/// allocates a fresh `state` tuple in every round. A loop that grew with its
/// rounds would grow by megabytes between ten thousand and a hundred thousand.
const LIVE_GROWTH_BOUND: u64 = 64;
const ARENA_GROWTH_BOUND: u32 = 4 * 1024;

#[test]
fn a_loop_through_an_arm_a_let_else_a_try_and_a_return_holds_a_bounded_live_arena() {
    for kind in Loop::ALL {
        let measure = |n| footprint(&checked(&kind.source(n)));
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
        assert_eq!(large.frame_depth, 0, "{kind:?}");
        assert_eq!(large.live, large.kept, "{kind:?}: only the module values");
    }
}

#[test]
fn the_same_loops_run_in_a_few_pages_of_memory() {
    // A hundred thousand rounds under a limit far below what a frame per round
    // would need: each form in tail position replaces its activation.
    for kind in Loop::ALL {
        let program = checked(&kind.source(100_000));
        let bytes = emitted(&program);
        let ty = entry_type(&program);
        let outcome = Runner::new(MemoryLimit::new(256 * 1024))
            .expect("a runner")
            .run_observed(&bytes, &ty, program.types())
            .expect("runs");
        assert!(
            matches!(outcome, Observed::Completed { .. }),
            "{kind:?} under 256 KiB: {outcome:?}"
        );
    }
}

#[test]
fn a_try_operand_is_not_a_tail_position() {
    // The operand of `try` is inspected before the activation continues, so a
    // loop through it is not a tail loop and exhausts a small memory.
    let source = format!(
        "(defn main () i32 (code (run (zero))))\n\
(defn code (o (option i32)) i32 (match o (option.some n) n (option.none) 0i32))\n\
(defn run (s state) (option i32)\n  (if (done s) (option.some 7i32) (do (let n (try (run (inc s)))) (option.some n))))\n{}",
        counter(100_000)
    );
    let program = checked(&source);
    let bytes = emitted(&program);
    let ty = entry_type(&program);
    assert_eq!(
        Runner::new(MemoryLimit::new(256 * 1024))
            .expect("a runner")
            .run_observed(&bytes, &ty, program.types())
            .expect("runs"),
        Observed::Stopped(Outcome::MemoryExhausted)
    );
    balanced_on(
        &source.replace(&counter(100_000), &counter(50)),
        &result("@i32", "7i32"),
    );
}

// -- the array pattern and the text wrapper pattern (Step 8b) ------------------------

#[test]
fn an_array_pattern_and_a_wrapper_pattern_over_text_lower_since_step_8b() {
    // An array pattern is an exact length and its elements in place, and the
    // `str` constructor pattern meets the array the registry makes of the text.
    balanced_on(
        "(defn main () i32 (size (array.of 1i32)))\n\
(defn size (items (array i32)) i32 (match items (array) 0i32 (array -) 1i32 - 2i32))\n",
        &result("@i32", "1i32"),
    );
    balanced_on(
        "(defn main () (tuple i32 i32)\n  (tupleof (size \"ab\") (size \"abc\")))\n\
(defn size (text str) i32 (match text (str (array - -)) 2i32 - 0i32))\n",
        &result(
            "(record type: @tuple arguments: (array @i32 @i32))",
            "(record kind: @tuple values: (array 2i32 0i32))",
        ),
    );
}
