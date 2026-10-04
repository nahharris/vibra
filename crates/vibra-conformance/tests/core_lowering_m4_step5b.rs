//! The core lowering (milestone 4 Step 5b): checked source programs run in
//! WebAssembly, and the host reads what they return.
//!
//! The reference interpreter is the oracle. A program here is checked from
//! source (or built from IR constructors when no source says what is needed),
//! run by both backends, and held to one result; an expectation written beside
//! it is authored from the specification's canonical value encoding, not copied
//! from a run. The tests also measure what the specification asks of the
//! activations and of reclamation: no engine stack per language call, a depth
//! bounded only by memory, and a live size that returns to where it started.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use std::collections::BTreeSet;

use vibra_diagnostics::ByteSpan;
use vibra_ir::{
    CheckedFunction, CheckedProgram, Expr, FunctionSignature, SourceOrigin, Type,
    TypeBody, TypeDefinition, TypeId,
};
use vibra_types::check_source;
use vibra_wasm::layout::{self, header, state};
use vibra_wasm_run::{
    Instance, LiveSizes, MemoryLimit, Observed, Outcome, Runner, Started, ValueId,
};

const MIB: usize = 1024 * 1024;

// -- the harness --------------------------------------------------------------

fn origin() -> SourceOrigin {
    SourceOrigin::new("input.vib", ByteSpan::new(0, 1))
}

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
fn agree_on(source: &str, expected: &str) -> LiveSizes {
    let (observed, live) = agree(source);
    assert_eq!(observed, expected, "not the specified encoding for:\n{source}");
    live
}

/// An instance that has run its entry once and whose result is released.
fn finished(program: &CheckedProgram) -> Instance {
    let bytes = emitted(program);
    let Started::Ready(mut instance) = runner().start(&bytes).expect("a v1 module")
    else {
        panic!("the module's own memory exceeds the limit");
    };
    run_and_release(&mut instance, program);
    instance
}

fn run_and_release(instance: &mut Instance, program: &CheckedProgram) {
    let Outcome::Completed { result, .. } = instance.call_entry().expect("runs") else {
        panic!("the entry did not complete");
    };
    instance
        .observe(result, &entry_type(program), program.types())
        .expect("the host reads the result");
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

/// Asserts the live size of a finished run is only what the module values keep,
/// on a first and a second run of the same instance.
fn assert_balanced(source: &str) {
    let program = checked(source);
    let bytes = emitted(&program);
    let Started::Ready(mut instance) = runner().start(&bytes).expect("a v1 module")
    else {
        panic!("the module's own memory exceeds the limit");
    };
    let start = instance.live_size().expect("live size");
    assert_eq!(start, 0, "a fresh instance holds nothing");
    for run in 1..=2 {
        run_and_release(&mut instance, &program);
        let live = instance.live_size().expect("live size");
        let kept = module_values_bytes(&mut instance);
        assert_eq!(
            live, kept,
            "run {run}: the live size is exactly what the module values keep:\n{source}"
        );
        assert_eq!(word(&mut instance, state::FRAME_TOP), 0, "no frame is left");
        assert_eq!(word(&mut instance, state::FRAME_DEPTH), 0, "no activation");
    }
}

fn result_variant(program: &CheckedProgram) -> u32 {
    let bytes = emitted(program);
    let Started::Ready(mut instance) = runner().start(&bytes).expect("a v1 module")
    else {
        panic!("the module's own memory exceeds the limit");
    };
    let Outcome::Completed { result, .. } = instance.call_entry().expect("runs") else {
        panic!("the entry did not complete");
    };
    let id: ValueId = instance.result_id(result);
    let variant = instance.variant(&id).expect("an enum or union");
    instance.release(id).expect("release");
    variant
}

// -- positive: every kind constructed, projected, and observed -------------------

#[test]
fn a_declared_record_is_built_and_observed_in_declaration_order() {
    agree_on(
        "(deftype point (record x i32 y i32))\n(defn main () point (point x: 1i32 y: 2i32))\n",
        "(record type: @point value: (record kind: @record type: @point fields: (record x: 1i32 y: 2i32)))\n",
    );
}

#[test]
fn an_anonymous_record_stores_its_fields_in_canonical_order() {
    // Written order is `name`, `id`; the canonical order is `id`, `name`, and
    // the operands still evaluate in the written order.
    agree_on(
        "(defn main () (record name str id u64) (recordof name: \"Ada\" id: 7u64))\n",
        "(record type: (record type: @record fields: (record id: @u64 name: @str)) value: (record kind: @record fields: (record id: 7u64 name: \"Ada\")))\n",
    );
}

#[test]
fn a_record_projection_is_a_direct_component_read() {
    agree_on(
        "(deftype point (record x i32 y i32))\n(defn main () (tuple i32 i32) (let p (point x: 1i32 y: 2i32)) (tupleof (p @y) (p @x)))\n",
        "(record type: (record type: @tuple arguments: (array @i32 @i32)) value: (record kind: @tuple values: (array 2i32 1i32)))\n",
    );
}

#[test]
fn tuples_declared_and_anonymous_are_built_and_projected() {
    agree_on(
        "(deftype pair (tuple i32 str))\n(defn main () (tuple pair (tuple bool char)) (tupleof (pair 1i32 \"x\") (tupleof true \\z)))\n",
        "(record type: (record type: @tuple arguments: (array @pair (record type: @tuple arguments: (array @bool @char)))) value: (record kind: @tuple values: (array (record kind: @tuple type: @pair values: (array 1i32 \"x\")) (record kind: @tuple values: (array true \\z)))))\n",
    );
    agree(
        "(deftype pair (tuple i32 str))\n(defn main () str ((pair 1i32 \"x\") 1))\n",
    );
}

#[test]
fn an_enum_with_only_void_payloads_has_discriminants_in_declaration_order() {
    let source = "(deftype color (enum red void green void blue void))\n(defn main () (tuple color color color) (tupleof (color.red) (color.green) (color.blue)))\n";
    agree_on(
        source,
        "(record type: (record type: @tuple arguments: (array @color @color @color)) value: (record kind: @tuple values: (array (record kind: @enum type: @color variant: @red) (record kind: @enum type: @color variant: @green) (record kind: @enum type: @color variant: @blue))))\n",
    );
    for (name, discriminant) in [("red", 0), ("green", 1), ("blue", 2)] {
        let program = checked(&format!(
            "(deftype color (enum red void green void blue void))\n(defn main () color (color.{name}))\n"
        ));
        assert_eq!(result_variant(&program), discriminant, "{name}");
    }
}

#[test]
fn an_anonymous_enum_orders_its_variants_canonically() {
    // Written `some` then `none`; the canonical order is by name, so `none` is
    // discriminant 0 and `some` is 1, whichever order the type was written in.
    let program = checked("(defn main () (enum some i32 none void) (enumof some: 4i32))\n");
    assert_eq!(result_variant(&program), 1);
    let none = checked("(defn main () (enum some i32 none void) (enumof none: void))\n");
    assert_eq!(result_variant(&none), 0);
    agree("(defn main () (enum some i32 none void) (enumof none: void))\n");
}

#[test]
fn an_enum_payload_of_every_class_round_trips() {
    agree_on(
        "(deftype value (enum small i8 wide i64 single f32 double f64 text str flag bool none void))\n\
(defn main () (tuple value value value value value value value)\n\
  (tupleof (value.small -5i8) (value.wide -9i64) (value.single 1.5f32) (value.double 2.25f64) (value.text \"héllo\") (value.flag true) (value.none)))\n",
        "(record type: (record type: @tuple arguments: (array @value @value @value @value @value @value @value)) value: (record kind: @tuple values: (array (record kind: @enum type: @value variant: @small payload: -5i8) (record kind: @enum type: @value variant: @wide payload: -9i64) (record kind: @enum type: @value variant: @single payload: 1.5f32) (record kind: @enum type: @value variant: @double payload: 2.25f64) (record kind: @enum type: @value variant: @text payload: \"héllo\") (record kind: @enum type: @value variant: @flag payload: true) (record kind: @enum type: @value variant: @none))))\n",
    );
}

#[test]
fn a_wrapper_and_a_wrapper_of_a_wrapper_keep_their_identities() {
    agree_on(
        "(deftype celsius f64)\n(deftype reading celsius)\n(defn main () reading (reading (celsius 21.5f64)))\n",
        "(record type: @reading value: (record kind: @wrapper type: @reading value: (record kind: @wrapper type: @celsius value: 21.5f64)))\n",
    );
}

#[test]
fn a_union_member_is_injected_with_its_written_position() {
    // Declared members `str` then `i32`: the discriminant is the written
    // position, so an `i32` is 1 and a `str` is 0.
    let source = |value: &str| {
        format!("(deftype number (union str i32))\n(defn main () number {value})\n")
    };
    agree_on(
        &source("5i32"),
        "(record type: @number value: (record kind: @union type: @number member: @i32 value: 5i32))\n",
    );
    agree_on(
        &source("\"five\""),
        "(record type: @number value: (record kind: @union type: @number member: @str value: \"five\"))\n",
    );
    assert_eq!(result_variant(&checked(&source("5i32"))), 1);
    assert_eq!(result_variant(&checked(&source("\"five\""))), 0);
}

#[test]
fn an_anonymous_union_takes_the_canonical_member_order() {
    // Written `str i32`, canonical `i32 str`: the discriminant of an `i32` is
    // the canonical position and not the written one.
    let canonical = vibra_ir::canonical_union(vec![Type::Str, Type::I32]);
    let i32_position = canonical
        .iter()
        .position(|member| *member == Type::I32)
        .expect("a member");
    let program = checked("(defn main () (union str i32) 5i32)\n");
    assert_eq!(
        usize::try_from(result_variant(&program)).expect("small"),
        i32_position
    );
    assert_eq!(i32_position, 0, "the canonical order puts `i32` first");
    agree("(defn main () (union str i32) 5i32)\n");
    agree("(defn main () (union str i32) \"s\")\n");
}

#[test]
fn a_union_of_many_members_has_one_discriminant_each() {
    let members = ["i32", "str", "bool", "u8", "f64", "char", "i64"];
    let values = ["1i32", "\"s\"", "true", "2u8", "3.5f64", "\\c", "4i64"];
    for (position, (member, value)) in members.iter().zip(values).enumerate() {
        let source = format!(
            "(deftype many (union i32 str bool u8 f64 char i64))\n(defn main () many {value})\n"
        );
        let program = checked(&source);
        assert_eq!(
            usize::try_from(result_variant(&program)).expect("small"),
            position,
            "{member}"
        );
        agree(&source);
    }
}

#[test]
fn a_union_of_two_members_holds_a_compound_member() {
    agree(
        "(deftype pair (tuple i32 str))\n(deftype either (union pair bool))\n(defn main () (tuple either either) (tupleof (pair 1i32 \"a\") false))\n",
    );
}

#[test]
fn aggregates_nest_through_every_kind() {
    agree(
        "(deftype celsius f64)\n\
(deftype color (enum red void green celsius))\n\
(deftype cell (record temp celsius pick color pair (tuple color (record n i32 s str))))\n\
(defn main () cell\n\
  (cell temp: (celsius 1.5f64) pick: (color.green (celsius 3.0f64)) pair: (tupleof (color.red) (recordof s: \"deep\" n: 9i32))))\n",
    );
}

#[test]
fn a_record_holds_another_through_an_option() {
    // A record cannot contain itself except through an array (Step 8b), so the
    // option holds a record of another type.
    agree_on(
        "(deftype leaf (record value i32))\n(deftype node (record value i32 next (option leaf)))\n\
(defn main () (tuple node node) (tupleof (node value: 1i32 next: (option.some (leaf value: 2i32))) (node value: 3i32 next: (option.none))))\n",
        "(record type: (record type: @tuple arguments: (array @node @node)) value: (record kind: @tuple values: (array (record kind: @record type: @node fields: (record value: 1i32 next: (record kind: @enum type: @std.option.option variant: @some payload: (record kind: @record type: @leaf fields: (record value: 2i32))))) (record kind: @record type: @node fields: (record value: 3i32 next: (record kind: @enum type: @std.option.option variant: @none))))))\n",
    );
}

#[test]
fn a_single_component_tuple_and_a_void_wrapper_lower() {
    agree("(deftype solo (tuple i32))\n(defn main () solo (solo 7i32))\n");
    agree("(deftype nothing void)\n(defn main () nothing (nothing void))\n");
}

#[test]
fn every_scalar_class_survives_a_trip_through_an_object() {
    agree(
        "(defn main () (tuple i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char void)\n\
  (tupleof -1i8 -2i16 -3i32 -4i64 5u8 6u16 7u32 8u64 0.5f32 0.25f64 \\q void))\n",
    );
}

// -- positive: control, bindings, and calls --------------------------------------

#[test]
fn let_if_and_a_sequence_choose_and_bind() {
    agree_on(
        "(defn main () (tuple i32 i32) (tupleof (pick true) (pick false)))\n\
(defn pick (flag bool) i32\n  (let first 1i32)\n  (let - (do 9i32 8i32))\n  (if flag (do first) 2i32))\n",
        "(record type: (record type: @tuple arguments: (array @i32 @i32)) value: (record kind: @tuple values: (array 1i32 2i32)))\n",
    );
}

#[test]
fn a_function_that_returns_early_skips_the_rest() {
    agree_on(
        "(defn main () (tuple i32 i32) (tupleof (early true) (early false)))\n\
(defn early (flag bool) i32 (if flag (return 1i32) void) 2i32)\n",
        "(record type: (record type: @tuple arguments: (array @i32 @i32)) value: (record kind: @tuple values: (array 1i32 2i32)))\n",
    );
}

#[test]
fn a_return_with_live_temporaries_and_bindings_drops_them() {
    // The `str` is bound and a tuple is half built when the return leaves.
    let source = "(defn main () (tuple i32) (tupleof (early true)))\n\
(defn early (flag bool) i32 (let held (tupleof \"c\" \"d\")) (let half (tupleof \"e\" (do (if flag (return 3i32) void) \"f\"))) 4i32)\n";
    agree(source);
    assert_balanced(source);
}

#[test]
fn a_call_moves_operands_of_every_class_into_the_callee() {
    agree_on(
        "(defn main () (tuple i32 i64 f32 f64 str bool char u8)\n\
  (tupleof (i32-id 1i32) (i64-id 2i64) (f32-id 3.5f32) (f64-id 4.5f64) (str-id \"s\") (bool-id true) (char-id \\c) (u8-id 255u8)))\n\
(defn i32-id (value i32) i32 value)\n(defn i64-id (value i64) i64 value)\n(defn f32-id (value f32) f32 value)\n\
(defn f64-id (value f64) f64 value)\n(defn str-id (value str) str value)\n(defn bool-id (value bool) bool value)\n\
(defn char-id (value char) char value)\n(defn u8-id (value u8) u8 value)\n",
        "(record type: (record type: @tuple arguments: (array @i32 @i64 @f32 @f64 @str @bool @char @u8)) value: (record kind: @tuple values: (array 1i32 2i64 3.5f32 4.5f64 \"s\" true \\c 255u8)))\n",
    );
}

#[test]
fn operands_evaluate_left_to_right_and_a_labelled_operand_has_its_slot() {
    agree_on(
        "(defn main () (tuple str str i32)\n  (tupleof (join (left) (right)) (join (right) (left)) (scale 4i32 factor: 3i32)))\n\
(defn left () str \"l\")\n(defn right () str \"r\")\n\
(defn join (a str b str) str (let r (str-pair a b)) r)\n\
(defn str-pair (a str b str) str a)\n\
(defn scale (value i32) i32 labelled: (factor i32 1i32) value)\n",
        "(record type: (record type: @tuple arguments: (array @str @str @i32)) value: (record kind: @tuple values: (array \"l\" \"r\" 4i32)))\n",
    );
}

#[test]
fn nested_calls_pass_results_as_operands() {
    agree(
        "(defn main () (tuple i32 i32) (tupleof (outer (inner 1i32) (inner 2i32)) (inner (inner 3i32))))\n\
(defn outer (a i32 b i32) i32 b)\n(defn inner (value i32) i32 value)\n",
    );
}

#[test]
fn mutual_recursion_that_is_not_a_tail_call_returns_through_every_frame() {
    // `main`'s call would be in tail position, which is Step 6's, so it is an
    // operand of a tuple.
    let program = checked(
        "(defn main () (tuple i32) (tupleof (a false)))\n\
(defn a (stop bool) i32 (if stop 1i32 (do (let inner (b true)) inner)))\n\
(defn b (stop bool) i32 (do (let inner (a stop)) inner))\n",
    );
    let expected = oracle(&program);
    assert_eq!(wasm(&program).0, expected);
    assert_balanced(
        "(defn main () (tuple i32) (tupleof (a false)))\n\
(defn a (stop bool) i32 (if stop 1i32 (do (let inner (b true)) inner)))\n\
(defn b (stop bool) i32 (do (let inner (a stop)) inner))\n",
    );
}

// -- positive: module values -------------------------------------------------------

#[test]
fn module_values_read_in_forward_order_and_through_each_other() {
    agree_on(
        "(def second i32 first)\n(def first i32 41i32)\n(def third (tuple i32 i32) (tupleof second first))\n\
(defn main () (tuple (tuple i32 i32) i32) (tupleof third second))\n",
        "(record type: (record type: @tuple arguments: (array (record type: @tuple arguments: (array @i32 @i32)) @i32)) value: (record kind: @tuple values: (array (record kind: @tuple values: (array 41i32 41i32)) 41i32)))\n",
    );
}

#[test]
fn a_module_value_whose_initializer_calls_a_function_is_read() {
    agree(
        "(deftype point (record x i32 y i32))\n(def origin point (make))\n\
(defn make () point (point x: 1i32 y: 2i32))\n(defn main () point origin)\n",
    );
}

#[test]
fn a_module_value_is_evaluated_once_so_a_second_read_shares_it() {
    // Two reads of one module value hold one object; two module values with
    // the same initializer hold two. The difference in the live size of the
    // finished result is exactly the object the second evaluation would build.
    let shared = "(deftype box (record text str))\n(def one box (box text: \"payload\"))\n\
(defn main () (tuple box box) (tupleof one one))\n";
    let twice = "(deftype box (record text str))\n(def one box (box text: \"payload\"))\n(def other box (box text: \"payload\"))\n\
(defn main () (tuple box box) (tupleof one other))\n";
    let (shared_result, shared_live) = agree(shared);
    let (twice_result, twice_live) = agree(twice);
    assert_eq!(shared_result, twice_result, "the same observation");
    // A box of one `str` reference is 24 + 8 + 8 = 40 bytes, class 64, and a
    // `str` of 7 characters is 24 + 28 = 52 bytes, class 64. Both programs'
    // tables fit the same 128-byte class.
    assert_eq!(
        twice_live.with_result - shared_live.with_result,
        64 + 64,
        "the second evaluation builds one more box and its string"
    );
    assert_balanced(shared);
    assert_balanced(twice);
}

#[test]
fn a_module_value_is_not_evaluated_unless_it_is_read() {
    let program = checked("(def unused i32 5i32)\n(defn main () i32 1i32)\n");
    let instance_kept = {
        let mut instance = finished(&program);
        module_values_bytes(&mut instance)
    };
    // Only the table: no value was cached, so no flag is set.
    assert_eq!(
        instance_kept, 128,
        "only the table of three values, two cells each, in its 128-byte class"
    );
    agree("(def unused i32 5i32)\n(defn main () i32 1i32)\n");
}

// -- balance and reclamation -------------------------------------------------------

#[test]
fn programs_of_every_kind_return_the_live_size_to_what_the_module_values_keep() {
    for source in [
        "(deftype point (record x i32 y i32))\n(defn main () point (point x: 1i32 y: 2i32))\n",
        "(deftype leaf (record value i32))\n(deftype node (record value i32 next (option leaf)))\n(defn main () node (node value: 1i32 next: (option.some (leaf value: 2i32))))\n",
        "(deftype number (union str i32))\n(defn main () (tuple number number) (tupleof 1i32 \"two\"))\n",
        "(defn main () (tuple str str) (let a \"a\") (let b (tupleof a a)) (tupleof a (b 0)))\n",
        "(defn main () i32 (let a (tupleof \"a\" \"b\")) (let - (a 0)) (if true 1i32 2i32))\n",
        "(def shared (tuple str str) (tupleof \"x\" \"y\"))\n(defn main () (tuple (tuple str str) (tuple str str)) (tupleof shared shared))\n",
        "(defn main () (tuple i32 i32) (tupleof (f 1i32) (f 2i32)))\n(defn f (value i32) i32 (let keep (tupleof \"k\" value)) value)\n",
    ] {
        assert_balanced(source);
    }
}

#[test]
fn the_host_holds_exactly_the_result_until_it_releases_it() {
    let (_, live) = agree("(deftype point (record x str y str))\n(defn main () point (point x: \"a\" y: \"b\"))\n");
    assert_eq!(live.start, 0);
    assert!(live.with_result > live.end, "{live:?}");
    assert!(live.end > 0, "the module values the program keeps");
}

// -- negative: what is still not lowered ----------------------------------------

#[test]
fn a_tail_call_is_not_lowered_as_a_call_that_grows_the_frame_stack() {
    let program = checked("(defn main () i32 (leaf))\n(defn leaf () i32 7i32)\n");
    let error = vibra_wasm::emit(&program).expect_err("a tail call is Step 6's");
    let kinds = error
        .forms()
        .iter()
        .filter(|used| used.form().name() == "call")
        .map(|used| used.detail())
        .collect::<Vec<_>>();
    assert_eq!(kinds, [Some("tail-direct")]);
}

#[test]
fn the_forms_of_later_steps_are_named_and_never_approximated() {
    for (source, form) in [
        ("(defn main () i32 ((lambda () i32 7i32)))\n", "closure"),
        ("(defn main () i32 (match 1i32 1i32 2i32 - 3i32))\n", "match"),
        ("(defn main () (array i32) (array.of 1i32))\n", "type"),
    ] {
        let program = checked(source);
        let error = vibra_wasm::emit(&program).expect_err("not lowered");
        assert!(
            error.forms().iter().any(|used| used.form().name() == form),
            "{source}: {error}"
        );
    }
}

// -- determinism -------------------------------------------------------------------

#[test]
fn emission_of_a_checked_program_is_byte_identical() {
    let source = "(deftype point (record x i32 y i32))\n(def origin point (point x: 0i32 y: 0i32))\n\
(defn main () (tuple point point) (tupleof origin (shift origin)))\n(defn shift (p point) point (point x: (p @y) y: (p @x)))\n";
    let first = emitted(&checked(source));
    let second = emitted(&checked(source));
    assert_eq!(first, second);
    assert_eq!(emitted(&checked(source)), first, "an independent check");
}

// -- depth: activations live in the arena -----------------------------------------

fn integer_chain(depth: usize) -> CheckedProgram {
    // `f0 = some(chain(f1()))`, ..., `f(n-1) = none`: each call is an operand,
    // so none is a tail call, and the nesting of the value is as deep as the
    // recursion.
    let chain = TypeId::new("chain", "chain");
    let maybe = TypeId::new("maybe", "maybe");
    let types = vec![
        TypeDefinition::new(
            chain.clone(),
            TypeBody::Record(vec![("next".to_owned(), Type::Declared(maybe.clone()))]),
        ),
        TypeDefinition::new(
            maybe.clone(),
            TypeBody::Enum(vec![
                ("none".to_owned(), Type::Void),
                ("some".to_owned(), Type::Declared(chain.clone())),
            ]),
        ),
    ];
    let maybe_type = Type::Declared(maybe);
    let chain_type = Type::Declared(chain);
    let mut functions = Vec::with_capacity(depth);
    for position in 0..depth {
        let body = if position + 1 == depth {
            Expr::Variant {
                value_type: maybe_type.clone(),
                variant: "none".to_owned(),
                payload: None,
                origin: origin(),
            }
        } else {
            let next = Expr::call(position + 1, Vec::new(), maybe_type.clone(), origin());
            Expr::Variant {
                value_type: maybe_type.clone(),
                variant: "some".to_owned(),
                payload: Some(Box::new(Expr::Record {
                    value_type: chain_type.clone(),
                    fields: vec![("next".to_owned(), next)],
                    origin: origin(),
                })),
                origin: origin(),
            }
        };
        functions.push(
            CheckedFunction::new(
                format!("link-{position}"),
                FunctionSignature::new(Vec::new(), maybe_type.clone()),
                body,
                origin(),
            )
            .expect("a checked function"),
        );
    }
    CheckedProgram::try_new_with_types(types, Vec::new(), functions, 0)
        .expect("a checked program")
}

#[test]
fn a_chain_of_calls_agrees_with_the_interpreter_at_a_small_depth() {
    let program = integer_chain(50);
    assert_eq!(wasm(&program).0, oracle(&program));
}

#[test]
fn non_tail_recursion_five_thousand_deep_runs_on_a_small_engine_stack() {
    // The checker's validation of a program is quadratic in its function
    // count, so a chain of functions stays at five thousand; the exhaustion
    // test below reaches millions of activations from one function.
    let depth = 5_000;
    let program = integer_chain(depth);
    let bytes = emitted(&program);
    // A 64 KiB engine stack: a nested Wasm call per activation would not fit a
    // few thousand, so the frames are in the arena.
    let small = Runner::with_wasm_stack(MemoryLimit::new(64 * MIB), 64 * 1024)
        .expect("a runner with a small engine stack");
    let ty = entry_type(&program);
    let Observed::Completed { value, live } = small
        .run_observed(&bytes, &ty, program.types())
        .expect("the module runs")
    else {
        panic!("the recursion did not complete");
    };
    let text = value.canonical_observation(&ty);
    assert_eq!(text.matches("variant: @some").count(), depth - 1);
    assert_eq!(text.matches("variant: @none").count(), 1);
    assert_eq!(live.start, 0);
    assert_eq!(live.end, 0, "the nested value is released and no frame remains");
    drop(value);
    println!(
        "depth {depth}: live with the result {} bytes, after release {} bytes",
        live.with_result, live.end
    );
}

#[test]
fn depth_is_bounded_only_by_memory() {
    // The same recursion completes under the limit that holds it and is the
    // host event, and not a trap, under a limit half that size.
    let depth = 2_000;
    let program = integer_chain(depth);
    let bytes = emitted(&program);
    let ty = entry_type(&program);
    let roomy = runner();
    let Observed::Completed { live, .. } = roomy
        .run_observed(&bytes, &ty, program.types())
        .expect("runs")
    else {
        panic!("completes");
    };
    // The live size at the end of the run is what the result held, which is a
    // lower bound on the memory the run needed.
    let tight = Runner::new(MemoryLimit::new(
        usize::try_from(live.with_result / 4).expect("fits"),
    ))
    .expect("runner");
    assert_eq!(
        tight.run_observed(&bytes, &ty, program.types()).expect("runs"),
        Observed::Stopped(Outcome::MemoryExhausted)
    );
}

fn depth_at_exhaustion(runner: &Runner, source: &str) -> u32 {
    let program = checked(source);
    let bytes = emitted(&program);
    let Started::Ready(mut instance) = runner.start(&bytes).expect("a v1 module") else {
        panic!("the module's own memory exceeds the limit");
    };
    assert_eq!(
        instance.call_entry().expect("the protocol"),
        Outcome::MemoryExhausted,
        "a recursion with no base case exhausts memory"
    );
    // The instance still answers after the stop.
    assert!(instance.live_size().is_ok());
    word(&mut instance, state::FRAME_DEPTH)
}

const FOREVER: &str =
    "(defn main () void (do (forever) void))\n(defn forever () void (do (forever) void))\n";

#[test]
fn a_recursion_with_no_base_case_is_the_host_event_at_any_engine_stack() {
    let normal = depth_at_exhaustion(&runner(), FOREVER);
    let small = depth_at_exhaustion(
        &Runner::with_wasm_stack(MemoryLimit::new(64 * MIB), 64 * 1024).expect("runner"),
        FOREVER,
    );
    assert!(normal > 1_000_000, "{normal} activations in 64 MiB");
    assert_eq!(normal, small, "the engine's stack does not bound the depth");
    println!("exhausted at {normal} nested activations under 64 MiB");
}

#[test]
fn recovery_after_exhaustion_a_new_instance_runs_with_a_larger_limit() {
    let program = checked(FOREVER);
    let bytes = emitted(&program);
    let tiny = Runner::new(MemoryLimit::new(MIB)).expect("runner");
    assert_eq!(tiny.run_entry(&bytes).expect("runs"), Outcome::MemoryExhausted);
    let depth_tiny = depth_at_exhaustion(&tiny, FOREVER);
    let depth_big = depth_at_exhaustion(&runner(), FOREVER);
    assert!(depth_big > 32 * depth_tiny, "{depth_tiny} against {depth_big}");
    // A program that does not recurse forever completes on a fresh instance of
    // the same runner.
    agree("(defn main () i32 1i32)\n");
}
