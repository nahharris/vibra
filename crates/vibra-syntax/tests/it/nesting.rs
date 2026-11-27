//! The nesting measure of `docs/spec/05-tooling.md`, "Limits": how many
//! forms enclose each token. Every expectation is counted by hand from the
//! definition there.

use vibra_diagnostics::ByteSpan;
use vibra_syntax::{Document, nesting, parse_data, parse_source};

fn source(text: &str) -> Document {
    let document = parse_source(None, text.as_bytes());
    assert!(
        !document.has_errors(),
        "{text:?}: {:?}",
        document.diagnostics()
    );
    document
}

fn data(text: &str) -> Document {
    let document = parse_data(None, text.as_bytes());
    assert!(
        !document.has_errors(),
        "{text:?}: {:?}",
        document.diagnostics()
    );
    document
}

fn deepest(document: &Document) -> usize {
    nesting(document, usize::MAX).deepest
}

/// The span of the first occurrence of `token` in `text`.
fn at(text: &str, token: &str) -> ByteSpan {
    let start = text.find(token).expect("the token is written");
    ByteSpan::new(start, start + token.len())
}

#[test]
fn a_data_value_is_nested_one_level_in_each_bracket_form() {
    assert_eq!(deepest(&data("1u8")), 0);
    assert_eq!(deepest(&data("[1u8]")), 1);
    assert_eq!(deepest(&data("[[1u8]]")), 2);
    assert_eq!(deepest(&data("{a: [1u8] b: 2u8}")), 2);
    assert_eq!(deepest(&data("[\"k\": [1u8]]")), 2);
    // The innermost brackets are tokens of the outer form only.
    assert_eq!(deepest(&data("[[]]")), 1);
}

#[test]
fn a_declaration_encloses_nothing() {
    assert_eq!(deepest(&source("let x: I32 be 1i32 endlet")), 0);
    assert_eq!(
        deepest(&source(
            "fn f takes {x: I32} returns r: I32 do return x endfn"
        )),
        1
    );
    assert_eq!(
        deepest(&source(
            "type Box of {value: I32}\n  fn get takes {self: Self} returns r: I32 do self.value endfn\nendtype"
        )),
        1
    );
}

#[test]
fn a_type_and_a_pattern_nest_in_their_brackets() {
    assert_eq!(deepest(&source("let x: List(List(I32)) be [] endlet")), 2);
    assert_eq!(
        deepest(&source(
            "let n: I32 be match v case Option.some(Option.some(x)) 1i32 case _ 0i32 endmatch endlet"
        )),
        3
    );
}

#[test]
fn an_expression_construct_and_a_let_statement_are_forms() {
    assert_eq!(
        deepest(&source("let x: I32 be do do 1i32 enddo enddo endlet")),
        2
    );
    assert_eq!(
        deepest(&source(
            "let x: I32 be do let a be do 1i32 enddo endlet a enddo endlet"
        )),
        3
    );
    assert_eq!(
        deepest(&source(
            "let x: I32 be if c then 1i32 else if d then 2i32 else 3i32 endif endif endlet"
        )),
        2
    );
    assert_eq!(
        deepest(&source(
            "let g: Fn({I32} I32) be fn takes {x: I32} returns r: I32 do x endfn endlet"
        )),
        2
    );
}

#[test]
fn applications_field_reads_and_prefix_words_enclose_what_they_apply_to() {
    let text = "let x: I32 be f(a)(b) endlet";
    let document = source(text);
    assert_eq!(deepest(&document), 2);
    // `f` and `a` are nested two levels, `b` one.
    assert_eq!(nesting(&document, 1).first_over, Some(at(text, "f")));
    assert_eq!(deepest(&source("let x: I32 be x.a.b endlet")), 2);
    // `f` is enclosed by `do`, `return`, `try`, `await`, and its
    // application.
    assert_eq!(
        deepest(&source(
            "let x: I32 be do return try await f() enddo endlet"
        )),
        5
    );
}

#[test]
fn the_first_token_over_the_limit_is_found_in_source_order() {
    let text = "let x: List(List(I32)) be [[1i32] [[2i32]] [[[3i32]]]] endlet";
    let document = source(text);
    assert_eq!(deepest(&document), 4);
    assert_eq!(nesting(&document, 2).first_over, Some(at(text, "2i32")));
    assert_eq!(nesting(&document, 3).first_over, Some(at(text, "3i32")));
    assert_eq!(nesting(&document, 4).first_over, None);
}

#[test]
fn the_measure_needs_no_host_stack_in_proportion_to_depth() {
    const DEPTH: usize = 100_000;
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let text = format!(
                "let x: I32 be {}1i32{} endlet",
                "do ".repeat(DEPTH),
                " enddo".repeat(DEPTH)
            );
            let document = source(&text);
            let found = nesting(&document, 1_024);
            assert_eq!(found.deepest, DEPTH);
            // The 1,026th `do` is the first token that 1,025 forms enclose;
            // `let x: I32 be ` is 14 bytes and each `do ` is 3.
            let offset = 14 + 1_025 * 3;
            assert_eq!(found.first_over, Some(ByteSpan::new(offset, offset + 2)));
            let data_text = format!("{}1u8{}", "[".repeat(DEPTH), "]".repeat(DEPTH));
            assert_eq!(deepest(&data(&data_text)), DEPTH);
        })
        .expect("a thread");
    assert!(handle.join().is_ok());
}

#[test]
fn exactly_the_limit_passes_and_one_more_level_does_not() {
    let nested = |depth: usize| {
        format!(
            "let x: I32 be {}1i32{} endlet",
            "do ".repeat(depth),
            " enddo".repeat(depth)
        )
    };
    let at_limit = nested(1_024);
    assert_eq!(nesting(&source(&at_limit), 1_024).first_over, None);
    let over = nested(1_025);
    assert_eq!(
        nesting(&source(&over), 1_024).first_over,
        Some(at(&over, "1i32"))
    );
}

/// The examples of the definition, each token at the level 05 gives it (the
/// value is named `q` here, a letter no other token holds).
#[test]
fn the_examples_of_the_definition_count_as_written() {
    let examples = [
        ("let x: I32 be do [q] enddo endlet\n", "q", 2),
        ("fn f returns r: I32 do return try await q endfn\n", "q", 3),
        ("let x: I32 be q.a.b endlet\n", "q", 2),
        ("let x: I32 be q()() endlet\n", "q", 2),
        (
            "let x: I32 be if c then 1i32 else if d then 2i32 else 3i32 endif endif endlet\n",
            // Past a limit of 1 the first token is `d`, not the second `if`,
            // which the first `if` encloses: the second `if` is one level.
            "d",
            2,
        ),
    ];
    for (text, token, levels) in examples {
        let document = source(text);
        assert_eq!(deepest(&document), levels, "{text:?}");
        assert_eq!(
            nesting(&document, levels - 1).first_over,
            Some(at(text, token)),
            "{text:?}"
        );
        assert_eq!(nesting(&document, levels).first_over, None, "{text:?}");
    }
}
