//! Bounds wherever a type argument meets a bounded parameter: written
//! applications of an interface, `impl` targets, inferred type arguments of
//! calls and of function and contract values, and parameters fixed only by
//! the expected type. Each breach is `@type.unsatisfiedBound` at the
//! argument, or at what fixes it, and never reaches the checked IR.
//! Expectations come from the type chapter ("Generics", "Bounds",
//! "Implementations", "Dispatch").

use vibra_diagnostics::{ByteSpan, DiagnosticCode};

use super::{SOURCE, at, faults, run, value};
use crate::check::check_source;

const FEED: &str = "interface Feed(S) given {S: Ordered} \
                    fn feed takes {x: Self item: S} returns r: U8 endfn endinterface\n\
                    type Plain of U8 endtype\n";

const FEED_IN_INTERFACE: &str = "interface Feed(S) given {S: Ordered} \
     fn feed takes {x: Self item: S} returns r: U8 endfn \
     fn make takes {x: Self} returns r: Option(S) endfn \
     impl U8 \
     fn feed takes {x: Self item: S} returns r: U8 do return 1u8 endfn \
     fn make takes {x: Self} returns r: Option(S) do return Option.none() endfn \
     endimpl endinterface\n\
     type Plain of U8 endtype\n";

/// Asserts that `source` has exactly one diagnostic, `@type.unsatisfiedBound`
/// at the `nth` `needle`, related to the first written `Ordered`.
fn breach(source: &str, needle: &str, nth: usize) {
    breach_at(source, at(source, needle, nth));
}

/// The span of the first character of the `nth` `needle` of `source`.
fn first(source: &str, needle: &str, nth: usize) -> ByteSpan {
    let span = at(source, needle, nth);
    ByteSpan::new(span.start(), span.start().saturating_add(1))
}

/// Asserts that `source` has exactly one diagnostic, `@type.unsatisfiedBound`
/// at `span`, related to the first written `Ordered`.
fn breach_at(source: &str, span: ByteSpan) {
    let result = check_source(SOURCE, source);
    let diagnostics = result.diagnostics();
    assert_eq!(diagnostics.len(), 1, "{source}\n{diagnostics:#?}");
    let Some(diagnostic) = diagnostics.first() else {
        return;
    };
    assert_eq!(
        diagnostic.code(),
        DiagnosticCode::TypeUnsatisfiedBound,
        "{source}"
    );
    assert_eq!(diagnostic.primary_span(), span, "{source}");
    let related = diagnostic
        .related()
        .iter()
        .map(|related| related.span)
        .collect::<Vec<_>>();
    assert_eq!(related, vec![at(source, "Ordered", 0)], "{source}");
}

#[test]
fn an_impl_target_meets_the_interface_bounds() {
    let target = |head: &str, item: &str| {
        format!(
            "{FEED}type {head} of U8 impl Feed({item}) \
             fn feed takes {{x: Self item: {item}}} returns r: U8 do return 1u8 endfn \
             endimpl endtype\n"
        )
    };
    breach(&target("Box", "Plain"), "Plain", 1);
    breach(&target("Box", "List(Plain)"), "List(Plain)", 0);
    // A parameter of the enclosing type carries no bound.
    let source = target("Box(T)", "T");
    breach_at(&source, first(&source, "T)", 1));
    let source = target("Box", "U8");
    assert_eq!(faults(&source), Vec::new());
}

#[test]
fn a_written_interface_type_meets_its_bounds_in_every_position() {
    for body in [
        "fn f takes {v: Feed(Plain)} returns r: U8 do return 1u8 endfn\n",
        "fn f returns r: Option(Feed(Plain)) do return Option.none() endfn\n",
        "fn f returns r: U8 do let v: Option(Feed(Plain)) be Option.none() endlet \
         return 1u8 endfn\n",
        "type Holder of {v: Option(Feed(Plain))} endtype\n",
        "type Holder of List(Feed(Plain)) endtype\n",
        "fn f takes {g: Fn({Feed(Plain)} U8)} returns r: U8 do return 1u8 endfn\n",
        "fn f(T) takes {x: T} returns r: U8 given {T: Feed(Plain)} do return 1u8 endfn\n",
    ] {
        breach(&format!("{FEED}{body}"), "Plain", 1);
    }
    let source = format!(
        "{FEED}fn f(T U) takes {{x: T u: U}} returns r: U8 given {{T: Feed(U)}} \
         do return 1u8 endfn\n"
    );
    breach_at(&source, first(&source, "U)", 1));
    let source = format!(
        "{FEED}fn f(T U) takes {{x: T u: U}} returns r: U8 \
         given {{U: Ordered T: Feed(U)}} do return 1u8 endfn\n\
         fn g takes {{v: Feed(U8)}} returns r: U8 do return 1u8 endfn\n"
    );
    assert_eq!(faults(&source), Vec::new());
}

#[test]
fn a_bounded_declared_type_meets_its_bound_in_every_position() {
    let pair =
        "type Pair(T) of T given {T: Ordered} endtype\ntype Plain of U8 endtype\n";
    for body in [
        "fn f takes {p: Pair(Plain)} returns r: U8 do return 1u8 endfn\n",
        "fn f returns r: Option(Pair(Plain)) do return Option.none() endfn\n",
        "fn f returns r: U8 do let p: List(Pair(Plain)) be [] endlet return 1u8 endfn\n",
        "type Holder of {p: Pair(Plain)} endtype\n",
        "type Holder of Pair(Plain) endtype\n",
        "fn f takes {g: Fn({Pair(Plain)} U8)} returns r: U8 do return 1u8 endfn\n",
    ] {
        breach(&format!("{pair}{body}"), "Plain", 1);
    }
    // An inferred application is the fault of the operand that fixes it.
    let source = format!(
        "{pair}fn f returns r: U8 do let p be Pair(Plain(1u8)) endlet return 1u8 endfn\n"
    );
    breach(&source, "Plain(1u8)", 0);
    // A type parameter meets the bound only through its own bound.
    let source = format!(
        "{pair}fn f(T) takes {{p: Pair(T)}} returns r: U8 do return 1u8 endfn\n"
    );
    breach_at(&source, first(&source, "T)}", 0));
    let source = format!(
        "{pair}fn f(T) takes {{p: Pair(T)}} returns r: U8 given {{T: Ordered}} \
         do return 1u8 endfn\nfn g takes {{p: Pair(U8)}} returns r: U8 do return 1u8 endfn\n"
    );
    assert_eq!(faults(&source), Vec::new());
    // Inside the type, its parameter has the members of its bound.
    let source = "fn main returns r: U8 do return Pair.larger(Pair(3u8) 5u8) endfn\n\
                  type Pair(T) of T given {T: Ordered} \
                  fn larger takes {p: Self other: T} returns r: T \
                  do let Pair(v) be p endlet return Ordered.max(v other) endfn endtype\n";
    assert_eq!(run(source).as_deref(), Ok(value("u8", "5u8").as_str()));
}

#[test]
fn a_function_value_meets_the_bounds_its_expected_type_fixes() {
    let need = "fn need(T) takes {x: T} returns r: U8 given {T: Ordered} do return 1u8 endfn\n";
    let source = format!(
        "{need}fn f returns r: U8 do let g: Fn({{Plain}} U8) be need endlet return 1u8 endfn\n\
         type Plain of U8 endtype\n"
    );
    breach(&source, "Fn({Plain} U8)", 0);
    let source = format!(
        "fn main returns r: U8 do let g: Fn({{U8}} U8) be need endlet return g(1u8) endfn\n{need}"
    );
    assert_eq!(run(&source).as_deref(), Ok(value("u8", "1u8").as_str()));
}

#[test]
fn a_contract_value_meets_the_bounds_its_expected_type_fixes() {
    let source = format!(
        "{FEED_IN_INTERFACE}fn f returns r: U8 do let g: Fn({{U8 Plain}} U8) be Feed.feed \
         endlet return 1u8 endfn\n"
    );
    breach(&source, "Fn({U8 Plain} U8)", 0);
    let pick = "interface Pick fn pick(T) takes {x: Self item: T} returns r: T \
                given {T: Ordered} endfn endinterface\n\
                type Plain of U8 endtype\n\
                type Box of U8 impl Pick fn pick(T) takes {x: Self item: T} returns r: T \
                given {T: Ordered} do return item endfn endimpl endtype\n";
    let source = format!(
        "{pick}fn f returns r: U8 do let g: Fn({{Box Plain}} Plain) be Pick.pick endlet \
         return 1u8 endfn\n"
    );
    breach(&source, "Fn({Box Plain} Plain)", 0);
    let source = format!(
        "fn main returns r: U8 do let g: Fn({{U8 I32}} U8) be Feed.feed endlet \
         return g(0u8 5i32) endfn\n{FEED_IN_INTERFACE}"
    );
    assert_eq!(run(&source).as_deref(), Ok(value("u8", "1u8").as_str()));
}

#[test]
fn an_interface_argument_breach_is_reported_at_the_operand_that_fixes_it() {
    let source = format!(
        "{FEED_IN_INTERFACE}fn f returns r: U8 do return Feed.feed(0u8 Plain(1u8)) endfn\n"
    );
    breach(&source, "Plain(1u8)", 0);
    // No implementation fits, and the breach is still the operand's.
    let source = format!(
        "{FEED}type Box of U8 impl Feed(U8) \
         fn feed takes {{x: Self item: U8}} returns r: U8 do return 1u8 endfn endimpl endtype\n\
         fn f returns r: U8 do return Feed.feed(Box(0u8) Plain(1u8)) endfn\n"
    );
    breach(&source, "Plain(1u8)", 0);
    let source = format!(
        "fn main returns r: U8 do return Feed.feed(0u8 2i32) endfn\n{FEED_IN_INTERFACE}"
    );
    assert_eq!(run(&source).as_deref(), Ok(value("u8", "1u8").as_str()));
}

#[test]
fn a_parameter_only_the_expected_type_fixes_meets_its_bound_once_fixed() {
    // The implementation in the interface is selected while `S` is still
    // open, so its bound waits until the expected type fixes `S`.
    for (annotation, needle) in [
        (
            "let o: Option(Plain) be Feed.make(0u8) endlet",
            "Option(Plain)",
        ),
        (
            "let o: Option(Option(Plain)) be Feed.make(0u8) endlet",
            "Option(Option(Plain))",
        ),
    ] {
        let make = if needle == "Option(Plain)" {
            FEED_IN_INTERFACE.to_owned()
        } else {
            FEED_IN_INTERFACE.replace("Option(S)", "Option(Option(S))")
        };
        let source =
            format!("{make}fn f returns r: U8 do {annotation} return 1u8 endfn\n");
        breach(&source, needle, 0);
    }
    let source = format!(
        "{FEED_IN_INTERFACE}fn h returns r: Option(Plain) do return Feed.make(0u8) endfn\n"
    );
    breach(&source, "Option(Plain)", 0);
    let source = "fn f returns r: U8 do let o: Option(Plain) be none() endlet return 1u8 endfn\n\
                  fn none(T) returns r: Option(T) given {T: Ordered} \
                  do return Option.none() endfn\n\
                  type Plain of U8 endtype\n";
    let result = check_source(SOURCE, source);
    let codes = result
        .diagnostics()
        .iter()
        .map(|diagnostic| (diagnostic.code(), diagnostic.primary_span()))
        .collect::<Vec<_>>();
    assert_eq!(
        codes,
        vec![(
            DiagnosticCode::TypeUnsatisfiedBound,
            at(source, "Option(Plain)", 0)
        )]
    );
    let source = format!(
        "fn main returns r: U8 do let o: Option(U8) be Feed.make(0u8) endlet return 1u8 endfn\n\
         {FEED_IN_INTERFACE}"
    );
    assert_eq!(run(&source).as_deref(), Ok(value("u8", "1u8").as_str()));
}

#[test]
fn a_library_bound_reaches_the_operand_whose_implementation_fixes_it() {
    // `compareLexicographic` takes `T` from the `Iter` choice of `I`, so a
    // `T` without `Ordered` is the fault of the operand that fixes `I`.
    let source = "use @std.iter.compareLexicographic enduse\n\
                  type Plain of U8 endtype\n\
                  fn f returns r: U8 do let o: Ordering be \
                  compareLexicographic([Plain(1u8)] [Plain(2u8)]) endlet return 1u8 endfn\n";
    assert_eq!(
        faults(source),
        vec![(
            DiagnosticCode::TypeUnsatisfiedBound,
            at(source, "[Plain(1u8)]", 0)
        )]
    );
    let source = "use @std.iter.compareLexicographic enduse\n\
                  fn main returns r: U8 do match compareLexicographic([1u8 2u8] [1u8 3u8]) \
                  case Ordering.less() return 1u8 case _ return 0u8 endmatch endfn\n";
    assert_eq!(run(source).as_deref(), Ok(value("u8", "1u8").as_str()));
}
