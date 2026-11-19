//! The callable grammar: `fn` and `proc` declarations, members, and
//! anonymous expressions, their signatures, guards, and provider sections,
//! and `test`.

use vibra_diagnostics::DiagnosticCode::{
    self, NameReservedDeclaration, NameReservedLabel, SyntaxDuplicateSection,
    SyntaxInvalidForm, SyntaxInvalidSection, SyntaxUnmatchedDelimiter,
};
use vibra_syntax::source::{
    Callable, CallableKind, Declaration, ExprForm, Primary, SourceOptions, Statement,
    Test,
};
use vibra_syntax::{
    CstNode, Document, SyntaxKind, module, parse_source, parse_source_with,
};

use crate::support::codes;

fn source(text: &str) -> Document {
    parse_source(None, text.as_bytes())
}

fn library(text: &str) -> Document {
    parse_source_with(
        None,
        text.as_bytes(),
        SourceOptions {
            standard_library: true,
        },
    )
}

fn diagnostics(text: &str) -> Vec<(DiagnosticCode, usize, usize)> {
    codes(source(text).diagnostics())
}

fn library_diagnostics(text: &str) -> Vec<(DiagnosticCode, usize, usize)> {
    codes(library(text).diagnostics())
}

/// The span of the first occurrence of `part` in `text`.
fn at(text: &str, part: &str) -> (usize, usize) {
    let start = text.find(part).unwrap();
    (start, start + part.len())
}

/// The top-level `fn` and `proc` declarations of a document.
fn callables(document: &Document) -> Vec<Callable<'_>> {
    module(document)
        .unwrap()
        .declarations()
        .into_iter()
        .filter_map(|declaration| match declaration {
            Declaration::Fn(view) | Declaration::Proc(view) => Some(view),
            _ => None,
        })
        .collect()
}

fn tests(document: &Document) -> Vec<Test<'_>> {
    module(document)
        .unwrap()
        .declarations()
        .into_iter()
        .filter_map(|declaration| match declaration {
            Declaration::Test(view) => Some(view),
            _ => None,
        })
        .collect()
}

/// The node kinds of a tree, leaves as their text, trivia as `_`.
fn outline(node: &CstNode, text: &str) -> String {
    if node.kind().is_leaf() {
        return match node.kind() {
            SyntaxKind::Whitespace | SyntaxKind::Comment => "_".to_owned(),
            SyntaxKind::Missing => "?".to_owned(),
            _ => node.text(text).to_owned(),
        };
    }
    let kind = node
        .kind()
        .tree_kind()
        .unwrap_or("group")
        .trim_start_matches('@');
    let children: Vec<String> = node
        .children()
        .iter()
        .map(|child| outline(child, text))
        .collect();
    format!("({kind} {})", children.join(" "))
}

fn tree(text: &str) -> String {
    let document = source(text);
    outline(document.root().unwrap(), text)
}

#[test]
fn greet() {
    let text = "fn greet\n  takes {name: Str}\n  returns greeting: Str\n  visibility @public\n  do\n    return Str.concat(\"hello, \" name)\nendfn\n";
    let document = source(text);
    assert!(document.diagnostics().is_empty());
    assert_eq!(
        tree(text),
        "(source (fnDecl fn _ greet _ (signature takes _ (params { (path name) : _ (path Str) }) _ returns _ greeting : _ (path Str)) _ (visibility visibility _ @public) _ do _ (body return _ (postfix Str . concat (group ( (postfix \"hello, \") _ (postfix name) )))) _ endfn) _)"
    );
    let view = callables(&document)[0];
    assert_eq!(view.kind(), CallableKind::Fn);
    assert!(!view.is_anonymous());
    assert_eq!(view.name().unwrap().text(), "greet");
    let takes = view.takes();
    assert_eq!(takes.len(), 1);
    let (result, result_type) = view.result().unwrap();
    assert_eq!(result.text(), "greeting");
    assert_eq!(result_type.span(), {
        let start = text.find("Str\n  visibility").unwrap();
        vibra_diagnostics::ByteSpan::new(start, start + 3)
    });
    assert_eq!(view.visibility().unwrap().tag().unwrap().text(), "@public");
    assert_eq!(view.body().unwrap().statements().len(), 1);
}

#[test]
fn section_order_and_one_line() {
    for text in [
        "fn next takes {value: I32} returns r: I32 do return I32.add(value 1i32) endfn",
        "fn f returns r: I32 takes {x: I32} do return x endfn",
        "fn f takes {} returns r: I32 do return 1i32 endfn",
        "fn f labelled {} returns r: I32 do return 1i32 endfn",
    ] {
        assert!(diagnostics(text).is_empty(), "{text}");
    }
    let text = "fn f returns r: I32 takes {x: I32} do return x endfn";
    let document = source(text);
    let view = callables(&document)[0];
    assert_eq!(view.signatures().len(), 1);
    assert_eq!(view.takes().len(), 1);
    assert_eq!(view.result().unwrap().0.text(), "r");
    // Signature sections written apart form one `@signature` node each.
    let text =
        "fn f takes {x: I32} visibility @public returns r: I32 do return x endfn";
    let document = source(text);
    assert!(document.diagnostics().is_empty());
    let view = callables(&document)[0];
    assert_eq!(view.signatures().len(), 2);
    assert_eq!(view.takes().len(), 1);
    assert_eq!(view.result().unwrap().0.text(), "r");
}

#[test]
fn labelled_and_guards() {
    let text = "fn splitCents\n  takes {total: Cents parts: I32}\n  labelled {roundUp: Bool be false}\n  returns shares: Result(List(Cents) SplitError)\n  requires Ordered.greaterOrEqual(parts 1i32)\n    else Result.err(error: SplitError.noParts())\n  do\n    return Result.ok(value: divide(total parts roundUp: roundUp))\nendfn\n";
    let document = source(text);
    assert!(document.diagnostics().is_empty());
    let view = callables(&document)[0];
    assert_eq!(view.takes().len(), 2);
    let labelled = view.labelled();
    assert_eq!(labelled.len(), 1);
    assert_eq!(labelled[0].0.text(), "roundUp");
    let ExprForm::Postfix(default) = labelled[0].2.form() else {
        panic!("the default is a postfix form");
    };
    assert_eq!(default.dotted().as_deref(), Some("false"));
    let guard = view.requires().unwrap();
    assert_eq!(guard.keyword(), "requires");
    assert!(guard.predicate().is_some());
    assert!(guard.consequence().is_some());
    assert!(view.ensures().is_none());
    // `ensures` reads the same way, and an anonymous `fn` in a guard
    // operand belongs to the operand.
    let text = "fn f takes {x: I32} returns r: I32 ensures fn takes {v: I32} returns ok: Bool do return true endfn else 0i32 do return x endfn";
    let document = source(text);
    assert!(document.diagnostics().is_empty());
    let view = callables(&document)[0];
    let guard = view.ensures().unwrap();
    let ExprForm::Postfix(predicate) = guard.predicate().unwrap().form() else {
        panic!("the predicate is a postfix form");
    };
    assert!(matches!(predicate.primary(), Some(Primary::Callable(_))));
    assert!(guard.consequence().is_some());
}

#[test]
fn a_do_that_starts_a_guard_operand_is_a_block() {
    // A `do` where a guard's expression is expected opens a block; only a
    // `do` at a section boundary opens the body.
    for text in [
        "fn f returns r: Void requires do true enddo do return void endfn",
        "fn f returns r: Void ensures do true enddo do return void endfn",
        "proc p returns r: Void requires do true enddo do return void endproc",
        "let g: Fn({} Void) be fn returns r: Void requires do true enddo do return void endfn endlet",
    ] {
        let document = source(text);
        assert!(document.diagnostics().is_empty(), "{text}");
    }
    let text = "fn f returns r: Void requires do true enddo do return void endfn";
    let document = source(text);
    let view = callables(&document)[0];
    let guard = view.requires().unwrap();
    let ExprForm::Postfix(predicate) = guard.predicate().unwrap().form() else {
        panic!("the predicate is a postfix form");
    };
    assert!(matches!(predicate.primary(), Some(Primary::Do(_))));
    assert!(guard.consequence().is_none());
    assert!(view.body().is_some());
    // A block in the guard's `else`, and blocks in both operands.
    for text in [
        "fn f returns r: Void requires ready else do false enddo do return void endfn",
        "fn f returns r: Void requires do true enddo else do false enddo do return void endfn",
        "fn f returns r: Void requires try do true enddo do return void endfn",
    ] {
        let document = source(text);
        assert!(document.diagnostics().is_empty(), "{text}");
        let view = callables(&document)[0];
        let guard = view.requires().unwrap();
        assert!(guard.predicate().is_some(), "{text}");
        assert!(view.body().is_some(), "{text}");
    }
    let text =
        "fn f returns r: Void requires ready else do false enddo do return void endfn";
    let document = source(text);
    let guard = callables(&document)[0].requires().unwrap();
    let ExprForm::Postfix(consequence) = guard.consequence().unwrap().form() else {
        panic!("the consequence is a postfix form");
    };
    assert!(matches!(consequence.primary(), Some(Primary::Do(_))));
}

#[test]
fn the_signature_is_required_and_the_names_are_kept() {
    let text = "fn f takes {x: I32} do return x endfn";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 0, 37)]);
    let text = "fn f returns : I32 do return 1i32 endfn";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 0, 39)]);
    let text = "fn f takes x returns r: I32 do return 1i32 endfn";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 0, text.len())]);
    // A brace that breaks its entry rule is invalid over the brace.
    let text = "fn f takes {x} returns r: I32 do return 1i32 endfn";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 11, 14)]);
    let text = "fn f labelled {x: I32} returns r: I32 do return 1i32 endfn";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 14, 22)]);
    // A reserved word is recovered as the label, the binder, the result
    // name, the declaration name, and a type parameter.
    let text =
        "fn f labelled {role: Bool be false} returns r: Bool do return false endfn";
    assert_eq!(diagnostics(text), [(NameReservedLabel, 15, 19)]);
    let text = "fn f takes {return: I32} returns r: I32 do return 1i32 endfn";
    assert_eq!(diagnostics(text), [(NameReservedDeclaration, 12, 18)]);
    let text = "fn f returns return: I32 do return 1i32 endfn";
    let document = source(text);
    assert_eq!(
        codes(document.diagnostics()),
        [(NameReservedDeclaration, 13, 19)]
    );
    assert_eq!(callables(&document)[0].result().unwrap().0.text(), "return");
    let text = "fn try returns r: I32 do return 1i32 endfn";
    assert_eq!(diagnostics(text), [(NameReservedDeclaration, 3, 6)]);
    let text = "fn Fn returns r: I32 do return 1i32 endfn";
    assert_eq!(diagnostics(text), [(NameReservedDeclaration, 3, 5)]);
    let text = "fn f(try) returns r: I32 do return 1i32 endfn";
    assert_eq!(diagnostics(text), [(NameReservedDeclaration, 5, 8)]);
}

#[test]
fn generic_and_pattern_parameters() {
    let text = "fn first(T) takes {items: List(T)} returns head: Option(T) visibility @public do return items(0u64) endfn";
    let document = source(text);
    assert!(document.diagnostics().is_empty());
    let view = callables(&document)[0];
    let params: Vec<&str> = view.params().iter().map(|param| param.text()).collect();
    assert_eq!(params, ["T"]);
    let text = "fn f takes {{a b}: {I32 I32}} returns r: I32 do return a endfn";
    let document = source(text);
    assert!(document.diagnostics().is_empty());
    let takes = callables(&document)[0].takes();
    assert_eq!(takes.len(), 1);
    assert_eq!(takes[0].0.node().kind(), SyntaxKind::Brace);
    assert_eq!(
        diagnostics("fn f() returns r: I32 do return 1i32 endfn"),
        [(SyntaxInvalidForm, 4, 6)]
    );
}

#[test]
fn sections_a_form_does_not_admit() {
    let text = "fn f returns r: I32 performs {X} do return 1i32 endfn";
    assert_eq!(diagnostics(text), [(SyntaxInvalidSection, 20, 32)]);
    let text = "proc p returns r: Void native \"x\" do return void endproc";
    assert_eq!(diagnostics(text), [(SyntaxInvalidSection, 23, 33)]);
    for section in [
        "where ready",
        "role @bool",
        "representation @public",
        "of U8",
        "be 1i32",
    ] {
        let text = format!("fn f returns r: I32 {section} do return 1i32 endfn");
        assert_eq!(
            diagnostics(&text),
            [(SyntaxInvalidSection, 20, 20 + section.len())],
            "{text}"
        );
    }
    // A repeated guard is reported at the repeat.
    let text =
        "fn f takes {x: I32} returns r: I32 requires a requires b do return x endfn";
    assert_eq!(diagnostics(text), [(SyntaxDuplicateSection, 46, 54)]);
    // A guard is one expression and an optional `else` expression.
    let text = "fn f returns r: I32 requires visibility @public do return 1i32 endfn";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 20, 28)]);
    let text = "fn f returns r: I32 requires a b do return 1i32 endfn";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 20, 32)]);
    let text =
        "fn f returns r: I32 requires a else visibility @public do return 1i32 endfn";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 20, 35)]);
    // A `do` where the guard's expression is expected opens a block, so a
    // body written there is a block that `endfn` leaves open.
    for text in [
        "fn f returns r: I32 requires do return 1i32 endfn",
        "fn f returns r: I32 requires a else do return 1i32 endfn",
    ] {
        let (start, end) = at(text, "endfn");
        assert_eq!(
            diagnostics(text),
            [(SyntaxUnmatchedDelimiter, start, end)],
            "{text}"
        );
    }
    let text = "fn f returns r: I32 given {T} do return 1i32 endfn";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 26, 29)]);
    let text = "proc p returns r: Void performs X do return void endproc";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 23, 33)]);
    // A construct between the sections is invalid over the construct.
    let text = "fn f returns r: I32 fn g returns r: I32 do return 1i32 endfn do return 1i32 endfn";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 20, 60)]);
}

#[test]
fn external_definitions() {
    let text = "fn addChecked takes {left: I32 right: I32} returns sum: Result(I32 ArithmeticError) external @compiler symbol \"i32.addChecked\" endfn";
    let document = library(text);
    assert!(document.diagnostics().is_empty());
    let view = callables(&document)[0];
    assert_eq!(view.external().unwrap().tag().unwrap().text(), "@compiler");
    assert_eq!(
        view.symbol().unwrap().value().as_deref(),
        Some("i32.addChecked")
    );
    assert!(view.body().is_none());
    let bad = text.replace("@compiler", "@vendor");
    let (start, end) = at(&bad, "external @vendor");
    assert_eq!(library_diagnostics(&bad), [(SyntaxInvalidForm, start, end)]);
    let with_body = text.replace(" endfn", " do return 1i32 endfn");
    assert_eq!(
        library_diagnostics(&with_body),
        [(SyntaxInvalidForm, 0, with_body.len())]
    );
    let without_symbol = "fn f returns r: I32 external @compiler endfn";
    assert_eq!(
        library_diagnostics(without_symbol),
        [(SyntaxInvalidForm, 0, without_symbol.len())]
    );
    let without_external = "fn f returns r: I32 symbol \"s\" do return 1i32 endfn";
    assert_eq!(
        library_diagnostics(without_external),
        [(SyntaxInvalidForm, 0, without_external.len())]
    );
    let native = "fn f returns r: I32 native \"n\" do return 1i32 endfn";
    let document = library(native);
    assert!(document.diagnostics().is_empty());
    assert_eq!(
        callables(&document)[0].native().unwrap().string(),
        Some("\"n\"")
    );
    let host = "proc p returns r: Void external @host symbol \"h\" endproc";
    assert!(library_diagnostics(host).is_empty());
    // Outside the standard library, an external declaration is reported
    // once, at `external`; `native` and a lone `symbol` at the section.
    let text = "fn f returns r: I32 external @compiler symbol \"x\" endfn";
    assert_eq!(diagnostics(text), [(SyntaxInvalidSection, 20, 38)]);
    assert_eq!(diagnostics(native), [(SyntaxInvalidSection, 20, 30)]);
    assert_eq!(
        diagnostics(without_external),
        [(SyntaxInvalidSection, 20, 30)]
    );
}

#[test]
fn a_body_is_required_unless_abstract_or_external() {
    assert_eq!(
        diagnostics("fn f returns r: I32 endfn"),
        [(SyntaxInvalidForm, 20, 25)]
    );
    assert_eq!(
        diagnostics("proc p returns r: Void endproc"),
        [(SyntaxInvalidForm, 23, 30)]
    );
    // An empty body is grammatical.
    assert!(
        diagnostics(
            "interface I fn f takes {v: Self} returns r: Str do endfn endinterface"
        )
        .is_empty()
    );
    assert!(diagnostics("fn f returns r: I32 do endfn").is_empty());
    let text = "type A of U8\n  impl P\n    fn f takes {v: Self} returns r: U8 endfn\n  endimpl\nendtype";
    let (start, end) = at(text, "endfn");
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, start, end)]);
}

#[test]
fn members_and_implementation_members() {
    let text = "type User of {name: Str id: UserId}\n  fn nameLength\n    takes {user: Self}\n    returns length: U64\n    do\n      return Str.length(user.name)\n  endfn\n  impl Printable\n    fn render\n      takes {value: Self}\n      returns text: Str\n      do\n        return value.name\n    endfn\n  endimpl\nendtype\n";
    let document = source(text);
    assert!(document.diagnostics().is_empty());
    let declaration = module(&document).unwrap().declarations()[0];
    let Declaration::Type(view) = declaration else {
        panic!("a type");
    };
    let members = view.members();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].name().unwrap().text(), "nameLength");
    let impls = view.impls();
    let members = impls[0].members();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].name().unwrap().text(), "render");
    // A member of an `impl` declares no guard.
    let text = "type A of U8\n  impl P\n    fn f takes {v: Self} returns r: U8 requires true do return 1u8 endfn\n  endimpl\nendtype";
    let (start, end) = at(text, "requires true");
    assert_eq!(diagnostics(text), [(SyntaxInvalidSection, start, end)]);
    let text = "type A of U8\n  impl P\n    fn f takes {v: Self} returns r: U8 ensures true do return 1u8 endfn\n  endimpl\nendtype";
    let (start, end) = at(text, "ensures true");
    assert_eq!(diagnostics(text), [(SyntaxInvalidSection, start, end)]);
    let text = "type A of U8\n  impl P\n    fn f takes {v: Self} returns r: U8 where true do return 1u8 endfn\n  endimpl\nendtype";
    let (start, end) = at(text, "where true");
    assert_eq!(diagnostics(text), [(SyntaxInvalidSection, start, end)]);
    // A type member and an interface member may.
    assert!(
        diagnostics("type A of U8 fn f takes {v: Self} returns r: U8 requires true do return 1u8 endfn endtype")
            .is_empty()
    );
    // A named declaration inside a body is no expression.
    let text = "fn f returns r: I32 do fn g returns s: I32 do return 1i32 endfn return 1i32 endfn";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 23, 63)]);
    for inner in [
        "type B of U8 endtype",
        "interface I endinterface",
        "test \"t\" do endtest",
    ] {
        let text = format!("fn f returns r: I32 do {inner} return 1i32 endfn");
        assert_eq!(
            diagnostics(&text),
            [(SyntaxInvalidForm, 23, 23 + inner.len())],
            "{text}"
        );
    }
}

#[test]
fn anonymous_callables() {
    let text = "let inc: Fn({I32} I32) be fn takes {value: I32} returns next: I32 do return I32.add(value 1i32) endfn endlet";
    let document = source(text);
    assert!(document.diagnostics().is_empty());
    let Declaration::Let(value) = module(&document).unwrap().declarations()[0] else {
        panic!("a module value");
    };
    let ExprForm::Postfix(postfix) = value.value().unwrap().form() else {
        panic!("a postfix form");
    };
    let Some(Primary::Callable(callable)) = postfix.primary() else {
        panic!("an anonymous fn");
    };
    assert!(callable.is_anonymous());
    assert!(callable.name().is_none());
    assert_eq!(callable.node().kind(), SyntaxKind::AnonFn);
    assert_eq!(callable.result().unwrap().0.text(), "next");
    let text = "let p: Proc({Str} Void {Out}) be proc takes {s: Str} returns r: Void performs {Out} do return void endproc endlet";
    let document = source(text);
    assert!(document.diagnostics().is_empty());
    assert!(tree(text).contains("(anonProc proc _ (signature takes _ (params {"));
    // A name after the keyword makes it a declaration, no expression.
    let text = "let g: Fn({} I32) be fn h returns r: I32 do return 1i32 endfn endlet";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 21, 61)]);
    // An anonymous callable has no name, visibility, doc, or provider; a
    // `fn` has no `performs`.
    for section in [
        "visibility @public",
        "doc \"\"\"d\"\"\"",
        "performs {X}",
        "external @compiler",
    ] {
        let text = format!(
            "let f: Fn({{}} I32) be fn returns r: I32 {section} do return 1i32 endfn endlet"
        );
        assert_eq!(
            diagnostics(&text),
            [(SyntaxInvalidSection, 39, 39 + section.len())],
            "{text}"
        );
    }
    // Type parameters, `given`, and guards are admitted, and `do` is required.
    assert!(
        diagnostics("let f: Fn({} I32) be fn(T) returns r: I32 given {T: Any} requires a else b ensures c do return 1i32 endfn endlet")
            .is_empty()
    );
    let text = "let f: Fn({} I32) be fn returns r: I32 endfn endlet";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 39, 44)]);
}

#[test]
fn test_declarations() {
    let text =
        "test \"adds one\"\n  do\n    assert.equal(2u64 add(1u64 1u64))\nendtest";
    let document = source(text);
    assert!(document.diagnostics().is_empty());
    let view = tests(&document)[0];
    assert_eq!(view.name(), Some("\"adds one\""));
    assert_eq!(view.name_value().as_deref(), Some("adds one"));
    assert!(view.takes().is_none());
    let statements = view.body().unwrap().statements();
    assert!(matches!(statements.as_slice(), [Statement::Expr(_)]));
    let text = "test \"reads\" takes {world: TestWorld} performs {TestWorld} do assert.true(true) endtest";
    let document = source(text);
    assert!(document.diagnostics().is_empty());
    assert_eq!(
        tree(text),
        "(source (test test _ \"reads\" _ takes _ (params { (path world) : _ (path TestWorld) }) _ (performs performs _ (row { (path TestWorld) })) _ do _ (body (postfix assert . true (group ( (postfix true) )))) _ endtest))"
    );
    let view = tests(&document)[0];
    assert_eq!(view.takes().unwrap().0.node().kind(), SyntaxKind::Path);
    assert_eq!(view.performs().unwrap().paths()[0].dotted(), "TestWorld");
    // `takes` admits exactly one parameter.
    for params in ["{a: A b: B}", "{}"] {
        let text = format!("test \"x\" takes {params} do assert.true(true) endtest");
        assert_eq!(
            diagnostics(&text),
            [(SyntaxInvalidForm, 0, text.len())],
            "{text}"
        );
    }
    // A test has a name and a body, and no signature but `takes`.
    let text = "test do endtest";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 0, 15)]);
    let text = "test x do endtest";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 0, 17)]);
    let text = "test \"t\" endtest";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 0, 16)]);
    let text = "test \"t\" returns r: Void do endtest";
    assert_eq!(diagnostics(text), [(SyntaxInvalidSection, 9, 24)]);
}

#[test]
fn fn_recovery() {
    let text = "fn f takes {x: I32 returns r: I32 do return x endfn\nfn g returns r: I32 do return 1i32 endfn";
    let document = source(text);
    assert_eq!(
        codes(document.diagnostics()),
        [(SyntaxUnmatchedDelimiter, 46, 51)]
    );
    // The innermost open construct is the `do` block the brace holds.
    let related = document.diagnostics()[0].related();
    assert_eq!(related.len(), 1);
    assert_eq!((related[0].span.start(), related[0].span.end()), (34, 36));
    let views = callables(&document);
    assert_eq!(views.len(), 2);
    assert_eq!(views[1].name().unwrap().text(), "g");
    assert_eq!(views[1].result().unwrap().0.text(), "r");
    assert_eq!(views[1].body().unwrap().statements().len(), 1);
}
