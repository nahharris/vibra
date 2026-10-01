//! Declarations containing line comments use the same canonical layout as
//! comment-free ones: the header shares the opening line, and a closing
//! delimiter stands alone only after a comment.

#![allow(clippy::expect_used)]

use std::path::Path;

use vibra_fmt::format_source;

fn format_twice(source: &str) -> String {
    let formatted =
        format_source(Path::new("layout.vib"), source).expect("source mode");
    let again =
        format_source(Path::new("layout.vib"), &formatted).expect("source mode");
    assert_eq!(formatted, again, "formatting is not idempotent");
    formatted
}

#[test]
fn a_commented_declaration_has_no_hanging_or_orphaned_delimiter() {
    let formatted = format_twice(
        "(defn commented\n  ; about the parameter\n  (value i32)\n  i32\n  0i32)\n",
    );
    assert_eq!(
        formatted,
        "(defn commented\n  ; about the parameter\n  (value i32)\n  i32\n  0i32)\n"
    );
}

#[test]
fn a_commented_declaration_matches_the_comment_free_layout() {
    let commented = format_twice("(defn f (value i32) i32\n  ; body\n  value)\n");
    let plain = format_twice(
        "(defn f (value i32) i32 (let long-binding-name value (let another-binding-name long-binding-name another-binding-name)))\n",
    );
    assert!(commented.starts_with("(defn f (value i32) i32\n"));
    assert!(plain.starts_with("(defn f (value i32) i32\n"));
    assert!(commented.ends_with("  value)\n"));
}

#[test]
fn a_comment_before_the_closing_delimiter_keeps_it_on_its_own_line() {
    let formatted = format_twice("(defn f () i32\n  0i32 ; trailing\n  )\n");
    assert!(formatted.ends_with("  ; trailing\n)\n"), "{formatted}");
}

#[test]
fn nested_methods_follow_the_same_layout() {
    let formatted = format_twice(
        "(deftype user (record name str)\n  ; a method\n  (defn m (value self) str \"x\"))\n",
    );
    assert!(!formatted.contains("(\n"), "{formatted}");
    assert!(formatted.ends_with("\"x\"))\n"), "{formatted}");
}

#[test]
fn record_fields_stay_in_name_type_pairs_around_comments() {
    let formatted = format_twice(
        "(deftype user\n  (record\n    name str\n    ; the numeric id\n    id u64))\n",
    );
    assert!(
        formatted.contains("    name str\n    ; the numeric id\n    id u64)"),
        "{formatted}"
    );
}
