//! The parity inventory holds the checked-in table to the runner
//! (`docs/spec/07-diagnostics-and-conformance.md`, "Differential execution";
//! milestone 4 Step 4).
//!
//! The test runs every executable case of `conformance/cases` through the
//! standard dispatcher, which runs the reference interpreter and the
//! WebAssembly backend on each, and checks `conformance/parity.tsv` against the
//! result. When it fails because a case has no row, the message lists the rows
//! to add, so a branch that adds executable cases, or lowers more forms, is
//! told exactly what to write.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::path::{Path, PathBuf};

use vibra_conformance::{
    ConformanceRunner, Corpus, Findings, Observed, ParityInventory, WasmStatus,
    standard_dispatcher,
};

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/cases")
}

/// The step that owns a form, for suggesting a row. It is a hint for the person
/// adding a row, never an authority: the table is the record, and the owner of a
/// case is the latest step among the forms it needs.
fn suggested_step(form: &str) -> Option<&'static str> {
    Some(match form {
        // Lowered by Step 5b; no longer reported, kept for a stale row.
        "sequence" | "variable" | "let" | "if" | "return" | "global"
        | "module-value" | "record" | "variant" | "project" | "tuple"
        | "tuple-project" | "widen" | "call:direct" => "5b",
        // Lowered by Step 6; no longer reported, kept for a stale row.
        "closure" | "captured" | "function" | "default" | "call:indirect"
        | "call:tail-direct" | "call:tail-indirect" | "type:param"
        | "type:function" => "6",
        "match" | "try" => "7",
        "array"
        | "dict"
        | "lookup"
        | "type:array"
        | "type:dict"
        | "parameters:variadic"
        | "wrap" => "8b",
        "type:interface" => "9",
        "call:contract" | "call:tail-contract" | "contract-implementation" => "9",
        "test-module" => "11",
        other => {
            let symbol = other.strip_prefix("external:")?;
            if ["array.", "dict.", "text.", "str.", "bytes."]
                .iter()
                .any(|prefix| symbol.starts_with(prefix))
            {
                "8b"
            } else if symbol.ends_with("to-str")
                || symbol.ends_with("parse")
                || symbol.starts_with("f32.")
                || symbol.starts_with("f64.")
            {
                "8c"
            } else {
                "8a"
            }
        }
    })
}

/// The latest step among `forms`, or `?` when a form has no suggestion.
fn suggested_owner(forms: &[String]) -> String {
    const ORDER: [&str; 10] = ["5a", "5b", "6", "7", "8a", "8b", "8c", "9", "10", "11"];
    let mut latest = None;
    for form in forms {
        let Some(step) = suggested_step(form) else {
            return "?".to_owned();
        };
        let position = ORDER.iter().position(|candidate| *candidate == step);
        latest = latest.max(position);
    }
    latest.map_or_else(|| "?".to_owned(), |position| ORDER[position].to_owned())
}

#[test]
fn the_inventory_matches_the_corpus_and_the_runner() {
    let corpus = Corpus::discover(corpus_root()).expect("the corpus loads");
    let path = corpus
        .parity_inventory_path()
        .expect("the corpus root has a parent");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let inventory =
        ParityInventory::parse(&text).unwrap_or_else(|error| panic!("{error}"));

    let runner = ConformanceRunner::new(standard_dispatcher());
    let reports = corpus
        .cases()
        .iter()
        .filter(|case| case.manifest().is_executable())
        .map(|case| runner.run_case(case))
        .collect::<Vec<_>>();
    assert!(!reports.is_empty(), "the corpus has no executable case");
    let statuses = reports
        .iter()
        .map(|report| {
            (
                report.case_id.as_str(),
                report
                    .backends
                    .as_ref()
                    .and_then(|backends| backends.wasm.as_ref()),
            )
        })
        .collect::<Vec<(&str, Option<&WasmStatus>)>>();
    let findings = inventory.check(statuses);
    assert!(findings.is_empty(), "{}", failure_message(&path, &findings));
}

/// The message of a failed inventory test: what disagrees, and the rows to add.
fn failure_message(path: &Path, findings: &Findings) -> String {
    let mut message = format!(
        "{} disagrees with the corpus and the runner.\n",
        path.display()
    );
    for problem in &findings.problems {
        message.push_str(&format!("  - {problem}\n"));
    }
    if !findings.missing.is_empty() {
        message.push_str(
            "\nExecutable cases with no row. Add these rows to the table, with a real tab\n\
             between columns, then re-run the test. The step is the latest step that owns a\n\
             form the case needs; a `?` means a form with no suggested owner and needs a\n\
             decision. Drop each trailing `# forms: ...` comment, because a row has no\n\
             comment column:\n\n",
        );
        for missing in &findings.missing {
            match &missing.observed {
                Observed::Matched => {
                    message.push_str(&format!("{}\tmatched\n", missing.case_id));
                }
                Observed::NotLowered(forms) => {
                    message.push_str(&format!(
                        "{}\tnot-lowered\t{}\t# forms: {}\n",
                        missing.case_id,
                        suggested_owner(forms),
                        forms.join(", ")
                    ));
                }
            }
        }
    }
    message
}

#[test]
fn every_executable_case_has_a_row_and_no_row_is_extra() {
    // The same rule without running a backend: the set of ids is the set of
    // executable cases.
    let corpus = Corpus::discover(corpus_root()).expect("the corpus loads");
    let path = corpus.parity_inventory_path().expect("a parent");
    let inventory =
        ParityInventory::parse(&std::fs::read_to_string(path).expect("table"))
            .expect("a well-formed table");
    let executable = corpus
        .cases()
        .iter()
        .filter(|case| case.manifest().is_executable())
        .map(|case| case.manifest().id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let rows = inventory
        .rows()
        .map(|(case_id, _)| case_id)
        .collect::<std::collections::BTreeSet<_>>();
    let without_row = executable.difference(&rows).collect::<Vec<_>>();
    let without_case = rows.difference(&executable).collect::<Vec<_>>();
    assert!(without_row.is_empty(), "cases with no row: {without_row:?}");
    assert!(
        without_case.is_empty(),
        "rows naming no case: {without_case:?}"
    );
}

#[test]
fn the_failure_message_names_the_rows_to_add() {
    // A synthetic run against an empty table, standing in for a branch that has
    // added executable cases the table does not know yet.
    let empty = ParityInventory::parse("# nothing\n").expect("an empty table");
    let lowering = WasmStatus::NotLowered {
        forms: vec![
            "call:direct".to_owned(),
            "external:array.of".to_owned(),
            "closure".to_owned(),
        ],
    };
    let matched = WasmStatus::Matched;
    let findings = empty.check([("A-new", Some(&matched)), ("B-new", Some(&lowering))]);
    assert_eq!(findings.missing.len(), 2);
    assert_eq!(
        findings.missing[1].observed,
        Observed::NotLowered(vec![
            "call:direct".to_owned(),
            "external:array.of".to_owned(),
            "closure".to_owned(),
        ])
    );
    if let Observed::NotLowered(forms) = &findings.missing[1].observed {
        assert_eq!(suggested_owner(forms), "8b");
    }
    assert_eq!(suggested_owner(&["mystery".to_owned()]), "?");

    let message = failure_message(Path::new("conformance/parity.tsv"), &findings);
    assert!(
        message.contains("A-new\tmatched\n"),
        "the matched row is spelled out:\n{message}"
    );
    assert!(
        message.contains(
            "B-new\tnot-lowered\t8b\t# forms: call:direct, external:array.of, closure\n"
        ),
        "the not-lowered row is spelled out with its suggested step:\n{message}"
    );
}
