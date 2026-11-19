//! Every example fenced `vibra` in the source chapter parses with no
//! diagnostic.

use vibra_syntax::source::SourceOptions;
use vibra_syntax::{SyntaxKind, parse_source_with};

/// How an example is read.
#[derive(Clone, Copy, Debug)]
enum Reading {
    /// A whole module.
    Module,
    /// A module of the embedded standard library.
    Library,
    /// Statements or expressions, read as the body of a `fn`.
    Body,
}

/// The reading of each fenced example of the chapter, in order.
const READINGS: &[Reading] = &[
    // The module of "Declarations".
    Reading::Module,
    // `type`.
    Reading::Module,
    // `interface`.
    Reading::Module,
    // A `type` with a member and an `impl`.
    Reading::Module,
    // `splitCents`, with `labelled` and a guard.
    Reading::Module,
    // An anonymous `fn`.
    Reading::Body,
    // `addChecked`, an external definition of `@std.builtin`.
    Reading::Library,
    // Construction.
    Reading::Body,
    // Anonymous products and collections.
    Reading::Body,
    // `let`.
    Reading::Body,
    // `if`, `let`, and `match`.
    Reading::Body,
];

#[test]
fn every_vibra_example_of_the_source_chapter_parses_with_no_diagnostic() {
    let chapter = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/spec/01-source-language.md"),
    )
    .unwrap();
    let examples: Vec<&str> = chapter
        .split("```vibra\n")
        .skip(1)
        .map(|block| block.split("```").next().unwrap())
        .collect();
    assert_eq!(examples.len(), READINGS.len(), "one reading per example");
    for (index, (example, reading)) in examples.iter().zip(READINGS).enumerate() {
        let text = match reading {
            Reading::Body => format!("fn example returns r: Void do\n{example}endfn\n"),
            Reading::Module | Reading::Library => (*example).to_owned(),
        };
        let options = SourceOptions {
            standard_library: matches!(reading, Reading::Library),
        };
        let document = parse_source_with(None, text.as_bytes(), options);
        assert!(
            document.diagnostics().is_empty(),
            "example {index}: {:?}\n{text}",
            document.diagnostics()
        );
        let root = document.root().unwrap();
        assert!(
            root.leaves()
                .iter()
                .all(|leaf| leaf.kind() != SyntaxKind::Missing),
            "example {index}"
        );
        assert!(
            root.children()
                .iter()
                .all(|child| child.kind() != SyntaxKind::Error),
            "example {index}"
        );
    }
}
