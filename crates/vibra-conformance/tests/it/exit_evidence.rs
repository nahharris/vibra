//! Contract evidence of the M1 exit, observed on committed cases.
//!
//! `docs/spec/02-type-system.md`, "Bare guards": a bare guard is a static
//! obligation, discharged by the rules `GUARD-1` to `GUARD-7` with the
//! integer facts `INT-1` to `INT-8`, and it leaves no run-time check.

use std::collections::BTreeMap;
use std::path::Path;

use vibra_conformance::{Case, Corpus, Verdict, corpus_root};
use vibra_types::{ObligationStatus, check_source};

fn case(id: &str) -> Case {
    let corpus = Corpus::discover(corpus_root()).expect("the corpus loads");
    corpus
        .cases()
        .iter()
        .find(|case| case.manifest().id == id)
        .cloned()
        .unwrap_or_else(|| panic!("the corpus has `{id}`"))
}

/// The one module of a case: its `source`, or the only `.vib` file of its
/// tree.
fn one_module(case: &Case) -> Option<String> {
    if let Some(source) = case.manifest().inputs.source.as_deref() {
        return case.read_file(source).ok();
    }
    let tree = case.tree_directory().ok()??;
    let mut found = Vec::new();
    let mut pending = vec![tree.join("src")];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "vib") {
                found.push(path);
            }
        }
    }
    match found.as_slice() {
        [only] => std::fs::read_to_string(Path::new(only)).ok(),
        _ => None,
    }
}

/// Every discharge rule and integer fact is recorded on a passed obligation
/// of an accepted contract case. The test prints the first such case of
/// each rule.
#[test]
fn every_discharge_rule_and_integer_fact_discharges_an_obligation() {
    let corpus = Corpus::discover(corpus_root()).expect("the corpus loads");
    let mut first: BTreeMap<String, String> = BTreeMap::new();
    for case in corpus.cases() {
        let manifest = case.manifest();
        if !manifest.id.starts_with("V2-CONTRACT-")
            || manifest.expectations.verdict != Verdict::Accepted
        {
            continue;
        }
        let Some(source) = one_module(case) else {
            continue;
        };
        let check = check_source("main.vib", &source);
        for obligation in check.obligations() {
            if obligation.status() != ObligationStatus::Passed {
                continue;
            }
            for rule in obligation.rules() {
                first
                    .entry(rule.clone())
                    .or_insert_with(|| manifest.id.clone());
            }
        }
    }
    let rules: Vec<String> = (1..=7)
        .map(|n| format!("GUARD-{n}"))
        .chain((1..=8).map(|n| format!("INT-{n}")))
        .collect();
    for rule in &rules {
        println!("{rule}: {}", first.get(rule).map_or("none", String::as_str));
    }
    let missing: Vec<&String> = rules
        .iter()
        .filter(|rule| !first.contains_key(*rule))
        .collect();
    assert!(
        missing.is_empty(),
        "no passed obligation records {missing:?}"
    );
}

/// `V2-CONTRACT-no-runtime-check`: the bare `requires` of `pick`, discharged
/// at its one call, leaves no guard on the lowered function, while the same
/// guard with an `else` does.
#[test]
fn a_discharged_bare_guard_leaves_no_guard_node() {
    let source = case("V2-CONTRACT-no-runtime-check")
        .read_file("main.vib")
        .expect("the source reads");
    let guard = |text: &str| {
        let check = check_source("main.vib", text);
        assert_eq!(check.diagnostics(), [], "the source checks");
        let set = check.module_set().expect("a module set");
        let pick = set
            .functions()
            .iter()
            .find(|function| function.name.ends_with(".pick"))
            .expect("the function pick is lowered");
        (pick.entry_guard.is_some(), pick.exit_guard.is_some())
    };
    assert_eq!(guard(&source), (false, false));
    let with_else = source.replacen(
        "requires Ordered.greater(n 0i32)",
        "requires Ordered.greater(n 0i32) else 0i32",
        1,
    );
    assert_ne!(with_else, source);
    assert_eq!(guard(&with_else), (true, false));
}
