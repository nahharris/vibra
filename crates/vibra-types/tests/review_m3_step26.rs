//! M3 Step 26: the non-exhaustive witness is the first uncovered shape in
//! declaration order, spelled as a pattern.

#![allow(clippy::expect_used)]

use vibra_diagnostics::DiagnosticCode;
use vibra_types::check_source;

fn witness(source: &str) -> String {
    let checked = check_source("case.vib", source);
    let diagnostic = checked
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code() == DiagnosticCode::PatternNonExhaustive)
        .expect("a non-exhaustive match");
    diagnostic.notes().first().cloned().unwrap_or_default()
}

#[test]
fn a_partly_covered_earlier_variant_comes_before_an_unnamed_later_one() {
    assert_eq!(
        witness(
            "(deftype e (enum a bool b void c void))\n\
             (defn f (value e) i32 (match value (e.a true) 0i32))"
        ),
        "`(e.a false)` is not covered"
    );
    assert_eq!(
        witness(
            "(defn f (value (option bool)) i32 (match value (option.some true) 0i32))"
        ),
        "`(option.some false)` is not covered"
    );
}

#[test]
fn a_str_witness_is_spelled_with_its_type_name() {
    assert_eq!(
        witness("(defn f (value str) i32 (match value (str (array)) 0i32))"),
        "`(str (array -))` is not covered"
    );
}
