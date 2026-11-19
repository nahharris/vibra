//! The type grammar: type expressions, `type`, `interface`, `impl`, their
//! sections, and recovery.

use vibra_diagnostics::DiagnosticCode::{
    self, NameReservedDeclaration, NameReservedLabel, SyntaxDuplicateSection,
    SyntaxInvalidForm, SyntaxInvalidSection, SyntaxUnmatchedDelimiter, ToolUnavailable,
};
use vibra_syntax::source::keyword::{
    ADMISSIBLE_SECTIONS, SECTIONS, TAG_SETS, TYPE_HEADS,
};
use vibra_syntax::source::{
    Declaration, Inputs, InterfaceDecl, Row, SourceOptions, TypeBody, TypeDecl,
    TypeExpr, TypeForm,
};
use vibra_syntax::{
    Document, SyntaxKind, module, parse_source, parse_source_with, tree_document,
};

use crate::support::{Random, codes};

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

/// The type declarations of a document.
fn types(document: &Document) -> Vec<TypeDecl<'_>> {
    module(document)
        .unwrap()
        .declarations()
        .into_iter()
        .filter_map(|declaration| match declaration {
            Declaration::Type(view) => Some(view),
            _ => None,
        })
        .collect()
}

fn interfaces(document: &Document) -> Vec<InterfaceDecl<'_>> {
    module(document)
        .unwrap()
        .declarations()
        .into_iter()
        .filter_map(|declaration| match declaration {
            Declaration::Interface(view) => Some(view),
            _ => None,
        })
        .collect()
}

fn show_inputs(inputs: Inputs<'_>) -> String {
    let mut parts: Vec<String> = inputs.positional().into_iter().map(show).collect();
    parts.extend(
        inputs
            .labelled()
            .into_iter()
            .map(|(label, entry)| format!("{}: {}", label.text(), show(entry))),
    );
    format!("{{{}}}", parts.join(" "))
}

fn show_row(row: Option<Row<'_>>) -> String {
    row.map_or_else(String::new, |row| {
        let paths: Vec<String> = row.paths().iter().map(|path| path.dotted()).collect();
        format!(" {{{}}}", paths.join(" "))
    })
}

/// A type expression written back from its views, one canonical spelling.
fn show(expr: TypeExpr<'_>) -> String {
    match expr.form().expect("a shaped type") {
        TypeForm::Path {
            path,
            positional,
            labelled,
        } => {
            if positional.is_empty() && labelled.is_empty() && expr.applied().is_none()
            {
                return path.dotted();
            }
            let mut parts: Vec<String> = positional.into_iter().map(show).collect();
            parts.extend(
                labelled
                    .into_iter()
                    .map(|(label, entry)| format!("{}: {}", label.text(), show(entry))),
            );
            format!("{}({})", path.dotted(), parts.join(" "))
        }
        TypeForm::Atom(atom) => atom.text().to_owned(),
        TypeForm::Tuple(components) => format!(
            "{{{}}}",
            components
                .into_iter()
                .map(show)
                .collect::<Vec<_>>()
                .join(" ")
        ),
        TypeForm::Record(fields) => format!(
            "{{{}}}",
            fields
                .into_iter()
                .map(|(name, entry)| format!("{}: {}", name.text(), show(entry)))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        TypeForm::Fn { inputs, result } => {
            format!("Fn({} {})", show_inputs(inputs), show(result))
        }
        TypeForm::Proc {
            inputs,
            result,
            row,
        } => format!(
            "Proc({} {}{})",
            show_inputs(inputs),
            show(result),
            show_row(row)
        ),
        TypeForm::Intent { result, row } => {
            format!("Intent({}{})", show(result), show_row(row))
        }
    }
}

fn body(view: TypeDecl<'_>) -> String {
    match view.body().expect("a body") {
        TypeBody::Of(expr) => show(expr),
        TypeBody::Intrinsic(atom) => format!("Intrinsic({})", atom.text()),
        TypeBody::Cases(cases) => cases
            .iter()
            .map(|case| {
                let name = case.name().unwrap().text().to_owned();
                match case.fields() {
                    Some(fields) => format!(
                        "case {name} {{{}}}",
                        fields
                            .into_iter()
                            .map(|(field, entry)| format!(
                                "{}: {}",
                                field.text(),
                                show(entry)
                            ))
                            .collect::<Vec<_>>()
                            .join(" ")
                    ),
                    None => format!("case {name}"),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// The one type declaration of `text`, which has no diagnostic, written back.
fn accepted(text: &str) -> String {
    let document = source(text);
    assert!(
        document.diagnostics().is_empty(),
        "{text:?}: {:?}",
        codes(document.diagnostics())
    );
    let declarations = types(&document);
    assert_eq!(declarations.len(), 1, "{text:?}");
    body(declarations[0])
}

#[test]
fn type_wrapper() {
    let document = source("type Celsius of F64 endtype");
    assert!(document.diagnostics().is_empty());
    let view = types(&document)[0];
    assert_eq!(view.name().unwrap().text(), "Celsius");
    assert!(view.params().is_empty());
    assert_eq!(body(view), "F64");
    assert!(matches!(
        view.body(),
        Some(TypeBody::Of(expr)) if expr.head().kind() == SyntaxKind::Path
    ));
}

#[test]
fn type_record() {
    let text = "type User of {name: Str id: UserId}\n  visibility @public\nendtype";
    assert_eq!(accepted(text), "{name: Str id: UserId}");
    let document = source(text);
    let view = types(&document)[0];
    assert_eq!(view.visibility().unwrap().tag().unwrap().text(), "@public");
    assert!(matches!(
        view.body(),
        Some(TypeBody::Of(expr)) if expr.head().kind() == SyntaxKind::RecordType
    ));
}

#[test]
fn type_tuple() {
    let document = source("type Pair of {I32 Str} endtype");
    assert!(document.diagnostics().is_empty());
    let Some(TypeBody::Of(expr)) = types(&document)[0].body() else {
        panic!("a type body");
    };
    let Some(TypeForm::Tuple(components)) = expr.form() else {
        panic!("a tuple type");
    };
    assert_eq!(components.len(), 2);
    assert_eq!(show(components[0]), "I32");
    assert_eq!(show(components[1]), "Str");
    // The empty tuple type is a tuple with no component.
    assert_eq!(accepted("type Unit of {} endtype"), "{}");
}

#[test]
fn type_variant() {
    let text = "type Lookup(T)\n  case found {value: T}\n  case missing\n  visibility @public\nendtype";
    assert_eq!(accepted(text), "case found {value: T} case missing");
    let document = source(text);
    let view = types(&document)[0];
    let params: Vec<&str> = view.params().iter().map(|param| param.text()).collect();
    assert_eq!(params, ["T"]);
    // Cases keep their written order, wherever the sections are written.
    assert_eq!(
        accepted("type V visibility @public case b case a given {} case c endtype"),
        "case b case a case c"
    );
}

#[test]
fn type_generic_given() {
    let document =
        source("type Box(T)\n  of {item: T}\n  given {T: Printable}\nendtype");
    assert!(document.diagnostics().is_empty());
    let view = types(&document)[0];
    let entries = view.given().unwrap().entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0.text(), "T");
    assert_eq!(show(entries[0].1), "Printable");
}

#[test]
fn given_bound_forms() {
    let document = source(
        "type Sorted(T I)\n  of {items: I}\n  given {T: Ordered I: Iter(Item: T)}\nendtype",
    );
    assert!(document.diagnostics().is_empty());
    let view = types(&document)[0];
    let entries = view.given().unwrap().entries();
    let shown: Vec<(&str, String)> = entries
        .iter()
        .map(|(name, bound)| (name.text(), show(*bound)))
        .collect();
    assert_eq!(
        shown,
        [
            ("T", "Ordered".to_owned()),
            ("I", "Iter(Item: T)".to_owned())
        ]
    );
    let Some(TypeForm::Path {
        positional,
        labelled,
        ..
    }) = entries[1].1.form()
    else {
        panic!("an applied path");
    };
    assert!(positional.is_empty());
    assert_eq!(labelled.len(), 1);
}

#[test]
fn type_reserved_name() {
    let document = source("type where of U8 endtype");
    assert_eq!(
        codes(document.diagnostics()),
        [(NameReservedDeclaration, 5, 10)]
    );
    let view = types(&document)[0];
    let name = view.name().unwrap();
    assert_eq!(name.text(), "where");
    assert!(name.is_reserved());
    assert_eq!(body(view), "U8");
    // A reserved word as a type parameter, and a reserved type head declared.
    assert_eq!(
        diagnostics("type A(where) of U8 endtype"),
        [(NameReservedDeclaration, 7, 12)]
    );
    assert_eq!(
        diagnostics("type Fn of U8 endtype"),
        [(NameReservedDeclaration, 5, 7)]
    );
    assert_eq!(
        diagnostics("interface Intent endinterface"),
        [(NameReservedDeclaration, 10, 16)]
    );
    assert_eq!(
        diagnostics("type A case then case Proc endtype"),
        [
            (NameReservedDeclaration, 12, 16),
            (NameReservedDeclaration, 22, 26)
        ]
    );
}

#[test]
fn type_reserved_field() {
    assert_eq!(
        diagnostics("type A of {role: U8} endtype"),
        [(NameReservedLabel, 11, 15)]
    );
    let document = source("type A of {role: U8} endtype");
    assert_eq!(body(types(&document)[0]), "{role: U8}");
    // A case field and a type-application label are label positions too.
    assert_eq!(
        diagnostics("type A case x {then: U8} endtype"),
        [(NameReservedLabel, 15, 19)]
    );
    assert_eq!(
        diagnostics("type A of Iter(then: U8) endtype"),
        [(NameReservedLabel, 15, 19)]
    );
}

#[test]
fn type_head_application() {
    let document = source("type H of Fn({I32} Bool) endtype");
    assert!(document.diagnostics().is_empty());
    let Some(TypeBody::Of(expr)) = types(&document)[0].body() else {
        panic!("a type body");
    };
    assert_eq!(expr.head().kind(), SyntaxKind::Identifier);
    assert!(matches!(expr.form(), Some(TypeForm::Fn { .. })));
    assert_eq!(show(expr), "Fn({I32} Bool)");
    // A reserved head never becomes a path, even where a path would parse.
    assert!(
        !tree_document(&document)
            .unwrap()
            .contains("@path children: [{kind: @identifier text: \"Fn\"}")
    );
}

#[test]
fn type_section_order() {
    let document = source("type A given {T: Any} of {x: T} visibility @public endtype");
    assert!(document.diagnostics().is_empty());
    let view = types(&document)[0];
    assert_eq!(body(view), "{x: T}");
    assert_eq!(view.given().unwrap().entries().len(), 1);
}

#[test]
fn type_callable_types() {
    let text = "type Handler of Fn({I32 verbose: Bool} Bool) endtype\n\n\
                type Job of Proc({Str} Void {DirWrite}) endtype\n\n\
                type Plan of Intent(Void {DirWrite}) endtype";
    let document = source(text);
    assert!(document.diagnostics().is_empty());
    let shown: Vec<String> = types(&document).into_iter().map(body).collect();
    assert_eq!(
        shown,
        [
            "Fn({I32 verbose: Bool} Bool)",
            "Proc({Str} Void {DirWrite})",
            "Intent(Void {DirWrite})"
        ]
    );
    // The row is the last operand: without it, `{DirWrite}` is the result.
    assert_eq!(
        accepted("type P of Proc({Str} {DirWrite}) endtype"),
        "Proc({Str} {DirWrite})"
    );
    let document = source("type P of Proc({Str} {DirWrite}) endtype");
    let Some(TypeBody::Of(expr)) = types(&document)[0].body() else {
        panic!("a type body");
    };
    let Some(TypeForm::Proc { result, row, .. }) = expr.form() else {
        panic!("a procedure type");
    };
    assert!(row.is_none());
    assert!(matches!(result.form(), Some(TypeForm::Tuple(_))));
    assert_eq!(accepted("type P of Intent(Void) endtype"), "Intent(Void)");
}

#[test]
fn type_assoc_label() {
    assert_eq!(
        accepted("type Items of Iter(Item: U8) endtype"),
        "Iter(Item: U8)"
    );
    assert_eq!(
        accepted("type T of TryFrom(Str Error: ParseError) endtype"),
        "TryFrom(Str Error: ParseError)"
    );
    assert_eq!(accepted("type A of io.Stdout endtype"), "io.Stdout");
    assert_eq!(accepted("type A of @red endtype"), "@red");
    assert_eq!(
        accepted("type A of List(Option({K V})) endtype"),
        "List(Option({K V}))"
    );
}

#[test]
fn type_missing_body() {
    assert_eq!(diagnostics("type A endtype"), [(SyntaxInvalidForm, 0, 14)]);
    // A type declaration's name comes first.
    assert_eq!(
        diagnostics("type {x: U8} endtype"),
        [(SyntaxInvalidForm, 0, 20)]
    );
    // Tokens that belong to no part make the declaration malformed, once.
    assert_eq!(
        diagnostics("type A B of U8 C endtype"),
        [(SyntaxInvalidForm, 0, 24)]
    );
    // A missing closer is the only fault of a declaration never closed.
    assert_eq!(diagnostics("type A"), [(SyntaxUnmatchedDelimiter, 0, 4)]);
}

#[test]
fn type_both_of_and_case() {
    assert_eq!(
        diagnostics("type A of U8 case x endtype"),
        [(SyntaxInvalidForm, 0, 27)]
    );
    assert_eq!(
        diagnostics("type A case x of U8 endtype"),
        [(SyntaxInvalidForm, 0, 27)]
    );
}

#[test]
fn type_paren_type() {
    assert_eq!(
        diagnostics("type A of (U8) endtype"),
        [(SyntaxInvalidForm, 10, 14)]
    );
    let document = source("type A of (U8) endtype");
    let tree = tree_document(&document).unwrap();
    assert!(tree.contains("{kind: @error children: [{kind: @punctuation text: \"(\"}"));
}

#[test]
fn type_mixed_product() {
    assert_eq!(
        diagnostics("type A of {U8 name: Str} endtype"),
        [(SyntaxInvalidForm, 10, 24)]
    );
}

#[test]
fn positional_and_labelled_order() {
    // A labelled type argument or input comes after the positional ones.
    assert_eq!(
        diagnostics("type A of Iter(Item: U8 T) endtype"),
        [(SyntaxInvalidForm, 14, 26)]
    );
    assert_eq!(
        diagnostics("type A of Fn({v: Bool I32} Bool) endtype"),
        [(SyntaxInvalidForm, 13, 26)]
    );
    // A case's fields are a record type.
    assert_eq!(
        diagnostics("type A case x {U8} endtype"),
        [(SyntaxInvalidForm, 14, 18)]
    );
    // A given entry names its parameter.
    assert_eq!(
        diagnostics("type A of U8 given {Any} endtype"),
        [(SyntaxInvalidForm, 19, 24)]
    );
}

#[test]
fn malformed_type_expressions() {
    for (text, start, end) in [
        // A reserved type head is applied to its own operands.
        ("type A of Fn endtype", 10, 12),
        ("type A of Fn({I32}) endtype", 12, 19),
        ("type A of Proc(I32 Bool) endtype", 14, 24),
        ("type A of Intent({} Void {X}) endtype", 16, 29),
        // `Intrinsic` is a type body only.
        ("type A of List(Intrinsic(@x)) endtype", 15, 28),
        // A row is a brace of paths.
        ("type A of Proc({} Void {Io(X)}) endtype", 23, 30),
        // Not types.
        ("type A of [U8] endtype", 10, 14),
        ("type A of 1u8 endtype", 10, 13),
        ("type A of io. endtype", 10, 13),
        ("type A of {x:} endtype", 11, 13),
    ] {
        assert_eq!(
            diagnostics(text),
            [(SyntaxInvalidForm, start, end)],
            "{text}"
        );
    }
}

#[test]
fn type_invalid_section() {
    assert_eq!(
        diagnostics("type A of U8 performs {X} endtype"),
        [(SyntaxInvalidSection, 13, 25)]
    );
    assert_eq!(
        diagnostics("interface I of U8 endinterface"),
        [(SyntaxInvalidSection, 12, 17)]
    );
    assert_eq!(
        diagnostics("interface I representation @public endinterface"),
        [(SyntaxInvalidSection, 12, 34)]
    );
    // A repeated section is reported once, as a duplicate.
    assert_eq!(
        diagnostics("type A of U8 doc \"\"\"a\"\"\" doc \"\"\"b\"\"\" endtype"),
        [(SyntaxDuplicateSection, 25, 28)]
    );
}

#[test]
fn type_role_outside_stdlib() {
    assert_eq!(
        diagnostics("type A of U8 role @bool endtype"),
        [(SyntaxInvalidSection, 13, 23)]
    );
    let document = library("type A of U8 role @bool endtype");
    assert!(document.diagnostics().is_empty());
    assert_eq!(
        types(&document)[0].role().unwrap().tag().unwrap().text(),
        "@bool"
    );
    // In the standard library the role is one of the closed set.
    assert_eq!(
        codes(library("type A of U8 role @boolean endtype").diagnostics()),
        [(SyntaxInvalidForm, 13, 26)]
    );
    let document = library("interface Iter role @iter type Item endtype endinterface");
    assert!(document.diagnostics().is_empty());
}

#[test]
fn type_intrinsic_outside_stdlib() {
    assert_eq!(
        diagnostics("type A of Intrinsic(@i32) endtype"),
        [(SyntaxInvalidSection, 10, 25)]
    );
    let document = library("type I32 of Intrinsic(@i32) visibility @public endtype");
    assert!(document.diagnostics().is_empty());
    assert_eq!(body(types(&document)[0]), "Intrinsic(@i32)");
    assert_eq!(
        codes(library("type A of Intrinsic(I32) endtype").diagnostics()),
        [(SyntaxInvalidForm, 19, 24)]
    );
}

#[test]
fn type_bad_tag() {
    assert_eq!(
        diagnostics("type A of U8 visibility @friends endtype"),
        [(SyntaxInvalidForm, 13, 32)]
    );
    assert_eq!(
        diagnostics("type A of U8 representation public endtype"),
        [(SyntaxInvalidForm, 13, 34)]
    );
    let document =
        source("type A of U8 representation @private visibility @public endtype");
    assert!(document.diagnostics().is_empty());
    let view = types(&document)[0];
    assert_eq!(
        view.representation().unwrap().tag().unwrap().text(),
        "@private"
    );
    assert_eq!(view.visibility().unwrap().keyword(), "visibility");
}

#[test]
fn type_doc_and_where() {
    let document = source("type A of U8\n  doc \"\"\"An A.\"\"\"\nendtype");
    assert!(document.diagnostics().is_empty());
    assert_eq!(
        types(&document)[0].doc().unwrap().doc_string(),
        Some("\"\"\"An A.\"\"\"")
    );
    assert_eq!(
        diagnostics("type A of U8 doc \"text\" endtype"),
        [(SyntaxInvalidForm, 13, 23)]
    );
    // The operand of `where` runs to the next section and holds the
    // expression constructs written in it.
    let text = "type Percent of U8\n  where fn takes {value: U8} returns valid: Bool do\n    return Ordered.lessOrEqual(value 100u8)\n  endfn else zero\n  visibility @public\nendtype";
    let document = source(text);
    assert!(
        document.diagnostics().is_empty(),
        "{:?}",
        codes(document.diagnostics())
    );
    let view = types(&document)[0];
    let invariant = view.invariant().unwrap();
    let predicate = invariant.predicate_tokens();
    assert_eq!(predicate.len(), 1);
    assert_eq!(
        predicate[0].text(text).split_whitespace().next(),
        Some("fn")
    );
    let consequence = invariant.else_tokens().unwrap();
    assert_eq!(consequence.len(), 1);
    assert_eq!(consequence[0].text(text), "zero");
    assert!(view.visibility().is_some());
    assert_eq!(
        diagnostics("type A of U8 where visibility @public endtype"),
        [(SyntaxInvalidForm, 13, 18)]
    );
}

#[test]
fn interface_basic() {
    let text = "interface Printable\n  visibility @public\n  fn render\n    takes {value: Self}\n    returns text: Str\n  endfn\nendinterface";
    let document = source(text);
    assert_eq!(codes(document.diagnostics()), [(ToolUnavailable, 43, 106)]);
    let view = interfaces(&document)[0];
    assert_eq!(view.name().unwrap().text(), "Printable");
    assert_eq!(view.members().len(), 1);
    assert_eq!(view.visibility().unwrap().tag().unwrap().text(), "@public");
    // An interface with no items is complete.
    assert!(diagnostics("interface I endinterface").is_empty());
}

#[test]
fn interface_assoc() {
    let document = source("interface Iter\n  type Item endtype\nendinterface");
    assert!(document.diagnostics().is_empty());
    let view = interfaces(&document)[0];
    let names: Vec<&str> = view
        .assoc_types()
        .iter()
        .map(|assoc| assoc.name().unwrap().text())
        .collect();
    assert_eq!(names, ["Item"]);
    let document = source(
        "interface Pair(T) given {T: Any} type First endtype type Second endtype doc \"\"\"x\"\"\" endinterface",
    );
    assert!(document.diagnostics().is_empty());
    let view = interfaces(&document)[0];
    assert_eq!(view.assoc_types().len(), 2);
    assert_eq!(view.params().len(), 1);
    assert!(view.given().is_some() && view.doc().is_some());
}

#[test]
fn interface_assoc_with_body() {
    assert_eq!(
        diagnostics("interface I type Item of U8 endtype endinterface"),
        [(SyntaxInvalidForm, 12, 35)]
    );
    for text in [
        "interface I type Item(T) endtype endinterface",
        "interface I type Item visibility @public endtype endinterface",
        "interface I type Item case a endtype endinterface",
    ] {
        assert_eq!(
            diagnostics(text),
            [(SyntaxInvalidForm, 12, text.len() - 13)],
            "{text}"
        );
    }
}

#[test]
fn impl_in_type() {
    let text = "type Once of Option(U8)\n  impl Iter\n    type Item of U8 endtype\n  endimpl\nendtype";
    let document = source(text);
    assert!(document.diagnostics().is_empty());
    let view = types(&document)[0];
    let impls = view.impls();
    assert_eq!(impls.len(), 1);
    assert_eq!(show(impls[0].target().unwrap()), "Iter");
    let choices = impls[0].choices();
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].name().unwrap().text(), "Item");
    assert_eq!(show(choices[0].chosen().unwrap()), "U8");
    // An interface's `impl` names the type; a choice names one type.
    let document = source(
        "interface Show impl List(T) type Item of {T U8} endtype endimpl endinterface",
    );
    assert!(document.diagnostics().is_empty());
    let view = interfaces(&document)[0];
    assert_eq!(show(view.impls()[0].target().unwrap()), "List(T)");
    assert_eq!(
        diagnostics("type A of U8 impl P type Item endtype endimpl endtype"),
        [(SyntaxInvalidForm, 20, 37)]
    );
    assert_eq!(
        diagnostics("type A of U8 impl endimpl endtype"),
        [(SyntaxInvalidForm, 13, 25)]
    );
}

#[test]
fn impl_top_level() {
    assert_eq!(
        diagnostics("impl Printable endimpl"),
        [(SyntaxInvalidForm, 0, 22)]
    );
    // Inside anything but a `type` or an `interface`, too.
    assert_eq!(
        diagnostics("fn f impl P endimpl endfn"),
        [(ToolUnavailable, 0, 25), (SyntaxInvalidForm, 5, 19)]
    );
    assert_eq!(
        diagnostics("type A of U8 impl P impl Q endimpl endimpl endtype"),
        [(SyntaxInvalidForm, 20, 34)]
    );
}

#[test]
fn impl_with_section() {
    assert_eq!(
        diagnostics("type A of U8 impl P visibility @public endimpl endtype"),
        [(SyntaxInvalidSection, 20, 38)]
    );
}

#[test]
fn nested_constructs_a_type_does_not_admit() {
    assert_eq!(
        diagnostics("type A of U8 type B of U8 endtype endtype"),
        [(SyntaxInvalidForm, 13, 33)]
    );
    assert_eq!(
        diagnostics("type A of U8 if c then x endif endtype"),
        [(SyntaxInvalidForm, 13, 30)]
    );
    // A named member is unavailable until the callable grammar exists.
    assert_eq!(
        diagnostics("type A of U8 fn f returns r: U8 endfn endtype"),
        [(ToolUnavailable, 13, 37)]
    );
}

#[test]
fn type_recovery() {
    let text = "type A of {x: endtype\ntype B of U8 endtype";
    let document = source(text);
    assert_eq!(
        codes(document.diagnostics()),
        [(SyntaxUnmatchedDelimiter, 14, 21)]
    );
    let related = document.diagnostics()[0].related();
    assert_eq!(related.len(), 1);
    assert_eq!((related[0].span.start(), related[0].span.end()), (10, 11));
    let declarations = types(&document);
    assert_eq!(declarations.len(), 2);
    assert_eq!(declarations[1].name().unwrap().text(), "B");
    assert_eq!(body(declarations[1]), "U8");
}

/// The section words of the alternatives of a grammar repetition, read from
/// the chapter text.
fn section_alternatives(grammar: &str, production: &str) -> Vec<String> {
    let start = grammar
        .find(&format!("\n{production}"))
        .unwrap_or_else(|| panic!("the production {production}"));
    let rest = &grammar[start..];
    let end = rest.find(" ;").unwrap();
    let body = &rest[..end];
    let alternatives = match body.rfind('{') {
        Some(open) if production != "type-item" => &body[open + 1..],
        _ => body.split_once('=').unwrap().1,
    };
    alternatives
        .split(['|', '}', '{', ','])
        .map(str::trim)
        .filter(|word| SECTIONS.contains(word))
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_admissibility_table_is_the_chapters_grammar() {
    let chapter = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/spec/01-source-language.md"),
    )
    .unwrap();
    let grammar = chapter
        .split("```ebnf")
        .nth(2)
        .and_then(|block| block.split("```").next())
        .unwrap();
    // Each admissible section is the production named after its keyword.
    for section in ADMISSIBLE_SECTIONS
        .iter()
        .flat_map(|(_, sections)| *sections)
    {
        assert!(
            grammar.contains(&format!("\n{section:<12}= \"{section}\""))
                || grammar.contains(&format!("\n{section} = \"{section}\"")),
            "{section}"
        );
    }
    let expected = [
        ("type", section_alternatives(grammar, "type-item")),
        ("interface", section_alternatives(grammar, "interface-decl")),
        ("impl", section_alternatives(grammar, "impl")),
    ];
    assert_eq!(ADMISSIBLE_SECTIONS.len(), expected.len());
    for ((kind, sections), (expected_kind, expected_sections)) in
        ADMISSIBLE_SECTIONS.iter().zip(&expected)
    {
        assert_eq!(kind, expected_kind);
        assert_eq!(
            sections
                .iter()
                .map(|word| (*word).to_owned())
                .collect::<Vec<_>>(),
            *expected_sections,
            "{kind}"
        );
    }
    // The tag sets are the chapter's closed sets.
    let roles = [
        "@bool", "@str", "@bytes", "@option", "@result", "@dict", "@iter",
    ];
    for (section, tags) in TAG_SETS {
        match *section {
            "role" => {
                for role in roles.iter().chain(&["@equatable", "@ordered"]) {
                    assert!(tags.contains(role), "{role}");
                }
                assert_eq!(tags.len(), 9);
            }
            _ => assert_eq!(*tags, ["@public", "@private"], "{section}"),
        }
    }
    assert_eq!(TYPE_HEADS, ["Fn", "Proc", "Intent", "Intrinsic"]);
}

#[test]
fn every_admitted_section_is_accepted_and_every_other_is_invalid() {
    let operands = [
        ("given", "{T: Any}"),
        ("where", "valid"),
        ("role", "@bool"),
        ("visibility", "@public"),
        ("representation", "@public"),
        ("doc", "\"\"\"d\"\"\""),
        ("takes", "{x: U8}"),
        ("labelled", "{x: U8 be 1u8}"),
        ("returns", "r: U8"),
        ("requires", "c"),
        ("ensures", "c"),
        ("performs", "{X}"),
        ("external", "@compiler"),
        ("symbol", "\"s\""),
        ("native", "\"n\""),
        ("be", "x"),
    ];
    for (kind, rest) in [
        ("type", ("type A of U8 ", " endtype")),
        ("interface", ("interface I ", " endinterface")),
        ("impl", ("type A of U8 impl P ", " endimpl endtype")),
    ] {
        let admitted = ADMISSIBLE_SECTIONS
            .iter()
            .find(|(row, _)| *row == kind)
            .unwrap()
            .1;
        for (section, operand) in operands {
            let text = format!("{}{section} {operand}{}", rest.0, rest.1);
            let document = library(&text);
            let start = rest.0.len();
            let expected = if admitted.contains(&section) {
                vec![]
            } else {
                vec![(
                    SyntaxInvalidSection,
                    start,
                    start + section.len() + 1 + operand.len(),
                )]
            };
            assert_eq!(codes(document.diagnostics()), expected, "{text}");
        }
    }
}

#[test]
fn deep_type_nesting_is_shaped_and_dropped_without_recursion() {
    const DEPTH: usize = 100_000;
    let text = format!(
        "type A of {}U8{} endtype",
        "List(".repeat(DEPTH),
        ")".repeat(DEPTH)
    );
    let document = source(&text);
    assert!(document.diagnostics().is_empty());
    assert!(tree_document(&document).unwrap().len() > DEPTH);
    let text = format!(
        "type A of {}U8{} endtype",
        "{".repeat(DEPTH),
        "}".repeat(DEPTH)
    );
    assert!(source(&text).diagnostics().is_empty());
    let text = format!("type A of {}U8 endtype", "{x: ".repeat(DEPTH));
    let recovered = source(&text);
    assert_eq!(recovered.diagnostics().len(), 1);
}

/// Checks the tree invariant, the diagnostic spans, and every view of one
/// source text.
fn check_invariants(text: &str) {
    let document = source(text);
    let root = document.root().expect("valid UTF-8 has a tree");
    let mut at = 0;
    for leaf in root.leaves() {
        assert_eq!(leaf.span().start(), at, "leaves are contiguous in {text:?}");
        assert_eq!(
            leaf.kind() == SyntaxKind::Missing,
            leaf.span().is_empty(),
            "{text:?}"
        );
        at = leaf.span().end();
    }
    assert_eq!(at, text.len(), "the leaves cover {text:?}");
    for diagnostic in document.diagnostics() {
        let span = diagnostic.primary_span();
        assert!(span.end() <= text.len(), "{text:?}");
    }
    let _ = tree_document(&document);
    for view in types(&document) {
        let _ = (view.name(), view.params(), view.given(), view.invariant());
        if let Some(TypeBody::Of(expr)) = view.body() {
            let _ = expr.form();
        }
        for case in view.cases() {
            let _ = (case.name(), case.fields());
        }
        for block in view.impls() {
            let _ = (block.target().map(|target| target.form()), block.choices());
        }
    }
    for view in interfaces(&document) {
        let _ = (view.name(), view.assoc_types(), view.impls());
    }
}

#[test]
fn random_type_declarations_never_panic_and_keep_every_byte_in_one_leaf() {
    const PIECES: &[&str] = &[
        "type",
        "endtype",
        "interface",
        "endinterface",
        "impl",
        "endimpl",
        "of",
        "case",
        "given",
        "where",
        "visibility",
        "role",
        "doc",
        "fn",
        "endfn",
        "else",
        "A",
        "T",
        "Fn",
        "Proc",
        "Intent",
        "Intrinsic",
        "@public",
        "@i32",
        "{",
        "}",
        "(",
        ")",
        "[",
        "]",
        ":",
        ".",
        "x",
        "\"\"\"d\"\"\"",
        "// c\n",
    ];
    const GAPS: &[&str] = &[" ", "\n", "", "  "];
    let mut random = Random::new(0x7E5_0005);
    for _ in 0..4000 {
        let len = random.below(28);
        let mut text = String::new();
        for _ in 0..len {
            text.push_str(PIECES[random.below(PIECES.len())]);
            text.push_str(GAPS[random.below(GAPS.len())]);
        }
        check_invariants(&text);
        check_invariants(&format!("type A of {text} endtype"));
    }
}
