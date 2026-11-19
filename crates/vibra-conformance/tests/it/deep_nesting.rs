//! Deep nesting is read, formatted, and dropped without the host stack.
//!
//! `docs/spec/01-source-language.md`, "Canonical format", indents each
//! nested form that does not fit one step of two spaces, so the canonical
//! text of a document nested `n` deep holds about `n * n` bytes of
//! indentation. The reader and the drop run at depth 100,000; the formatter
//! runs at depth 3,000, where its output is about nine million bytes. Every
//! run uses a thread whose stack is far smaller than any recursion over the
//! depth would need, so a recursive reader, formatter, or drop overflows it.

use vibra_fmt::format_document;
use vibra_syntax::{Document, parse_data, parse_source, tree_document};

/// The depth the reader and the drop run at.
const READ_DEPTH: usize = 100_000;
/// The depth the formatter runs at.
const FORMAT_DEPTH: usize = 3_000;
/// The stack of the thread every run uses.
const STACK: usize = 256 * 1024;

/// The documents nested `depth` deep: data `[[…]]`, and source module
/// values whose initializers are `[[…]]` and `do do … enddo enddo`.
fn documents(depth: usize) -> [(bool, String); 3] {
    [
        (
            false,
            format!("{}1u8{}", "[".repeat(depth), "]".repeat(depth)),
        ),
        (
            true,
            format!(
                "let x: List(I32) be {}1i32{} endlet",
                "[".repeat(depth),
                "]".repeat(depth)
            ),
        ),
        (
            true,
            format!(
                "let x: I32 be {}1i32{} endlet",
                "do ".repeat(depth),
                " enddo".repeat(depth)
            ),
        ),
    ]
}

fn read(source: bool, text: &str) -> Document {
    if source {
        parse_source(None, text.as_bytes())
    } else {
        parse_data(None, text.as_bytes())
    }
}

/// Runs `body` on a thread with a small stack and reports whether it
/// finished; a stack overflow aborts the test process instead.
fn on_small_stack(body: impl FnOnce() + Send + 'static) {
    let handle = std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(body)
        .expect("a thread");
    assert!(handle.join().is_ok(), "the run finished");
}

#[test]
fn deeply_nested_source_and_data_survive() {
    on_small_stack(|| {
        for (source, text) in documents(READ_DEPTH) {
            let document = read(source, &text);
            assert!(
                document.diagnostics().is_empty(),
                "{:?}",
                document.diagnostics().first()
            );
            let leaves: String = document
                .root()
                .expect("a tree")
                .leaves()
                .iter()
                .map(|leaf| leaf.text(&text))
                .collect();
            assert_eq!(leaves, text);
            assert!(
                tree_document(&document).expect("a tree document").len() > READ_DEPTH
            );
            drop(document);
        }
    });
    on_small_stack(|| {
        for (source, text) in documents(FORMAT_DEPTH) {
            let once = format_document(&read(source, &text)).expect("a formatted text");
            assert!(
                once.len() > FORMAT_DEPTH * FORMAT_DEPTH,
                "the nesting is laid out"
            );
            let twice =
                format_document(&read(source, &once)).expect("a formatted text");
            assert_eq!(twice, once);
        }
    });
}
