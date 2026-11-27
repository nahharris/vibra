//! The checker's performed rows are compared with the rows the checked IR
//! recomputes on every accepted corpus case of prefix `EFFECT`, by a host
//! test of `vibra-types`. That crate reads no file, so it embeds the cases;
//! this test keeps its list equal to the corpus: the same cases, each with
//! the same modules.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::support::workspace_root;

/// The file of `vibra-types` that embeds the cases.
const EMBEDDING: &str = "crates/vibra-types/src/check/tests/ceilings.rs";

/// Each accepted case of prefix `EFFECT`, with its modules as `app <stem>`
/// for `tree/src/app/<stem>.vib` and `tests <stem>` for
/// `tree/tests/<stem>.vib`.
fn accepted(root: &Path) -> BTreeMap<String, BTreeSet<String>> {
    let mut found = BTreeMap::new();
    for entry in
        std::fs::read_dir(root.join("conformance/cases/EFFECT")).expect("the cases")
    {
        let case = entry.expect("a case").path();
        let manifest: toml::Table = std::fs::read_to_string(case.join("case.toml"))
            .expect("a manifest")
            .parse()
            .expect("a valid manifest");
        let verdict = manifest
            .get("expect")
            .and_then(|expect| expect.get("verdict"))
            .and_then(toml::Value::as_str);
        if verdict != Some("accepted") {
            continue;
        }
        let id = manifest
            .get("id")
            .and_then(toml::Value::as_str)
            .expect("an id")
            .to_owned();
        let mut modules = BTreeSet::new();
        for (unit, directory) in [("app", "tree/src/app"), ("tests", "tree/tests")] {
            let Ok(files) = std::fs::read_dir(case.join(directory)) else {
                continue;
            };
            for file in files {
                let path = file.expect("a module").path();
                if path.extension().and_then(|extension| extension.to_str())
                    == Some("vib")
                {
                    let stem = path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .expect("a stem");
                    modules.insert(format!("{unit} {stem}"));
                }
            }
        }
        found.insert(id, modules);
    }
    found
}

/// Each case the embedding lists, with its modules: a line
/// `case!("<id>": app "main", tests "run"),`.
fn embedded(source: &str) -> BTreeMap<String, BTreeSet<String>> {
    source
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("case!(\"")?;
            let (id, modules) = rest.split_once("\":")?;
            let modules = modules
                .trim()
                .trim_end_matches(',')
                .trim_end_matches(')')
                .split(',')
                .map(|module| module.trim().replace('"', ""))
                .collect();
            Some((id.to_owned(), modules))
        })
        .collect()
}

#[test]
fn the_row_comparison_embeds_every_accepted_effect_case() {
    let root = workspace_root();
    let source = std::fs::read_to_string(root.join(EMBEDDING)).expect("the embedding");
    let accepted = accepted(&root);
    assert!(!accepted.is_empty(), "no accepted `EFFECT` case");
    assert_eq!(
        embedded(&source),
        accepted,
        "{EMBEDDING} differs from the corpus"
    );
}

#[test]
fn a_case_line_is_read_with_its_modules() {
    let found = embedded("    case!(\"V2-EFFECT-x\": app \"main\", tests \"run\"),\n");
    assert_eq!(
        found,
        BTreeMap::from([(
            "V2-EFFECT-x".to_owned(),
            BTreeSet::from(["app main".to_owned(), "tests run".to_owned()])
        )])
    );
}
