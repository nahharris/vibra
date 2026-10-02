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
        "(defn f (value i32) i32 (let long-binding-name value) (let another-binding-name long-binding-name) another-binding-name)\n",
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

#[test]
fn a_commented_declaration_orders_every_attribute_label() {
    let formatted = format_twice(
        "(deftype opt (enum some t none void)\n  ; c\n  visibility: @public role: @option where: (t any))\n",
    );
    assert_eq!(
        formatted,
        "(deftype opt (enum some t none void)\n  where: (t any)\n  role: @option\n  ; c\n  visibility: @public)\n"
    );
}

#[test]
fn a_comment_elsewhere_does_not_change_an_uncommented_declaration() {
    let declaration = "(defn f () i32 doc: \"d\" visibility: @public 0i32)\n";
    let alone = format_twice(declaration);
    let beside = format_twice(&format!("; note\n(def one i32 1i32)\n\n{declaration}"));
    assert!(beside.ends_with(&alone), "{beside}");
}

#[test]
fn a_trailing_comment_stays_behind_its_form() {
    let formatted =
        format_twice("(deftype point (record x i32 ; the x\n  y i32 ; the y\n  ))\n");
    assert_eq!(
        formatted,
        "(deftype point\n  (record\n    x i32\n    ; the x\n    y i32\n    ; the y\n  ))\n"
    );
}

#[test]
fn a_lambda_orders_its_attributes_in_a_commented_file() {
    let formatted = format_twice(
        "; note\n(defn g () i32\n  ((lambda (x i32) i32 variadic: (r (array i32)) labelled: (k i32 1i32) x) 1i32))\n",
    );
    assert_eq!(
        formatted,
        "; note\n(defn g () i32\n  ((lambda (x i32) i32 labelled: (k i32 1i32) variadic: (r (array i32)) x) 1i32))\n"
    );
}

#[test]
fn a_comment_moves_with_its_lambda_attribute() {
    let formatted = format_twice(
        "(defn g () i32\n  ((lambda (x i32) i32\n    ; the tail\n    variadic: (r (array i32))\n    ; the label\n    labelled: (k i32 1i32)\n    x) 1i32))\n",
    );
    let label = formatted.find("; the label").expect("label comment");
    let tail = formatted.find("; the tail").expect("tail comment");
    assert!(label < tail, "{formatted}");
    assert!(
        formatted.find("labelled:").expect("label") < tail,
        "{formatted}"
    );
}
