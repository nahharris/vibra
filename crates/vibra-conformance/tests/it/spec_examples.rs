//! Every fenced example of the specification is read by the reader.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Required implementation
//! suites", requires a job for every example of the specification, and
//! "Rule identities and example coverage" requires every accepted source
//! example to be exercised. This test scans `docs/spec/*.md`, reads every
//! `vibra` fence with `parse_source` and every `vibon` fence with
//! `parse_data`, and checks that the leaves of the tree concatenate to the
//! fence and that the fence reads with no diagnostic. The fences the
//! chapters present as rejected, as fragments of a larger form, or as
//! modules of the embedded standard library are named in [`READINGS`],
//! each with the exact codes it produces. A row that names no fence, or
//! whose fence no longer produces its codes, fails the test, so no fence is
//! skipped silently.

use vibra_diagnostics::DiagnosticCode;
use vibra_fmt::format_document;
use vibra_syntax::source::SourceOptions;
use vibra_syntax::{Document, SyntaxKind, parse_data, parse_source_with};

use crate::support::workspace_root;

/// The language of a fence the test reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Language {
    /// A `vibra` fence: a source document.
    Source,
    /// A `vibon` fence: a data document.
    Data,
}

/// One fenced block of a Markdown file.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Fence {
    /// The info string after the opening delimiter, trimmed.
    info: String,
    /// The 1-based line of the opening delimiter.
    line: usize,
    /// The exact text between the delimiter lines, each line with its line
    /// feed.
    text: String,
}

/// The fenced blocks of `markdown`, in order. A fence opens with three
/// backticks or three tildes and closes with a line that starts with the
/// same delimiter; an unclosed fence runs to the end of the file.
fn fences(markdown: &str) -> Vec<Fence> {
    let mut found = Vec::new();
    let mut open: Option<(&str, Fence)> = None;
    for (index, line) in markdown.split_inclusive('\n').enumerate() {
        let bare = line.strip_suffix('\n').unwrap_or(line);
        open = match open {
            None => ["```", "~~~"].into_iter().find_map(|delimiter| {
                bare.strip_prefix(delimiter).map(|info| {
                    let fence = Fence {
                        info: info.trim().to_owned(),
                        line: index + 1,
                        text: String::new(),
                    };
                    (delimiter, fence)
                })
            }),
            Some((delimiter, fence)) if bare.starts_with(delimiter) => {
                found.push(fence);
                None
            }
            Some((delimiter, mut fence)) => {
                fence.text.push_str(line);
                Some((delimiter, fence))
            }
        };
    }
    found.extend(open.map(|(_, fence)| fence));
    found
}

/// How a named fence is read, and what it produces.
#[derive(Clone, Copy, Debug)]
enum Reading {
    /// Read as an ordinary document; it produces exactly these codes, in
    /// the reader's order.
    Rejected(&'static [DiagnosticCode]),
    /// Read as a module of the embedded standard library, where it produces
    /// no diagnostic; as an ordinary module it produces some.
    StandardLibrary,
}

/// One row of the table of named fences: the chapter file, the ordinal of
/// the fence among the chapter's `vibra` and `vibon` fences (from 1), and
/// how it reads.
struct Row {
    chapter: &'static str,
    ordinal: usize,
    reading: Reading,
}

const fn row(chapter: &'static str, ordinal: usize, reading: Reading) -> Row {
    Row {
        chapter,
        ordinal,
        reading,
    }
}

const SOURCE: &str = "01-source-language.md";
const TYPES: &str = "02-type-system.md";
const EFFECTS: &str = "03-effects.md";
const PROGRAMS: &str = "04-programs-and-packages.md";

const FORM: DiagnosticCode = DiagnosticCode::SyntaxInvalidForm;

/// The fences that do not read clean as ordinary documents. Every other
/// `vibra` and `vibon` fence reads with no diagnostic and is already in the
/// canonical format.
const READINGS: &[Row] = &[
    // An anonymous `fn`, an expression: no top-level form.
    row(SOURCE, 6, Reading::Rejected(&[FORM])),
    // The `addChecked` excerpt of `@std.builtin`.
    row(SOURCE, 7, Reading::StandardLibrary),
    // Construction expressions, one run of tokens that opens no form.
    row(SOURCE, 8, Reading::Rejected(&[FORM])),
    // Product and collection expressions, one run of tokens.
    row(SOURCE, 9, Reading::Rejected(&[FORM])),
    // Four body statements: the `let` without a type, the `let` with a
    // pattern, and the `let` with `else` are no module value; the `let`
    // with a name and a type is one.
    row(SOURCE, 10, Reading::Rejected(&[FORM, FORM, FORM])),
    // Body statements: an `if`, a `let` without a type, and a `match`.
    row(SOURCE, 11, Reading::Rejected(&[FORM, FORM, FORM])),
    // `List(T)` of `@std.builtin`, with `Intrinsic` and external members.
    row(TYPES, 1, Reading::StandardLibrary),
    // `Equatable` and `Ordered`, which claim their roles.
    row(TYPES, 3, Reading::StandardLibrary),
    // `Iter`, which claims its role.
    row(TYPES, 7, Reading::StandardLibrary),
    // Two body statements: `let` with a pattern and `let` without a type.
    row(EFFECTS, 6, Reading::Rejected(&[FORM, FORM])),
    // Two body statements: `let` without a type, twice.
    row(EFFECTS, 10, Reading::Rejected(&[FORM, FORM])),
    // The `@stdlibManifest.v2` example. Its `symbols` record has a field
    // named `native`, a reserved word, which the VIBON grammar rejects at
    // the word: the schema and the data grammar disagree, an open question
    // of the programs chapter.
    row(PROGRAMS, 5, Reading::Rejected(&[FORM])),
];

/// The `vibra` and `vibon` fences of one chapter, numbered from 1.
fn read_fences(chapter: &str, markdown: &str) -> Vec<(usize, Language, Fence)> {
    fences(markdown)
        .into_iter()
        .filter_map(|fence| match fence.info.as_str() {
            "vibra" => Some((Language::Source, fence)),
            "vibon" => Some((Language::Data, fence)),
            _ => None,
        })
        .enumerate()
        .map(|(index, (language, fence))| {
            assert!(
                fence.text.ends_with('\n') || fence.text.is_empty(),
                "{chapter}:{}: the fence is closed",
                fence.line
            );
            (index + 1, language, fence)
        })
        .collect()
}

fn read(language: Language, text: &str, standard_library: bool) -> Document {
    match language {
        Language::Source => parse_source_with(
            None,
            text.as_bytes(),
            SourceOptions { standard_library },
        ),
        Language::Data => parse_data(None, text.as_bytes()),
    }
}

fn codes(document: &Document) -> Vec<DiagnosticCode> {
    document
        .diagnostics()
        .iter()
        .map(vibra_diagnostics::Diagnostic::code)
        .collect()
}

/// The leaf texts of `document`, concatenated.
fn leaf_text(document: &Document) -> Option<String> {
    let text = document.text()?;
    let root = document.root()?;
    Some(root.leaves().iter().map(|leaf| leaf.text(text)).collect())
}

#[test]
fn every_fenced_block_of_the_specification_is_read() {
    let spec = workspace_root().join("docs/spec");
    let mut chapters: Vec<_> = std::fs::read_dir(&spec)
        .expect("the specification directory")
        .map(|entry| entry.expect("a readable entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    chapters.sort();
    let mut failures = Vec::new();
    let mut used = vec![false; READINGS.len()];
    let mut read_count = 0;
    for path in &chapters {
        let chapter = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("a UTF-8 file name");
        let markdown = std::fs::read_to_string(path).expect("a readable chapter");
        for (ordinal, language, fence) in read_fences(chapter, &markdown) {
            read_count += 1;
            let at = format!("{chapter} fence {ordinal} (line {})", fence.line);
            let named = READINGS
                .iter()
                .position(|row| row.chapter == chapter && row.ordinal == ordinal);
            if let Some(index) = named
                && let Some(slot) = used.get_mut(index)
            {
                *slot = true;
            }
            let reading = named.and_then(|index| READINGS.get(index)).map(|row| row.reading);
            let plain = read(language, &fence.text, false);
            if leaf_text(&plain).as_deref() != Some(fence.text.as_str()) {
                failures.push(format!("{at}: the leaves do not concatenate to the fence"));
            }
            let observed = codes(&plain);
            match reading {
                None if !observed.is_empty() => {
                    failures.push(format!("{at}: unexpected diagnostics {observed:?}"));
                }
                None => {
                    if let Some(root) = plain.root()
                        && root
                            .leaves()
                            .iter()
                            .any(|leaf| leaf.kind() == SyntaxKind::Missing)
                    {
                        failures.push(format!("{at}: a clean fence has a missing leaf"));
                    }
                    let formatted = format_document(&plain);
                    if formatted.as_deref() != Some(fence.text.as_str()) {
                        failures.push(format!(
                            "{at}: not in the canonical format; formatted:\n{}",
                            formatted.unwrap_or_default()
                        ));
                    }
                }
                Some(Reading::Rejected(expected)) => {
                    if expected.is_empty() {
                        failures.push(format!("{at}: a rejected row names no code"));
                    } else if observed.is_empty() {
                        failures.push(format!("{at}: stale row, the fence reads clean"));
                    } else if observed != expected {
                        failures.push(format!(
                            "{at}: expected {expected:?}, observed {observed:?}"
                        ));
                    }
                }
                Some(Reading::StandardLibrary) => {
                    let library = read(language, &fence.text, true);
                    if observed.is_empty() {
                        failures.push(format!("{at}: stale row, the fence reads clean"));
                    }
                    let library_codes = codes(&library);
                    if !library_codes.is_empty() {
                        failures.push(format!(
                            "{at}: as a standard-library module, {library_codes:?}"
                        ));
                    }
                }
            }
        }
    }
    for (row, used) in READINGS.iter().zip(&used) {
        if !used {
            failures.push(format!(
                "{} fence {}: the row names no fence",
                row.chapter, row.ordinal
            ));
        }
    }
    assert!(read_count > 0, "the specification has fences");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_scanner_tracks_tilde_fences_and_matching_delimiters() {
    let markdown =
        "# Example\n~~~vibon\n{text: \"value\"}\n```\n~~~\n\n```vibra\nuse @a enduse\n```\n";
    let found = fences(markdown);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].info, "vibon");
    assert_eq!(found[0].line, 2);
    assert_eq!(found[0].text, "{text: \"value\"}\n```\n");
    assert_eq!(found[1].info, "vibra");
    assert_eq!(found[1].line, 7);
    assert_eq!(found[1].text, "use @a enduse\n");
}

#[test]
fn an_unclosed_fence_runs_to_the_end_of_the_file() {
    let found = fences("```vibra\nuse @a enduse\n");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].text, "use @a enduse\n");
    let numbered = read_fences("x.md", "```text\nx\n```\n```vibon\n[]\n```\n");
    assert_eq!(numbered.len(), 1);
    assert_eq!(numbered[0].0, 1);
    assert_eq!(numbered[0].1, Language::Data);
}
