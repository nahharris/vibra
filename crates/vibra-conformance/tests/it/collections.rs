//! Collections, text, bytes, and dict (milestone 4 Step 8b), in WebAssembly.
//!
//! The reference interpreter is the oracle. A program here is checked from
//! source and run by both backends, and the result is held to one expectation
//! that is written beside it from the specification's canonical value encoding,
//! never copied from a run. Every test also holds the arena to what the runtime
//! chapter asks of reclamation: once the host has released the result, the live
//! size returns to what the module values keep, so a native import that builds
//! or copies a value kept its counts exact.

use crate::support::{
    MIB, checked, footprint, oracle, result, tuple_type, tuple_value, wasm,
};

// -- the harness --------------------------------------------------------------

/// Runs `source` in both backends, asserts they agree and are the specified
/// result `expected`, and asserts the arena is balanced: after the host
/// released the result nothing is live but what the module values keep, and no
/// frame is left.
fn balanced_on(source: &str, expected: &str) {
    let program = checked(source);
    let want = oracle(&program);
    assert_eq!(
        want, expected,
        "the interpreter is not the specified encoding for:\n{source}"
    );
    let (observed, _) = wasm(&program);
    assert_eq!(observed, want, "the backends disagree on:\n{source}");
    let after = footprint(&program, 64 * MIB);
    assert_eq!(
        after.live, after.kept,
        "the live size is not at its baseline for:\n{source}"
    );
    assert_eq!(after.frame_depth, 0, "a frame is left by:\n{source}");
}

// -- the encoding of a result ---------------------------------------------------

fn array_type(element: &str) -> String {
    format!("(record type: @array arguments: (array {element}))")
}

fn array_value(values: &[&str]) -> String {
    let elements = if values.is_empty() {
        "(array)".to_owned()
    } else {
        format!("(array {})", values.join(" "))
    };
    format!("(record kind: @array values: {elements})")
}

// -- arrays -----------------------------------------------------------------------

#[test]
fn an_array_of_every_kind_of_element_is_built_and_read() {
    // Scalars of each width, a reference, a nested array, and an empty array.
    let source = "(defn main ()\n\
  (tuple (array i32) (array u64) (array str) (array (array u8)) (array bool))\n\
  (tupleof\n\
    (array.of 1i32 -2i32 3i32)\n\
    (array.of 10000000000u64 0u64)\n\
    (array.of \"a\" \"\" \"héllo\")\n\
    (array.of (array.of 1u8 2u8) (array.of) (array.of 255u8))\n\
    (array.of true false true)))\n";
    let types = tuple_type(&[
        &array_type("@i32"),
        &array_type("@u64"),
        &array_type("@str"),
        &array_type(&array_type("@u8")),
        &array_type("@bool"),
    ]);
    let value = tuple_value(&[
        &array_value(&["1i32", "-2i32", "3i32"]),
        &array_value(&["10000000000u64", "0u64"]),
        &array_value(&["\"a\"", "\"\"", "\"héllo\""]),
        &array_value(&[
            &array_value(&["1u8", "2u8"]),
            &array_value(&[]),
            &array_value(&["255u8"]),
        ]),
        &array_value(&["true", "false", "true"]),
    ]);
    balanced_on(source, &result(&types, &value));
}

/// The type of `(option t)`.
fn option_type(payload: &str) -> String {
    format!("(record type: @std.option.option arguments: (array {payload}))")
}

fn some(payload: &str) -> String {
    format!(
        "(record kind: @enum type: @std.option.option variant: @some payload: {payload})"
    )
}

const NONE: &str = "(record kind: @enum type: @std.option.option variant: @none)";

#[test]
fn a_lookup_is_some_in_range_and_none_out_of_it_for_an_array_a_str_and_bytes() {
    let source = "(import option @std.option)\n\
(defn main ()\n\
  (tuple (option.option i32) (option.option i32) (option.option str)\n\
         (option.option char) (option.option char) (option.option u8) (option.option u8)\n\
         (option.option i32))\n\
  (let items (array.of 10i32 20i32))\n\
  (tupleof\n\
    (items 1u64)\n\
    (items 2u64)\n\
    ((array.of \"x\" \"y\") 0u64)\n\
    (\"héllo\" 1u64)\n\
    (\"hi\" 9u64)\n\
    ((bytes (array.of 7u8 8u8)) 1u64)\n\
    ((bytes (array.of)) 0u64)\n\
    (items 18446744073709551615u64)))\n";
    let types = tuple_type(&[
        &option_type("@i32"),
        &option_type("@i32"),
        &option_type("@str"),
        &option_type("@char"),
        &option_type("@char"),
        &option_type("@u8"),
        &option_type("@u8"),
        &option_type("@i32"),
    ]);
    let value = tuple_value(&[
        &some("20i32"),
        NONE,
        &some("\"x\""),
        &some("\\é"),
        NONE,
        &some("8u8"),
        NONE,
        NONE,
    ]);
    balanced_on(source, &result(&types, &value));
}

#[test]
fn the_array_rows_build_new_arrays_and_report_a_bad_range_as_none() {
    let source = "(import option @std.option)\n\
(defn main ()\n\
  (tuple u64 (array str) (array str) (option.option (array str))\n\
         (option.option (array str)) (option.option (array str)))\n\
  (let words (array.of \"a\" \"b\" \"c\"))\n\
  (tupleof\n\
    (array.length words)\n\
    (array.append words \"d\")\n\
    (array.concat words words)\n\
    (array.slice words 1u64 3u64)\n\
    (array.slice words 2u64 1u64)\n\
    (array.slice words 3u64 3u64)))\n";
    let strs = array_type("@str");
    let types = tuple_type(&[
        "@u64",
        &strs,
        &strs,
        &option_type(&strs),
        &option_type(&strs),
        &option_type(&strs),
    ]);
    let value = tuple_value(&[
        "3u64",
        &array_value(&["\"a\"", "\"b\"", "\"c\"", "\"d\""]),
        &array_value(&["\"a\"", "\"b\"", "\"c\"", "\"a\"", "\"b\"", "\"c\""]),
        &some(&array_value(&["\"b\"", "\"c\""])),
        NONE,
        &some(&array_value(&[])),
    ]);
    balanced_on(source, &result(&types, &value));
}

#[test]
fn an_array_pattern_matches_its_exact_length_and_binds_its_elements() {
    let source = "(defn main () (tuple atom atom atom str str)\n\
  (tupleof\n\
    (size (array.of 1i32 2i32))\n\
    (size (array.of))\n\
    (size (array.of 5i32))\n\
    (joined (array.of \"a\" \"b\"))\n\
    (joined (array.of \"a\" \"b\" \"c\"))))\n\
(defn size (items (array i32)) atom\n\
  (match items\n\
    (array) @empty\n\
    (array -) @one\n\
    - @many))\n\
(defn joined (items (array str)) str\n\
  (match items\n\
    (array first second) (match (tupleof first second) (tupleof a b) a)\n\
    - \"other\"))\n";
    let types = tuple_type(&["@atom", "@atom", "@atom", "@str", "@str"]);
    let value = tuple_value(&["@many", "@empty", "@one", "\"a\"", "\"other\""]);
    balanced_on(source, &result(&types, &value));
}

fn dict_type(key: &str, value: &str) -> String {
    format!("(record type: @dict arguments: (array {key} {value}))")
}

fn dict_value(entries: &[(&str, &str)]) -> String {
    let rows = if entries.is_empty() {
        "(array)".to_owned()
    } else {
        let rows = entries
            .iter()
            .map(|(key, value)| format!("(tuple {key} {value})"))
            .collect::<Vec<_>>()
            .join(" ");
        format!("(array {rows})")
    };
    format!("(record kind: @dict entries: {rows})")
}

#[test]
fn a_dict_keeps_key_order_whatever_the_insertion_order_and_a_later_pair_wins() {
    let source = "(defn main () (tuple (dict str i32) (dict str i32) (dict str i32) (dict u64 str))\n\
  (tupleof (dict.of \"b\" 2i32 \"a\" 1i32 \"b\" 3i32) (dict.of \"a\" 1i32 \"b\" 3i32)\n\
           (as (dict str i32) (dict.of))\n\
           (dict.of 9u64 \"nine\" 1u64 \"one\" 5u64 \"five\" 1u64 \"uno\")))\n";
    let strs = dict_type("@str", "@i32");
    let types = tuple_type(&[&strs, &strs, &strs, &dict_type("@u64", "@str")]);
    let value = tuple_value(&[
        &dict_value(&[("\"a\"", "1i32"), ("\"b\"", "3i32")]),
        &dict_value(&[("\"a\"", "1i32"), ("\"b\"", "3i32")]),
        &dict_value(&[]),
        &dict_value(&[
            ("1u64", "\"uno\""),
            ("5u64", "\"five\""),
            ("9u64", "\"nine\""),
        ]),
    ]);
    balanced_on(source, &result(&types, &value));
}

#[test]
fn a_dict_lookup_finds_a_closed_key_of_any_shape_and_misses_cleanly() {
    let source = "(import option @std.option)\n\
(defn main () (tuple (option.option str) (option.option str) (option.option i32) (option.option i32) u64)\n\
  (let names (dict.of (tupleof 1i32 \"a\") \"one\" (tupleof 2i32 \"a\") \"two\"))\n\
  (let signed (dict.of -5i32 10i32 3i32 30i32 -1i32 20i32))\n\
  (tupleof (names (tupleof 2i32 \"a\")) (names (tupleof 3i32 \"a\")) (signed -1i32) (signed 0i32)\n\
           (array.length (dict.entries signed))))\n";
    let strs = option_type("@str");
    let ints = option_type("@i32");
    let types = tuple_type(&[&strs, &strs, &ints, &ints, "@u64"]);
    let value = tuple_value(&[&some("\"two\""), NONE, &some("20i32"), NONE, "3u64"]);
    balanced_on(source, &result(&types, &value));
}

#[test]
fn a_declared_key_is_ordered_by_its_own_compare_called_as_a_language_function() {
    let source = "(import core @std.core)\n(import ordered @std.core.ordered)\n\
(defn main () (dict point str)\n\
  (dict.of (point x: 1i32 y: 5i32) \"first\" (point x: 3i32 y: 5i32) \"second\" (point x: 0i32 y: 1i32) \"low\"))\n\
(deftype point (record x i32 y i32)\n\
  (impl ordered\n\
    (defn compare (left self right self) core.ordering\n\
      (ordered.compare (left @y) (right @y)))))\n";
    let types = dict_type("@app.main.point", "@str");
    let point = |x: &str, y: &str| {
        format!(
            "(record kind: @record type: @app.main.point fields: (record x: {x} y: {y}))"
        )
    };
    let value = dict_value(&[
        (&point("0i32", "1i32"), "\"low\""),
        (&point("3i32", "5i32"), "\"second\""),
    ]);
    balanced_on(source, &result(&types, &value));
}
