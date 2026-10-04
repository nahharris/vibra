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
use vibra_wasm::{Form, NotLowered};
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
fn literal_patterns_test_a_scalar_a_wide_scalar_a_character_a_text_and_an_atom() {
    let source = "(defn main () (tuple str str str str str str str str str str)\n\
  (tupleof (int 1i32) (int -5i32) (int 7i32) (wide 10000000000u64) (wide 3u64)\n\
           (word \"alpha\") (word \"alphabet\") (word \"\") (letter \\a) (color @green)))\n\
(defn int (n i32) str (match n 1i32 \"one\" -5i32 \"minus five\" - \"other\"))\n\
(defn wide (n u64) str (match n 10000000000u64 \"ten billion\" - \"other\"))\n\
(defn word (s str) str (match s \"alpha\" \"first\" \"\" \"empty\" - \"other\"))\n\
(defn letter (c char) str (match c \\a \"a\" - \"other\"))\n\
(defn color (c atom) str (match c @red \"red\" @green \"green\" - \"other\"))\n";
    balanced_on(
        source,
        &strs(&[
            "one",
            "minus five",
            "other",
            "ten billion",
            "other",
            "first",
            "other",
            "empty",
            "a",
            "green",
        ]),
    );
}

#[test]
fn a_literal_of_every_narrow_integer_type_matches_its_own_value_only() {
    let source = "(defn main () (tuple bool bool bool bool bool bool bool bool)\n\
  (tupleof (a -3i8) (b 40000u16) (c -300i16) (d 200u8) (e 4000000000u32) (a 3i8) (d 7u8) (e 1u32)))\n\
(defn a (n i8) bool (match n -3i8 true - false))\n\
(defn b (n u16) bool (match n 40000u16 true - false))\n\
(defn c (n i16) bool (match n -300i16 true - false))\n\
(defn d (n u8) bool (match n 200u8 true - false))\n\
(defn e (n u32) bool (match n 4000000000u32 true - false))\n";
    balanced_on(
        source,
        &result(
            &tuple_type(&["@bool"; 8]),
            &tuple_value(&[
                "true", "true", "true", "true", "true", "false", "false", "false",
            ]),
        ),
    );
}

#[test]
fn constructor_patterns_match_a_record_a_tuple_an_enum_and_a_wrapper() {
    let source = "(deftype dims (record w i32 h i32))\n\
(deftype pair (tuple i32 str))\n\
(deftype meters i32)\n\
(deftype shape (enum empty void circle i32 rect dims))\n\
(defn main () (tuple i32 i32 i32 i32 str i32)\n\
  (tupleof (describe (shape.empty)) (describe (shape.circle 0i32)) (describe (shape.circle 4i32))\n\
           (describe (shape.rect (dims w: 6i32 h: 7i32)))\n\
           (second (pair 1i32 \"label\")) (unwrap (meters 12i32))))\n\
(defn describe (value shape) i32\n\
  (match value\n\
    (shape.empty) 0i32\n\
    (shape.circle 0i32) 1i32\n\
    (shape.circle radius) radius\n\
    (shape.rect (dims w: width)) width))\n\
(defn second ((pair - label) pair) str label)\n\
(defn unwrap ((meters length) meters) i32 length)\n";
    balanced_on(
        source,
        &result(
            &tuple_type(&["@i32", "@i32", "@i32", "@i32", "@str", "@i32"]),
            &tuple_value(&["0i32", "1i32", "4i32", "6i32", "\"label\"", "12i32"]),
        ),
    );
}

#[test]
fn tuple_recordof_and_enumof_patterns_match_anonymous_values() {
    let source = "(defn main () (tuple atom atom atom atom i32 str str)\n\
  (tupleof (pick (tupleof true true)) (pick (tupleof true false)) (pick (tupleof false true))\n\
           (pick (tupleof false false))\n\
           (a-of (recordof a: 5i32 b: \"z\"))\n\
           (label-of (enumof a: 1i32)) (label-of (enumof b: \"enumof\"))))\n\
(defn pick (flags (tuple bool bool)) atom\n\
  (match flags\n\
    (tupleof true true) @both\n\
    (tupleof true -) @first\n\
    (tupleof - true) @second\n\
    (tupleof false false) @neither))\n\
(defn a-of ((recordof a: a) (record a i32 b str)) i32 a)\n\
(defn label-of (value (enum a i32 b str)) str\n\
  (match value\n\
    (enumof a: -) \"a\"\n\
    (enumof b: text) text))\n";
    balanced_on(
        source,
        &result(
            &tuple_type(&["@atom", "@atom", "@atom", "@atom", "@i32", "@str", "@str"]),
            &tuple_value(&[
                "@both",
                "@first",
                "@second",
                "@neither",
                "5i32",
                "\"a\"",
                "\"enumof\"",
            ]),
        ),
    );
}

#[test]
fn a_constant_pattern_is_the_pattern_of_the_constants_value() {
    let source = "(deftype color (enum red void green void))\n\
(def limit i32 10i32)\n\
(def top i32 limit)\n\
(def favorite color (color.green))\n\
(def origin (tuple i32 i32) (tupleof 0i32 0i32))\n\
(defn main () (tuple str str str str str str str str str)\n\
  (tupleof (by-limit 10i32) (by-limit 9i32) (by-top 10i32) (by-color (color.green))\n\
           (by-color (color.red)) (by-origin (tupleof 0i32 0i32)) (by-origin (tupleof 0i32 1i32))\n\
           (by-flag true) (by-flag false)))\n\
(defn by-limit (n i32) str (match n limit \"limit\" - \"other\"))\n\
(defn by-top (n i32) str (match n top \"top\" - \"other\"))\n\
(defn by-color (value color) str (match value favorite \"favorite\" - \"other\"))\n\
(defn by-origin (point (tuple i32 i32)) str (match point origin \"origin\" - \"elsewhere\"))\n\
(defn by-flag (flag bool) str (match flag true \"yes\" false \"no\"))\n";
    balanced_on(
        source,
        &strs(&[
            "limit",
            "other",
            "top",
            "favorite",
            "other",
            "origin",
            "elsewhere",
            "yes",
            "no",
        ]),
    );
}

#[test]
fn an_as_pattern_narrows_a_union_by_its_discriminant() {
    let source = "(deftype number (union i32 str))\n\
(defn main () (tuple str str str i32)\n\
  (tupleof (render 1i32) (render \"text\") (kind true) (width 7u8)))\n\
(defn render (value number) str\n\
  (match value (as i32 -) \"integer\" (as str text) text))\n\
(defn kind (value (union bool u8)) str\n\
  (match value (as bool true) \"yes\" (as bool false) \"no\" (as u8 -) \"byte\"))\n\
(defn width (value (union bool u8)) i32\n\
  (match value (as u8 8u8) 8i32 (as u8 -) 1i32 (as bool -) 0i32))\n";
    balanced_on(
        source,
        &result(
            &tuple_type(&["@str", "@str", "@str", "@i32"]),
            &tuple_value(&["\"integer\"", "\"text\"", "\"yes\"", "1i32"]),
        ),
    );
}

#[test]
fn a_union_of_many_members_has_an_arm_for_each_discriminant() {
    let members = [
        ("i32", "5i32", "a"),
        ("str", "\"s\"", "b"),
        ("bool", "true", "c"),
        ("u8", "7u8", "d"),
        ("f64", "1.5f64", "e"),
        ("char", "\\x", "f"),
        ("i64", "9i64", "g"),
    ];
    let arms = members
        .iter()
        .map(|(ty, _, name)| format!("(as {ty} -) \"{name}\""))
        .collect::<Vec<_>>()
        .join(" ");
    let calls = members
        .iter()
        .map(|(_, value, _)| format!("(kind {value})"))
        .collect::<Vec<_>>()
        .join(" ");
    let source = format!(
        "(deftype many (union i32 str bool u8 f64 char i64))\n\
(defn main () (tuple str str str str str str str) (tupleof {calls}))\n\
(defn kind (value many) str (match value {arms}))\n"
    );
    balanced_on(&source, &strs(&["a", "b", "c", "d", "e", "f", "g"]));
}

#[test]
fn a_binder_on_an_as_pattern_has_the_member_type() {
    let source = "(deftype number (union i32 str))\n\
(defn main () (tuple i32 str)\n\
  (tupleof (get-int 5i32) (get-text \"five\")))\n\
(defn get-int (value number) i32 (match value (as i32 n) n (as str -) 0i32))\n\
(defn get-text (value number) str (match value (as str s) s (as i32 -) \"\"))\n";
    balanced_on(
        source,
        &result(
            &tuple_type(&["@i32", "@str"]),
            &tuple_value(&["5i32", "\"five\""]),
        ),
    );
}

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

#[test]
fn deeply_nested_destructuring_binds_the_innermost_parts() {
    let source = "(deftype leaf (enum x i32 y str))\n\
(deftype deep (record a (tuple i32 (option (tuple str (record c i32 d leaf))))))\n\
(defn main () (tuple str str str)\n\
  (tupleof (dig (deep a: (tupleof 1i32 (option.some (tupleof \"s\" (recordof c: 2i32 d: (leaf.y \"inner\")))))))\n\
           (dig (deep a: (tupleof 1i32 (option.some (tupleof \"s\" (recordof c: 2i32 d: (leaf.x 3i32)))))))\n\
           (dig (deep a: (tupleof 1i32 (option.none))))))\n\
(defn dig (value deep) str\n\
  (match value\n\
    (deep a: (tupleof - (option.some (tupleof - (recordof d: (leaf.y text)))))) text\n\
    (deep a: (tupleof - (option.some (tupleof tag -)))) tag\n\
    (deep a: -) \"none\"))\n";
    balanced_on(source, &strs(&["inner", "s", "none"]));
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
fn let_else_binds_on_a_match_and_runs_its_fallback_otherwise() {
    let source = "(defn main () (tuple i32 i32)\n\
  (tupleof (pick (option.some 5i32)) (pick (option.none))))\n\
(defn pick (o (option i32)) i32\n\
  (let-else (option.some v) o (return 100i32))\n\
  v)\n";
    balanced_on(
        source,
        &result(
            &tuple_type(&["@i32", "@i32"]),
            &tuple_value(&["5i32", "100i32"]),
        ),
    );
}

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

#[test]
fn a_fallback_that_calls_a_function_of_type_never() {
    // `fail` never completes, so the fallback has no value and no slot; the
    // program only takes the path that does not reach it.
    let source = "(defn main () (tuple i32 str)\n\
  (tupleof (require (option.some 4i32)) (describe true)))\n\
(defn fail (message str) never (fail message))\n\
(defn require (o (option i32)) i32\n\
  (let-else (option.some n) o (fail \"missing\"))\n\
  n)\n\
(defn describe (flag bool) str (if flag \"yes\" (fail \"no\")))\n";
    balanced_on(
        source,
        &result(
            &tuple_type(&["@i32", "@str"]),
            &tuple_value(&["4i32", "\"yes\""]),
        ),
    );
}

// -- try ----------------------------------------------------------------------------

#[test]
fn try_continues_with_a_payload_or_leaves_with_the_variant() {
    let source = "(defn main () (tuple i32 i32 str str)\n\
  (tupleof (code (first (option.some 3i32))) (code (first (option.none)))\n\
           (text (checked true)) (text (checked false))))\n\
(defn first (o (option i32)) (option i32) (option.some (try o)))\n\
(defn code (o (option i32)) i32 (match o (option.some n) n (option.none) -1i32))\n\
(defn parse (flag bool) (result i32 str)\n\
  (if flag (result.ok 1i32) (result.err \"bad flag\")))\n\
(defn checked (flag bool) (result i32 str)\n\
  (let value (try (parse flag)))\n\
  (result.ok value))\n\
(defn text (r (result i32 str)) str (match r (result.ok -) \"ok\" (result.err message) message))\n";
    balanced_on(
        source,
        &result(
            &tuple_type(&["@i32", "@i32", "@str", "@str"]),
            &tuple_value(&["3i32", "-1i32", "\"ok\"", "\"bad flag\""]),
        ),
    );
}

#[test]
fn try_changes_the_success_type_and_keeps_the_error() {
    let source = "(defn main () (tuple str str)\n\
  (tupleof (text (widen true)) (text (widen false))))\n\
(defn parse (flag bool) (result i32 str)\n\
  (if flag (result.ok 7i32) (result.err \"e\")))\n\
(defn widen (flag bool) (result str str)\n\
  (let n (try (parse flag)))\n\
  (result.ok \"parsed\"))\n\
(defn text (r (result str str)) str (match r (result.ok s) s (result.err message) message))\n";
    balanced_on(source, &strs(&["parsed", "e"]));
}

#[test]
fn nested_try_leaves_at_the_first_failure() {
    let source = "(defn main () (tuple i32 i32 i32)\n\
  (tupleof (code (flatten (option.some (option.some 7i32)))) (code (flatten (option.some (option.none))))\n\
           (code (flatten (option.none)))))\n\
(defn flatten (nested (option (option i32))) (option i32) (option.some (try (try nested))))\n\
(defn code (o (option i32)) i32 (match o (option.some n) n (option.none) -1i32))\n";
    balanced_on(
        source,
        &result(
            &tuple_type(&["@i32", "@i32", "@i32"]),
            &tuple_value(&["7i32", "-1i32", "-1i32"]),
        ),
    );
}

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

#[test]
fn an_atom_singleton_widened_into_a_union_with_an_atom_member() {
    // The checker admits no chain of widenings, so a singleton reaches the
    // union's `atom` member only after it has widened to `atom` (which the
    // runtime erases) and the result is then widened into the union. The
    // interpreter records the static type of the operand of the widening, the
    // Wasm backend reads the union's member list at the discriminant, and both
    // must say `atom`; `as` over the union then narrows to the atom, whatever
    // atom it is.
    let source = "(deftype tag (union atom i32))\n\
(defn main () (tuple tag tag str str str str)\n\
  (tupleof (as atom @ok) 5i32 (name (as atom @ok)) (name 3i32) (is-ok (as atom @ok)) (is-ok (as atom @err))))\n\
(defn name (value tag) str (match value (as atom -) \"atom\" (as i32 -) \"int\"))\n\
(defn is-ok (value tag) str\n\
  (match value (as atom @ok) \"ok\" (as atom -) \"other atom\" (as i32 -) \"int\"))\n";
    let program = checked(source);
    let in_interpreter = oracle(&program);
    let (in_wasm, _) = wasm(&program);
    assert_eq!(
        in_wasm, in_interpreter,
        "the backends disagree on:\n{source}"
    );
    let ty = tuple_type(&["@tag", "@tag", "@str", "@str", "@str", "@str"]);
    let value = tuple_value(&[
        "(record kind: @union type: @tag member: @atom value: @ok)",
        "(record kind: @union type: @tag member: @i32 value: 5i32)",
        "\"atom\"",
        "\"int\"",
        "\"ok\"",
        "\"other atom\"",
    ]);
    assert_eq!(in_wasm, result(&ty, &value));
}

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
/// `state`, `(zero)`, `(inc s)`, and `(done s)`, true exactly when the state is
/// `n`. The language has no arithmetic until Step 8a; a counter needs only `if`,
/// a record, and a call, so it drives a loop of any length.
fn counter(n: u32) -> String {
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

// -- what stays for Step 8b ---------------------------------------------------------

fn not_lowered(source: &str) -> NotLowered {
    vibra_wasm::emit(&checked(source)).expect_err("the program is not lowered")
}

#[test]
fn an_array_pattern_is_not_lowered_until_step_8b() {
    let source = "(defn main () i32 (size (array.of 1i32)))\n\
(defn size (items (array i32)) i32 (match items (array) 0i32 (array -) 1i32 - 2i32))\n";
    let forms = not_lowered(source);
    assert!(
        forms
            .forms()
            .iter()
            .any(|used| used.form() == Form::Array && used.detail() == Some("pattern")),
        "{forms}"
    );
    // A pattern kind of this step is no longer reported.
    assert!(
        forms
            .forms()
            .iter()
            .all(|used| used.form().name() != "match" && used.form().name() != "try"),
        "{forms}"
    );
}

#[test]
fn a_wrapper_pattern_over_text_is_not_lowered_until_step_8b() {
    // A string literal is the wrapper over its scalars, so the `str` constructor
    // pattern over an array of characters is a wrapper pattern over text.
    let source = "(defn main () i32 (size \"ab\"))\n\
(defn size (text str) i32 (match text (str (array - -)) 2i32 - 0i32))\n";
    let forms = not_lowered(source);
    assert!(
        forms.forms().iter().any(|used| used.form() == Form::Wrap),
        "{forms}"
    );
}
