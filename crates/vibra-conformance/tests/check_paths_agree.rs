//! Differential evidence that the single-source checker (`check_source`,
//! which the static corpus exercises) and the workspace `check` path
//! (`check_resolved`, which the CLI uses) reach the same verdict.
//!
//! Every type-check and interpret case in the corpus is checked both ways:
//! directly, and as the only module of a fresh one-library workspace. The two
//! paths must agree on acceptance and on the set of error codes. Every
//! permitted difference is listed in [`EXPECTED_DIFFERENCES`] with its reason.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use vibra_diagnostics::Level;
use vibra_types::{check_bootstrap_text_import, check_source, verify_bootstrap};

/// Cases whose two verdicts legitimately differ, with the reason.
///
/// Both are profile scope, not rule disagreements: the single-source adapter
/// admits only one exact `@std.text` import and requires an executable
/// declaration, and reports anything else as `@tool.unavailable`.
const EXPECTED_DIFFERENCES: &[(&str, &str)] = &[
    (
        "V1-TYPE-INFER-bootstrap-extra-import",
        "the single-source adapter admits only the exact (import text @std.text)",
    ),
    (
        "V1-TYPE-NAMES-binding-empty-module",
        "the single-source checker needs a module value or function; a workspace module may be empty",
    ),
];

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/cases")
}

/// `(case id, source text)` for every single-source type-check or interpret case.
fn single_source_cases() -> Vec<(String, String)> {
    let mut cases = Vec::new();
    for entry in fs::read_dir(corpus_root()).expect("corpus directory") {
        let directory = entry.expect("corpus entry").path();
        let manifest: toml::Table = fs::read_to_string(directory.join("case.toml"))
            .expect("case manifest")
            .parse()
            .expect("case manifest is TOML");
        let operation = manifest.get("operation").and_then(toml::Value::as_str);
        if !matches!(operation, Some("type-check" | "interpret")) {
            continue;
        }
        let source = manifest["inputs"]["source"].as_str().expect("source input");
        let id = manifest["id"].as_str().expect("case id").to_owned();
        let text = fs::read_to_string(directory.join(source)).expect("case source");
        cases.push((id, text));
    }
    cases.sort();
    assert!(cases.len() >= 50, "the corpus lost its static cases");
    cases
}

/// Acceptance and the distinct error codes. Codes are compared as a set: the
/// workspace path may repeat a diagnostic that the single-source path reports
/// once, which is a reporting difference rather than a different verdict.
type Verdict = (bool, BTreeSet<String>);

fn verdict<'a>(
    diagnostics: impl IntoIterator<Item = &'a vibra_diagnostics::Diagnostic>,
) -> Verdict {
    let codes = diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.level() == Level::Error)
        .map(|diagnostic| diagnostic.code().as_atom().to_owned())
        .collect::<BTreeSet<_>>();
    (codes.is_empty(), codes)
}

fn has_exact_text_import(source: &str) -> bool {
    vibra_syntax::parse_source("input.vib", source)
        .ok()
        .and_then(|document| document.ast().cloned())
        .is_some_and(|ast| {
            ast.declarations().iter().any(|declaration| {
                matches!(declaration, vibra_syntax::Declaration::Import(import)
                    if import.alias().value() == "text"
                        && import.target().value() == "std.text")
            })
        })
}

fn single_source_verdict(source: &str) -> Verdict {
    let checked = if has_exact_text_import(source) {
        let verification = verify_bootstrap().expect("embedded bootstrap");
        check_bootstrap_text_import(&verification, "input.vib", source)
    } else {
        check_source("input.vib", source)
    };
    let (_, codes) = verdict(checked.diagnostics());
    (checked.accepted(), codes)
}

fn workspace_verdict(id: &str, source: &str) -> Verdict {
    let root = std::env::temp_dir()
        .join(format!("vibra-check-paths-{}-{id}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src/app")).expect("create workspace");
    fs::write(
        root.join("project.vibon"),
        "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @lib root: \"src/app\")) dependencies: (map))\n",
    )
    .expect("write project");
    fs::write(root.join("src/app/main.vib"), source).expect("write module");
    let snapshot = vibra_workspace::WorkspaceSnapshot::load(&root)
        .expect("load workspace snapshot");
    let verification = snapshot
        .requires_bootstrap_verification()
        .expect("bootstrap requirement")
        .then(|| verify_bootstrap().expect("embedded bootstrap"));
    let checked = vibra_workspace::semantic::check_all_with_bootstrap(
        &snapshot,
        verification.as_ref(),
    );
    let _ = fs::remove_dir_all(&root);
    let (_, codes) = verdict(checked.diagnostics());
    (
        checked.status() == vibra_workspace::semantic::CheckStatus::Accepted,
        codes,
    )
}

#[test]
fn single_source_and_workspace_checks_agree_on_the_static_corpus() {
    let expected = EXPECTED_DIFFERENCES
        .iter()
        .copied()
        .collect::<BTreeMap<_, _>>();
    let mut disagreements = Vec::new();
    for (id, source) in single_source_cases() {
        let single = single_source_verdict(&source);
        let workspace = workspace_verdict(&id, &source);
        let agrees = single == workspace;
        match (agrees, expected.contains_key(id.as_str())) {
            (true, false) | (false, true) => {}
            (false, false) => disagreements.push(format!(
                "{id}: check_source {single:?}, workspace check {workspace:?}"
            )),
            (true, true) => disagreements.push(format!(
                "{id} is listed as an expected difference but now agrees"
            )),
        }
    }
    assert!(
        disagreements.is_empty(),
        "the two check paths disagree:\n{}",
        disagreements.join("\n")
    );
}
