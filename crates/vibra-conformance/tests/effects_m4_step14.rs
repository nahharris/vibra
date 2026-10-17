//! M4 Step 14 evidence that spans the checker and the formatter: the checker's
//! fact for each written effect row, and the formatter's idempotence over every
//! corpus program that writes one.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::fs;
use std::path::{Path, PathBuf};

use vibra_fmt::format_source_with_facts;
use vibra_types::check_source;

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/cases")
}

/// The single-source inputs of the cases that write an effect row.
fn effect_inputs() -> Vec<(String, String)> {
    let mut inputs = Vec::new();
    for entry in fs::read_dir(corpus_root()).expect("corpus directory") {
        let directory = entry.expect("corpus entry").path();
        let name = directory
            .file_name()
            .and_then(|name| name.to_str())
            .expect("case name")
            .to_owned();
        if !(name.starts_with("V1-EFFECT-") || name.starts_with("V1-RUNTIME-effect-")) {
            continue;
        }
        if let Ok(source) = fs::read_to_string(directory.join("input.vib")) {
            inputs.push((name, source));
        }
    }
    inputs.sort();
    assert!(inputs.len() >= 20, "the effect corpus lost cases");
    inputs
}

#[test]
fn the_checker_gives_each_row_the_order_of_its_resolved_roots() {
    let source = "(deffect beta (defn b () void (do)))\n(deffect alpha (defn a () void (do)))\n(defn f () void effects: (beta alpha) (beta.b) (alpha.a))\n";
    let checked = check_source("rows.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let rows = checked.effect_row_orders();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].order(), [1, 0]);
    let formatted = format_source_with_facts(
        "rows.vib",
        source,
        checked.application_bindings(),
        rows,
    )
    .expect("format");
    assert!(
        formatted.text().contains("effects: (alpha beta)"),
        "{}",
        formatted.text()
    );
}

#[test]
fn every_effect_corpus_program_formats_to_a_fixed_point() {
    for (name, source) in effect_inputs() {
        let checked = check_source("input.vib", &source);
        let (bindings, rows) = if checked.accepted() {
            (checked.application_bindings(), checked.effect_row_orders())
        } else {
            (&[][..], &[][..])
        };
        let first = format_source_with_facts("input.vib", &source, bindings, rows)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let once = first.text().to_owned();
        let rechecked = check_source("input.vib", &once);
        assert_eq!(
            rechecked.accepted(),
            checked.accepted(),
            "{name}: formatting changed the verdict"
        );
        let (bindings, rows) = if rechecked.accepted() {
            (rechecked.application_bindings(), rechecked.effect_row_orders())
        } else {
            (&[][..], &[][..])
        };
        let second = format_source_with_facts("input.vib", &once, bindings, rows)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(second.text(), once, "{name}: formatting is not idempotent");
    }
}
