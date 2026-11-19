//! The layout of module values, expressions, statements, and patterns.

use vibra_fmt::format_document;
use vibra_syntax::{CstNode, SyntaxKind, parse_source};

/// Formats `text`, which must be accepted, and checks that formatting the
/// output again changes nothing.
fn format(text: &str) -> String {
    let document = parse_source(None, text.as_bytes());
    assert!(document.diagnostics().is_empty(), "{text:?} is accepted");
    let output = format_document(&document).expect("a text");
    let again = format_document(&parse_source(None, output.as_bytes()));
    assert_eq!(
        again.as_deref(),
        Some(output.as_str()),
        "idempotent for {text:?}"
    );
    output
}

/// The tokens of a source text in order, without whitespace and comments.
fn tokens(text: &str) -> Vec<String> {
    let document = parse_source(None, text.as_bytes());
    let root: &CstNode = document.root().expect("a tree");
    root.leaves()
        .into_iter()
        .filter(|leaf| {
            !matches!(leaf.kind(), SyntaxKind::Whitespace | SyntaxKind::Comment)
        })
        .map(|leaf| leaf.text(text).to_owned())
        .collect()
}

#[test]
fn a_value_that_fits_is_one_line() {
    for text in [
        "let defaultRetries: U8 be 3u8 endlet",
        "let a: I32 be choose(c)(x) endlet",
        "let u: User be User(id: UserId(1u64) name: \"Ada\") endlet",
        "let n: U64 be Str.length(user.name) endlet",
        "let t: {Str U64} be {\"Ada\" 42u64} endlet",
        "let r: {name: Str id: U64} be {name: \"Ada\" id: 42u64} endlet",
        "let d: Dict(Str Str) be [\"name\": \"Ada\" \"role\": \"dev\"] endlet",
        "let e: Dict(Str Str) be [:] endlet",
        "let z: List(I32) be [] endlet",
        "let label: Str be if ready then \"go\" else \"wait\" endif endlet",
        "let n: I32 be do return try await f() enddo endlet",
        "let n: U8 be match v case _ 0u8 endmatch endlet",
    ] {
        assert_eq!(format(text), format!("{text}\n"));
    }
    // Spacing is canonical whatever was written.
    assert_eq!(
        format("let  a:I32  be\n  f(  x\n y )  endlet"),
        "let a: I32 be f(x y) endlet\n"
    );
}

#[test]
fn a_module_value_with_sections_is_one_line_when_it_fits() {
    assert_eq!(
        format("let limit: U8\n  visibility @public\n  be 3u8\nendlet"),
        "let limit: U8 visibility @public be 3u8 endlet\n"
    );
    // A section after the initializer is rejected, so the document is
    // returned byte for byte.
    let late = "let limit: U8 be 3u8 visibility @public endlet";
    let document = parse_source(None, late.as_bytes());
    assert!(!document.diagnostics().is_empty());
    assert_eq!(format_document(&document).as_deref(), Some(late));
}

#[test]
fn a_multiline_if_keeps_then_on_the_condition_line_and_else_alone() {
    assert_eq!(
        format(
            "let label: Str be if ready then log(\"starting the request now\") \"go\" else log(\"waiting for the request now\") \"wait\" endif endlet"
        ),
        "let label: Str be\n  if ready then\n    log(\"starting the request now\")\n    \"go\"\n  else\n    log(\"waiting for the request now\")\n    \"wait\"\n  endif\nendlet\n"
    );
}

#[test]
fn a_multiline_match_has_each_case_one_step_in() {
    assert_eq!(
        format(
            "let n: U64 be match value\n  case Option.some(value: text)\n    Str.length(text)\n  case Option.none()\n    0u64\nendmatch endlet"
        ),
        "let n: U64 be\n  match value\n    case Option.some(value: text)\n      Str.length(text)\n    case Option.none()\n      0u64\n  endmatch\nendlet\n"
    );
}

#[test]
fn a_nested_let_breaks_every_construct_around_it() {
    assert_eq!(
        format("let n: I32 be do let a be 1i32 endlet I32.add(a a) enddo endlet"),
        "let n: I32 be\n  do\n    let a be 1i32 endlet\n    I32.add(a a)\n  enddo\nendlet\n"
    );
}

#[test]
fn a_let_keeps_else_at_the_end_of_the_initializer_line() {
    assert_eq!(
        format(
            "let n: I32 be do\n  let Option.some(value: head) be items(0u64) else\n    return 0i32\n  endlet\n  head\nenddo endlet"
        ),
        "let n: I32 be\n  do\n    let Option.some(value: head) be items(0u64) else return 0i32 endlet\n    head\n  enddo\nendlet\n"
    );
    assert_eq!(
        format(
            "let n: I32 be do let Option.some(value: head) be lookupTheValueWithAVeryLongName(items 0u64 otherArgument) else return 0i32 endlet head enddo endlet"
        ),
        "let n: I32 be\n  do\n    let Option.some(value: head) be\n      lookupTheValueWithAVeryLongName(items 0u64 otherArgument) else\n      return 0i32\n    endlet\n    head\n  enddo\nendlet\n"
    );
}

#[test]
fn overflowing_arguments_are_one_per_line() {
    let call = "Report.build(customerName: \"Ada Lovelace\" accountNumber: 12345678u64 region: \"emea\" active: enabled)";
    assert_eq!(call.len(), 100);
    assert_eq!(
        format(&format!("let r: Report be {call} endlet")),
        "let r: Report be\n  Report.build(\n    customerName: \"Ada Lovelace\"\n    accountNumber: 12345678u64\n    region: \"emea\"\n    active: enabled\n  )\nendlet\n"
    );
    let entries = (0..12)
        .map(|index| format!("\"entry{index}\""))
        .collect::<Vec<_>>()
        .join(" ");
    let expected = (0..12)
        .map(|index| format!("    \"entry{index}\"\n"))
        .collect::<String>();
    assert_eq!(
        format(&format!("let l: List(Str) be [{entries}] endlet")),
        format!("let l: List(Str) be\n  [\n{expected}  ]\nendlet\n")
    );
}

#[test]
fn comments_between_statements_are_kept() {
    assert_eq!(
        format(
            "let n: I32 be if c then // why\n  a\n  // between\n  b // end\nelse\n  // only\n  d\nendif endlet"
        ),
        "let n: I32 be\n  if c then // why\n    a\n    // between\n    b // end\n  else\n    // only\n    d\n  endif\nendlet\n"
    );
    assert_eq!(
        format(
            "let n: I32 be match v\n  // first arm\n  case _ // any\n    a\n  // tail\nendmatch endlet"
        ),
        "let n: I32 be\n  match v\n    // first arm\n    case _ // any\n      a\n      // tail\n  endmatch\nendlet\n"
    );
}

/// The layouts of a comment in every kind of group: a comment on a token's
/// line at the end of that line, any other on its own line before what
/// follows it, one step in before a closing token; a group that holds a
/// comment is multi-line, and a comment after a group's last token does not
/// break that group.
#[test]
fn a_comment_inside_any_group_is_laid_out() {
    for (text, expected) in [
        (
            "let a: I32 be f(x // c\n y) endlet",
            "let a: I32 be\n  f(\n    x // c\n    y\n  )\nendlet\n",
        ),
        (
            "let a: I32 be f(\n  // first\n  x\n  y\n  // last\n) endlet",
            "let a: I32 be\n  f(\n    // first\n    x\n    y\n    // last\n  )\nendlet\n",
        ),
        (
            "let a: I32 be f( // args\n x) endlet",
            "let a: I32 be\n  f( // args\n    x\n  )\nendlet\n",
        ),
        // The spaces written before a comment at the end of a line are kept,
        // and a comment written against its token gets one.
        (
            "let r: R be {a: 1u8     // one\n b: 2u8} endlet",
            "let r: R be\n  {\n    a: 1u8     // one\n    b: 2u8\n  }\nendlet\n",
        ),
        (
            "let a: I32 be f(x// c\n y) endlet",
            "let a: I32 be\n  f(\n    x // c\n    y\n  )\nendlet\n",
        ),
        // A comment before `endmatch` is in the last arm's body.
        (
            "let n: U8 be match v case _ a\n// nothing more\nendmatch endlet",
            "let n: U8 be\n  match v\n    case _\n      a\n      // nothing more\n  endmatch\nendlet\n",
        ),
        (
            "let a: I32 be f(x: // c\n y) endlet",
            "let a: I32 be\n  f(\n    x: // c\n    y\n  )\nendlet\n",
        ),
        (
            "let r: R be {a: 1u8 // one\n b: 2u8} endlet",
            "let r: R be\n  {\n    a: 1u8 // one\n    b: 2u8\n  }\nendlet\n",
        ),
        (
            "let l: List(U8) be [1u8\n // two\n 2u8] endlet",
            "let l: List(U8) be\n  [\n    1u8\n    // two\n    2u8\n  ]\nendlet\n",
        ),
        (
            "let d: Dict(Str U8) be [\"a\": // v\n 1u8] endlet",
            "let d: Dict(Str U8) be\n  [\n    \"a\": // v\n    1u8\n  ]\nendlet\n",
        ),
        (
            "let n: I32 be do return // why\n x enddo endlet",
            "let n: I32 be\n  do\n    return // why\n    x\n  enddo\nendlet\n",
        ),
        (
            "let n: I32 be if // test\n c then a else b endif endlet",
            "let n: I32 be\n  if // test\n  c then\n    a\n  else\n    b\n  endif\nendlet\n",
        ),
        (
            "let n: U8 be user // c\n .name endlet",
            "let n: U8 be\n  user // c\n  .name\nendlet\n",
        ),
        (
            "let n: U8 be match v case Pair(a // first\n b) a endmatch endlet",
            "let n: U8 be\n  match v\n    case Pair(\n      a // first\n      b\n    )\n      a\n  endmatch\nendlet\n",
        ),
        (
            "let f: F be fn // anon\n returns r: I32 do return x endfn endlet",
            "let f: F be\n  fn // anon\n    returns r: I32\n    do\n      return x\n  endfn\nendlet\n",
        ),
        (
            "let n: I32 be 1i32 // one\nendlet",
            "let n: I32 be 1i32 // one\nendlet\n",
        ),
        (
            "let a: I32 be f(x) // after\nendlet",
            "let a: I32 be f(x) // after\nendlet\n",
        ),
        (
            "let limit: U8 // the limit\n  visibility @public // public\n  be 3u8 // three\nendlet",
            "let limit: U8 // the limit\n  visibility @public // public\n  be 3u8 // three\nendlet\n",
        ),
    ] {
        assert_eq!(format(text), expected, "{text:?}");
    }
}

/// The only document returned unchanged is one with a diagnostic: a valid
/// document goes through the layout, even when it holds comments the old
/// layout could not place.
#[test]
fn only_a_document_with_a_diagnostic_is_returned_unchanged() {
    for text in [
        "let a: I32 be f(x // c\n y) endlet",
        "let a: I32 be f(x // c\n  y\n) endlet\n",
        "type A of {x: U8 // c\n  y: U8} endtype",
        "fn f\n  returns r: // c\n  I32\n  do\n    return 1i32\nendfn",
    ] {
        let output = format(text);
        assert_ne!(output, text, "{text:?} is laid out");
        // A valid document already in its canonical layout comes back equal,
        // because it went through the layout.
        assert_eq!(format(&output), output);
    }
    let late = "let a: I32 be 1i32 visibility @public // c\nendlet";
    let document = parse_source(None, late.as_bytes());
    assert!(!document.diagnostics().is_empty());
    assert_eq!(format_document(&document).as_deref(), Some(late));
}

#[test]
fn the_formatter_never_reorders_arguments_fields_entries_or_statements() {
    for text in [
        "let u: User be User(id: UserId(1u64) name: \"Ada\" role2: x) endlet",
        "let r: R be {zeta: 1u8 alpha: 2u8 mid: {b a}} endlet",
        "let d: Dict(Str Str) be [\"z\": \"1\" \"a\": \"2\"] endlet",
        "let n: I32 be do\n  let b be second() endlet\n  let a be first() endlet\n  f(b a)\nenddo endlet",
        "let n: U8 be match v case Pair(b a) a case {y x} x case [q p] p endmatch endlet",
        &format!(
            "let r: Report be Report.build({}) endlet",
            (0..30)
                .map(|index| format!("a{index}: v{index}"))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    ] {
        let output = format(text);
        assert_eq!(tokens(&output), tokens(text), "{text}");
    }
}

#[test]
fn nested_anonymous_callables_and_types_are_formatted_on_a_small_stack() {
    const DEPTH: usize = 400;
    let callables = format!(
        "let f: F be {}x{} endlet",
        "fn returns r: I32 do return ".repeat(DEPTH),
        " endfn".repeat(DEPTH)
    );
    let types = format!(
        "let f: {}I32{} be x endlet",
        "List(".repeat(DEPTH),
        ")".repeat(DEPTH)
    );
    let guards = format!(
        "let f: F be {}x{} endlet",
        "fn returns r: I32 requires ".repeat(DEPTH),
        " do return x endfn".repeat(DEPTH)
    );
    // The reader reads these on this stack too; the formatter must not
    // recurse through a callable, its sections, or its body.
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            for text in [callables, types, guards] {
                let output = format(&text);
                assert_eq!(tokens(&output), tokens(&text));
            }
        })
        .expect("a thread")
        .join()
        .expect("no stack overflow");
}

#[test]
fn deep_nesting_is_formatted_without_recursion() {
    const DEPTH: usize = 2_000;
    for (open, close) in [
        ("f(", ")"),
        ("{", "}"),
        ("[", "]"),
        ("do ", " enddo"),
        ("if c then ", " endif"),
        ("match v case _ ", " endmatch"),
    ] {
        let text = format!(
            "let a: I32 be {}x{} endlet",
            open.repeat(DEPTH),
            close.repeat(DEPTH)
        );
        let output = format(&text);
        assert_eq!(tokens(&output), tokens(&text), "{open}");
    }
}
