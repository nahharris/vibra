//! M3 Step 18: the layout of a multiline declaration
//! (`docs/spec/01-source-language.md`, "Canonical format").

#![allow(clippy::expect_used)]

use std::path::Path;

use vibra_fmt::format_source;

fn format_twice(source: &str) -> String {
    let formatted =
        format_source(Path::new("layout.vib"), source).expect("source mode");
    let again =
        format_source(Path::new("layout.vib"), &formatted).expect("source mode");
    assert_eq!(formatted, again, "formatting is not idempotent");
    for line in formatted.lines() {
        assert!(line.chars().count() <= 88, "line passes 88 columns: {line}");
    }
    formatted
}

#[test]
fn the_header_shares_the_opening_line_and_attributes_sit_beside_their_values() {
    let formatted = format_twice(
        "(deftype stack (record items (array t)) where: (t any) visibility: @public (defn push (value self item t) self visibility: @public (stack items: (array.append (value @items) item))))\n",
    );
    assert_eq!(
        formatted,
        "(deftype stack (record items (array t))\n  where: (t any)\n  visibility: @public\n  (defn push (value self item t) self\n    visibility: @public\n    (stack items: (array.append (value @items) item))))\n"
    );
}

#[test]
fn a_header_form_that_does_not_fit_takes_its_own_line_with_those_after_it() {
    let formatted = format_twice(
        "(defn a-function-with-a-long-name (first-parameter i32 second-parameter i32 third-parameter i32 fourth i32) i32 first-parameter)\n",
    );
    assert_eq!(
        formatted,
        "(defn a-function-with-a-long-name\n  (first-parameter i32 second-parameter i32 third-parameter i32 fourth i32)\n  i32\n  first-parameter)\n"
    );
}

#[test]
fn a_last_list_breaks_rather_than_orphaning_the_closing_delimiters() {
    let formatted = format_twice(
        "(deftype part (record name str unit-cents u64 count u64) (impl priced (defn cents (value self) u64 (times (value @unit-cents) (value @count)))))\n",
    );
    assert_eq!(
        formatted,
        "(deftype part (record name str unit-cents u64 count u64)\n  (impl priced\n    (defn cents (value self) u64 (times (value @unit-cents) (value @count)))))\n"
    );
    assert!(!formatted.contains("\n)"), "{formatted}");
}

#[test]
fn a_last_atom_that_does_not_fit_still_leaves_the_delimiter_alone() {
    let atom = "x".repeat(86);
    let formatted = format_twice(&format!("(first {atom})\n"));
    assert_eq!(formatted, format!("(first\n  {atom}\n)\n"));
}

#[test]
fn a_commented_declaration_lays_out_like_a_plain_one() {
    let plain = format_twice(
        "(defint priced (defn cents (value self) u64) (defn above (value self limit u64) bool visibility: @public (above-limit (priced.cents value) limit (priced.cents value) limit)))\n",
    );
    let commented = format_twice(
        "(defint priced\n  ; What the value is worth.\n  (defn cents (value self) u64) (defn above (value self limit u64) bool visibility: @public (above-limit (priced.cents value) limit (priced.cents value) limit)))\n",
    );
    assert_eq!(
        commented.replace("  ; What the value is worth.\n", ""),
        plain
    );
}

#[test]
fn a_comment_between_header_forms_ends_the_opening_line() {
    let formatted = format_twice(
        "(defn commented\n  ; about the parameter\n  (value i32) i32 0i32)\n",
    );
    assert_eq!(
        formatted,
        "(defn commented\n  ; about the parameter\n  (value i32)\n  i32\n  0i32)\n"
    );
}
