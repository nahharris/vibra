//! Formatting is idempotent over every input of the conformance corpus and
//! keeps the tokens it was given.
//!
//! `docs/spec/01-source-language.md`, "Canonical format": formatting MUST be
//! idempotent and MUST preserve meaning, and it never reorders anything that
//! is evaluated. The formatter changes whitespace and comment placement,
//! moves the sections and members of a declaration to their canonical
//! order, and removes an empty `takes {}`, `labelled {}`, `given {}`, or
//! `performs {}`; it changes no token.

use std::path::{Path, PathBuf};

use vibra_fmt::format_document;
use vibra_syntax::source::keyword::is_section;
use vibra_syntax::{DocumentMode, lex, parse};

/// The sections the formatter removes when their brace is empty.
const REMOVABLE_WHEN_EMPTY: [&str; 4] = ["takes", "labelled", "given", "performs"];

/// The quoted strings of a manifest value: `"a"` or `["a", "b"]`.
fn quoted(value: &str) -> Vec<String> {
    value
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

/// Every `.vib` and `.vibon` file below `directory`.
fn documents_below(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("a readable input tree") {
        let path = entry.expect("a readable entry").path();
        if path.is_dir() {
            documents_below(&path, found);
        } else if DocumentMode::from_path(&path).is_some() {
            found.push(path);
        }
    }
}

/// The input documents a case manifest declares: the values of `source`,
/// `project`, and `data` in its `[inputs]` table, and every document of its
/// `tree` directory.
fn declared_inputs(case: &Path, manifest: &str, found: &mut Vec<PathBuf>) {
    let mut in_inputs = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_inputs = line == "[inputs]";
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !in_inputs {
            continue;
        }
        for relative in quoted(value) {
            let path = case.join(relative);
            if key.trim() == "tree" {
                documents_below(&path, found);
            } else {
                found.push(path);
            }
        }
    }
}

/// The declared input documents of every case below `directory`.
fn corpus_inputs(directory: &Path, found: &mut Vec<PathBuf>) {
    let manifest = directory.join("case.toml");
    if manifest.is_file() {
        let text = std::fs::read_to_string(&manifest).expect("a readable manifest");
        declared_inputs(directory, &text, found);
        return;
    }
    for entry in std::fs::read_dir(directory).expect("a readable corpus directory") {
        let path = entry.expect("a readable entry").path();
        if path.is_dir() {
            corpus_inputs(&path, found);
        }
    }
}

/// The texts of the tokens of `text` that are not whitespace or comments.
fn significant_tokens(text: &str) -> Vec<&str> {
    lex(text)
        .tokens
        .iter()
        .filter(|token| !token.kind.is_trivia())
        .map(|token| token.span.text(text).unwrap_or_default())
        .collect()
}

/// `tokens` without the sections the formatter removes when empty.
fn without_empty_sections<'a>(tokens: &[&'a str]) -> Vec<&'a str> {
    let mut kept = Vec::new();
    let mut index = 0;
    while let Some(token) = tokens.get(index) {
        if REMOVABLE_WHEN_EMPTY.contains(token)
            && tokens.get(index + 1) == Some(&"{")
            && tokens.get(index + 2) == Some(&"}")
        {
            index += 3;
        } else {
            kept.push(*token);
            index += 1;
        }
    }
    kept
}

/// Checks that `output` holds the tokens of `input`: the same sequence, or,
/// when a declaration's sections or members moved, the same tokens in an
/// order that only a move can explain.
fn assert_same_tokens(input: &str, output: &str, at: &Path) {
    let before = without_empty_sections(&significant_tokens(input));
    let after = significant_tokens(output);
    if before == after {
        return;
    }
    assert!(
        before.iter().any(|token| is_section(token)),
        "{}: the tokens changed where no section can move",
        at.display()
    );
    let mut sorted_before = before.clone();
    sorted_before.sort_unstable();
    let mut sorted_after = after.clone();
    sorted_after.sort_unstable();
    assert_eq!(
        sorted_before,
        sorted_after,
        "{}: formatting added or lost a token",
        at.display()
    );
}

#[test]
fn every_corpus_input_formats_idempotently_and_keeps_its_tokens() {
    let cases = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/cases");
    let mut inputs = Vec::new();
    corpus_inputs(&cases, &mut inputs);
    inputs.sort();
    inputs.dedup();
    let mut formatted = 0;
    for path in &inputs {
        let Some(mode) = DocumentMode::from_path(path) else {
            continue;
        };
        let bytes = std::fs::read(path).expect("a readable input");
        let document = parse(mode, None, &bytes);
        let Some(once) = format_document(&document) else {
            assert!(document.text().is_none(), "{}", path.display());
            continue;
        };
        let reparsed = parse(mode, None, once.as_bytes());
        let twice = format_document(&reparsed);
        assert_eq!(twice.as_deref(), Some(once.as_str()), "{}", path.display());
        let Some(text) = document.text() else {
            continue;
        };
        if document.diagnostics().is_empty() {
            assert!(
                reparsed.diagnostics().is_empty(),
                "{}: the formatted text reads with {:?}",
                path.display(),
                reparsed.diagnostics()
            );
            assert_same_tokens(text, &once, path);
            formatted += 1;
        } else {
            assert_eq!(once, text, "{}: a recovered input is kept", path.display());
        }
    }
    assert!(
        inputs.len() >= 180 && formatted >= 90,
        "the corpus has its inputs: {} inputs, {formatted} formatted",
        inputs.len()
    );
}

#[test]
fn a_moved_section_keeps_the_tokens_and_a_lost_token_does_not() {
    let at = Path::new("example.vib");
    assert_same_tokens(
        "fn f returns r: I32 takes {x: I32} do return x endfn",
        "fn f\n  takes {x: I32}\n  returns r: I32\n  do\n    return x\nendfn\n",
        at,
    );
    assert_same_tokens(
        "fn f takes {} returns r: I32 do return 1i32 endfn",
        "fn f\n  returns r: I32\n  do\n    return 1i32\nendfn\n",
        at,
    );
    let lost = std::panic::catch_unwind(|| {
        assert_same_tokens(
            "fn f takes {x: I32} returns r: I32 do return x endfn",
            "fn f\n  returns r: I32\n  do\n    return x\nendfn\n",
            at,
        );
    });
    assert!(lost.is_err(), "a lost section is a changed token sequence");
}
