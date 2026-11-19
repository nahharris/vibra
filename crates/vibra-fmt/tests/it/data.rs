//! The source-preserving VIBON formatter.

use std::path::{Path, PathBuf};

use vibra_fmt::{COLUMN_LIMIT, format_document};
use vibra_syntax::{Document, DocumentMode, parse, parse_data, parse_source};

fn format(text: &str) -> String {
    format_document(&parse_data(None, text.as_bytes())).expect("a text")
}

/// Formats `text`, checks that formatting is idempotent and keeps the decoded
/// value, and returns the output.
fn format_checked(text: &str) -> String {
    let document = parse_data(None, text.as_bytes());
    let output = format_document(&document).expect("a text");
    let reparsed = parse_data(None, output.as_bytes());
    assert_eq!(
        format_document(&reparsed).as_deref(),
        Some(output.as_str()),
        "idempotent for {text:?}"
    );
    if let Some(value) = document.value() {
        assert!(reparsed.diagnostics().is_empty(), "{output:?}");
        assert!(
            reparsed
                .value()
                .is_some_and(|reparsed| reparsed.decoded_eq(value)),
            "the decoded value is kept for {text:?}"
        );
    }
    output
}

#[test]
fn a_form_that_fits_is_one_line_and_keeps_its_spellings() {
    let input = "{text: \"a\\n\" ch: 'x' n: -7i64 u: 1u8 f: 2.5f64 e: 1e3 b: true a: @std.io v: void}";
    assert_eq!(format_checked(input), format!("{input}\n"));
}

#[test]
fn whitespace_between_tokens_is_one_space_and_none_inside_brackets() {
    assert_eq!(
        format_checked("[{1i32 \"a\"}  {} [] [:] [\"k\": {x: 1u8}]]"),
        "[{1i32 \"a\"} {} [] [:] [\"k\": {x: 1u8}]]\n"
    );
    assert_eq!(
        format_checked("[ \"k\"  :{ x :1u8 } ]\r\n\r\n"),
        "[\"k\": {x: 1u8}]\n"
    );
    assert_eq!(format_checked("{{1u8}{2u8}}"), "{{1u8} {2u8}}\n");
}

#[test]
fn comments_stay_where_they_were_written() {
    assert_eq!(
        format_checked("// top\n{a: 1u8 // one\nb: 2u8}"),
        "// top\n{\n  a: 1u8 // one\n  b: 2u8\n}\n"
    );
    assert_eq!(
        format_checked(
            "[ // first\n  1u8\n  // before two\n  2u8\n  // last\n] // after"
        ),
        "[ // first\n  1u8\n  // before two\n  2u8\n  // last\n] // after\n"
    );
    assert_eq!(format_checked("{} // c   \n// d\n"), "{} // c\n// d\n");
}

#[test]
fn a_form_that_does_not_fit_is_one_entry_per_line() {
    let input = "{alpha: \"aaaaaaaaaaaaaaaa\" beta: \"bbbbbbbbbbbbbbbb\" gamma: \"cccccccccccccccc\" delta: \"ddddddd\"}";
    assert_eq!(input.chars().count(), 95);
    assert_eq!(
        format_checked(input),
        "{\n  alpha: \"aaaaaaaaaaaaaaaa\"\n  beta: \"bbbbbbbbbbbbbbbb\"\n  \
         gamma: \"cccccccccccccccc\"\n  delta: \"ddddddd\"\n}\n"
    );
    // A nested form that fits stays on one line inside a broken one.
    let wide = format!("[\"x\"{} {{x: 1u8}}]", " \"x\"".repeat(30));
    let output = format_checked(&wide);
    assert!(
        output
            .lines()
            .all(|line| line.chars().count() <= COLUMN_LIMIT),
        "{output}"
    );
    assert!(output.contains("\n  {x: 1u8}\n"), "{output}");
}

#[test]
fn nothing_is_reordered_respelled_or_suffixed() {
    assert_eq!(
        format_checked("[\"b\": 1 \"\\u{61}\": 2.0]"),
        "[\"b\": 1 \"\\u{61}\": 2.0]\n"
    );
    assert_eq!(format_checked("{z: 007 a: 1E3}"), "{z: 007 a: 1E3}\n");
}

#[test]
fn a_recovered_document_is_returned_byte_for_byte() {
    for input in ["[1u8}", "\"open", "{a: [1u8 2u8}", "{a:   foo}", "{} {}"] {
        assert_eq!(format(input), input);
    }
    let source = "fn f returns r: I32 do return 1i32 endfn";
    assert_eq!(
        format_document(&parse_source(None, source.as_bytes())).as_deref(),
        Some(source)
    );
    assert!(format_document(&parse_data(None, &[0x7B, 0xFF, 0x7D])).is_none());
}

#[test]
fn a_multi_line_string_breaks_its_form_and_keeps_its_bytes() {
    assert_eq!(format_checked("[\"a\nb\" 1]"), "[\n  \"a\nb\"\n  1\n]\n");
}

fn corpus_inputs(directory: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("a readable entry").path();
        if path.is_dir() {
            corpus_inputs(&path, found);
        } else if path.file_stem().is_some_and(|stem| stem == "input") {
            found.push(path);
        }
    }
}

#[test]
fn formatting_is_idempotent_over_the_corpus_inputs() {
    let cases = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/cases");
    let mut inputs = Vec::new();
    corpus_inputs(&cases.join("SRC-READER"), &mut inputs);
    corpus_inputs(&cases.join("SRC-DECL"), &mut inputs);
    assert!(
        inputs.len() >= 44,
        "the reader corpus has its inputs: {inputs:?}"
    );
    for path in inputs {
        let bytes = std::fs::read(&path).expect("a readable input");
        let Some(mode) = DocumentMode::from_path(&path) else {
            continue;
        };
        let document: Document = parse(mode, None, &bytes);
        let Some(once) = format_document(&document) else {
            continue;
        };
        let twice = format_document(&parse(mode, None, once.as_bytes()));
        assert_eq!(twice.as_deref(), Some(once.as_str()), "{}", path.display());
    }
}
