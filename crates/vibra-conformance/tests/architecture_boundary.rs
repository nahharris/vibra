//! Enforces the roadmap's architecture boundary as a test rather than a
//! convention.
//!
//! `docs/roadmap/v1.md` states that crate boundaries may evolve but that
//! dependency arrows may not point from language semantics into CLI, MCP,
//! filesystem UI, or a backend. This test reads every workspace manifest and
//! fails when the real dependency graph disagrees with the declared
//! architecture, so adding a forbidden arrow cannot pass CI unaccompanied by
//! a deliberate change to the table below.

// A test asserts by panicking, and indexes fixtures it just built.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The permitted intra-workspace dependencies of each crate.
///
/// A crate may depend on exactly the crates listed for it, and every crate in
/// `crates/` must appear here. Widening a row is a deliberate architecture
/// change: record it in the active milestone's step plan.
const ARCHITECTURE: &[(&str, &[&str])] = &[
    // The leaf. Diagnostics are a language surface and know nothing else.
    ("vibra-diagnostics", &[]),
    // The reader. Emits diagnostics; must not reach the formatter or schemas.
    ("vibra-syntax", &["vibra-diagnostics"]),
    // Consumes the reader's tree; nothing in the language depends on it.
    ("vibra-fmt", &["vibra-diagnostics", "vibra-syntax"]),
    // The wire format. Depends on language facts; no phase depends on it.
    ("vibra-schema", &["vibra-diagnostics"]),
    // The harness. Legitimately sits above every node.
    (
        "vibra-conformance",
        &[
            "vibra-diagnostics",
            "vibra-fmt",
            "vibra-schema",
            "vibra-syntax",
        ],
    ),
];

/// Every dependency section a manifest may use to create an arrow.
const DEPENDENCY_SECTIONS: &[&str] =
    &["dependencies", "dev-dependencies", "build-dependencies"];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate lives two levels below the workspace root")
        .to_path_buf()
}

fn read_manifest(path: &Path) -> toml::Table {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    text.parse::<toml::Table>()
        .unwrap_or_else(|error| panic!("cannot parse {}: {error}", path.display()))
}

/// Reads every manifest under `crates/`, paired with its Vibra dependencies.
fn crate_manifests(root: &Path) -> Vec<(PathBuf, toml::Table)> {
    let entries = std::fs::read_dir(root.join("crates"))
        .expect("the workspace has a crates directory");

    let mut manifests = Vec::new();
    for entry in entries {
        let manifest_path = entry
            .expect("readable directory entry")
            .path()
            .join("Cargo.toml");
        if manifest_path.is_file() {
            let manifest = read_manifest(&manifest_path);
            manifests.push((manifest_path, manifest));
        }
    }

    assert!(!manifests.is_empty(), "the workspace defines no crates");
    manifests
}

/// The Vibra crates named in any dependency section of `manifest`.
fn vibra_dependencies(manifest: &toml::Table) -> BTreeSet<String> {
    let mut edges = BTreeSet::new();
    for section in DEPENDENCY_SECTIONS {
        let Some(table) = manifest.get(*section).and_then(toml::Value::as_table) else {
            continue;
        };
        for dependency in table.keys() {
            if dependency.starts_with("vibra-") {
                edges.insert(dependency.clone());
            }
        }
    }
    edges
}

fn workspace_graph(root: &Path) -> BTreeMap<String, BTreeSet<String>> {
    crate_manifests(root)
        .into_iter()
        .map(|(path, manifest)| {
            let name = manifest
                .get("package")
                .and_then(|package| package.get("name"))
                .and_then(toml::Value::as_str)
                .unwrap_or_else(|| panic!("{} has no package name", path.display()))
                .to_owned();
            let edges = vibra_dependencies(&manifest);
            (name, edges)
        })
        .collect()
}

fn declared_architecture() -> BTreeMap<String, BTreeSet<String>> {
    ARCHITECTURE
        .iter()
        .map(|(crate_name, allowed)| {
            (
                (*crate_name).to_owned(),
                allowed.iter().map(|name| (*name).to_owned()).collect(),
            )
        })
        .collect()
}

/// Describes every way `graph` disagrees with `allowed`.
///
/// Kept pure so the checker itself is testable against synthetic graphs
/// rather than only against a workspace that is expected to pass.
fn violations(
    graph: &BTreeMap<String, BTreeSet<String>>,
    allowed: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<String> {
    let mut found = Vec::new();

    for (crate_name, edges) in graph {
        let Some(permitted) = allowed.get(crate_name) else {
            found.push(format!(
                "crate `{crate_name}` exists but the architecture table does not \
                 describe it"
            ));
            continue;
        };
        for edge in edges.difference(permitted) {
            found.push(format!(
                "crate `{crate_name}` depends on `{edge}`, which the architecture \
                 boundary does not permit"
            ));
        }
    }

    for crate_name in allowed.keys() {
        if !graph.contains_key(crate_name) {
            found.push(format!(
                "the architecture table describes `{crate_name}`, which no manifest \
                 defines"
            ));
        }
    }

    found
}

#[test]
fn workspace_matches_the_declared_architecture() {
    let found = violations(
        &workspace_graph(&workspace_root()),
        &declared_architecture(),
    );
    assert!(
        found.is_empty(),
        "architecture boundary violated:\n{}",
        found.join("\n")
    );
}

#[test]
fn a_forbidden_arrow_is_reported() {
    let graph = BTreeMap::from([
        (
            "vibra-diagnostics".to_owned(),
            BTreeSet::from(["vibra-fmt".to_owned()]),
        ),
        ("vibra-fmt".to_owned(), BTreeSet::new()),
    ]);
    let allowed = BTreeMap::from([
        ("vibra-diagnostics".to_owned(), BTreeSet::new()),
        ("vibra-fmt".to_owned(), BTreeSet::new()),
    ]);

    let found = violations(&graph, &allowed);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("vibra-diagnostics"), "{}", found[0]);
    assert!(found[0].contains("vibra-fmt"), "{}", found[0]);
}

#[test]
fn an_undescribed_crate_is_reported() {
    let graph = BTreeMap::from([("vibra-newcomer".to_owned(), BTreeSet::new())]);
    let found = violations(&graph, &BTreeMap::new());
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("does not describe it"), "{}", found[0]);
}

#[test]
fn a_missing_crate_is_reported() {
    let allowed = BTreeMap::from([("vibra-departed".to_owned(), BTreeSet::new())]);
    let found = violations(&BTreeMap::new(), &allowed);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("no manifest defines"), "{}", found[0]);
}

#[test]
fn no_crate_depends_on_the_archived_tree() {
    for (manifest_path, manifest) in crate_manifests(&workspace_root()) {
        for section in DEPENDENCY_SECTIONS {
            let Some(table) = manifest.get(*section).and_then(toml::Value::as_table)
            else {
                continue;
            };
            for (dependency, value) in table {
                let path = value
                    .get("path")
                    .and_then(toml::Value::as_str)
                    .unwrap_or_default();
                assert!(
                    !path.contains("archive"),
                    "{} depends on `{dependency}` at `{path}`; `archive/pre-v1` is \
                     evidence, never a build input",
                    manifest_path.display()
                );
            }
        }
    }
}

#[test]
fn the_workspace_excludes_the_archived_tree() {
    let manifest = read_manifest(&workspace_root().join("Cargo.toml"));
    let excluded = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("exclude"))
        .and_then(toml::Value::as_array)
        .expect("the workspace declares an exclude list");

    assert!(
        excluded
            .iter()
            .filter_map(toml::Value::as_str)
            .any(|path| path == "archive"),
        "`archive/pre-v1/Cargo.toml` must stay outside the workspace"
    );
}
