//! The canonical layout of `type`, `interface`, `impl`, and type
//! expressions.

use vibra_fmt::format_document;
use vibra_syntax::{parse_source, parse_source_with, source::SourceOptions};

/// Formats `text`, which has no diagnostic, and checks that formatting the
/// output again changes nothing.
fn format(text: &str) -> String {
    let document = parse_source(None, text.as_bytes());
    assert!(document.diagnostics().is_empty(), "{text:?}");
    let output = format_document(&document).expect("a text");
    let again = format_document(&parse_source(None, output.as_bytes()));
    assert_eq!(
        again.as_deref(),
        Some(output.as_str()),
        "idempotent for {text:?}"
    );
    output
}

#[test]
fn a_type_that_fits_is_one_line() {
    assert_eq!(
        format("type   Celsius\n  of   F64\nendtype"),
        "type Celsius of F64 endtype\n"
    );
    assert_eq!(
        format("type User of {name: Str id: UserId}\n  visibility @public\nendtype"),
        "type User of {name: Str id: UserId} visibility @public endtype\n"
    );
    assert_eq!(
        format(
            "type Lookup(T)\n  case found {value: T}\n  case missing\n  visibility @public\nendtype"
        ),
        "type Lookup(T) case found {value: T} case missing visibility @public endtype\n"
    );
}

#[test]
fn sections_take_the_canonical_order_and_cases_keep_theirs() {
    assert_eq!(
        format("type A given {T: Any} of {x: T} visibility @public endtype"),
        "type A of {x: T} given {T: Any} visibility @public endtype\n"
    );
    assert_eq!(
        format(
            "type V visibility @public case b case a doc \"\"\"v\"\"\" case c endtype"
        ),
        "type V case b case a case c visibility @public doc \"\"\"v\"\"\" endtype\n"
    );
    assert_eq!(
        format(
            "interface I doc \"\"\"i\"\"\" type Item endtype given {T: Any} endinterface"
        ),
        "interface I given {T: Any} doc \"\"\"i\"\"\" type Item endtype endinterface\n"
    );
}

#[test]
fn an_empty_given_is_removed() {
    assert_eq!(
        format("type A of U8 given {} endtype"),
        "type A of U8 endtype\n"
    );
    assert_eq!(
        format("interface I given {  } endinterface"),
        "interface I endinterface\n"
    );
}

#[test]
fn type_expressions_are_spaced_canonically() {
    assert_eq!(
        format("type Box( T  U ) of {a :T   b: U} endtype"),
        "type Box(T U) of {a: T b: U} endtype\n"
    );
    assert_eq!(
        format("type T of TryFrom( Str  Error :ParseError ) endtype"),
        "type T of TryFrom(Str Error: ParseError) endtype\n"
    );
    assert_eq!(
        format("type H of Fn( {I32   verbose: Bool}   Bool ) endtype"),
        "type H of Fn({I32 verbose: Bool} Bool) endtype\n"
    );
    assert_eq!(
        format("type J of Proc({Str} Void { DirWrite  io.Stdout }) endtype"),
        "type J of Proc({Str} Void {DirWrite io.Stdout}) endtype\n"
    );
    assert_eq!(format("type U of { } endtype"), "type U of {} endtype\n");
}

#[test]
fn a_type_that_does_not_fit_has_one_part_per_line() {
    assert_eq!(
        format(
            "type Config of {name: Str path: Str retries: U32 timeout: U64 verbose: Bool owner: UserId} endtype"
        ),
        "type Config\n  of {name: Str path: Str retries: U32 timeout: U64 verbose: Bool owner: UserId}\nendtype\n"
    );
    assert_eq!(
        format(
            "type Wide of {alpha: Str beta: Str gamma: Str delta: Str epsilon: Str zeta: Str eta: Str theta: Str} endtype"
        ),
        "type Wide\n  of {\n    alpha: Str\n    beta: Str\n    gamma: Str\n    delta: Str\n    epsilon: Str\n    zeta: Str\n    eta: Str\n    theta: Str\n  }\nendtype\n"
    );
}

#[test]
fn impl_blocks_and_associated_types_are_members() {
    assert_eq!(
        format(
            "type Once of Option(U8)\n  impl Iter\n    type Item of U8 endtype\n  endimpl\nendtype"
        ),
        "type Once of Option(U8) impl Iter type Item of U8 endtype endimpl endtype\n"
    );
    assert_eq!(
        format("interface Iter\n  type Item endtype\nendinterface"),
        "interface Iter type Item endtype endinterface\n"
    );
    assert_eq!(
        format("type A impl P endimpl visibility @public of U8 endtype"),
        "type A of U8 visibility @public impl P endimpl endtype\n"
    );
}

#[test]
fn a_comment_breaks_the_type_and_stays_with_its_part() {
    assert_eq!(
        format("type A // head\n  of U8\nendtype"),
        "type A // head\n  of U8\nendtype\n"
    );
    assert_eq!(
        format("type V\n  case a // first\n  // then\n  case b\nendtype"),
        "type V\n  case a // first\n  // then\n  case b\nendtype\n"
    );
    assert_eq!(
        format(
            "type A\n  // the visibility\n  visibility @public of U8 // the body\nendtype"
        ),
        "type A\n  of U8 // the body\n  // the visibility\n  visibility @public\nendtype\n"
    );
}

#[test]
fn a_comment_inside_a_type_expression_leaves_the_document_unchanged() {
    let text = "type A of {x: U8 // c\n  y: U8} endtype";
    assert_eq!(
        format_document(&parse_source(None, text.as_bytes())).as_deref(),
        Some(text)
    );
}

#[test]
fn a_standard_library_type_is_formatted_too() {
    let text =
        "type I32   of Intrinsic( @i32 )\n  role @ordered visibility @public\nendtype";
    let document = parse_source_with(
        None,
        text.as_bytes(),
        SourceOptions {
            standard_library: true,
        },
    );
    assert!(document.diagnostics().is_empty());
    assert_eq!(
        format_document(&document).as_deref(),
        Some("type I32 of Intrinsic(@i32) role @ordered visibility @public endtype\n")
    );
}

#[test]
fn a_type_with_a_fault_is_returned_byte_for_byte() {
    for input in [
        "type  A endtype",
        "type A of (U8)  endtype",
        "type  A of {x: endtype",
        "interface I type Item of U8 endtype  endinterface",
    ] {
        assert_eq!(
            format_document(&parse_source(None, input.as_bytes())).as_deref(),
            Some(input)
        );
    }
}

#[test]
fn deep_type_nesting_is_laid_out_without_recursion() {
    const DEPTH: usize = 2000;
    let text = format!(
        "type A of {}U8{} endtype",
        "List(".repeat(DEPTH),
        ")".repeat(DEPTH)
    );
    let output = format_document(&parse_source(None, text.as_bytes())).unwrap();
    assert!(output.starts_with("type A\n  of List(\n    List(\n"));
    assert!(output.ends_with("\n  )\nendtype\n"));
}
