//! Canonical layout of `do`, `let`, and `let-else`
//! (`docs/roadmap/pre-m4/01-bindings-return-never.md`, section H).

#![allow(clippy::expect_used, missing_docs)]

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

fn body(element: &str) -> String {
    format!("(defn f () i32\n  {element}\n  a)\n")
}

#[test]
fn a_single_pair_let_that_fits_is_one_line() {
    // H01.
    // The whole declaration fits, so it is one line too.
    assert_eq!(
        format_twice(&body("(let   a   1i32)")),
        "(defn f () i32 (let a 1i32) a)\n"
    );
}

#[test]
fn a_let_of_two_or_more_pairs_is_always_multiline() {
    // H02, H03.
    assert_eq!(
        format_twice(&body("(let a 1i32   b 2i32)")),
        body("(let\n    a 1i32\n    b 2i32)")
    );
    assert_eq!(
        format_twice(&body("(let a 1i32 b 2i32 c 3i32)")),
        body("(let\n    a 1i32\n    b 2i32\n    c 3i32)")
    );
}

#[test]
fn a_pair_that_does_not_fit_splits_its_pattern_and_value() {
    // H04.
    let value = "(describe-entry-with-all-of-its-attributes entry count (default-describe-options))";
    assert_eq!(
        format_twice(&body(&format!("(let summary {value})"))),
        body(&format!("(let\n    summary\n    {value})"))
    );
}

#[test]
fn a_let_else_lays_out_in_three_ways() {
    // H05-H07.
    assert_eq!(
        format_twice(&body(
            "(let-else   (option.some second) (items 1u64) (return (result.ok entry)))"
        )),
        body("(let-else (option.some second) (items 1u64) (return (result.ok entry)))")
    );
    assert_eq!(
        format_twice(&body(
            "(let-else (option.some picked) (pick-entry-by-key-and-priority entry key (priority-of entry)) (return (result.err (lookup-error.missing key))))"
        )),
        body(
            "(let-else\n    (option.some picked) (pick-entry-by-key-and-priority entry key (priority-of entry))\n    (return (result.err (lookup-error.missing key))))"
        )
    );
    assert_eq!(
        format_twice(&body(
            "(let-else (option.some chosen) (choose-entry-by-key-and-priority entry key (priority-of-entry entry)) (return (result.err (lookup-error.missing key))))"
        )),
        body(
            "(let-else\n    (option.some chosen)\n    (choose-entry-by-key-and-priority entry key (priority-of-entry entry))\n    (return (result.err (lookup-error.missing key))))"
        )
    );
}

#[test]
fn a_long_do_puts_each_element_on_its_own_line() {
    // H08.
    let call =
        "(call-three 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27)";
    assert_eq!(
        format_twice(&format!(
            "(defn f () i32 (do (call-one 1 2) (call-two 3 4) {call}) 1i32)"
        )),
        format!(
            "(defn f () i32\n  (do\n    (call-one 1 2)\n    (call-two 3 4)\n    {call})\n  1i32)\n"
        )
    );
}

#[test]
fn comments_keep_their_own_lines_between_and_inside_pairs() {
    // H09-H11.
    assert_eq!(
        format_twice(&body(
            "(let\n    ; first\n    a 1i32\n    ; second\n    b 2i32)"
        )),
        body("(let\n    ; first\n    a 1i32\n    ; second\n    b 2i32)")
    );
    assert_eq!(
        format_twice(&body("(let c 1i32 ; trailing\n    d 2i32)")),
        body("(let\n    c 1i32\n    ; trailing\n    d 2i32)")
    );
    assert_eq!(
        format_twice(&body(
            "(let-else (option.some v) o ; why\n    (return 0i32))"
        )),
        body("(let-else\n    (option.some v) o\n    ; why\n    (return 0i32))")
    );
}

#[test]
fn a_do_in_a_match_arm_stays_one_arm_per_line() {
    // H12.
    let source = "(defn f (v (option i32)) i32 (match v (option.some n) (do (let m n) (let k m) k) (option.none) 0i32))\n";
    let formatted = format_twice(source);
    assert!(formatted.contains(
        "(match v (option.some n) (do (let m n) (let k m) k) (option.none) 0i32)"
    ));
}

#[test]
fn return_is_an_ordinary_list() {
    // H14.
    assert_eq!(
        format_twice(
            "(defn f () i32 (if true (return   (some-call   a   b)) void) 1i32)\n"
        ),
        "(defn f () i32 (if true (return (some-call a b)) void) 1i32)\n"
    );
}
