//! Expressions, statements, patterns, and the module value `let`.

use vibra_diagnostics::DiagnosticCode::{
    self, NameReservedDeclaration, NameReservedLabel, SyntaxDuplicateSection,
    SyntaxInvalidForm, SyntaxInvalidSection, SyntaxMisplacedBinding,
    SyntaxUnmatchedDelimiter,
};
use vibra_syntax::source::{
    Declaration, Expr, ExprForm, ModuleLet, Pattern, PatternForm, Postfix, Primary,
    Statement, Suffix,
};
use vibra_syntax::{Document, SyntaxKind, module, parse_source, tree_document};

use crate::support::codes;

fn source(text: &str) -> Document {
    parse_source(None, text.as_bytes())
}

fn diagnostics(text: &str) -> Vec<(DiagnosticCode, usize, usize)> {
    codes(source(text).diagnostics())
}

/// The module values of a document.
fn values(document: &Document) -> Vec<ModuleLet<'_>> {
    module(document)
        .unwrap()
        .declarations()
        .into_iter()
        .filter_map(|declaration| match declaration {
            Declaration::Let(view) => Some(view),
            _ => None,
        })
        .collect()
}

/// The postfix form of an expression with no prefix.
fn postfix<'a>(expr: &Expr<'a>) -> Postfix<'a> {
    match expr.form() {
        ExprForm::Postfix(postfix) => postfix,
        other => panic!("not a postfix form: {other:?}"),
    }
}

/// The value of the first module value of an accepted document.
fn accepted(text: &str) -> Document {
    let document = source(text);
    assert!(
        document.diagnostics().is_empty(),
        "{text:?}: {:?}",
        codes(document.diagnostics())
    );
    document
}

fn name(primary: Option<Primary<'_>>) -> String {
    match primary {
        Some(Primary::Name(name)) => name.text().to_owned(),
        other => panic!("not a name: {other:?}"),
    }
}

/// Walks every view under an expression, iteratively, so that the views of
/// any tree are total.
pub(crate) fn walk(expr: Option<Expr<'_>>) {
    enum Work<'a> {
        Expr(Expr<'a>),
        Pattern(Pattern<'a>),
        Statements(Vec<Statement<'a>>),
    }
    let mut work: Vec<Work<'_>> = expr.into_iter().map(Work::Expr).collect();
    while let Some(item) = work.pop() {
        match item {
            Work::Statements(statements) => {
                for statement in statements {
                    match statement {
                        Statement::Expr(expr) => work.push(Work::Expr(expr)),
                        Statement::Let(binding) => {
                            work.extend(binding.pattern().map(Work::Pattern));
                            work.extend(binding.value().map(Work::Expr));
                            let _ = binding.written_type();
                            if let Some(body) = binding.fallback() {
                                work.push(Work::Statements(body.statements()));
                            }
                        }
                    }
                }
            }
            Work::Pattern(pattern) => match pattern.form() {
                PatternForm::Apply {
                    positional,
                    labelled,
                    ..
                } => {
                    work.extend(positional.into_iter().map(Work::Pattern));
                    work.extend(labelled.into_iter().map(|(_, p)| Work::Pattern(p)));
                }
                PatternForm::Tuple(items) | PatternForm::List(items) => {
                    work.extend(items.into_iter().map(Work::Pattern));
                }
                PatternForm::Record(fields) => {
                    work.extend(fields.into_iter().map(|(_, p)| Work::Pattern(p)));
                }
                PatternForm::Discard
                | PatternForm::Literal(_)
                | PatternForm::Path(_) => {}
            },
            Work::Expr(expr) => match expr.form() {
                ExprForm::Return(inner)
                | ExprForm::Try(inner)
                | ExprForm::Await(inner) => {
                    work.push(Work::Expr(inner));
                }
                ExprForm::Let(binding) => {
                    work.push(Work::Statements(vec![Statement::Let(binding)]));
                }
                ExprForm::Postfix(postfix) => {
                    for suffix in postfix.suffixes() {
                        if let Suffix::Call(arguments) = suffix {
                            work.extend(
                                arguments.into_iter().map(|a| Work::Expr(a.value)),
                            );
                        }
                    }
                    match postfix.primary() {
                        Some(Primary::Tuple(items) | Primary::List(items)) => {
                            work.extend(items.into_iter().map(Work::Expr));
                        }
                        Some(Primary::Record(fields)) => {
                            work.extend(fields.into_iter().map(|(_, e)| Work::Expr(e)));
                        }
                        Some(Primary::Dict(entries)) => {
                            for (key, value) in entries {
                                work.push(Work::Expr(key));
                                work.push(Work::Expr(value));
                            }
                        }
                        Some(Primary::If(view)) => {
                            work.extend(view.condition().map(Work::Expr));
                            for body in [view.then_body(), view.else_body()]
                                .into_iter()
                                .flatten()
                            {
                                work.push(Work::Statements(body.statements()));
                            }
                        }
                        Some(Primary::Match(view)) => {
                            work.extend(view.subject().map(Work::Expr));
                            for arm in view.arms() {
                                work.extend(arm.pattern().map(Work::Pattern));
                                if let Some(body) = arm.body() {
                                    work.push(Work::Statements(body.statements()));
                                }
                            }
                        }
                        Some(Primary::Do(view)) => {
                            if let Some(body) = view.body() {
                                work.push(Work::Statements(body.statements()));
                            }
                        }
                        _ => {}
                    }
                }
            },
        }
    }
}

#[test]
fn application() {
    let document = accepted("let a: I32 be choose(c)(x) endlet");
    let value = values(&document)[0].value().unwrap();
    let form = postfix(&value);
    assert_eq!(name(form.primary()), "choose");
    let suffixes = form.suffixes();
    assert_eq!(suffixes.len(), 2);
    for (suffix, argument) in suffixes.iter().zip(["c", "x"]) {
        let Suffix::Call(arguments) = suffix else {
            panic!("an application");
        };
        assert_eq!(arguments.len(), 1);
        assert_eq!(name(postfix(&arguments[0].value).primary()), argument);
    }
    // One `@postfix` holds the callee and both argument lists.
    let tree = tree_document(&document).unwrap();
    assert_eq!(tree.matches("@postfix").count(), 3);
}

#[test]
fn nullary_and_value() {
    let document = accepted("let a: I32 be f() endlet\nlet g: Fn({} I32) be f endlet");
    let lets = values(&document);
    let call = lets[0].value().unwrap();
    assert!(
        matches!(postfix(&call).suffixes().as_slice(), [Suffix::Call(arguments)] if arguments.is_empty())
    );
    let value = lets[1].value().unwrap();
    assert!(postfix(&value).suffixes().is_empty());
    assert_eq!(postfix(&value).dotted().as_deref(), Some("f"));
}

#[test]
fn labelled_arguments_keep_their_order() {
    let document =
        accepted("let u: User be User(id: UserId(1u64) name: \"Ada\") endlet");
    let value = values(&document)[0].value().unwrap();
    let [Suffix::Call(arguments)] =
        <[Suffix<'_>; 1]>::try_from(postfix(&value).suffixes()).unwrap()
    else {
        panic!("one application");
    };
    let labels: Vec<&str> = arguments
        .iter()
        .map(|argument| argument.label.unwrap().text())
        .collect();
    assert_eq!(labels, ["id", "name"]);
}

#[test]
fn field_path() {
    let document = accepted("let n: U64 be Str.length(user.name) endlet");
    let value = values(&document)[0].value().unwrap();
    let form = postfix(&value);
    assert_eq!(name(form.primary()), "Str");
    let suffixes = form.suffixes();
    let [Suffix::Field(field), Suffix::Call(arguments)] = suffixes.as_slice() else {
        panic!("a field read, then an application");
    };
    assert_eq!(field.text(), "length");
    assert_eq!(
        postfix(&arguments[0].value).dotted().as_deref(),
        Some("user.name")
    );
}

#[test]
fn products() {
    let document = accepted(
        "let t: {Str U64} be {\"Ada\" 42u64} endlet\nlet r: {name: Str id: U64} be {name: \"Ada\" id: 42u64} endlet",
    );
    let lets = values(&document);
    let tuple = lets[0].value().unwrap();
    assert!(
        matches!(postfix(&tuple).primary(), Some(Primary::Tuple(items)) if items.len() == 2)
    );
    let record = lets[1].value().unwrap();
    let Some(Primary::Record(fields)) = postfix(&record).primary() else {
        panic!("a record value");
    };
    let labels: Vec<&str> = fields.iter().map(|(label, _)| label.text()).collect();
    assert_eq!(labels, ["name", "id"]);
}

#[test]
fn collections() {
    let document = accepted(
        "let l: List(I32) be [1i32 2i32] endlet\nlet d: Dict(Str Str) be [\"name\": \"Ada\" \"role\": \"dev\"] endlet\nlet e: Dict(Str Str) be [:] endlet\nlet z: List(I32) be [] endlet",
    );
    let primaries: Vec<Primary<'_>> = values(&document)
        .iter()
        .map(|value| postfix(&value.value().unwrap()).primary().unwrap())
        .collect();
    assert!(matches!(&primaries[0], Primary::List(items) if items.len() == 2));
    assert!(matches!(&primaries[1], Primary::Dict(entries) if entries.len() == 2));
    assert!(matches!(&primaries[2], Primary::Dict(entries) if entries.is_empty()));
    assert!(matches!(&primaries[3], Primary::List(items) if items.is_empty()));
}

#[test]
fn a_mixed_brace_or_bracket_form_is_invalid_over_the_form() {
    assert_eq!(
        diagnostics("let r: {name: Str} be {name: \"Ada\" 1u8} endlet"),
        [(SyntaxInvalidForm, 22, 39)]
    );
    assert_eq!(
        diagnostics("let r: {Str} be {\"Ada\" name: 1u8} endlet"),
        [(SyntaxInvalidForm, 16, 33)]
    );
    assert_eq!(
        diagnostics("let d: Dict(Str Str) be [\"a\": \"b\" \"c\"] endlet"),
        [(SyntaxInvalidForm, 24, 38)]
    );
    assert_eq!(
        diagnostics("let d: Dict(Str Str) be [: \"c\"] endlet"),
        [(SyntaxInvalidForm, 24, 31)]
    );
}

#[test]
fn if_expression() {
    let document =
        accepted("let label: Str be if ready then \"go\" else \"wait\" endif endlet");
    let value = values(&document)[0].value().unwrap();
    let Some(Primary::If(view)) = postfix(&value).primary() else {
        panic!("an `if`");
    };
    assert_eq!(
        postfix(&view.condition().unwrap()).dotted().as_deref(),
        Some("ready")
    );
    assert_eq!(view.then_body().unwrap().statements().len(), 1);
    assert_eq!(view.else_body().unwrap().statements().len(), 1);
    // `then` and a condition are required; an empty body is a body.
    accepted("let a: Void be if ready then endif endlet");
    assert_eq!(
        diagnostics("let a: I32 be if ready 1i32 endif endlet"),
        [(SyntaxInvalidForm, 14, 33)]
    );
    assert_eq!(
        diagnostics("let a: I32 be if then 1i32 endif endlet"),
        [(SyntaxInvalidForm, 14, 32)]
    );
}

#[test]
fn match_arms_end_at_the_next_case() {
    let document = accepted(
        "let n: U64 be match value\n  case Option.some(value: text)\n    Str.length(text)\n  case Option.none()\n    0u64\nendmatch endlet",
    );
    let value = values(&document)[0].value().unwrap();
    let Some(Primary::Match(view)) = postfix(&value).primary() else {
        panic!("a `match`");
    };
    let arms = view.arms();
    assert_eq!(arms.len(), 2);
    for arm in &arms {
        assert_eq!(arm.body().unwrap().statements().len(), 1);
    }
    let PatternForm::Apply { path, labelled, .. } = arms[0].pattern().unwrap().form()
    else {
        panic!("a constructor pattern");
    };
    assert_eq!(path.dotted(), "Option.some");
    assert_eq!(labelled[0].0.text(), "value");
    let PatternForm::Apply {
        positional,
        labelled,
        ..
    } = arms[1].pattern().unwrap().form()
    else {
        panic!("a constructor pattern");
    };
    assert!(positional.is_empty() && labelled.is_empty());
}

#[test]
fn a_match_with_no_arm_is_invalid() {
    assert_eq!(
        diagnostics("let n: U64 be match value endmatch endlet"),
        [(SyntaxInvalidForm, 14, 34)]
    );
    // Neither a subject nor a pattern is optional.
    assert_eq!(
        diagnostics("let n: U64 be match case _ 0u64 endmatch endlet"),
        [(SyntaxInvalidForm, 14, 40)]
    );
    assert_eq!(
        diagnostics("let n: U64 be match v case endmatch endlet"),
        [(SyntaxInvalidForm, 14, 35)]
    );
}

#[test]
fn do_block() {
    let document = accepted(
        "let n: I32 be do\n  let a be 1i32 endlet\n  I32.add(a a)\nenddo endlet",
    );
    let value = values(&document)[0].value().unwrap();
    let Some(Primary::Do(view)) = postfix(&value).primary() else {
        panic!("a `do` block");
    };
    let statements = view.body().unwrap().statements();
    assert!(matches!(
        statements.as_slice(),
        [Statement::Let(_), Statement::Expr(_)]
    ));
    // A body holds statements and nothing else.
    assert_eq!(
        diagnostics("let n: I32 be do then enddo endlet"),
        [(SyntaxInvalidForm, 14, 27)]
    );
}

#[test]
fn let_else() {
    let document = accepted(
        "let n: I32 be do\n  let Option.some(value: head) be items(0u64) else\n    return 0i32\n  endlet\n  head\nenddo endlet",
    );
    let value = values(&document)[0].value().unwrap();
    let Some(Primary::Do(view)) = postfix(&value).primary() else {
        panic!("a `do` block");
    };
    let statements = view.body().unwrap().statements();
    let Statement::Let(binding) = &statements[0] else {
        panic!("a `let`");
    };
    assert!(matches!(
        binding.pattern().unwrap().form(),
        PatternForm::Apply { .. }
    ));
    assert!(binding.written_type().is_none());
    assert!(binding.value().is_some());
    let fallback = binding.fallback().unwrap().statements();
    let [Statement::Expr(fallback)] = fallback.as_slice() else {
        panic!("one statement");
    };
    assert!(matches!(fallback.form(), ExprForm::Return(_)));
    // A written type sits between the pattern and `be`.
    let document =
        accepted("let n: I32 be do let a: I32 be 1i32 endlet a enddo endlet");
    let value = values(&document)[0].value().unwrap();
    let Some(Primary::Do(view)) = postfix(&value).primary() else {
        panic!("a `do` block");
    };
    let Statement::Let(binding) = &view.body().unwrap().statements()[0] else {
        panic!("a `let`");
    };
    assert!(binding.written_type().is_some());
}

#[test]
fn a_let_in_an_operand_position_is_misplaced() {
    let text = "let n: I32 be f(let a be 1i32 endlet) endlet";
    let document = source(text);
    assert_eq!(
        codes(document.diagnostics()),
        [(SyntaxMisplacedBinding, 16, 19)]
    );
    let value = values(&document)[0].value().unwrap();
    let [Suffix::Call(arguments)] =
        <[Suffix<'_>; 1]>::try_from(postfix(&value).suffixes()).unwrap()
    else {
        panic!("one application");
    };
    assert!(matches!(arguments[0].value.form(), ExprForm::Let(_)));
    assert_eq!(
        diagnostics("let n: I32 be if let a be 1i32 endlet then a endif endlet"),
        [(SyntaxMisplacedBinding, 17, 20)]
    );
    assert_eq!(
        diagnostics("let n: I32 be let a be 1i32 endlet endlet"),
        [(SyntaxMisplacedBinding, 14, 17)]
    );
}

#[test]
fn a_let_without_be_is_invalid_over_the_let() {
    assert_eq!(
        diagnostics("let n: I32 be do let a 1i32 endlet a enddo endlet"),
        [(SyntaxInvalidForm, 17, 34)]
    );
    // A local `let` admits no section but `be`.
    assert_eq!(
        diagnostics(
            "let n: I32 be do let a be 1i32 visibility @public endlet a enddo endlet"
        ),
        [(SyntaxInvalidSection, 31, 49)]
    );
    assert_eq!(
        diagnostics("let n: I32 be do let a be 1i32 be 2i32 endlet a enddo endlet"),
        [(SyntaxDuplicateSection, 31, 33)]
    );
    // An unclosed `let` is an unmatched delimiter, not checked further.
    assert_eq!(
        codes(source("let n: I32 be do let a be enddo endlet").diagnostics()),
        [(SyntaxUnmatchedDelimiter, 26, 31)]
    );
}

#[test]
fn return_try_await_nest_as_prefixes() {
    let document = accepted("let n: I32 be do return try await f() enddo endlet");
    let value = values(&document)[0].value().unwrap();
    let Some(Primary::Do(view)) = postfix(&value).primary() else {
        panic!("a `do` block");
    };
    let statements = view.body().unwrap().statements();
    let [Statement::Expr(expr)] = statements.as_slice() else {
        panic!("one statement");
    };
    let ExprForm::Return(operand) = expr.form() else {
        panic!("return");
    };
    let ExprForm::Try(operand) = operand.form() else {
        panic!("try");
    };
    let ExprForm::Await(operand) = operand.form() else {
        panic!("await");
    };
    assert_eq!(name(postfix(&operand).primary()), "f");
    // A prefix needs an operand.
    assert_eq!(
        diagnostics("let n: I32 be do return enddo endlet"),
        [(SyntaxInvalidForm, 14, 29)]
    );
}

#[test]
fn a_paren_where_an_expression_is_expected_is_invalid_once() {
    assert_eq!(
        diagnostics("let n: I32 be (a) endlet"),
        [(SyntaxInvalidForm, 14, 17)]
    );
    assert_eq!(
        diagnostics("let n: I32 be do (a) b enddo endlet"),
        [(SyntaxInvalidForm, 17, 20)]
    );
}

#[test]
fn as_is_reported_at_the_as() {
    assert_eq!(
        diagnostics("let n: I32 be x as I32 endlet"),
        [(SyntaxInvalidForm, 16, 18)]
    );
}

#[test]
fn a_reserved_label_is_recovered() {
    let document = source("let u: User be User(role: 1u8) endlet");
    assert_eq!(codes(document.diagnostics()), [(NameReservedLabel, 20, 24)]);
    let value = values(&document)[0].value().unwrap();
    let [Suffix::Call(arguments)] =
        <[Suffix<'_>; 1]>::try_from(postfix(&value).suffixes()).unwrap()
    else {
        panic!("one application");
    };
    let label = arguments[0].label.unwrap();
    assert_eq!(label.text(), "role");
    assert!(label.is_reserved());
    assert_eq!(
        diagnostics("let r: R be {doc: 1u8} endlet"),
        [(NameReservedLabel, 13, 16)]
    );
}

/// A module value whose value is a `match` over `v` with one arm per
/// pattern, each with the body `0u8`.
fn match_text(patterns: &[&str]) -> String {
    let arms: String = patterns
        .iter()
        .map(|pattern| format!("  case {pattern}\n    0u8\n"))
        .collect();
    format!("let n: U8 be match v\n{arms}endmatch endlet")
}

#[test]
fn patterns() {
    let text = match_text(&[
        "_",
        "0i32",
        "\"a\"",
        "'x'",
        "@ok",
        "Lookup.found(value: v)",
        "User(name: n)",
        "Celsius(c)",
        "Pair(a b)",
        "{p q}",
        "{name: p}",
        "[p q]",
    ]);
    let document = accepted(&text);
    let value = values(&document)[0].value().unwrap();
    let Some(Primary::Match(view)) = postfix(&value).primary() else {
        panic!("a `match`");
    };
    let forms: Vec<PatternForm<'_>> = view
        .arms()
        .iter()
        .map(|arm| arm.pattern().unwrap().form())
        .collect();
    assert_eq!(forms.len(), 12);
    assert!(matches!(forms[0], PatternForm::Discard));
    let literal_kinds: Vec<SyntaxKind> = forms[1..5]
        .iter()
        .map(|form| match form {
            PatternForm::Literal(node) => node.kind(),
            other => panic!("not a literal: {other:?}"),
        })
        .collect();
    assert_eq!(
        literal_kinds,
        [
            SyntaxKind::Integer,
            SyntaxKind::String,
            SyntaxKind::Character,
            SyntaxKind::Atom
        ]
    );
    let applications: Vec<(String, usize, usize)> = forms[5..9]
        .iter()
        .map(|form| match form {
            PatternForm::Apply {
                path,
                positional,
                labelled,
            } => (path.dotted(), positional.len(), labelled.len()),
            other => panic!("not a constructor: {other:?}"),
        })
        .collect();
    assert_eq!(
        applications,
        [
            ("Lookup.found".to_owned(), 0, 1),
            ("User".to_owned(), 0, 1),
            ("Celsius".to_owned(), 1, 0),
            ("Pair".to_owned(), 2, 0)
        ]
    );
    assert!(matches!(&forms[9], PatternForm::Tuple(items) if items.len() == 2));
    assert!(matches!(&forms[10], PatternForm::Record(fields) if fields.len() == 1));
    assert!(matches!(&forms[11], PatternForm::List(items) if items.len() == 2));
    // A single name is a path, a binder or a nullary constructor.
    let document = accepted(&match_text(&["x", "Color.red"]));
    let value = values(&document)[0].value().unwrap();
    let Some(Primary::Match(view)) = postfix(&value).primary() else {
        panic!("a `match`");
    };
    for (arm, dotted) in view.arms().iter().zip(["x", "Color.red"]) {
        let PatternForm::Path(path) = arm.pattern().unwrap().form() else {
            panic!("a path");
        };
        assert_eq!(path.dotted(), dotted);
    }
}

#[test]
fn a_mixed_constructor_pattern_is_invalid_over_its_form() {
    let text = match_text(&["User(n id: i)"]);
    let start = text.find("(n id").unwrap();
    assert_eq!(diagnostics(&text), [(SyntaxInvalidForm, start, start + 9)]);
    let text = match_text(&["{n id: i}"]);
    let start = text.find("{n id").unwrap();
    assert_eq!(diagnostics(&text), [(SyntaxInvalidForm, start, start + 9)]);
    let text = match_text(&["[id: i]"]);
    let start = text.find("[id").unwrap();
    assert_eq!(diagnostics(&text), [(SyntaxInvalidForm, start, start + 7)]);
}

#[test]
fn a_reserved_binder_is_recovered() {
    let text = match_text(&["Lookup.found(value: where)"]);
    let document = source(&text);
    let at = text.find("where").unwrap();
    assert_eq!(
        codes(document.diagnostics()),
        [(NameReservedDeclaration, at, at + 5)]
    );
    let value = values(&document)[0].value().unwrap();
    let Some(Primary::Match(view)) = postfix(&value).primary() else {
        panic!("a `match`");
    };
    let PatternForm::Apply { labelled, .. } = view.arms()[0].pattern().unwrap().form()
    else {
        panic!("a constructor pattern");
    };
    let PatternForm::Path(binder) = labelled[0].1.form() else {
        panic!("a binder");
    };
    assert!(binder.segments()[0].is_reserved());
    let text = match_text(&["{then: p}"]);
    let at = text.find("then").unwrap();
    assert_eq!(diagnostics(&text), [(NameReservedLabel, at, at + 4)]);
    let text = "let n: U8 be do let enddo be 1u8 endlet 0u8 enddo endlet";
    let at = text.find("enddo be").unwrap();
    assert_eq!(diagnostics(text), [(NameReservedDeclaration, at, at + 5)]);
}

#[test]
fn a_float_or_void_is_no_pattern() {
    for pattern in ["1.5f64", "void"] {
        let text = match_text(&[pattern]);
        let start = text.find("match").unwrap();
        let end = text.find("endmatch").unwrap() + 8;
        assert_eq!(
            diagnostics(&text),
            [(SyntaxInvalidForm, start, end)],
            "{pattern}"
        );
    }
}

#[test]
fn module_value() {
    let document = accepted("let defaultRetries: U8 be 3u8 endlet");
    let view = values(&document)[0];
    assert_eq!(view.name().unwrap().text(), "defaultRetries");
    assert!(view.written_type().is_some());
    assert!(matches!(
        postfix(&view.value().unwrap()).primary(),
        Some(Primary::Literal(node)) if node.kind() == SyntaxKind::Integer
    ));
    let document = accepted(
        "let limit: U8\n  visibility @public\n  doc \"\"\"A limit.\"\"\"\n  be 3u8\nendlet",
    );
    let view = values(&document)[0];
    assert_eq!(view.visibility().unwrap().tag().unwrap().text(), "@public");
    assert_eq!(
        view.doc().unwrap().doc_string(),
        Some("\"\"\"A limit.\"\"\"")
    );
    assert!(view.value().is_some());
    // The sections admit their own operands only.
    assert_eq!(
        diagnostics("let a: U8 visibility @nowhere be 1u8 endlet"),
        [(SyntaxInvalidForm, 10, 29)]
    );
    assert_eq!(
        diagnostics("let a: U8 where ready be 1u8 endlet"),
        [(SyntaxInvalidSection, 10, 21)]
    );
    assert_eq!(
        diagnostics("let a: U8 be 1u8 be 2u8 endlet"),
        [(SyntaxDuplicateSection, 17, 19)]
    );
    assert_eq!(
        diagnostics("let do: U8 be 1u8 endlet"),
        [(NameReservedDeclaration, 4, 6)]
    );
}

#[test]
fn a_module_value_is_a_name_a_type_and_no_fallback() {
    for text in [
        "let {a b}: {U8 U8} be {1u8 2u8} endlet",
        "let a be 1u8 endlet",
        "let a: U8 be 1u8 else return 0u8 endlet",
        "let a: U8 endlet",
        "let a: U8 be endlet",
    ] {
        assert_eq!(
            diagnostics(text),
            [(SyntaxInvalidForm, 0, text.len())],
            "{text}"
        );
    }
}

#[test]
fn a_doc_string_outside_doc_is_invalid() {
    assert_eq!(
        diagnostics("let a: Str be \"\"\"x\"\"\" endlet"),
        [(SyntaxInvalidForm, 0, 28)]
    );
}

#[test]
fn an_anonymous_callable_is_read_in_the_callable_grammar() {
    let text = "let f: Fn({} I32) be fn returns r: I32 do return 1i32 endfn endlet";
    assert!(diagnostics(text).is_empty());
    // Without `do`, it is invalid at its closer.
    let text = "let f: Fn({} Void) be fn returns r: Void endfn endlet";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 41, 46)]);
    // A named declaration is no expression.
    let text = "let f: U8 be type A of U8 endtype endlet";
    assert_eq!(diagnostics(text), [(SyntaxInvalidForm, 13, 33)]);
}

#[test]
fn recovery_keeps_the_next_value_exact() {
    let text = "let a: I32 be f( endlet\nlet b: I32 be 1i32 endlet";
    let document = source(text);
    let diagnostics = document.diagnostics();
    assert_eq!(codes(diagnostics), [(SyntaxUnmatchedDelimiter, 17, 23)]);
    let related: Vec<(usize, usize)> = diagnostics[0]
        .related()
        .iter()
        .map(|related| (related.span.start(), related.span.end()))
        .collect();
    assert_eq!(related, [(15, 16)]);
    let lets = values(&document);
    assert_eq!(lets.len(), 2);
    assert_eq!(lets[1].name().unwrap().text(), "b");
    assert_eq!(lets[1].node().span().start(), 24);
    assert_eq!(lets[1].node().span().end(), text.len());
}

#[test]
fn deep_expression_nesting_is_shaped_without_recursion() {
    const DEPTH: usize = 100_000;
    for (open, close) in [
        ("if c then ", " endif"),
        ("do ", " enddo"),
        ("f(", ")"),
        ("{", "}"),
        ("[", "]"),
        ("match v case _ ", " endmatch"),
    ] {
        let text = format!(
            "let a: I32 be {}x{} endlet",
            open.repeat(DEPTH),
            close.repeat(DEPTH)
        );
        let document = source(&text);
        assert!(document.diagnostics().is_empty(), "{open}");
        assert!(tree_document(&document).unwrap().len() > DEPTH);
        drop(document);
    }
    let text = format!("let a: I32 be {}x endlet", "return ".repeat(DEPTH));
    let document = source(&text);
    assert!(document.diagnostics().is_empty());
    walk(values(&document)[0].value());
    let text = format!(
        "let a: I32 be match v case {}x{} 0i32 endmatch endlet",
        "[".repeat(DEPTH),
        "]".repeat(DEPTH)
    );
    assert!(source(&text).diagnostics().is_empty());
}
