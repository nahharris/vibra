//! The three forms Step 9 left unlowered, in both backends (milestone 4
//! Step 11):
//!
//! - converting an interface-typed result nested in an array, a dict, a record,
//!   or a union (`interface-result`);
//! - opening the pair of a generic-typed argument whose run-time type is an
//!   interface, where it is a bare generic parameter operand of a direct call;
//! - a `void` payload in a builtin `option` iterator.
//!
//! The reference interpreter is the oracle: a program is checked from source,
//! run by both backends, and held to one result.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use vibra_ir::CheckedProgram;
use vibra_types::check_source;
use vibra_wasm_run::{LiveSizes, MemoryLimit, Observed, Runner};

fn checked(source: &str) -> CheckedProgram {
    let result = check_source("input.vib", source);
    assert!(result.accepted(), "{:?}\n{source}", result.diagnostics());
    result.program().expect("an accepted program").clone()
}

/// Runs `source` in both backends and asserts they agree.
fn agree(source: &str) -> (String, LiveSizes) {
    let program = checked(source);
    let expected = vibra_interp::run(&program)
        .expect("the interpreter runs the program")
        .canonical_result();
    let ty = program.entry().signature().result();
    let module = vibra_wasm::emit(&program)
        .unwrap_or_else(|error| panic!("the program lowers: {error}\n{source}"));
    let heads = |head: u32| module.descriptors().type_of(head).cloned();
    let runner = Runner::new(MemoryLimit::new(64 * 1024 * 1024)).expect("an engine");
    match runner
        .run_observed_with(module.bytes(), &ty, program.types(), &heads)
        .expect("the module runs")
    {
        Observed::Completed { value, live } => {
            let observed = value.canonical_observation(&ty);
            assert_eq!(observed, expected, "the backends disagree on:\n{source}");
            (observed, live)
        }
        Observed::Stopped(outcome) => {
            panic!("the entry did not complete: {outcome:?}\n{source}")
        }
    }
}

#[test]
fn an_interface_result_in_an_array_converts_each_element() {
    let (result, live) = agree(
        "(defint walker (defn pair (value self) (array self)))\n\
         (deftype counter (record n u32)\n\
           (impl walker (defn pair (value self) (array self) (array.of value value))))\n\
         (defn main () u64 (array.length (walker.pair (as walker (counter n: 1u32)))))\n",
    );
    assert_eq!(result, "(record type: @u64 value: 2u64)\n");
    // What an instance holds after the run does not grow with the elements it
    // converted.
    let (_, one) = agree(
        "(defint walker (defn pair (value self) (array self)))
\n         (deftype counter (record n u32)
\n           (impl walker (defn pair (value self) (array self) (array.of value))))
\n         (defn main () u64 (array.length (walker.pair (as walker (counter n: 1u32)))))
",
    );
    assert_eq!(live.end, one.end);
    let (observed, _) = agree(
        "(defint walker (defn pair (value self) (array self)))\n\
         (deftype counter (record n u32)\n\
           (impl walker (defn pair (value self) (array self) (array.of value))))\n\
         (defn main () (array walker) (walker.pair (as walker (counter n: 1u32))))\n",
    );
    assert!(observed.contains("counter"), "{observed}");
}

#[test]
fn an_interface_result_in_a_dict_converts_each_value() {
    let (observed, _) = agree(
        "(defint walker (defn labelled (value self) (dict str self)))\n\
         (deftype counter (record n u32)\n\
           (impl walker (defn labelled (value self) (dict str self) (dict.of \"b\" value \"a\" value))))\n\
         (defn main () (dict str walker) (walker.labelled (as walker (counter n: 2u32))))\n",
    );
    assert!(observed.contains("@dict"), "{observed}");
}

#[test]
fn an_interface_result_in_a_record_converts_the_field() {
    let (observed, _) = agree(
        "(defint walker (defn boxed (value self) (record item self count u32)))\n\
         (deftype counter (record n u32)\n\
           (impl walker (defn boxed (value self) (record item self count u32) (recordof item: value count: 7u32))))\n\
         (defn main () (record item walker count u32) (walker.boxed (as walker (counter n: 3u32))))\n",
    );
    assert!(observed.contains("counter"), "{observed}");
}

#[test]
fn an_interface_result_in_a_union_converts_the_member() {
    let (observed, _) = agree(
        "(deftype either (union (array t) str) where: (t any))\n\
         (defint walker (defn either (value self) (either self)))\n\
         (deftype ticker (record n u32)\n\
           (impl walker (defn either (value self) (either self) (as (either self) (array.of value)))))\n\
         (defn main () (either walker) (walker.either (as walker (ticker n: 4u32))))\n",
    );
    // The union value itself is observed: its member is the declared
    // `(array walker)` in both backends (`V1-RUNTIME-union-converted-member`).
    assert!(
        observed.contains("member: (record type: @array arguments: (array @walker))"),
        "{observed}"
    );
}

/// A generic function reaches an element whose type is its own parameter. The
/// array it was given has interface values, so that parameter is instantiated
/// at an interface type and the element is a pair the function never opened.
/// It passes the element to a direct call whose parameter is a bare generic
/// parameter, which holds the same pair, since a parameter instantiated at an
/// interface type holds one. A parameter bound by `any` is the only kind an
/// interface type instantiates, so nothing inspects the element. The entry is
/// the first function of a source.
fn with_helpers(main: &str) -> String {
    format!(
        "(import option @std.option)\n{main}\n\
         (defint named (defn name (value self) str))\n\
         (deftype dog (record n u32) (impl named (defn name (value self) str \"dog\")))\n\
         (deftype cat (record n u32) (impl named (defn name (value self) str \"cat\")))\n\
         (defn keep (value u) (tuple u u) where: (u any) (tupleof value value))\n\
         (defn again (value u) (tuple u u) where: (u any) (keep value))\n\
         (defn kept (items (array t)) (option.option (tuple t t)) where: (t any)\n\
           (match (items 0u64) (option.some first) (option.some (again first)) (option.none) (option.none)))\n"
    )
}

#[test]
fn the_pair_of_a_generic_argument_that_is_an_interface_at_run_time_reaches_a_bare_parameter()
 {
    let (result, live) = agree(&with_helpers(
        "(defn main () (option.option (tuple named named))\n\
           (kept (array.of (as named (dog n: 4u32)) (as named (cat n: 5u32)))))",
    ));
    assert!(
        result.contains("dog") && !result.contains("cat"),
        "the first element, twice: {result}"
    );
    // What the instance holds after the run is the same for a longer array.
    let (_, longer) = agree(&with_helpers(
        "(defn main () (option.option (tuple named named))\n\
           (kept (array.of (as named (dog n: 4u32)) (as named (cat n: 5u32)) (as named (cat n: 6u32)))))",
    ));
    assert_eq!(live.end, longer.end);
}

#[test]
fn a_bare_parameter_operand_that_is_a_value_and_not_an_interface_is_unchanged() {
    let (result, _) = agree(&with_helpers(
        "(defn main () (option.option (tuple dog dog)) (kept (array.of (dog n: 7u32))))",
    ));
    assert!(result.contains("dog"), "{result}");
}

/// The items of an iterator, as `drain` of the standard library's own `iter`.
fn drained(main: &str) -> String {
    format!(
        "(import iter @std.iter)\n(import option @std.option)\n{main}\n\
         (defn drain (values (iter.iter item) seen (array item)) (array item)\n\
           where: (item any)\n\
           (match (iter.iter.next values)\n\
             (option.some (tupleof item rest)) (drain rest (array.append seen item))\n\
             (option.none) seen))\n"
    )
}

#[test]
fn a_void_payload_in_a_builtin_option_iterator_yields_one_void_item() {
    let (result, _) = agree(&drained(
        "(defn main () (array void) (drain (option.some) (array.of)))",
    ));
    assert_eq!(
        result,
        "(record type: (record type: @array arguments: (array @void)) value: (record kind: @array values: (array void)))\n"
    );
    let (empty, _) = agree(&drained(
        "(defn main () (array void) (drain (as (option void) (option.none)) (array.of)))",
    ));
    assert!(empty.contains("(array)"), "{empty}");
}

#[test]
fn an_option_iterator_of_a_value_still_yields_it_and_a_void_item_is_one_item_among_others()
 {
    let (result, _) = agree(&drained(
        "(defn main () (tuple (array i32) (array void)) (tupleof (drain (option.some 3i32) (array.of)) (drain (option.some) (array.of))))",
    ));
    assert!(result.contains("3i32"), "{result}");
}

#[test]
fn an_interface_result_nested_through_arrays_converts_at_every_depth() {
    let (observed, _) = agree(
        "(defint walker (defn nest (value self) (array (array self))))\n\
         (deftype counter (record n u32)\n\
           (impl walker (defn nest (value self) (array (array self)) (array.of (array.of value) (array.of)))))\n\
         (defn main () (array (array walker)) (walker.nest (as walker (counter n: 1u32))))\n",
    );
    assert!(observed.contains("@array"), "{observed}");
}
