//! The data grammar, recovery, documents, and decoded values.

use vibra_diagnostics::ByteSpan;
use vibra_diagnostics::DiagnosticCode::{
    DataDuplicateField, DataDuplicateKey, DataInvalidExtension, SyntaxInvalidEncoding,
    SyntaxInvalidForm, SyntaxInvalidName, SyntaxUnmatchedDelimiter,
};
use vibra_syntax::{
    DataValue, Document, DocumentMode, SyntaxKind, load, parse_data, parse_source,
};

use crate::support::{Random, codes};

fn data(text: &str) -> Document {
    parse_data(None, text.as_bytes())
}

fn accepted(text: &str) -> Document {
    let document = data(text);
    assert!(
        document.diagnostics().is_empty(),
        "{text}: {:?}",
        document.diagnostics()
    );
    assert!(document.value().is_some(), "{text}");
    document
}

#[test]
fn every_literal_category_is_a_value() {
    let document = accepted(
        "{text: \"a\\n\" ch: 'x' n: -7i64 u: 1u8 f: 2.5f64 e: 1e3 b: true a: @std.io v: void}",
    );
    let DataValue::Record(fields) = document.value().unwrap().value() else {
        panic!("a record");
    };
    let names = fields.iter().map(|field| field.name()).collect::<Vec<_>>();
    assert_eq!(names, ["text", "ch", "n", "u", "f", "e", "b", "a", "v"]);
    assert!(
        matches!(fields[0].value().value(), DataValue::String(text) if text == "a\n")
    );
    assert!(matches!(
        fields[1].value().value(),
        DataValue::Character('x')
    ));
    assert!(
        matches!(fields[2].value().value(), DataValue::Integer(literal) if literal.text() == "-7i64")
    );
    assert!(matches!(fields[6].value().value(), DataValue::Bool(true)));
    assert!(
        matches!(fields[7].value().value(), DataValue::Atom(atom) if atom == "@std.io")
    );
    assert!(matches!(fields[8].value().value(), DataValue::Void));
}

#[test]
fn the_brace_and_bracket_forms_are_their_productions() {
    let document = accepted("[{1i32 \"a\"}  {} [] [:] [\"k\": {x: 1u8}]]");
    let root = document.root().unwrap();
    assert_eq!(root.kind(), SyntaxKind::Data);
    let list = &root.children()[0];
    assert_eq!(list.kind(), SyntaxKind::List);
    let kinds = list
        .children()
        .iter()
        .map(vibra_syntax::CstNode::kind)
        .filter(|kind| {
            *kind != SyntaxKind::Whitespace && *kind != SyntaxKind::Punctuation
        })
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            SyntaxKind::Tuple,
            SyntaxKind::Tuple,
            SyntaxKind::List,
            SyntaxKind::Dict,
            SyntaxKind::Dict
        ]
    );
    let dict = list
        .children()
        .iter()
        .rfind(|node| node.kind() == SyntaxKind::Dict)
        .unwrap();
    assert!(
        dict.children()
            .iter()
            .any(|node| node.kind() == SyntaxKind::Entry)
    );
}

#[test]
fn a_bracket_separates_adjacent_values() {
    accepted("{{1u8}{2u8}}");
}

#[test]
fn a_document_is_exactly_one_value() {
    assert_eq!(
        codes(data("{} {}").diagnostics()),
        [(SyntaxInvalidForm, 3, 5)]
    );
    assert_eq!(codes(data("").diagnostics()), [(SyntaxInvalidForm, 0, 0)]);
    assert_eq!(
        codes(data("// c\n").diagnostics()),
        [(SyntaxInvalidForm, 0, 5)]
    );
}

#[test]
fn identifiers_doc_strings_and_applications_are_not_data() {
    assert_eq!(
        codes(data("{a: foo}").diagnostics()),
        [(SyntaxInvalidForm, 4, 7)]
    );
    assert_eq!(
        codes(data("{a: Option.some(value: 1i32)}").diagnostics()),
        [(SyntaxInvalidForm, 4, 10)]
    );
    assert_eq!(
        codes(data("{a: \"\"\"x\"\"\"}").diagnostics()),
        [(SyntaxInvalidForm, 4, 11)]
    );
    // A malformed name is reported once as a name, as a field name and as a
    // value.
    assert_eq!(
        codes(data("{my_field: 1u8}").diagnostics()),
        [(SyntaxInvalidName, 1, 9)]
    );
    assert_eq!(
        codes(data("{a: my_field}").diagnostics()),
        [(SyntaxInvalidName, 4, 12)]
    );
    // The application's tokens are kept inside one error node.
    let document = data("{a: Option.some(value: 1i32)}");
    let field = &document.root().unwrap().children()[0].children()[1];
    assert_eq!(field.kind(), SyntaxKind::Field);
    let error = field.children().last().unwrap();
    assert_eq!(error.kind(), SyntaxKind::Error);
    assert_eq!(error.span(), ByteSpan::new(4, 28));
}

#[test]
fn repeated_fields_and_keys_are_reported_at_the_later_one() {
    assert_eq!(
        codes(data("{a: 1u8 a: 2u8}").diagnostics()),
        [(DataDuplicateField, 8, 9)]
    );
    assert_eq!(
        codes(data("[\"a\": 1u8 \"\\u{61}\": 2u8]").diagnostics()),
        [(DataDuplicateKey, 10, 18)]
    );
    // Without a schema the written suffix is part of a number's value.
    accepted("[1: \"a\" 1u8: \"b\"]");
    assert_eq!(
        codes(data("[1u8: \"a\" 01u8: \"b\"]").diagnostics()),
        [(DataDuplicateKey, 10, 14)]
    );
    assert_eq!(
        codes(data("[{a: 1 b: 2}: 0 {b: 2 a: 1}: 1]").diagnostics()),
        [(DataDuplicateKey, 16, 27)]
    );
    accepted("[0.0f64: 1 -0.0f64: 2]");
}

#[test]
fn a_crossed_closer_closes_the_open_form_of_its_kind() {
    let document = data("{a: [1u8 2u8}");
    let diagnostics = document.diagnostics();
    assert_eq!(codes(diagnostics), [(SyntaxUnmatchedDelimiter, 12, 13)]);
    assert_eq!(diagnostics[0].related().len(), 1);
    assert_eq!(diagnostics[0].related()[0].span, ByteSpan::new(4, 5));
    let record = &document.root().unwrap().children()[0];
    assert_eq!(record.kind(), SyntaxKind::Record);
    let list = record.children()[1].children().last().unwrap();
    assert_eq!(list.kind(), SyntaxKind::List);
    assert_eq!(list.children().last().unwrap().kind(), SyntaxKind::Missing);
    assert!(document.value().is_none());
}

#[test]
fn an_open_form_at_the_end_is_missing_its_closer() {
    let document = data("{a: 1u8");
    assert_eq!(
        codes(document.diagnostics()),
        [(SyntaxUnmatchedDelimiter, 0, 1)]
    );
    let record = &document.root().unwrap().children()[0];
    assert_eq!(
        record.children().last().unwrap().kind(),
        SyntaxKind::Missing
    );
}

#[test]
fn a_closer_with_nothing_of_its_kind_open_is_kept_in_an_error_node() {
    let document = data("[1u8}");
    assert_eq!(
        codes(document.diagnostics()),
        [
            (SyntaxUnmatchedDelimiter, 0, 1),
            (SyntaxUnmatchedDelimiter, 4, 5)
        ]
    );
    assert!(document.diagnostics()[1].related().is_empty());
}

#[test]
fn encoding_faults_are_reported_once() {
    let bom = parse_data(None, &[0xEF, 0xBB, 0xBF, 0x7B, 0x7D]);
    assert_eq!(codes(bom.diagnostics()), [(SyntaxInvalidEncoding, 0, 3)]);
    assert!(bom.root().is_some());
    let nul = parse_data(None, &[0x7B, 0x00, 0x7D]);
    assert_eq!(codes(nul.diagnostics()), [(SyntaxInvalidEncoding, 1, 2)]);
    let invalid = parse_data(None, &[0x7B, 0xFF, 0x7D]);
    assert_eq!(
        codes(invalid.diagnostics()),
        [(SyntaxInvalidEncoding, 1, 2)]
    );
    assert!(invalid.root().is_none(), "no tree");
    assert!(invalid.text().is_none());
}

#[test]
fn every_byte_of_a_random_input_is_in_exactly_one_leaf() {
    let mut random = Random::new(0xB17E_5EED);
    for round in 0..3000 {
        let text = random.text(40);
        let document = if round % 2 == 0 {
            parse_data(None, text.as_bytes())
        } else {
            parse_source(None, text.as_bytes())
        };
        let root = document.root().expect("valid UTF-8 has a tree");
        let mut at = 0;
        for leaf in root.leaves() {
            assert!(leaf.kind().is_leaf());
            assert_eq!(leaf.span().start(), at, "leaves are contiguous in {text:?}");
            if leaf.kind() == SyntaxKind::Missing {
                assert!(leaf.span().is_empty());
            } else {
                assert!(!leaf.span().is_empty(), "{text:?}");
            }
            at = leaf.span().end();
        }
        assert_eq!(at, text.len(), "the leaves cover {text:?}");
        let joined = root
            .leaves()
            .iter()
            .map(|leaf| leaf.text(&text))
            .collect::<String>();
        assert_eq!(joined, text);
    }
}

#[test]
fn deep_nesting_is_parsed_compared_and_dropped_without_recursion() {
    const DEPTH: usize = 200_000;
    let text = format!("{}1u8{}", "[".repeat(DEPTH), "]".repeat(DEPTH));
    let document = accepted(&text);
    let value = document.value().unwrap();
    assert!(value.decoded_eq(value));
    let tree = vibra_syntax::tree_document(&document).unwrap();
    assert!(tree.len() > DEPTH);
    let open = format!("{}{{}}", "{".repeat(DEPTH));
    let recovered = data(&open);
    assert_eq!(recovered.diagnostics().len(), DEPTH);
    drop(recovered);
    drop(document);
}

#[test]
fn decoded_equality_ignores_spelling_and_record_order_only() {
    let a = accepted("{x: \"a\" y: [1u8 2.50]}");
    let b = accepted("{y: [01u8 2.5] x: \"\\u{61}\"}");
    let c = accepted("{y: [1 2.5] x: \"a\"}");
    let d = accepted("{y: [2u8 1u8] x: \"a\"}");
    let same = |left: &Document, right: &Document| {
        left.value().unwrap().decoded_eq(right.value().unwrap())
    };
    assert!(same(&a, &b));
    assert!(!same(&a, &c), "a suffix is part of the value");
    assert!(!same(&a, &d), "a list keeps its order");
    let e = accepted("[\"k\": 1 \"j\": [:]]");
    let f = accepted("[\"j\": [:] \"k\": 1]");
    assert!(same(&e, &f), "a dict is its set of entries");
}

#[test]
fn the_mode_is_chosen_by_exact_extension() {
    assert_eq!(
        DocumentMode::from_path("a/b.vib"),
        Some(DocumentMode::Source)
    );
    assert_eq!(
        DocumentMode::from_path("input.vibon"),
        Some(DocumentMode::Data)
    );
    assert_eq!(DocumentMode::from_path("input.VIBON"), None);
    assert_eq!(DocumentMode::from_path("input.json"), None);
    assert_eq!(DocumentMode::from_path("vibon"), None);
}

#[test]
fn a_document_presented_to_the_wrong_loader_is_never_read() {
    let mut read = false;
    let document = load::<()>("input.vib", DocumentMode::Data, || {
        read = true;
        Ok(b"{}".to_vec())
    })
    .unwrap();
    assert!(!read, "no contents read");
    assert_eq!(
        codes(document.diagnostics()),
        [(DataInvalidExtension, 0, 0)]
    );
    assert_eq!(document.diagnostics()[0].source_id(), Some("input.vib"));
    assert!(document.root().is_none());

    let document =
        load::<()>("input.vibon", DocumentMode::Data, || Ok(b"{}".to_vec())).unwrap();
    assert!(document.diagnostics().is_empty());
}
